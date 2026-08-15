//! Melodies handler implementation.

use super::patterns::{occurrences_in_window, resolve_window};
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
pub(crate) const LOOKAHEAD_MS: u64 = 50;

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
            static TICK_COUNTER: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let tick_count = TICK_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if tick_count.is_multiple_of(100) && !melody_ids.is_empty() {
                tracing::debug!(
                    "MelodiesHandler::tick: beat={:.2}, {} playing melodies, lookahead={:.3} beats",
                    current_beat.to_f64(),
                    melody_ids.len(),
                    lookahead_beats.to_f64()
                );
            }

            let time_sig = state.time_sig;

            for melody_id in melody_ids {
                if let Some(melody) = state.melodies.get_mut(&melody_id) {
                    if !melody.playing {
                        continue;
                    }

                    // Skip melodies with zero or near-zero length to prevent
                    // runaway triggering. If a pending swap would give the
                    // melody a usable length again, apply it immediately so
                    // it can recover.
                    let min_length = Beat::from_f64(0.0625); // 1/64th note
                    if melody.content.length < min_length {
                        melody.apply_pending_swap();
                        if melody.content.length < min_length {
                            tracing::trace!(
                                "MelodiesHandler: melody {:?} skipped (length={})",
                                melody_id,
                                melody.content.length.to_f64()
                            );
                            continue;
                        }
                    }

                    // Scheduling runs on an absolute global-beat window
                    // [w_from, w_to) — see PatternsHandler::tick. Melodies
                    // are anchored at global beat 0 (absolute time).
                    let (w_from, w_to) =
                        resolve_window(melody.scheduled_until, current_beat, lookahead_beats);
                    if w_to <= w_from {
                        // Transport paused: the window is already covered.
                        continue;
                    }

                    // Content swaps apply when the *lookahead* crosses the
                    // quantization boundary: old content is scheduled strictly
                    // before the boundary, new content from the boundary on,
                    // so the new bar's downbeat always comes from the new
                    // content.
                    let boundary = melody
                        .pending_swap_boundary(w_from, time_sig)
                        .filter(|b| *b < w_to);
                    let mut scheduled: Vec<(NoteEvent, Beat)> = Vec::new();
                    match boundary {
                        Some(b) => {
                            scheduled.extend(occurrences_in_window(
                                &melody.content.notes,
                                |n| n.beat,
                                melody.content.length,
                                Beat::ZERO,
                                w_from,
                                b,
                            ));
                            melody.apply_pending_swap();
                            tracing::debug!(
                                "Melody {:?}: applied pending content swap at beat {:.2}",
                                melody_id,
                                b.to_f64()
                            );
                            scheduled.extend(occurrences_in_window(
                                &melody.content.notes,
                                |n| n.beat,
                                melody.content.length,
                                Beat::ZERO,
                                b,
                                w_to,
                            ));
                        }
                        None => scheduled.extend(occurrences_in_window(
                            &melody.content.notes,
                            |n| n.beat,
                            melody.content.length,
                            Beat::ZERO,
                            w_from,
                            w_to,
                        )),
                    }

                    // Advance the watermark and keep the loop-local mirror in
                    // sync for observability (websocket status et al.).
                    melody.scheduled_until = Some(w_to);
                    melody.loop_position = w_to % melody.content.length;

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

                    for (note_event, abs_beat) in scheduled {
                        // Multiply voice amp by note velocity
                        let final_velocity = base_amp * note_event.velocity;

                        // Wall-clock timestamp straight from the absolute
                        // beat position of this occurrence.
                        let offset_secs = (abs_beat - current_beat).to_f64() * 60.0 / tempo;
                        // Clamp to non-negative (schedule immediately if somehow in the past)
                        let on_timestamp = now + Duration::from_secs_f64(offset_secs.max(0.0));

                        // Calculate note-off timestamp
                        let duration_secs = note_event.duration.to_f64() * 60.0 / tempo;
                        let off_offset_secs = offset_secs + duration_secs;
                        // Clamp to non-negative
                        let off_timestamp = now + Duration::from_secs_f64(off_offset_secs.max(0.0));
                        let off_beat =
                            current_beat + Beat::from_f64(off_offset_secs.max(0.0) * tempo / 60.0);

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
                        .note_on_at(
                            trigger.voice_id,
                            trigger.note,
                            trigger.velocity,
                            Some(trigger.on_timestamp),
                        )
                        .await;
                } else {
                    let _ = self
                        .voices
                        .note_on_at_with_params(
                            trigger.voice_id,
                            trigger.note,
                            trigger.velocity,
                            Some(trigger.on_timestamp),
                            &trigger.params,
                        )
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
                        .note_on_with_params(
                            trigger.voice_id,
                            trigger.note,
                            trigger.velocity,
                            &trigger.params,
                        )
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
                let _ = self
                    .voices
                    .note_off_at(voice_id, note, Some(timestamp))
                    .await;
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

        let current_beat = state.current_beat;
        let melody = state
            .melodies
            .get_mut(&id)
            .ok_or(Error::MelodyNotFound(id))?;

        melody.playing = true;
        // Melodies run on absolute song time (anchor 0), so starting one —
        // cold boot, live `.start()`, or hot reload alike — means resuming
        // the phase `current_beat % length`. The watermark starts at "now":
        // nothing already passed fires (no burst), nothing ahead is skipped.
        melody.scheduled_until = Some(current_beat);
        melody.loop_position = if melody.content.length > Beat::ZERO {
            current_beat % melody.content.length
        } else {
            Beat::ZERO
        };

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
                    id,
                    cleared_count,
                    vid
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
                            id,
                            vid
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
            id,
            melody.playing
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

    /// Absolute-beat occurrences of the given notes inside `[from, to)`,
    /// looped at `length` and anchored at global beat 0 — thin wrapper over
    /// the production helper used by `tick` (shared with patterns).
    fn window(notes: &[NoteEvent], length: f64, from: f64, to: f64) -> Vec<f64> {
        occurrences_in_window(
            notes,
            |n| n.beat,
            Beat::from_f64(length),
            Beat::ZERO,
            Beat::from_f64(from),
            Beat::from_f64(to),
        )
        .into_iter()
        .map(|(_, abs)| abs.to_f64())
        .collect()
    }

    // =========================================================================
    // Scheduling-window tests (half-open [from, to), absolute beats)
    // =========================================================================

    #[test]
    fn window_start_inclusive_end_exclusive() {
        let notes = vec![note_at(0.25, 60)];
        // Note exactly at the window start fires; exactly at the end it
        // belongs to the next window — no double-fire across ticks.
        assert_eq!(window(&notes, 4.0, 0.25, 0.5), vec![0.25]);
        assert_eq!(window(&notes, 4.0, 0.0, 0.25), Vec::<f64>::new());
    }

    #[test]
    fn window_wraps_across_loop_boundary() {
        // Window straddling abs beat 4.0 of a 4-beat loop: catches the tail
        // of iteration 0 and the head of iteration 1.
        let notes = vec![note_at(0.0, 60), note_at(3.875, 62)];
        assert_eq!(window(&notes, 4.0, 3.85, 4.05), vec![3.875, 4.0]);
    }

    #[test]
    fn consecutive_windows_never_double_fire() {
        let notes = vec![note_at(0.0, 60), note_at(0.5, 62), note_at(1.0, 64)];
        let mut all = Vec::new();
        let mut from = 0.0;
        while from < 4.0 {
            all.extend(window(&notes, 4.0, from, from + 0.1));
            from += 0.1;
        }
        assert_eq!(all, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn very_short_melody_yields_every_iteration() {
        // 0.5-beat loop with notes at 0.0 and 0.125: a window across the wrap
        // catches both the tail and the next head — and nothing twice.
        let notes = vec![note_at(0.0, 60), note_at(0.125, 62)];
        assert_eq!(window(&notes, 0.5, 0.25, 0.6), vec![0.5]);
        assert_eq!(window(&notes, 0.5, 0.0625, 0.25), vec![0.125]);
    }

    #[test]
    fn multiple_notes_at_same_beat_all_fire() {
        // Chord: multiple notes at beat 0.5
        let notes = vec![
            note_at(0.5, 60), // C
            note_at(0.5, 64), // E
            note_at(0.5, 67), // G
        ];
        assert_eq!(window(&notes, 4.0, 0.25, 0.75).len(), 3);
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
        // Ensure notes with params are still found by the scheduling window
        let mut params = std::collections::HashMap::new();
        params.insert("cutoff".to_string(), 2000.0_f32);

        let notes = vec![
            note_at_with_params(0.5, 60, params.clone()),
            note_at(1.0, 62), // no params
        ];

        let triggered = window(&notes, 4.0, 0.0, 1.25);
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
        assert_ne!(
            note_a, note_b,
            "Notes with different params should not be equal"
        );
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
