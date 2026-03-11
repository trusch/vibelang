//! Melodies handler implementation.

use crate::backend::Backend;
use crate::compat::{Instant, RwLock};
use crate::handlers::VoicesHandler;
use crate::state::{MelodyState, State};
use crate::traits::{Melodies, MelodyConfig, NoteEvent, Voices};
use crate::types::params::ParamMap;
use crate::types::{Beat, MelodyId, VoiceId};
use crate::validation::Validate;
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Lookahead window in milliseconds for MIDI scheduling.
///
/// Notes within this window ahead of the current beat are scheduled with
/// precise wall-clock timestamps. This provides sub-millisecond timing accuracy
/// for MIDI output while still responding to transport changes promptly.
///
/// 50ms provides a good balance:
/// - Enough buffer to absorb scheduling jitter (~10ms typical)
/// - Short enough to respond to transport stops quickly
/// - At 120 BPM: 0.1 beats of lookahead
/// - At 180 BPM: 0.15 beats of lookahead
const LOOKAHEAD_MS: u64 = 50;

/// Tracks pending note-off: (voice_id, note) -> (beat, wall-clock timestamp)
///
/// We track both the beat position (for display/debugging) and the wall-clock
/// timestamp (for precise scheduling). The timestamp is calculated at note-on
/// time based on the current tempo.
type PendingNoteOffs = HashMap<(VoiceId, u8), (Beat, Instant)>;

/// Tracks which notes belong to which melody (for cleanup on stop/delete)
type MelodyNotes = HashMap<MelodyId, Vec<(VoiceId, u8)>>;

/// Handler for melody operations.
pub struct MelodiesHandler<B: Backend> {
    voices: Arc<VoicesHandler<B>>,
    state: Arc<RwLock<State>>,
    /// Tracks pending note-offs for releasing notes at the correct beat.
    pending_note_offs: Arc<RwLock<PendingNoteOffs>>,
    /// Tracks which notes belong to which melody (for cleanup).
    melody_notes: Arc<RwLock<MelodyNotes>>,
}

/// Info about a note that needs to be triggered.
struct NoteTrigger {
    melody_id: MelodyId,
    voice_id: VoiceId,
    note: u8,
    velocity: f32,
    /// Per-note voice parameters (cutoff, pan, etc.) to merge at trigger time.
    params: ParamMap,
    /// Beat position when the note should end.
    off_beat: Beat,
    /// Wall-clock time when note-on should fire.
    on_timestamp: Instant,
    /// Wall-clock time when note-off should fire.
    off_timestamp: Instant,
}

impl<B: Backend> MelodiesHandler<B> {
    /// Create a new melodies handler.
    pub fn new(state: Arc<RwLock<State>>, voices: Arc<VoicesHandler<B>>) -> Self {
        Self {
            voices,
            state,
            pending_note_offs: Arc::new(RwLock::new(HashMap::new())),
            melody_notes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Process melodies for the current beat.
    ///
    /// Called by the runtime's tick loop to trigger note events.
    ///
    /// Uses lookahead scheduling for MIDI: notes are scheduled ahead of time
    /// with precise wall-clock timestamps for sub-millisecond timing accuracy.
    pub async fn tick(&self, current_beat: Beat) {
        let now = Instant::now();

        // First, process note-offs that are due
        self.process_note_offs_timed(now).await;

        // Collect note-on triggers while holding lock
        let triggers = {
            let mut state = self.state.write().await;
            let mut triggers = Vec::new();

            // Get tempo for beat-to-time conversion
            let tempo = state.tempo;
            let lookahead_beats = Beat::from_f64(LOOKAHEAD_MS as f64 * tempo / 60000.0);

            // Get IDs of playing melodies
            let melody_ids: Vec<MelodyId> = state
                .melodies
                .iter()
                .filter(|(_, m)| m.playing)
                .map(|(id, _)| *id)
                .collect();

            // Debug: log playing melodies count periodically (every ~1 second at 100Hz tick)
            static TICK_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let tick_count = TICK_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if tick_count.is_multiple_of(100) && !melody_ids.is_empty() {
                tracing::debug!(
                    "MelodiesHandler::tick: beat={:.2}, {} playing melodies, lookahead={:.3} beats",
                    current_beat.to_f64(),
                    melody_ids.len(),
                    lookahead_beats.to_f64()
                );
            }

            // Calculate tolerance for quantization boundary detection
            // At 100Hz tick rate, ~10ms between ticks = ~0.02 beats at 120 BPM
            let tolerance = 0.02 * tempo / 120.0;
            let time_sig = state.time_sig;

            for melody_id in melody_ids {
                if let Some(melody) = state.melodies.get_mut(&melody_id) {
                    // Check for pending content swap at musical boundary
                    if melody.try_apply_pending(current_beat, time_sig, tolerance) {
                        tracing::debug!(
                            "Melody {:?}: applied pending content swap at beat {:.2}",
                            melody_id,
                            current_beat.to_f64()
                        );
                    }

                    // Skip melodies with zero or near-zero length to prevent runaway triggering
                    let min_length = Beat::from_f64(0.0625); // 1/64th note
                    if !melody.playing || melody.content.length < min_length {
                        tracing::trace!(
                            "MelodiesHandler: melody {:?} skipped (playing={}, length={})",
                            melody_id,
                            melody.playing,
                            melody.content.length.to_f64()
                        );
                        continue;
                    }

                    let length = melody.content.length;
                    let last_pos = melody.loop_position;

                    // Calculate lookahead position (where we're scheduling up to)
                    // This is ahead of current_beat by the lookahead window
                    let lookahead_pos = (current_beat + lookahead_beats) % length;

                    // Find notes that should be scheduled
                    // Notes between last_pos (exclusive) and lookahead_pos (inclusive)
                    let notes_to_trigger: Vec<NoteEvent> = if lookahead_pos < last_pos {
                        // Wrapped around - schedule notes from last_pos to end and 0 to lookahead_pos
                        melody
                            .content
                            .notes
                            .iter()
                            .filter(|n| n.beat > last_pos || n.beat <= lookahead_pos)
                            .cloned()
                            .collect()
                    } else if last_pos == lookahead_pos {
                        // First tick case: schedule notes exactly at this position
                        melody
                            .content
                            .notes
                            .iter()
                            .filter(|n| n.beat == lookahead_pos)
                            .cloned()
                            .collect()
                    } else {
                        // Normal case - schedule notes between last_pos (exclusive) and lookahead_pos (inclusive)
                        melody
                            .content
                            .notes
                            .iter()
                            .filter(|n| n.beat > last_pos && n.beat <= lookahead_pos)
                            .cloned()
                            .collect()
                    };

                    // Update loop position to what we've scheduled up to
                    melody.loop_position = lookahead_pos;

                    // Get voice info for triggering
                    let melody_name = melody.content.name.clone();
                    let notes_count = melody.content.notes.len();
                    let voice_id = match melody.content.voice {
                        Some(id) => id,
                        None => {
                            tracing::warn!(
                                "Melody {:?} '{}': no voice configured, skipping. notes_count={}",
                                melody_id,
                                melody_name,
                                notes_count
                            );
                            continue;
                        }
                    };

                    // Verify voice exists
                    if !state.voices.contains_key(&voice_id) {
                        tracing::warn!(
                            "Melody {:?} '{}': voice {:?} not found in state. Available voices: {:?}",
                            melody_id,
                            melody_name,
                            voice_id,
                            state.voices.keys().collect::<Vec<_>>()
                        );
                        continue;
                    }

                    // Get base amp from voice config (for velocity multiplication)
                    let base_amp = state
                        .voices
                        .get(&voice_id)
                        .map(|v| v.config.params.get("amp").copied().unwrap_or(1.0))
                        .unwrap_or(1.0);

                    for note_event in notes_to_trigger {
                        // Multiply voice amp by note velocity
                        let final_velocity = base_amp * note_event.velocity;

                        // Calculate beat offset from current position to the note
                        // We need to figure out how far ahead (in beats) the note is from current_beat
                        let current_pos_in_loop = current_beat % length;
                        let note_beat_in_loop = note_event.beat;

                        // Calculate beat offset, handling wrap-around
                        let beat_offset = if note_beat_in_loop >= current_pos_in_loop {
                            // Note is ahead of current position in the loop
                            note_beat_in_loop - current_pos_in_loop
                        } else {
                            // Note wrapped around (it's at the start of the next loop iteration)
                            (length - current_pos_in_loop) + note_beat_in_loop
                        };

                        // Calculate wall-clock timestamp for note-on
                        // offset_secs = beat_offset * 60.0 / tempo
                        let offset_secs = beat_offset.to_f64() * 60.0 / tempo;
                        // Clamp to non-negative (schedule immediately if somehow in the past)
                        let on_timestamp = now + Duration::from_secs_f64(offset_secs.max(0.0));

                        // Calculate note-off timestamp
                        let duration_secs = note_event.duration.to_f64() * 60.0 / tempo;
                        let off_offset_secs = offset_secs + duration_secs;
                        // Clamp to non-negative
                        let off_timestamp = now + Duration::from_secs_f64(off_offset_secs.max(0.0));
                        let off_beat = current_beat + Beat::from_f64(off_offset_secs.max(0.0) * tempo / 60.0);

                        tracing::debug!(
                            "Melody note scheduled: voice={:?}, note={}, vel={:.2}, on_offset={:.3}ms, off_offset={:.3}ms",
                            voice_id,
                            note_event.note,
                            final_velocity,
                            offset_secs * 1000.0,
                            off_offset_secs * 1000.0
                        );

                        triggers.push(NoteTrigger {
                            melody_id,
                            voice_id,
                            note: note_event.note,
                            velocity: final_velocity,
                            params: note_event.params.clone(),
                            off_beat,
                            on_timestamp,
                            off_timestamp,
                        });
                    }
                }
            }

            triggers
        };

        // Send triggers through voice handler and schedule note-offs (lock released)
        for trigger in triggers {
            // Check if this note is already playing - if so, the new scheduling will replace it
            {
                let mut note_offs = self.pending_note_offs.write().await;
                let key = (trigger.voice_id, trigger.note);
                if note_offs.remove(&key).is_some() {
                    tracing::debug!(
                        "Re-triggering note {} on voice {:?}, replacing pending note-off",
                        trigger.note,
                        trigger.voice_id
                    );
                }
            }

            // Trigger note through voice handler with timestamp (for MIDI lookahead)
            // Uses note_on_at if available (MIDI feature), otherwise falls back to immediate
            // When per-note params are present, use the _with_params variant to merge them
            // into the synth creation params (only affects this specific note instance).
            #[cfg(feature = "midi")]
            {
                if trigger.params.is_empty() {
                    let _ = self
                        .voices
                        .note_on_at(trigger.voice_id, trigger.note, trigger.velocity, Some(trigger.on_timestamp))
                        .await;
                } else {
                    let _ = self
                        .voices
                        .note_on_at_with_params(trigger.voice_id, trigger.note, trigger.velocity, Some(trigger.on_timestamp), &trigger.params)
                        .await;
                }
            }
            #[cfg(not(feature = "midi"))]
            {
                if trigger.params.is_empty() {
                    let _ = self
                        .voices
                        .note_on(trigger.voice_id, trigger.note, trigger.velocity)
                        .await;
                } else {
                    let _ = self
                        .voices
                        .note_on_with_params(trigger.voice_id, trigger.note, trigger.velocity, &trigger.params)
                        .await;
                }
            }

            // Schedule the note-off with timestamp
            {
                let mut note_offs = self.pending_note_offs.write().await;
                let key = (trigger.voice_id, trigger.note);
                tracing::debug!(
                    "Scheduling note-off: voice={:?}, note={}, off_beat={:.2}, ts={:?}",
                    trigger.voice_id,
                    trigger.note,
                    trigger.off_beat.to_f64(),
                    trigger.off_timestamp
                );
                note_offs.insert(key, (trigger.off_beat, trigger.off_timestamp));
            }

            // Track this note for the melody (for cleanup on stop/delete)
            {
                let mut melody_notes = self.melody_notes.write().await;
                melody_notes
                    .entry(trigger.melody_id)
                    .or_default()
                    .push((trigger.voice_id, trigger.note));
            }
        }
    }

    /// Process pending note-offs using wall-clock timestamps.
    ///
    /// This method checks pending note-offs against the current time (Instant),
    /// not beat position, for precise timing independent of tempo fluctuations.
    async fn process_note_offs_timed(&self, now: Instant) {
        // Note-offs that are due now or have a timestamp within the next few ms
        // will be sent with their exact timestamp for precise timing
        let notes_to_off: Vec<((VoiceId, u8), Instant)> = {
            let mut note_offs = self.pending_note_offs.write().await;
            let mut to_remove = Vec::new();

            if !note_offs.is_empty() {
                tracing::trace!(
                    "Checking {} pending note-offs at {:?}",
                    note_offs.len(),
                    now
                );
            }

            // Find note-offs that are due (timestamp has passed or is imminent)
            // We use a small threshold to batch nearby note-offs together
            let threshold = now + Duration::from_millis(5);

            for (&key, &(off_beat, off_timestamp)) in note_offs.iter() {
                if off_timestamp <= threshold {
                    tracing::debug!(
                        "Note-off due: voice={:?}, note={}, off_beat={:.2}, ts={:?}",
                        key.0,
                        key.1,
                        off_beat.to_f64(),
                        off_timestamp
                    );
                    to_remove.push((key, off_timestamp));
                }
            }

            for (key, _) in &to_remove {
                note_offs.remove(key);
            }

            to_remove
        };

        // Clean up melody_notes tracking
        if !notes_to_off.is_empty() {
            let mut melody_notes = self.melody_notes.write().await;
            for ((voice_id, note), _) in &notes_to_off {
                // Remove from all melody note lists
                for notes in melody_notes.values_mut() {
                    notes.retain(|n| n != &(*voice_id, *note));
                }
            }
        }

        // Send note-offs through voice handler with timestamps
        for ((voice_id, note), timestamp) in notes_to_off {
            tracing::debug!(
                "Sending note-off: voice={:?}, note={}, ts={:?}",
                voice_id,
                note,
                timestamp
            );
            #[cfg(feature = "midi")]
            {
                let _ = self.voices.note_off_at(voice_id, note, Some(timestamp)).await;
            }
            #[cfg(not(feature = "midi"))]
            {
                let _ = self.voices.note_off(voice_id, note).await;
            }
        }
    }

    /// Release all active notes for a melody and clear its pending note-offs.
    async fn release_melody_notes(&self, melody_id: MelodyId) {
        // Get all notes for this melody
        let notes_to_release: Vec<(VoiceId, u8)> = {
            let mut melody_notes = self.melody_notes.write().await;
            melody_notes.remove(&melody_id).unwrap_or_default()
        };

        if notes_to_release.is_empty() {
            tracing::debug!(
                "release_melody_notes: no tracked notes for melody {:?}",
                melody_id
            );
            return;
        }

        tracing::debug!(
            "Releasing {} tracked notes for melody {:?}: {:?}",
            notes_to_release.len(),
            melody_id,
            notes_to_release
        );

        // Remove pending note-offs for these notes
        {
            let mut note_offs = self.pending_note_offs.write().await;
            for key in &notes_to_release {
                note_offs.remove(key);
            }
        }

        // Send note-offs through voice handler
        for (voice_id, note) in notes_to_release {
            let _ = self.voices.note_off(voice_id, note).await;
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Melodies for MelodiesHandler<B> {
    async fn create(&self, id: MelodyId, config: MelodyConfig) -> Result<()> {
        // Validate configuration before acquiring lock
        config.validate()?;

        let mut state = self.state.write().await;

        if state.melodies.contains_key(&id) {
            return Err(Error::MelodyExists(id));
        }

        // Verify the voice exists if specified
        if let Some(voice_id) = config.voice {
            if !state.voices.contains_key(&voice_id) {
                return Err(Error::VoiceNotFound(voice_id));
            }
        }

        state.melodies.insert(id, MelodyState::new(id, config));

        Ok(())
    }

    async fn delete(&self, id: MelodyId) -> Result<()> {
        // Release any active notes for this melody first
        self.release_melody_notes(id).await;

        let mut state = self.state.write().await;

        state
            .melodies
            .remove(&id)
            .ok_or(Error::MelodyNotFound(id))?;

        Ok(())
    }

    async fn start(&self, id: MelodyId) -> Result<()> {
        let mut state = self.state.write().await;

        let melody = state
            .melodies
            .get_mut(&id)
            .ok_or(Error::MelodyNotFound(id))?;

        melody.playing = true;
        melody.loop_position = Beat::ZERO;

        Ok(())
    }

    async fn stop(&self, id: MelodyId) -> Result<()> {
        tracing::debug!("MelodiesHandler::stop called for melody {:?}", id);

        // Get voice info before releasing notes (for MIDI all-notes-off failsafe)
        let voice_id = {
            let state = self.state.read().await;
            state.melodies.get(&id).and_then(|m| m.content.voice)
        };

        // Release any active notes for this melody
        self.release_melody_notes(id).await;

        // Additionally, clear ALL pending note-offs for this melody's voice
        // This handles cases where notes were scheduled but not yet in melody_notes
        if let Some(vid) = voice_id {
            let cleared_count = {
                let mut note_offs = self.pending_note_offs.write().await;
                let before = note_offs.len();
                note_offs.retain(|(voice, _note), _| *voice != vid);
                before - note_offs.len()
            };
            if cleared_count > 0 {
                tracing::debug!(
                    "Melody {:?}: cleared {} additional pending note-offs for voice {:?}",
                    id, cleared_count, vid
                );
            }

            // Send note-offs for notes 0-127 as a failsafe for MIDI voices
            // This ensures stuck notes on external MIDI devices are released
            // Only send for notes that might reasonably be playing (common range)
            #[cfg(feature = "midi")]
            {
                let state = self.state.read().await;
                if let Some(voice) = state.voices.get(&vid) {
                    if voice.config.midi_output.is_some() {
                        tracing::debug!(
                            "Melody {:?}: sending all-notes-off for MIDI voice {:?}",
                            id, vid
                        );
                        // Send note-offs for common MIDI note range (21-108, piano range)
                        // This is more efficient than 0-127 and covers most use cases
                        drop(state); // Release lock before async calls
                        for note in 21..=108u8 {
                            let _ = self.voices.note_off(vid, note).await;
                        }
                    }
                }
            }
        }

        let mut state = self.state.write().await;

        let melody = state
            .melodies
            .get_mut(&id)
            .ok_or(Error::MelodyNotFound(id))?;

        tracing::debug!(
            "Melody {:?}: setting playing=false (was {})",
            id, melody.playing
        );
        melody.playing = false;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::NoteEvent;

    /// Create a note event at a given beat position.
    fn note_at(beat: f64, note: u8) -> NoteEvent {
        NoteEvent {
            beat: Beat::from_f64(beat),
            note,
            velocity: 1.0,
            duration: Beat::from_f64(0.5),
            params: std::collections::HashMap::new(),
        }
    }

    /// Filter notes that should trigger between last_pos and new_pos.
    /// This mirrors the logic in MelodiesHandler::tick.
    fn filter_notes(
        notes: &[NoteEvent],
        last_pos: Beat,
        new_pos: Beat,
        _length: Beat,
    ) -> Vec<Beat> {
        if new_pos < last_pos {
            // Wrapped around - trigger notes from last_pos to end and 0 to new_pos
            notes
                .iter()
                .filter(|n| n.beat >= last_pos || n.beat <= new_pos)
                .map(|n| n.beat)
                .collect()
        } else if last_pos == new_pos {
            // First tick case: trigger notes exactly at this position
            notes
                .iter()
                .filter(|n| n.beat == new_pos)
                .map(|n| n.beat)
                .collect()
        } else {
            // Normal case - trigger notes between last_pos (exclusive) and new_pos (inclusive)
            // Note: This is DIFFERENT from patterns! (exclusive start, inclusive end)
            notes
                .iter()
                .filter(|n| n.beat > last_pos && n.beat <= new_pos)
                .map(|n| n.beat)
                .collect()
        }
    }

    // =========================================================================
    // Basic Note Triggering Tests
    // =========================================================================

    #[test]
    fn test_normal_tick_does_not_trigger_note_at_last_pos() {
        // Note at 0.0, tick from 0.0 to 0.25
        // Melody uses EXCLUSIVE start, so note at last_pos should NOT trigger
        let notes = vec![note_at(0.0, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        // Note: This is the opposite of pattern behavior!
        assert_eq!(
            triggered.len(),
            0,
            "Note at last_pos should NOT trigger (exclusive)"
        );
    }

    #[test]
    fn test_normal_tick_triggers_note_at_new_pos() {
        // Note at 0.25, tick from 0.0 to 0.25
        // Melody uses INCLUSIVE end, so note at new_pos should trigger
        let notes = vec![note_at(0.25, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        // Note: This is the opposite of pattern behavior!
        assert_eq!(
            triggered.len(),
            1,
            "Note at new_pos should trigger (inclusive)"
        );
    }

    #[test]
    fn test_normal_tick_triggers_note_between_positions() {
        // Note at 0.125, tick from 0.0 to 0.25
        let notes = vec![note_at(0.125, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1, "Note between positions should trigger");
    }

    // =========================================================================
    // First Tick Tests (last_pos == new_pos)
    // =========================================================================

    #[test]
    fn test_first_tick_triggers_note_at_zero() {
        // First tick: last_pos == new_pos == 0
        let notes = vec![note_at(0.0, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.0),
            Beat::from_f64(0.0),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1, "Note at 0 should trigger on first tick");
    }

    #[test]
    fn test_first_tick_only_triggers_exact_match() {
        // First tick at 0.5, notes at 0.0, 0.5, 1.0
        let notes = vec![note_at(0.0, 60), note_at(0.5, 62), note_at(1.0, 64)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.5),
            Beat::from_f64(0.5),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            1,
            "Only exact match should trigger on first tick"
        );
        assert_eq!(triggered[0].to_f64(), 0.5);
    }

    // =========================================================================
    // Loop Wrap-Around Tests (Critical Edge Cases)
    // =========================================================================

    #[test]
    fn test_wrap_triggers_note_at_zero() {
        // Melody loops: last_pos = 3.75, new_pos = 0.25
        // Note at 0.0 should trigger
        let notes = vec![note_at(0.0, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1, "Note at 0 should trigger after wrap");
    }

    #[test]
    fn test_wrap_triggers_note_near_end() {
        // Melody loops: last_pos = 3.75, new_pos = 0.25
        // Note at 3.875 should trigger (between 3.75 and end)
        let notes = vec![note_at(3.875, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            1,
            "Note near end should trigger during wrap"
        );
    }

    #[test]
    fn test_wrap_triggers_note_at_new_pos() {
        // Melody loops: last_pos = 3.75, new_pos = 0.25
        // Note at 0.25 should trigger (inclusive during wrap)
        let notes = vec![note_at(0.25, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            1,
            "Note at new_pos should trigger during wrap"
        );
    }

    #[test]
    fn test_wrap_does_not_double_trigger() {
        // Melody loops: last_pos = 3.75, new_pos = 0.25
        // Note at 0.5 should NOT trigger (it's past new_pos)
        let notes = vec![note_at(0.5, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            0,
            "Note past new_pos should NOT trigger during wrap"
        );
    }

    #[test]
    fn test_wrap_multiple_notes() {
        // Melody loops: last_pos = 3.5, new_pos = 0.5
        // Notes at 3.75, 0.0, 0.25, 0.5 should all trigger
        // Note at 1.0 should NOT trigger
        let notes = vec![
            note_at(3.75, 60),
            note_at(0.0, 62),
            note_at(0.25, 64),
            note_at(0.5, 65),
            note_at(1.0, 67),
        ];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(3.5),
            Beat::from_f64(0.5),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 4, "4 notes should trigger during wrap");
    }

    // =========================================================================
    // CRITICAL: Pattern vs Melody Boundary Difference
    // =========================================================================

    /// This test documents the DIFFERENCE between pattern and melody triggering.
    /// - Patterns: last_pos INCLUSIVE, new_pos EXCLUSIVE
    /// - Melodies: last_pos EXCLUSIVE, new_pos INCLUSIVE
    ///
    /// This means a note at beat 1.0 with tick from 0.75 to 1.0:
    /// - Pattern: Would NOT trigger (1.0 < new_pos is false)
    /// - Melody: WOULD trigger (1.0 <= new_pos is true)
    #[test]
    fn test_melody_boundary_is_inclusive() {
        let notes = vec![note_at(1.0, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.75),
            Beat::from_f64(1.0),
            Beat::from_f64(4.0),
        );
        // In melody, new_pos is INCLUSIVE
        assert_eq!(
            triggered.len(),
            1,
            "Melody should trigger at new_pos (inclusive)"
        );
    }

    /// And the note at last_pos should NOT trigger in melody (but WOULD in pattern).
    #[test]
    fn test_melody_last_pos_is_exclusive() {
        let notes = vec![note_at(0.75, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.75),
            Beat::from_f64(1.0),
            Beat::from_f64(4.0),
        );
        // In melody, last_pos is EXCLUSIVE
        assert_eq!(
            triggered.len(),
            0,
            "Melody should NOT trigger at last_pos (exclusive)"
        );
    }

    // =========================================================================
    // Note at Beat 0 After Start
    // =========================================================================

    #[test]
    fn test_note_at_zero_triggers_on_first_tick_only() {
        // First tick (last_pos == new_pos == 0): should trigger
        let notes = vec![note_at(0.0, 60)];
        let first_tick = filter_notes(
            &notes,
            Beat::from_f64(0.0),
            Beat::from_f64(0.0),
            Beat::from_f64(4.0),
        );
        assert_eq!(first_tick.len(), 1, "Should trigger on first tick");

        // Second tick (last_pos = 0, new_pos = 0.25): should NOT trigger again
        let second_tick = filter_notes(
            &notes,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            second_tick.len(),
            0,
            "Should NOT re-trigger on second tick (exclusive start)"
        );
    }

    // =========================================================================
    // Short Melody Tests
    // =========================================================================

    #[test]
    fn test_very_short_melody_half_beat() {
        // 0.5 beat melody with note at 0.25
        let notes = vec![note_at(0.25, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(0.5),
        );
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_very_short_melody_wrap() {
        // 0.5 beat melody, tick from 0.4 to 0.1 (wraps)
        let notes = vec![note_at(0.0, 60), note_at(0.1, 62)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.4),
            Beat::from_f64(0.1),
            Beat::from_f64(0.5),
        );
        // Should trigger 0.0 and 0.1 (both after wrap, 0.0 >= 0.4 is false, 0.0 <= 0.1 is true)
        assert_eq!(triggered.len(), 2);
    }

    // =========================================================================
    // Multiple Notes at Same Beat
    // =========================================================================

    #[test]
    fn test_multiple_notes_at_same_beat() {
        // Chord: multiple notes at beat 0.5
        let notes = vec![
            note_at(0.5, 60), // C
            note_at(0.5, 64), // E
            note_at(0.5, 67), // G
        ];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.25),
            Beat::from_f64(0.5),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 3, "All chord notes should trigger");
    }

    // =========================================================================
    // Boundary Precision Tests
    // =========================================================================

    #[test]
    fn test_note_exactly_at_boundary_normal() {
        // Test the exact boundary behavior
        // Note at 1.0, tick from 0.5 to 1.0
        let notes = vec![note_at(1.0, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.5),
            Beat::from_f64(1.0),
            Beat::from_f64(4.0),
        );
        // new_pos is INCLUSIVE in melody (unlike pattern)
        assert_eq!(
            triggered.len(),
            1,
            "Boundary should be INCLUSIVE in normal tick"
        );
    }

    #[test]
    fn test_note_not_retriggered_next_tick() {
        // The note at 1.0 should NOT trigger again on the next tick
        let notes = vec![note_at(1.0, 60)];
        let triggered = filter_notes(
            &notes,
            Beat::from_f64(1.0),
            Beat::from_f64(1.25),
            Beat::from_f64(4.0),
        );
        // last_pos is EXCLUSIVE in melody, so note at 1.0 won't trigger again
        assert_eq!(
            triggered.len(),
            0,
            "Note should NOT re-trigger when it becomes last_pos"
        );
    }

    // =========================================================================
    // Per-Note Params Tests
    // =========================================================================

    /// Create a note event at a given beat position with per-note params.
    fn note_at_with_params(
        beat: f64,
        note: u8,
        params: std::collections::HashMap<String, f32>,
    ) -> NoteEvent {
        NoteEvent {
            beat: Beat::from_f64(beat),
            note,
            velocity: 1.0,
            duration: Beat::from_f64(0.5),
            params,
        }
    }

    #[test]
    fn test_note_with_params_triggers_correctly() {
        // Ensure notes with params are still found by filter_notes
        let mut params = std::collections::HashMap::new();
        params.insert("cutoff".to_string(), 2000.0_f32);

        let notes = vec![
            note_at_with_params(0.5, 60, params.clone()),
            note_at(1.0, 62), // no params
        ];

        let triggered = filter_notes(
            &notes,
            Beat::from_f64(0.0),
            Beat::from_f64(1.0),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            2,
            "Both notes (with and without params) should trigger"
        );
    }

    #[test]
    fn test_note_with_params_preserves_params_through_filter() {
        let mut params = std::collections::HashMap::new();
        params.insert("cutoff".to_string(), 2000.0_f32);
        params.insert("pan".to_string(), -0.5_f32);

        let notes = vec![note_at_with_params(0.5, 60, params)];

        // Filter finds the note
        let triggered: Vec<&NoteEvent> = notes
            .iter()
            .filter(|n| n.beat > Beat::from_f64(0.0) && n.beat <= Beat::from_f64(1.0))
            .collect();

        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].params.len(), 2);
        assert_eq!(*triggered[0].params.get("cutoff").unwrap(), 2000.0);
        assert_eq!(*triggered[0].params.get("pan").unwrap(), -0.5);
    }

    #[test]
    fn test_notes_with_different_params_are_distinct() {
        let mut params_a = std::collections::HashMap::new();
        params_a.insert("cutoff".to_string(), 2000.0_f32);

        let mut params_b = std::collections::HashMap::new();
        params_b.insert("cutoff".to_string(), 500.0_f32);

        let note_a = note_at_with_params(0.0, 60, params_a);
        let note_b = note_at_with_params(0.0, 60, params_b);

        // Notes at the same beat with different params should not be equal
        assert_ne!(note_a, note_b, "Notes with different params should not be equal");
    }

    #[test]
    fn test_note_with_empty_params_equals_no_params() {
        let note_a = note_at(0.0, 60);
        let note_b = note_at_with_params(0.0, 60, std::collections::HashMap::new());
        assert_eq!(
            note_a, note_b,
            "Note with no params should equal note with empty params"
        );
    }
}
