//! Melodies handler implementation.

use crate::backend::{AddAction, Backend};
use crate::compat::RwLock;
use crate::state::{MelodyState, State};
use crate::traits::{Melodies, MelodyConfig, NoteEvent};
use crate::types::{Beat, MelodyId, NodeId, ParamMap, VoiceId};
use crate::validation::Validate;
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Tracks pending note-off: when it should fire
type PendingNoteOffs = HashMap<NodeId, Beat>;

/// Tracks which nodes belong to which melody (for cleanup on stop/delete)
type MelodyNodes = HashMap<MelodyId, Vec<NodeId>>;

/// Handler for melody operations.
pub struct MelodiesHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Tracks pending note-offs for releasing notes at the correct beat.
    pending_note_offs: Arc<RwLock<PendingNoteOffs>>,
    /// Tracks which nodes belong to which melody (for cleanup).
    melody_nodes: Arc<RwLock<MelodyNodes>>,
}

/// Info about a note that needs to be triggered.
struct NoteTrigger {
    melody_id: MelodyId,
    // Voice ID kept for debugging/logging purposes, may be used in future error messages
    #[allow(dead_code)]
    voice_id: VoiceId,
    note: u8,
    synthdef: String,
    group_node_id: NodeId,
    node_id: NodeId,
    /// Old node that this trigger replaces (if re-triggering same note)
    old_node_id: Option<NodeId>,
    params: ParamMap,
    off_beat: Beat,
    /// Modulations to apply after synth creation (param_name, control_bus)
    modulations: Vec<(String, u32)>,
}

/// Convert MIDI note number to frequency in Hz.
fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}

impl<B: Backend> MelodiesHandler<B> {
    /// Create a new melodies handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self {
            backend,
            state,
            pending_note_offs: Arc::new(RwLock::new(HashMap::new())),
            melody_nodes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Process melodies for the current beat.
    ///
    /// Called by the runtime's tick loop to trigger note events.
    pub async fn tick(&self, current_beat: Beat) {
        // First, process note-offs that are due
        self.process_note_offs(current_beat).await;

        // Collect note-on triggers while holding lock
        let triggers = {
            let mut state = self.state.write().await;
            let mut triggers = Vec::new();

            // Get IDs of playing melodies
            let melody_ids: Vec<MelodyId> = state
                .melodies
                .iter()
                .filter(|(_, m)| m.playing)
                .map(|(id, _)| *id)
                .collect();

            for melody_id in melody_ids {
                if let Some(melody) = state.melodies.get_mut(&melody_id) {
                    if !melody.playing || melody.config.length == Beat::ZERO {
                        continue;
                    }

                    let length = melody.config.length;
                    let last_pos = melody.loop_position;

                    // Calculate new position (wrapped to melody length)
                    let new_pos = current_beat % length;

                    // Find notes that should trigger
                    // Handle wrap-around case (when new_pos < last_pos)
                    // Also handle first tick case where last_pos == new_pos
                    let notes_to_trigger: Vec<NoteEvent> = if new_pos < last_pos {
                        // Wrapped around - trigger notes from last_pos to end and 0 to new_pos
                        melody
                            .config
                            .notes
                            .iter()
                            .filter(|n| n.beat >= last_pos || n.beat <= new_pos)
                            .cloned()
                            .collect()
                    } else if last_pos == new_pos {
                        // First tick case: trigger notes exactly at this position
                        melody
                            .config
                            .notes
                            .iter()
                            .filter(|n| n.beat == new_pos)
                            .cloned()
                            .collect()
                    } else {
                        // Normal case - trigger notes between last_pos (exclusive) and new_pos (inclusive)
                        // Use exclusive start to avoid re-triggering notes that were just played
                        melody
                            .config
                            .notes
                            .iter()
                            .filter(|n| n.beat > last_pos && n.beat <= new_pos)
                            .cloned()
                            .collect()
                    };

                    // Update loop position
                    melody.loop_position = new_pos;

                    // Get voice info for triggering (clone data to avoid borrow conflicts)
                    let voice_id = match melody.config.voice {
                        Some(id) => id,
                        None => continue, // Skip melodies without a voice
                    };
                    let voice_info = state.voices.get(&voice_id).map(|v| {
                        // Collect modulations for this voice
                        let mut modulations = Vec::new();
                        for (param_name, modulator_id) in &v.config.modulations {
                            if let Some(modulator_state) = state.modulators.get(modulator_id) {
                                modulations
                                    .push((param_name.clone(), modulator_state.control_bus.raw()));
                            } else {
                                tracing::warn!(
                                    "Modulator {:?} not found for param '{}' on voice {:?}",
                                    modulator_id,
                                    param_name,
                                    voice_id
                                );
                            }
                        }
                        (
                            v.config.synthdef.clone(),
                            v.config.params.clone(),
                            v.config.group,
                            modulations,
                        )
                    });

                    if let Some((synthdef, base_params, group_id, modulations)) = voice_info {
                        let group_info = state
                            .groups
                            .get(&group_id)
                            .map(|g| (g.node_id, g.audio_bus));

                        if let Some((group_node_id, audio_bus)) = group_info {
                            for note_event in notes_to_trigger {
                                // Build params with note info
                                let mut params = base_params.clone();
                                params.insert("freq".to_string(), midi_to_freq(note_event.note));

                                // Multiply voice amp by note velocity (don't overwrite voice's amp)
                                let voice_amp = base_params.get("amp").copied().unwrap_or(1.0);
                                let final_amp = voice_amp * note_event.velocity;
                                params.insert("amp".to_string(), final_amp);

                                tracing::debug!(
                                    "Melody note: synthdef={}, note={}, voice_amp={}, velocity={}, final_amp={}",
                                    synthdef, note_event.note, voice_amp, note_event.velocity, final_amp
                                );

                                params.insert("gate".to_string(), 1.0);

                                // Set output bus to group's audio bus (for proper routing)
                                params.insert("out".to_string(), audio_bus.0 as f32);

                                let node_id = state.alloc_node_id();

                                // Calculate when the note should end
                                let off_beat = current_beat + note_event.duration;

                                // Track the new node in voice state and capture old node if re-triggering
                                let old_node_id =
                                    if let Some(voice) = state.voices.get_mut(&voice_id) {
                                        // If note already playing, we'll replace it
                                        let old = voice.note_nodes.remove(&note_event.note);
                                        if let Some(old_node) = old {
                                            voice.active_nodes.retain(|n| *n != old_node);
                                        }
                                        voice.note_nodes.insert(note_event.note, node_id);
                                        old
                                    } else {
                                        None
                                    };

                                triggers.push(NoteTrigger {
                                    melody_id,
                                    voice_id,
                                    note: note_event.note,
                                    synthdef: synthdef.clone(),
                                    group_node_id,
                                    node_id,
                                    old_node_id,
                                    params,
                                    off_beat,
                                    modulations: modulations.clone(),
                                });
                            }
                        }
                    }
                }
            }

            triggers
        };

        // Send triggers to backend and schedule note-offs (lock released)
        for trigger in triggers {
            // If re-triggering a note, release the old synth immediately
            if let Some(old_node) = trigger.old_node_id {
                // Remove the old pending note-off (it's no longer needed)
                {
                    let mut note_offs = self.pending_note_offs.write().await;
                    note_offs.remove(&old_node);
                }
                // Send gate=0 to release the old synth
                tracing::debug!(
                    "Releasing old node {} (re-triggered note {})",
                    old_node.0,
                    trigger.note
                );
                let _ = self.backend.set_param(old_node, "gate", 0.0).await;
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

            // Apply modulations - map control buses to synth parameters
            for (param_name, control_bus) in &trigger.modulations {
                let _ = self
                    .backend
                    .map_param_to_bus(trigger.node_id, param_name, *control_bus)
                    .await;
            }

            // Schedule the note-off using node_id as key (each synth is unique)
            {
                let mut note_offs = self.pending_note_offs.write().await;
                tracing::debug!(
                    "Scheduling note-off: node={}, note={}, off_beat={}",
                    trigger.node_id.0,
                    trigger.note,
                    trigger.off_beat.to_f64()
                );
                note_offs.insert(trigger.node_id, trigger.off_beat);
            }

            // Track this node for the melody (for cleanup on stop/delete)
            {
                let mut melody_nodes = self.melody_nodes.write().await;
                melody_nodes
                    .entry(trigger.melody_id)
                    .or_default()
                    .push(trigger.node_id);
            }
        }
    }

    /// Process pending note-offs that are due.
    async fn process_note_offs(&self, current_beat: Beat) {
        let notes_to_off: Vec<NodeId> = {
            let mut note_offs = self.pending_note_offs.write().await;
            let mut to_remove = Vec::new();

            if !note_offs.is_empty() {
                tracing::debug!(
                    "Checking {} pending note-offs at beat {}",
                    note_offs.len(),
                    current_beat.to_f64()
                );
            }

            for (&node_id, &off_beat) in note_offs.iter() {
                if off_beat <= current_beat {
                    tracing::debug!(
                        "Note-off due: node={}, off_beat={}",
                        node_id.0,
                        off_beat.to_f64()
                    );
                    to_remove.push(node_id);
                }
            }

            for node_id in &to_remove {
                note_offs.remove(node_id);
            }

            to_remove
        };

        // Send gate=0 to release notes and clean up melody_nodes tracking
        if !notes_to_off.is_empty() {
            let mut melody_nodes = self.melody_nodes.write().await;
            for node_id in &notes_to_off {
                // Remove from all melody node lists
                for nodes in melody_nodes.values_mut() {
                    nodes.retain(|n| n != node_id);
                }
            }
        }

        for node_id in notes_to_off {
            tracing::debug!(
                "Sending note-off: node={}, current_beat={}",
                node_id.0,
                current_beat.to_f64()
            );
            let _ = self.backend.set_param(node_id, "gate", 0.0).await;
        }
    }

    /// Release all active notes for a melody and clear its pending note-offs.
    async fn release_melody_notes(&self, melody_id: MelodyId) {
        // Get all nodes for this melody
        let nodes_to_release: Vec<NodeId> = {
            let mut melody_nodes = self.melody_nodes.write().await;
            melody_nodes.remove(&melody_id).unwrap_or_default()
        };

        if nodes_to_release.is_empty() {
            return;
        }

        tracing::debug!(
            "Releasing {} notes for melody {:?}",
            nodes_to_release.len(),
            melody_id
        );

        // Remove pending note-offs for these nodes
        {
            let mut note_offs = self.pending_note_offs.write().await;
            for node_id in &nodes_to_release {
                note_offs.remove(node_id);
            }
        }

        // Send gate=0 to release all notes
        for node_id in nodes_to_release {
            let _ = self.backend.set_param(node_id, "gate", 0.0).await;
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

        state.melodies.insert(
            id,
            MelodyState {
                id,
                config,
                playing: false,
                loop_position: Beat::ZERO,
            },
        );

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
        // Release any active notes for this melody
        self.release_melody_notes(id).await;

        let mut state = self.state.write().await;

        let melody = state
            .melodies
            .get_mut(&id)
            .ok_or(Error::MelodyNotFound(id))?;

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
}
