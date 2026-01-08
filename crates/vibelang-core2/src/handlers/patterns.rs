//! Patterns handler implementation.

use crate::backend::{AddAction, Backend};
use crate::state::{PatternState, State};
use crate::traits::{PatternConfig, Patterns, Step};
use crate::types::{Beat, NodeId, ParamMap, PatternId, VoiceId};
use crate::validation::Validate;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

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
                    let steps_to_trigger: Vec<Step> = if new_pos < last_pos {
                        // Wrapped around - trigger steps from last_pos to end and 0 to new_pos
                        pattern
                            .config
                            .steps
                            .iter()
                            .filter(|s| s.beat >= last_pos || s.beat < new_pos)
                            .cloned()
                            .collect()
                    } else {
                        // Normal case - trigger steps between last_pos and new_pos
                        pattern
                            .config
                            .steps
                            .iter()
                            .filter(|s| s.beat >= last_pos && s.beat < new_pos)
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
                        let group_node_id = state.groups.get(&group_id).map(|g| g.node_id);

                        if let Some(group_node_id) = group_node_id {
                            for step in steps_to_trigger {
                                // Merge voice params with step params
                                let mut params = base_params.clone();
                                params.extend(step.params);

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
                                            choke_nodes.extend(other_voice.active_nodes.drain(..));
                                            choke_nodes
                                                .extend(other_voice.note_nodes.drain().map(|(_, n)| n));
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
        for trigger in triggers {
            // Choke nodes from other voices in the same choke group
            for choke_node in trigger.choke_nodes {
                let _ = self.backend.free_node(choke_node).await;
            }

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

#[async_trait]
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
