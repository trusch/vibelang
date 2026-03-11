//! Patterns handler implementation.

use crate::backend::Backend;
use crate::compat::{Instant, RwLock};
use crate::handlers::VoicesHandler;
use crate::state::{PatternState, State};
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
    voice_id: VoiceId,
    params: ParamMap,
    /// Note number if this is a MIDI pattern step (from params).
    note: Option<u8>,
    /// Wall-clock time when this step should fire.
    timestamp: Instant,
}

impl<B: Backend> PatternsHandler<B> {
    /// Create a new patterns handler.
    pub fn new(state: Arc<RwLock<State>>, voices: Arc<VoicesHandler<B>>) -> Self {
        Self { state, voices }
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

                    // Calculate lookahead position (where we're scheduling up to)
                    let lookahead_pos = (current_beat + lookahead_beats) % length;

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
                        // We need to figure out how far ahead (in beats) the step is from current_beat
                        let current_pos_in_loop = current_beat % length;
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

                        tracing::debug!(
                            "Pattern step scheduled: voice={:?}, note={:?}, offset={:.3}ms",
                            voice_id,
                            note,
                            offset_secs * 1000.0
                        );

                        triggers.push(StepTrigger {
                            voice_id,
                            params,
                            note,
                            timestamp,
                        });
                    }
                }
            }

            triggers
        };

        // Trigger through voice handler (lock released)
        if !triggers.is_empty() {
            tracing::debug!("Patterns: triggering {} steps through voice handler", triggers.len());
        }
        for trigger in triggers {
            // If step has a note parameter and voice has MIDI output, use note_on_at
            // Otherwise, use trigger for audio synths
            #[cfg(feature = "midi")]
            if let Some(note) = trigger.note {
                let velocity = trigger.params.get("amp").copied().unwrap_or(1.0);
                let _ = self
                    .voices
                    .note_on_at(trigger.voice_id, note, velocity, Some(trigger.timestamp))
                    .await;
            } else {
                // Audio synth trigger (no MIDI)
                let _ = self.voices.trigger(trigger.voice_id, &trigger.params).await;
            }

            #[cfg(not(feature = "midi"))]
            {
                let _ = self.voices.trigger(trigger.voice_id, &trigger.params).await;
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Patterns for PatternsHandler<B> {
    async fn create(&self, id: PatternId, config: PatternConfig) -> Result<()> {
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

        state.patterns.insert(id, PatternState::new(id, config));

        Ok(())
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

        let pattern = state
            .patterns
            .get_mut(&id)
            .ok_or(Error::PatternNotFound(id))?;

        pattern.playing = true;
        pattern.loop_position = Beat::ZERO;

        Ok(())
    }

    async fn stop(&self, id: PatternId) -> Result<()> {
        let mut state = self.state.write().await;

        let pattern = state
            .patterns
            .get_mut(&id)
            .ok_or(Error::PatternNotFound(id))?;

        pattern.playing = false;

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
}
