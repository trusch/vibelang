//! Patterns handler implementation.

use crate::backend::{AddAction, Backend};
use crate::compat::RwLock;
use crate::state::{PatternState, State};
use crate::traits::{PatternConfig, Patterns, Step};
use crate::types::{Beat, NodeId, ParamMap, PatternId, VoiceId};
use crate::validation::Validate;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// Handler for pattern operations.
pub struct PatternsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

/// Info about a step that needs to be triggered.
struct StepTrigger {
    voice_id: VoiceId,
    synthdef: String,
    group_node_id: NodeId,
    node_id: NodeId,
    params: ParamMap,
    polyphony: usize,
    /// Nodes to choke (stop) before creating this synth.
    choke_nodes: Vec<NodeId>,
}

impl<B: Backend> PatternsHandler<B> {
    /// Create a new patterns handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }

    /// Process patterns for the current beat.
    ///
    /// Called by the runtime's tick loop to trigger pattern events.
    pub async fn tick(&self, current_beat: Beat) {
        // Collect triggers while holding lock
        let triggers = {
            let mut state = self.state.write().await;
            let mut triggers = Vec::new();

            // Get IDs of playing patterns
            let pattern_ids: Vec<PatternId> = state
                .patterns
                .iter()
                .filter(|(_, p)| p.playing)
                .map(|(id, _)| *id)
                .collect();

            if !pattern_ids.is_empty() {
                tracing::trace!(
                    "Patterns tick at beat {:?}: {} playing patterns",
                    current_beat,
                    pattern_ids.len()
                );
            }

            for pattern_id in pattern_ids {
                if let Some(pattern) = state.patterns.get_mut(&pattern_id) {
                    if !pattern.playing || pattern.config.length == Beat::ZERO {
                        continue;
                    }

                    let length = pattern.config.length;
                    let last_pos = pattern.loop_position;

                    // Calculate new position (wrapped to pattern length)
                    let new_pos = current_beat % length;

                    // Find steps that should trigger
                    // Handle wrap-around case (when new_pos < last_pos)
                    // Note: last_pos is where we were (already triggered), new_pos is where we are now
                    let steps_to_trigger: Vec<Step> = if new_pos < last_pos {
                        // Wrapped around - trigger steps AFTER last_pos to end, and 0 to new_pos (inclusive)
                        pattern
                            .config
                            .steps
                            .iter()
                            .filter(|s| s.beat > last_pos || s.beat <= new_pos)
                            .cloned()
                            .collect()
                    } else if last_pos == new_pos {
                        // First tick case: trigger steps exactly at this position
                        pattern
                            .config
                            .steps
                            .iter()
                            .filter(|s| s.beat == new_pos)
                            .cloned()
                            .collect()
                    } else {
                        // Normal case - trigger steps between last_pos (exclusive) and new_pos (inclusive)
                        // Exclusive start: last_pos was already triggered on previous tick
                        // Inclusive end: new_pos is where we are now
                        pattern
                            .config
                            .steps
                            .iter()
                            .filter(|s| s.beat > last_pos && s.beat <= new_pos)
                            .cloned()
                            .collect()
                    };

                    // Update loop position
                    pattern.loop_position = new_pos;

                    // Get voice info for triggering (clone data to avoid borrow conflicts)
                    let voice_id = match pattern.config.voice {
                        Some(id) => id,
                        None => continue, // Skip patterns without a voice
                    };
                    let voice_info = state.voices.get(&voice_id).map(|v| {
                        (
                            v.config.synthdef.clone(),
                            v.config.params.clone(),
                            v.config.polyphony as usize,
                            v.config.group,
                            v.config.round_robin_count,
                            v.config.choke_group.clone(),
                        )
                    });

                    if let Some((
                        synthdef,
                        base_params,
                        polyphony,
                        group_id,
                        round_robin_count,
                        choke_group,
                    )) = voice_info
                    {
                        let group_info = state
                            .groups
                            .get(&group_id)
                            .map(|g| (g.node_id, g.audio_bus));

                        if let Some((group_node_id, audio_bus)) = group_info {
                            tracing::debug!(
                                "Pattern triggering: group_node_id={}, audio_bus={}, synthdef={}",
                                group_node_id.0,
                                audio_bus.0,
                                synthdef
                            );
                            for step in steps_to_trigger {
                                // Merge voice params with step params
                                let mut params = base_params.clone();

                                // Multiply voice amp by step velocity (don't overwrite voice's amp)
                                // This matches melodies.rs behavior
                                let voice_amp = base_params.get("amp").copied().unwrap_or(1.0);
                                let step_velocity = step.params.get("amp").copied().unwrap_or(1.0);
                                let final_amp = voice_amp * step_velocity;

                                // Extend with step params but then set the correct amp
                                params.extend(step.params);
                                params.insert("amp".to_string(), final_amp);

                                // Set output bus to group's audio bus (for proper routing)
                                params.insert("out".to_string(), audio_bus.0 as f32);
                                tracing::debug!("  Step params: {:?}", params);

                                // Handle round-robin: add `rr` parameter and increment position
                                if round_robin_count > 0 {
                                    if let Some(voice) = state.voices.get_mut(&voice_id) {
                                        params.insert(
                                            "rr".to_string(),
                                            voice.round_robin_position as f32,
                                        );
                                        voice.round_robin_position =
                                            (voice.round_robin_position + 1) % round_robin_count;
                                    }
                                }

                                // Handle choke groups: collect nodes to choke
                                let mut choke_nodes = Vec::new();
                                if let Some(ref choke) = choke_group {
                                    for (other_voice_id, other_voice) in state.voices.iter_mut() {
                                        // Skip the voice being triggered
                                        if *other_voice_id == voice_id {
                                            continue;
                                        }
                                        // Check if in same choke group
                                        if other_voice.config.choke_group.as_ref() == Some(choke) {
                                            // Collect all active nodes to choke
                                            choke_nodes.append(&mut other_voice.active_nodes);
                                            choke_nodes.extend(
                                                other_voice.note_nodes.drain().map(|(_, n)| n),
                                            );
                                        }
                                    }
                                }

                                let node_id = state.alloc_node_id();

                                triggers.push(StepTrigger {
                                    voice_id,
                                    synthdef: synthdef.clone(),
                                    group_node_id,
                                    node_id,
                                    params,
                                    polyphony,
                                    choke_nodes,
                                });

                                // Track the new node in voice state
                                if let Some(voice) = state.voices.get_mut(&voice_id) {
                                    voice.active_nodes.push(node_id);
                                }
                            }
                        }
                    }
                }
            }

            triggers
        };

        // Send triggers to backend (lock released)
        if !triggers.is_empty() {
            tracing::debug!("Patterns: triggering {} synths", triggers.len());
        }
        for trigger in triggers {
            // Choke nodes from other voices in the same choke group
            for choke_node in trigger.choke_nodes {
                let _ = self.backend.free_node(choke_node).await;
            }

            // Use Head so voices execute before effects/link synths (which are at tail)
            let _ = self
                .backend
                .create_synth(
                    &trigger.synthdef,
                    trigger.node_id,
                    trigger.group_node_id,
                    AddAction::Head,
                    &trigger.params,
                )
                .await;

            // Clean up old nodes if over polyphony limit
            self.cleanup_voice_nodes(trigger.voice_id, trigger.polyphony)
                .await;
        }
    }

    /// Clean up excess nodes for a voice based on polyphony limit.
    async fn cleanup_voice_nodes(&self, voice_id: VoiceId, polyphony: usize) {
        let nodes_to_free = {
            let mut state = self.state.write().await;
            let mut to_free = Vec::new();

            if let Some(voice) = state.voices.get_mut(&voice_id) {
                while voice.active_nodes.len() > polyphony {
                    if let Some(old_node) = voice.active_nodes.first().copied() {
                        voice.active_nodes.remove(0);
                        to_free.push(old_node);
                    }
                }
            }

            to_free
        };

        for node_id in nodes_to_free {
            let _ = self.backend.free_node(node_id).await;
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

        state.patterns.insert(
            id,
            PatternState {
                id,
                config,
                playing: false,
                loop_position: Beat::ZERO,
            },
        );

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

        pattern.config.steps.iter_mut().for_each(|step| {
            step.params.insert(param.to_string(), value);
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ParamMap;

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
        // Step at 0.0, tick from 0.0 to 0.25
        // The step at 0.0 was already triggered when the tick reached 0.0, so it should NOT trigger again
        let steps = vec![step_at(0.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            0,
            "Step at last_pos should NOT trigger (exclusive start)"
        );
    }

    #[test]
    fn test_normal_tick_triggers_step_at_new_pos() {
        // Step at 0.25, tick from 0.0 to 0.25
        // The step at new_pos SHOULD trigger (inclusive end)
        let steps = vec![step_at(0.25)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            1,
            "Step at new_pos SHOULD trigger (inclusive end)"
        );
    }

    #[test]
    fn test_normal_tick_triggers_step_between_positions() {
        // Step at 0.125, tick from 0.0 to 0.25
        let steps = vec![step_at(0.125)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1, "Step between positions should trigger");
    }

    // =========================================================================
    // First Tick Tests (last_pos == new_pos)
    // =========================================================================

    #[test]
    fn test_first_tick_triggers_step_at_zero() {
        // First tick: last_pos == new_pos == 0
        let steps = vec![step_at(0.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.0),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1, "Step at 0 should trigger on first tick");
    }

    #[test]
    fn test_first_tick_only_triggers_exact_match() {
        // First tick at 0.5, steps at 0.0, 0.5, 1.0
        let steps = vec![step_at(0.0), step_at(0.5), step_at(1.0)];
        let triggered = filter_steps(
            &steps,
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
    fn test_wrap_triggers_step_at_zero() {
        // Pattern loops: last_pos = 3.75, new_pos = 0.25
        // Step at 0.0 should trigger
        let steps = vec![step_at(0.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 1, "Step at 0 should trigger after wrap");
    }

    #[test]
    fn test_wrap_triggers_step_near_end() {
        // Pattern loops: last_pos = 3.75, new_pos = 0.25
        // Step at 3.875 should trigger (between 3.75 and end)
        let steps = vec![step_at(3.875)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            1,
            "Step near end should trigger during wrap"
        );
    }

    #[test]
    fn test_wrap_triggers_step_at_new_pos() {
        // Pattern loops: last_pos = 3.75, new_pos = 0.25
        // Step at 0.25 should trigger (inclusive during wrap)
        let steps = vec![step_at(0.25)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            1,
            "Step at new_pos should trigger during wrap"
        );
    }

    #[test]
    fn test_wrap_does_not_double_trigger() {
        // Pattern loops: last_pos = 3.75, new_pos = 0.25
        // Step at 0.5 should NOT trigger (it's past new_pos)
        let steps = vec![step_at(0.5)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(3.75),
            Beat::from_f64(0.25),
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            0,
            "Step past new_pos should NOT trigger during wrap"
        );
    }

    #[test]
    fn test_wrap_multiple_steps() {
        // Pattern loops: last_pos = 3.5, new_pos = 0.5
        // Steps at 3.75, 0.0, 0.25 should all trigger
        // Step at 1.0 should NOT trigger
        let steps = vec![step_at(3.75), step_at(0.0), step_at(0.25), step_at(1.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(3.5),
            Beat::from_f64(0.5),
            Beat::from_f64(4.0),
        );
        assert_eq!(triggered.len(), 3, "3 steps should trigger during wrap");
    }

    // =========================================================================
    // Edge Case: Step at Exact Loop Boundary
    // =========================================================================

    #[test]
    fn test_step_at_exact_length_never_triggers() {
        // Step at beat 4.0 in a 4-beat pattern should never trigger
        // because positions are always 0 <= pos < length (after modulo)
        // In real operation, 4.0 % 4.0 = 0.0, so we test with the wrapped value
        let steps = vec![step_at(4.0)];

        // Tick that wraps from 3.75 to 0.0 (4.0 % 4.0 = 0.0)
        // This is a wrap-around case since new_pos (0.0) < last_pos (3.75)
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(3.75),
            Beat::from_f64(0.0), // Wrapped from 4.0 % 4.0
            Beat::from_f64(4.0),
        );
        // Step at 4.0 won't match any condition:
        // - 4.0 > 3.75 = true, but the OR part (4.0 <= 0.0) = false → overall false for the wrap case
        assert_eq!(triggered.len(), 0, "Step at length boundary is unreachable");
    }

    #[test]
    fn test_step_at_last_beat_triggers_before_wrap() {
        // Step at 3.99, tick from 3.75 to 4.0 (which wraps to 0.0)
        // This should trigger because 3.99 > 3.75 (exclusive start, inclusive end)
        let steps = vec![step_at(3.99)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(3.75),
            Beat::from_f64(0.0), // Wrapped
            Beat::from_f64(4.0),
        );
        assert_eq!(
            triggered.len(),
            1,
            "Step near end should trigger before wrap"
        );
    }

    // =========================================================================
    // Short Pattern Tests
    // =========================================================================

    #[test]
    fn test_very_short_pattern_half_beat() {
        // 0.5 beat pattern with step at 0.0
        // If last_pos == new_pos (first tick), step at 0.0 would trigger
        // But this test has last_pos=0.0, new_pos=0.25 (second tick)
        // Step at 0.0 was already triggered, so it should NOT trigger again
        let steps = vec![step_at(0.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.25),
            Beat::from_f64(0.5),
        );
        // Step at 0.0 was triggered on first tick (when last_pos == new_pos == 0)
        // On this tick (0.0 to 0.25), it should NOT trigger again
        assert_eq!(triggered.len(), 0);
    }

    #[test]
    fn test_very_short_pattern_first_tick() {
        // 0.5 beat pattern with step at 0.0 - FIRST TICK
        let steps = vec![step_at(0.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(0.0), // First tick: last_pos == new_pos
            Beat::from_f64(0.5),
        );
        // Step at 0.0 SHOULD trigger on first tick
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_very_short_pattern_wrap() {
        // 0.5 beat pattern, tick from 0.4 to 0.1 (wraps)
        let steps = vec![step_at(0.0), step_at(0.25)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.4),
            Beat::from_f64(0.1),
            Beat::from_f64(0.5),
        );
        // Should trigger 0.0 (after wrap) but not 0.25 (before last_pos)
        assert_eq!(triggered.len(), 1);
        assert!((triggered[0].to_f64() - 0.0).abs() < 0.01);
    }

    // =========================================================================
    // Multiple Loops Per Tick (High Tempo Edge Case)
    // =========================================================================

    #[test]
    fn test_tick_larger_than_pattern_length() {
        // Tick spans more than one pattern length
        // This shouldn't happen in normal operation, but test the behavior
        // last_pos = 0.0, new_pos = 1.0 (already wrapped via modulo)
        // With (exclusive, inclusive] convention:
        // - 0.0 > 0.0 = false → NOT triggered
        // - 0.5 > 0.0 && 0.5 <= 1.0 = true → triggered
        // - 2.0 > 0.0 && 2.0 <= 1.0 = false → NOT triggered
        let steps = vec![step_at(0.0), step_at(0.5), step_at(2.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.0),
            Beat::from_f64(1.0),
            Beat::from_f64(4.0),
        );
        // Only step at 0.5 triggers (step at 0.0 was already triggered on previous tick)
        assert_eq!(triggered.len(), 1);
    }

    // =========================================================================
    // Step Ordering Tests
    // =========================================================================

    #[test]
    fn test_unordered_steps_still_work() {
        // Steps not in order
        let steps = vec![step_at(2.0), step_at(0.5), step_at(1.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.25),
            Beat::from_f64(1.5),
            Beat::from_f64(4.0),
        );
        // Should trigger 0.5 and 1.0
        assert_eq!(triggered.len(), 2);
    }

    // =========================================================================
    // Boundary Precision Tests
    // =========================================================================

    #[test]
    fn test_step_exactly_at_boundary_normal() {
        // Test the exact boundary behavior
        // Step at 1.0, tick from 0.5 to 1.0
        // With (exclusive, inclusive] convention, step at new_pos SHOULD trigger
        let steps = vec![step_at(1.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(0.5),
            Beat::from_f64(1.0),
            Beat::from_f64(4.0),
        );
        // new_pos is INCLUSIVE - step at exactly 1.0 triggers when we reach 1.0
        assert_eq!(
            triggered.len(),
            1,
            "Step at new_pos should trigger (inclusive end)"
        );
    }

    #[test]
    fn test_step_not_triggered_again_next_tick() {
        // The step at 1.0 should NOT trigger on the next tick (already triggered)
        let steps = vec![step_at(1.0)];
        let triggered = filter_steps(
            &steps,
            Beat::from_f64(1.0),
            Beat::from_f64(1.25),
            Beat::from_f64(4.0),
        );
        // last_pos is EXCLUSIVE - step at 1.0 was already triggered, should NOT trigger again
        assert_eq!(
            triggered.len(),
            0,
            "Step at last_pos should NOT trigger again (exclusive start)"
        );
    }
}
