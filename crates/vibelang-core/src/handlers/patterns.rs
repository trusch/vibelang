//! Patterns handler implementation.

use crate::backend::Backend;
use crate::compat::{Instant, RwLock};
use crate::handlers::VoicesHandler;
use crate::state::{PatternOwner, PatternState, State};
use crate::traits::{PatternConfig, PatternContent, Patterns, Step, Voices};
use crate::types::{Beat, ParamMap, PatternId, VoiceId};
use crate::validation::Validate;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// Lookahead window in milliseconds for pattern scheduling.
///
/// Matches the melody lookahead for consistent timing behavior.
/// Notes within this window ahead of the current beat are scheduled with
/// precise wall-clock timestamps.
const LOOKAHEAD_MS: u64 = 50;

/// Handler for pattern operations.
pub struct PatternsHandler<B: Backend> {
    state: Arc<RwLock<State>>,
    voices: Arc<VoicesHandler<B>>,
}

/// Info about a step that needs to be triggered.
struct StepTrigger {
    /// Owning pattern. Re-checked just before dispatch so that a `stop()` call
    /// that lands after the tick collected triggers but before the dispatch
    /// loop fires them is honoured (otherwise the last lookahead-window of
    /// notes leaks past the stop — observable as the previous loop bleeding
    /// over the start of a new looper recording).
    pattern_id: PatternId,
    voice_id: VoiceId,
    params: ParamMap,
    /// Note number if this is a MIDI pattern step (from params).
    note: Option<u8>,
    /// Wall-clock time when this step should fire.
    timestamp: Instant,
    /// Duration after `timestamp` at which to send a matching MIDI note-off.
    /// `None` for audio-synth steps or pattern steps without a `gate` param.
    /// Set from the step's `gate` param (recorded beats) × `60 / tempo`.
    /// Without this, the looper plays a note that's never released, and the
    /// polyphony pool only evicts it when the NEXT note for the same voice
    /// arrives — so the last note in a loop is held for the full loop length.
    gate_dur: Option<Duration>,
}

impl<B: Backend> PatternsHandler<B> {
    /// Create a new patterns handler.
    pub fn new(state: Arc<RwLock<State>>, voices: Arc<VoicesHandler<B>>) -> Self {
        Self { state, voices }
    }

    /// Create a pattern with an explicit runtime owner.
    pub async fn create_with_owner(
        &self,
        id: PatternId,
        config: PatternConfig,
        owner: PatternOwner,
    ) -> Result<()> {
        // Validate configuration before acquiring lock
        config.validate()?;

        let mut state = self.state.write().await;

        if state.patterns.contains_key(&id) {
            return Err(Error::PatternExists(id));
        }

        // Verify the voice exists if specified
        if let Some(voice_id) = config.voice {
            if !state.voices.contains_key(&voice_id) {
                return Err(Error::VoiceNotFound(voice_id));
            }
        }

        state
            .patterns
            .insert(id, PatternState::with_owner(id, config, owner));

        Ok(())
    }

    /// Process patterns for the current beat.
    ///
    /// Called by the runtime's tick loop to trigger pattern events.
    ///
    /// Uses lookahead scheduling: steps within the lookahead window ahead of
    /// the current beat are scheduled with precise wall-clock timestamps.
    /// This enables sub-millisecond timing accuracy for MIDI patterns.
    pub async fn tick(&self, current_beat: Beat) {
        let now = Instant::now();

        // Collect triggers while holding lock
        let triggers = {
            let mut state = self.state.write().await;
            let mut triggers = Vec::new();

            // Get tempo for beat-to-time conversion
            let tempo = state.tempo;
            let lookahead_beats = Beat::from_f64(LOOKAHEAD_MS as f64 * tempo / 60000.0);

            // Get IDs of playing patterns
            let pattern_ids: Vec<PatternId> = state
                .patterns
                .iter()
                .filter(|(_, p)| p.playing)
                .map(|(id, _)| *id)
                .collect();

            if !pattern_ids.is_empty() {
                tracing::trace!(
                    "Patterns tick at beat {:?}: {} playing patterns, lookahead={:.3} beats",
                    current_beat,
                    pattern_ids.len(),
                    lookahead_beats.to_f64()
                );
            }

            // Calculate tolerance for quantization boundary detection
            // At 100Hz tick rate, ~10ms between ticks = ~0.02 beats at 120 BPM
            let tolerance = 0.02 * tempo / 120.0;
            let time_sig = state.time_sig;

            for pattern_id in pattern_ids {
                if let Some(pattern) = state.patterns.get_mut(&pattern_id) {
                    // Check for pending content swap at musical boundary
                    if pattern.try_apply_pending(current_beat, time_sig, tolerance) {
                        tracing::debug!(
                            "Pattern {:?}: applied pending content swap at beat {:.2}",
                            pattern_id,
                            current_beat.to_f64()
                        );
                    }

                    // Skip patterns with zero or near-zero length to prevent runaway triggering
                    let min_length = Beat::from_f64(0.0625); // 1/64th note
                    if !pattern.playing || pattern.content.length < min_length {
                        continue;
                    }

                    let length = pattern.content.length;
                    let last_pos = pattern.loop_position;

                    // Position math runs in pattern-local time so playback
                    // always starts at the pattern's beat 0, not at
                    // `current_beat % length`. Clamp to zero in case a clock
                    // jump moved `current_beat` backwards past `start_beat`.
                    let elapsed_beats =
                        Beat::from_f64((current_beat - pattern.start_beat).to_f64().max(0.0));

                    // Calculate lookahead position (where we're scheduling up to)
                    let lookahead_pos = (elapsed_beats + lookahead_beats) % length;

                    // Find steps that should be scheduled
                    // Steps between last_pos (exclusive) and lookahead_pos (inclusive)
                    let steps_to_trigger: Vec<Step> = if lookahead_pos < last_pos {
                        // Wrapped around - schedule steps AFTER last_pos to end, and 0 to lookahead_pos
                        pattern
                            .content
                            .steps
                            .iter()
                            .filter(|s| s.beat > last_pos || s.beat <= lookahead_pos)
                            .cloned()
                            .collect()
                    } else if last_pos == lookahead_pos {
                        // First tick case: schedule steps exactly at this position
                        pattern
                            .content
                            .steps
                            .iter()
                            .filter(|s| s.beat == lookahead_pos)
                            .cloned()
                            .collect()
                    } else {
                        // Normal case - schedule steps between last_pos (exclusive) and lookahead_pos (inclusive)
                        pattern
                            .content
                            .steps
                            .iter()
                            .filter(|s| s.beat > last_pos && s.beat <= lookahead_pos)
                            .cloned()
                            .collect()
                    };

                    // Update loop position to what we've scheduled up to
                    pattern.loop_position = lookahead_pos;

                    // Get voice info for triggering
                    let voice_id = match pattern.content.voice {
                        Some(id) => id,
                        None => continue, // Skip patterns without a voice
                    };

                    // Get base params from voice config
                    let base_params = state
                        .voices
                        .get(&voice_id)
                        .map(|v| v.config.params.clone())
                        .unwrap_or_default();

                    for step in steps_to_trigger {
                        // Merge voice params with step params
                        let mut params = base_params.clone();

                        // Multiply voice amp by step velocity (don't overwrite voice's amp)
                        let voice_amp = base_params.get("amp").copied().unwrap_or(1.0);
                        let step_velocity = step.params.get("amp").copied().unwrap_or(1.0);
                        let final_amp = voice_amp * step_velocity;

                        // Extend with step params but then set the correct amp
                        params.extend(step.params.clone());
                        params.insert("amp".to_string(), final_amp);

                        // Calculate beat offset and wall-clock timestamp
                        // We need to figure out how far ahead (in beats) the
                        // step is from `current_beat`. Use pattern-local time
                        // so the offset is measured against the pattern's own
                        // beat 0, matching the `lookahead_pos` math above.
                        let current_pos_in_loop = elapsed_beats % length;
                        let step_beat = step.beat;

                        // Calculate beat offset, handling wrap-around
                        let beat_offset = if step_beat >= current_pos_in_loop {
                            // Step is ahead of current position in the loop
                            step_beat - current_pos_in_loop
                        } else {
                            // Step wrapped around (it's at the start of the next loop iteration)
                            (length - current_pos_in_loop) + step_beat
                        };

                        let offset_secs = beat_offset.to_f64() * 60.0 / tempo;
                        // Clamp to non-negative (schedule immediately if somehow in the past)
                        let timestamp = now + Duration::from_secs_f64(offset_secs.max(0.0));

                        // Check for note parameter (for MIDI patterns)
                        let note = step.params.get("note").map(|n| *n as u8);

                        // Translate the recorded `gate` (beats) into a
                        // wall-clock duration so the dispatch loop can
                        // schedule a matching note-off. Only meaningful for
                        // MIDI steps; ignored for audio-synth triggers.
                        let gate_dur = if note.is_some() {
                            step.params.get("gate").and_then(|gate_beats| {
                                let secs = (*gate_beats as f64) * 60.0 / tempo;
                                if secs.is_finite() && secs > 0.0 {
                                    Some(Duration::from_secs_f64(secs))
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        };

                        tracing::debug!(
                            "Pattern step scheduled: voice={:?}, note={:?}, offset={:.3}ms, gate={:?}",
                            voice_id,
                            note,
                            offset_secs * 1000.0,
                            gate_dur
                        );

                        triggers.push(StepTrigger {
                            pattern_id,
                            voice_id,
                            params,
                            note,
                            timestamp,
                            gate_dur,
                        });
                    }
                }
            }

            triggers
        };

        // Trigger through voice handler (lock released)
        if !triggers.is_empty() {
            tracing::debug!(
                "Patterns: triggering {} steps through voice handler",
                triggers.len()
            );
        }
        for trigger in triggers {
            // Re-check that the owning pattern is still playing. The tick
            // collected lookahead triggers inside a state lock that we have
            // since released; if `stop()` ran in the meantime, the pattern
            // is no longer playing and we must drop the trigger rather than
            // fire one last batch of stale note-ons.
            let still_playing = {
                let state = self.state.read().await;
                state
                    .patterns
                    .get(&trigger.pattern_id)
                    .map(|p| p.playing)
                    .unwrap_or(false)
            };
            if !still_playing {
                continue;
            }

            // If step has a note parameter and voice has MIDI output, use note_on_at
            // Otherwise, use trigger for audio synths
            #[cfg(feature = "midi")]
            if let Some(note) = trigger.note {
                let velocity = trigger.params.get("amp").copied().unwrap_or(1.0);
                let note_generation = self
                    .voices
                    .note_on_at_tracked(trigger.voice_id, note, velocity, Some(trigger.timestamp))
                    .await
                    .ok();

                // Schedule the matching note-off after the step's gate. Without
                // this, MIDI sustains the note forever — the polyphony pool
                // only evicts it when the NEXT note-on for the same voice
                // arrives, so the loop's last note hangs for the full loop
                // period. Fire-and-forget tokio task; if the pattern is
                // stopped in the meantime, `stop()` already sweeps held notes,
                // so the redundant note-off here is benign (or no-op if the
                // pattern was deleted).
                if let (Some(gate), Some(note_generation)) = (trigger.gate_dur, note_generation) {
                    let voices = self.voices.clone();
                    let state = self.state.clone();
                    let pattern_id = trigger.pattern_id;
                    let voice_id = trigger.voice_id;
                    // Anchor the release to the note-on's scheduled start,
                    // not to dispatch time: audio-voice note-ons are now
                    // backend-scheduled (no sleep before dispatch returns),
                    // so sleeping a bare `gate` here would release up to a
                    // full lookahead window early.
                    let off_deadline = trigger.timestamp + gate;
                    tokio::spawn(async move {
                        tokio::time::sleep_until(tokio::time::Instant::from_std(off_deadline))
                            .await;
                        let still_playing = state
                            .read()
                            .await
                            .patterns
                            .get(&pattern_id)
                            .map(|p| p.playing)
                            .unwrap_or(false);
                        if still_playing {
                            let _ = voices
                                .note_off_at_if_generation(voice_id, note, note_generation, None)
                                .await;
                        }
                    });
                }
            } else {
                // Audio synth trigger (no MIDI note param). The lookahead
                // timestamp flows through to the backend so the trigger
                // lands sample-accurately (scsynth OSC bundle time-tag)
                // instead of firing on the tick that crossed it.
                let _ = self
                    .voices
                    .trigger_at(trigger.voice_id, &trigger.params, Some(trigger.timestamp))
                    .await;
            }

            #[cfg(not(feature = "midi"))]
            {
                let _ = self
                    .voices
                    .trigger_at(trigger.voice_id, &trigger.params, Some(trigger.timestamp))
                    .await;
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Patterns for PatternsHandler<B> {
    async fn create(&self, id: PatternId, config: PatternConfig) -> Result<()> {
        self.create_with_owner(id, config, PatternOwner::Script)
            .await
    }

    async fn delete(&self, id: PatternId) -> Result<()> {
        let mut state = self.state.write().await;

        state
            .patterns
            .remove(&id)
            .ok_or(Error::PatternNotFound(id))?;

        Ok(())
    }

    async fn start(&self, id: PatternId) -> Result<()> {
        let mut state = self.state.write().await;

        let current_beat = state.current_beat;
        let pattern = state
            .patterns
            .get_mut(&id)
            .ok_or(Error::PatternNotFound(id))?;

        pattern.playing = true;
        pattern.loop_position = Beat::ZERO;
        // Anchor the pattern to "now" so position math in `tick` (which works
        // on `current_beat - start_beat`) replays from beat 0 of the pattern.
        // For a looper finalising a recording mid-song this is the difference
        // between "playback starts at the beginning of what you played" and
        // "playback starts at a random offset into the recording".
        pattern.start_beat = current_beat;

        Ok(())
    }

    async fn stop(&self, id: PatternId) -> Result<()> {
        // Flip `playing` to false inside the lock and capture the voice plus
        // its currently held notes so we can send note-offs *outside* the
        // lock. The looper relies on `stop()` actually silencing the loop —
        // not just disabling the scheduler — because the next `tick()` will
        // already have queued lookahead note-ons into the dispatch path,
        // and MIDI sustains held notes until an explicit note-off arrives.
        // Without this, the previous loop bleeds over the start of the next
        // recording (audible as "altes Pattern noch da während ich das neue
        // einspiele").
        let (voice_id, held_notes) = {
            let mut state = self.state.write().await;
            let pattern = state
                .patterns
                .get_mut(&id)
                .ok_or(Error::PatternNotFound(id))?;
            pattern.playing = false;
            let voice_id = pattern.content.voice;

            // Collect every note that's currently sounding for the pattern's
            // voice. Audio synths track held notes in `voice.note_nodes`;
            // MIDI-output voices instead push through `state.midi_voice_pool`
            // (slots + overflow). Without sweeping the pool, the previous
            // loop's MIDI note-ons stay latched on the external synth — the
            // pool only releases them when the next note-on for the same
            // (voice, note) arrives, so the old pattern bleeds into the
            // pauses of the new recording.
            let mut held: Vec<u8> = Vec::new();
            if let Some(vid) = voice_id {
                if let Some(v) = state.voices.get(&vid) {
                    held.extend(v.note_nodes.keys().copied());
                }
                #[cfg(feature = "midi")]
                if let Some(pool) = state.midi_voice_pool.get(&vid) {
                    for slot in &pool.slots {
                        if let Some((note, _)) = slot {
                            held.push(*note);
                        }
                    }
                    for (note, _) in &pool.overflow {
                        held.push(*note);
                    }
                }
            }
            (voice_id, held)
        };

        if let Some(vid) = voice_id {
            for note in held_notes {
                let _ = self.voices.note_off(vid, note).await;
            }
        }

        Ok(())
    }

    async fn set_param(&self, id: PatternId, param: &str, value: f32) -> Result<()> {
        let mut state = self.state.write().await;

        let pattern = state
            .patterns
            .get_mut(&id)
            .ok_or(Error::PatternNotFound(id))?;

        // Clone current content, modify steps, and replace
        let mut new_steps = pattern.content.steps.clone();
        for step in &mut new_steps {
            step.params.insert(param.to_string(), value);
        }

        // Create new content with modified steps
        let new_content = Arc::new(PatternContent {
            name: pattern.content.name.clone(),
            voice: pattern.content.voice,
            steps: new_steps,
            length: pattern.content.length,
            swing: pattern.content.swing,
        });

        pattern.content = new_content;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a step at a given beat position.
    fn step_at(beat: f64) -> Step {
        Step {
            beat: Beat::from_f64(beat),
            params: ParamMap::new(),
        }
    }

    /// Filter steps that should trigger between last_pos and new_pos.
    /// This mirrors the logic in PatternsHandler::tick.
    ///
    /// Boundary convention (same as melodies):
    /// - last_pos: EXCLUSIVE (already triggered on previous tick)
    /// - new_pos: INCLUSIVE (current position)
    fn filter_steps(steps: &[Step], last_pos: Beat, new_pos: Beat, length: Beat) -> Vec<Beat> {
        if new_pos < last_pos {
            // Wrapped around - trigger steps AFTER last_pos to end, and 0 to new_pos (inclusive)
            // Note: steps must be < length (enforced by validation)
            steps
                .iter()
                .filter(|s| s.beat < length && (s.beat > last_pos || s.beat <= new_pos))
                .map(|s| s.beat)
                .collect()
        } else if last_pos == new_pos {
            // First tick case: trigger steps exactly at this position
            steps
                .iter()
                .filter(|s| s.beat == new_pos)
                .map(|s| s.beat)
                .collect()
        } else {
            // Normal case - trigger steps between last_pos (exclusive) and new_pos (inclusive)
            steps
                .iter()
                .filter(|s| s.beat > last_pos && s.beat <= new_pos)
                .map(|s| s.beat)
                .collect()
        }
    }

    // =========================================================================
    // Basic Step Triggering Tests
    // =========================================================================
    //
    // Boundary convention: (last_pos, new_pos]
    // - last_pos: EXCLUSIVE (was already triggered on previous tick)
    // - new_pos: INCLUSIVE (current position)

    #[test]
    fn test_normal_tick_does_not_trigger_step_at_last_pos() {
        let steps = vec![step_at(0.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 0);
    }

    #[test]
    fn test_normal_tick_triggers_step_at_new_pos() {
        let steps = vec![step_at(0.25)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_normal_tick_triggers_step_between_positions() {
        let steps = vec![step_at(0.125)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_first_tick_triggers_step_at_zero() {
        let steps = vec![step_at(0.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.0),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_wrap_triggers_step_at_zero() {
        let steps = vec![step_at(0.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_wrap_triggers_step_near_end() {
        let steps = vec![step_at(3.875)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1);
    }

    // =========================================================================
    // Lookahead timestamps flow to the backend (sample-accurate triggers)
    // =========================================================================

    mod scheduling {
        use super::*;
        use crate::backend::{AddAction, Backend, BufferInfo};
        use crate::state::GroupState;
        use crate::traits::VoiceConfig;
        use crate::types::{BufferId, BusId, GroupId, NodeId};
        use std::path::Path;
        use std::sync::Mutex;

        #[derive(Debug)]
        struct MockError;

        impl std::fmt::Display for MockError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "mock error")
            }
        }

        impl std::error::Error for MockError {}

        /// Backend that records the scheduling deadline of every synth
        /// creation reaching it through `create_synth_at`.
        struct SchedulingBackend {
            calls: Mutex<Vec<(String, Option<Instant>)>>,
        }

        impl SchedulingBackend {
            fn new() -> Self {
                Self {
                    calls: Mutex::new(Vec::new()),
                }
            }

            fn calls(&self) -> Vec<(String, Option<Instant>)> {
                self.calls.lock().unwrap().clone()
            }
        }

        #[async_trait::async_trait]
        impl Backend for SchedulingBackend {
            type Error = MockError;

            async fn load_synthdef(
                &self,
                _name: &str,
                _data: &[u8],
            ) -> std::result::Result<(), Self::Error> {
                Ok(())
            }

            async fn create_synth(
                &self,
                _def: &str,
                _node: NodeId,
                _target: NodeId,
                _action: AddAction,
                _params: &ParamMap,
            ) -> std::result::Result<(), Self::Error> {
                Ok(())
            }

            async fn create_synth_at(
                &self,
                def: &str,
                _node: NodeId,
                _target: NodeId,
                _action: AddAction,
                _params: &ParamMap,
                _param_buses: &[(String, u32)],
                at: Option<Instant>,
            ) -> std::result::Result<(), Self::Error> {
                self.calls.lock().unwrap().push((def.to_string(), at));
                Ok(())
            }

            async fn create_group(
                &self,
                _node: NodeId,
                _target: NodeId,
                _action: AddAction,
            ) -> std::result::Result<(), Self::Error> {
                Ok(())
            }

            async fn free_node(&self, _node: NodeId) -> std::result::Result<(), Self::Error> {
                Ok(())
            }

            async fn run_node(
                &self,
                _node: NodeId,
                _running: bool,
            ) -> std::result::Result<(), Self::Error> {
                Ok(())
            }

            async fn set_param(
                &self,
                _node: NodeId,
                _param: &str,
                _value: f32,
            ) -> std::result::Result<(), Self::Error> {
                Ok(())
            }

            async fn map_param_to_bus(
                &self,
                _node: NodeId,
                _param: &str,
                _bus: u32,
            ) -> std::result::Result<(), Self::Error> {
                Ok(())
            }

            async fn load_buffer(
                &self,
                _id: BufferId,
                _path: &Path,
            ) -> std::result::Result<BufferInfo, Self::Error> {
                Ok(BufferInfo {
                    frames: 44100,
                    channels: 2,
                    sample_rate: 44100.0,
                })
            }

            async fn alloc_buffer(
                &self,
                _id: BufferId,
                frames: u32,
                channels: u16,
            ) -> std::result::Result<BufferInfo, Self::Error> {
                Ok(BufferInfo {
                    frames,
                    channels,
                    sample_rate: 44100.0,
                })
            }

            async fn write_buffer(
                &self,
                _id: BufferId,
                _path: &Path,
            ) -> std::result::Result<(), Self::Error> {
                Ok(())
            }

            async fn free_buffer(&self, _id: BufferId) -> std::result::Result<(), Self::Error> {
                Ok(())
            }

            fn current_time(&self) -> Instant {
                Instant::now()
            }
        }

        async fn setup_voice(
            backend: &Arc<SchedulingBackend>,
            state: &Arc<RwLock<State>>,
        ) -> (Arc<VoicesHandler<SchedulingBackend>>, VoiceId) {
            {
                let mut s = state.write().await;
                s.synthdefs.insert("test_synth".to_string());
                let group_id = GroupId::new(1);
                s.groups.insert(
                    group_id,
                    GroupState {
                        id: group_id,
                        name: "TestGroup".to_string(),
                        parent: None,
                        node_id: NodeId(100),
                        audio_bus: BusId(16),
                        link_synth_node_id: None,
                        muted: false,
                        soloed: false,
                        params: ParamMap::new(),
                        output_bus: None,
                        output_channels: None,
                    },
                );
            }

            let voices = Arc::new(VoicesHandler::new(backend.clone(), state.clone()));
            let voice_id = VoiceId::new(1);
            voices
                .create(
                    voice_id,
                    VoiceConfig::new("test_voice", "test_synth", GroupId::new(1)),
                )
                .await
                .expect("voice creation");
            (voices, voice_id)
        }

        /// A pattern step scheduled by the lookahead tick must reach the
        /// backend with its precise future deadline — this is the end-to-end
        /// path that makes audio triggers sample-accurate.
        #[tokio::test]
        async fn pattern_step_timestamp_flows_to_backend() {
            let backend = Arc::new(SchedulingBackend::new());
            let state = Arc::new(RwLock::new(State::default()));
            let (voices, voice_id) = setup_voice(&backend, &state).await;

            let handler = PatternsHandler::new(state.clone(), voices);
            let pattern_id = PatternId::new(1);
            // One step 0.05 beats into the pattern: at the default 120 BPM
            // that is 25ms ahead of beat 0 — inside the 50ms lookahead
            // window of the first tick.
            let config = PatternConfig {
                name: "p".to_string(),
                voice: Some(voice_id),
                steps: vec![Step {
                    beat: Beat::from_f64(0.05),
                    params: ParamMap::new(),
                }],
                length: Beat::from_f64(4.0),
                swing: 0.0,
            };
            handler.create(pattern_id, config).await.expect("create");
            handler.start(pattern_id).await.expect("start");

            let before = Instant::now();
            handler.tick(Beat::ZERO).await;

            let calls = backend.calls();
            assert_eq!(calls.len(), 1, "exactly one step should fire");
            let (def, at) = &calls[0];
            assert_eq!(def, "test_synth");
            let at = at.expect("pattern trigger must carry a future timestamp");
            assert!(at >= before, "timestamp must not be in the past");
            let lead = at.duration_since(before);
            assert!(
                lead >= Duration::from_millis(20) && lead <= Duration::from_millis(250),
                "timestamp should be ~25ms ahead of the tick, got {:?}",
                lead
            );
        }

        /// Direct (non-lookahead) triggers — live MIDI input, HTTP API,
        /// immediate script eval — must keep the immediate path: no
        /// scheduling deadline is attached.
        #[tokio::test]
        async fn immediate_trigger_carries_no_timestamp() {
            let backend = Arc::new(SchedulingBackend::new());
            let state = Arc::new(RwLock::new(State::default()));
            let (voices, voice_id) = setup_voice(&backend, &state).await;

            voices
                .trigger(voice_id, &ParamMap::new())
                .await
                .expect("trigger");

            let calls = backend.calls();
            assert_eq!(calls.len(), 1);
            assert!(
                calls[0].1.is_none(),
                "immediate triggers must not carry a schedule deadline"
            );
        }
    }
}
