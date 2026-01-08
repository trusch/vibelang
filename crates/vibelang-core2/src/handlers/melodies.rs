//! Melodies handler implementation.

use crate::backend::{AddAction, Backend};
use crate::state::{MelodyState, State};
use crate::traits::{Melodies, MelodyConfig, NoteEvent};
use crate::types::{Beat, MelodyId, NodeId, ParamMap, VoiceId};
use crate::validation::Validate;
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Type alias for tracking pending note-offs: (melody_id, note) -> (off_beat, node_id)
type PendingNoteOffs = HashMap<(MelodyId, u8), (Beat, NodeId)>;

/// Handler for melody operations.
pub struct MelodiesHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Tracks pending note-offs for releasing notes at the correct beat.
    pending_note_offs: Arc<RwLock<PendingNoteOffs>>,
}

/// Info about a note that needs to be triggered.
struct NoteTrigger {
    voice_id: VoiceId,
    note: u8,
    synthdef: String,
    group_node_id: NodeId,
    node_id: NodeId,
    params: ParamMap,
    off_beat: Beat,
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
                    let notes_to_trigger: Vec<NoteEvent> = if new_pos < last_pos {
                        // Wrapped around - trigger notes from last_pos to end and 0 to new_pos
                        melody
                            .config
                            .notes
                            .iter()
                            .filter(|n| n.beat >= last_pos || n.beat < new_pos)
                            .cloned()
                            .collect()
                    } else {
                        // Normal case - trigger notes between last_pos and new_pos
                        melody
                            .config
                            .notes
                            .iter()
                            .filter(|n| n.beat >= last_pos && n.beat < new_pos)
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
                        (
                            v.config.synthdef.clone(),
                            v.config.params.clone(),
                            v.config.group,
                        )
                    });

                    if let Some((synthdef, base_params, group_id)) = voice_info {
                        let group_node_id = state.groups.get(&group_id).map(|g| g.node_id);

                        if let Some(group_node_id) = group_node_id {
                            for note_event in notes_to_trigger {
                                // Build params with note info
                                let mut params = base_params.clone();
                                params.insert("freq".to_string(), midi_to_freq(note_event.note));
                                params.insert("amp".to_string(), note_event.velocity);
                                params.insert("gate".to_string(), 1.0);

                                let node_id = state.alloc_node_id();

                                // Calculate when the note should end
                                let off_beat = current_beat + note_event.duration;

                                triggers.push(NoteTrigger {
                                    voice_id,
                                    note: note_event.note,
                                    synthdef: synthdef.clone(),
                                    group_node_id,
                                    node_id,
                                    params,
                                    off_beat,
                                });

                                // Track the new node in voice state
                                if let Some(voice) = state.voices.get_mut(&voice_id) {
                                    // If note already playing, we'll replace it
                                    if let Some(old_node) = voice.note_nodes.remove(&note_event.note) {
                                        // Will be freed when we send the new note
                                        voice.active_nodes.retain(|n| *n != old_node);
                                    }
                                    voice.note_nodes.insert(note_event.note, node_id);
                                }
                            }
                        }
                    }
                }
            }

            triggers
        };

        // Send triggers to backend and schedule note-offs (lock released)
        for trigger in triggers {
            let _ = self
                .backend
                .create_synth(
                    &trigger.synthdef,
                    trigger.node_id,
                    trigger.group_node_id,
                    AddAction::Tail,
                    &trigger.params,
                )
                .await;

            // Schedule the note-off
            // For melodies, we use a unique key based on voice and note
            let key = (
                MelodyId::new(trigger.voice_id.0), // Reuse melody ID space for tracking
                trigger.note,
            );
            let mut note_offs = self.pending_note_offs.write().await;
            note_offs.insert(key, (trigger.off_beat, trigger.node_id));
        }
    }

    /// Process pending note-offs that are due.
    async fn process_note_offs(&self, current_beat: Beat) {
        let notes_to_off: Vec<NodeId> = {
            let mut note_offs = self.pending_note_offs.write().await;
            let mut to_remove = Vec::new();
            let mut nodes = Vec::new();

            for (key, (off_beat, node_id)) in note_offs.iter() {
                if *off_beat <= current_beat {
                    to_remove.push(*key);
                    nodes.push(*node_id);
                }
            }

            for key in to_remove {
                note_offs.remove(&key);
            }

            nodes
        };

        // Send gate=0 to release notes
        for node_id in notes_to_off {
            let _ = self.backend.set_param(node_id, "gate", 0.0).await;
        }
    }
}

#[async_trait]
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
        let mut state = self.state.write().await;

        let melody = state
            .melodies
            .get_mut(&id)
            .ok_or(Error::MelodyNotFound(id))?;

        melody.playing = false;

        Ok(())
    }
}
