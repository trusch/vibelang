//! Voices handler implementation.

use crate::backend::{AddAction, Backend};
use crate::state::{State, VoiceState};
use crate::traits::{VoiceConfig, Voices};
use crate::types::{NodeId, ParamMap, VoiceId};
use crate::validation::Validate;
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handler for voice operations.
pub struct VoicesHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

impl<B: Backend> VoicesHandler<B> {
    /// Create a new voices handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }
}

#[async_trait]
impl<B: Backend> Voices for VoicesHandler<B> {
    async fn create(&self, id: VoiceId, config: VoiceConfig) -> Result<()> {
        // Validate configuration before acquiring lock
        config.validate()?;

        let mut state = self.state.write().await;

        if state.voices.contains_key(&id) {
            return Err(Error::VoiceExists(id));
        }

        // Verify the group exists
        if !state.groups.contains_key(&config.group) {
            return Err(Error::GroupNotFound(config.group));
        }

        // Store state
        state.voices.insert(
            id,
            VoiceState {
                id,
                config,
                active_nodes: Vec::new(),
                note_nodes: HashMap::new(),
                round_robin_position: 0,
            },
        );

        Ok(())
    }

    async fn delete(&self, id: VoiceId) -> Result<()> {
        let nodes_to_free = {
            let mut state = self.state.write().await;
            let voice = state.voices.remove(&id).ok_or(Error::VoiceNotFound(id))?;
            voice.active_nodes
        };

        // Free all active synth nodes (lock released)
        for node_id in nodes_to_free {
            let _ = self.backend.free_node(node_id).await;
        }

        Ok(())
    }

    async fn trigger(&self, id: VoiceId, params: &ParamMap) -> Result<()> {
        // Gather info and allocate node while holding lock
        let (node_id, group_node_id, synthdef, merged_params, old_nodes, choke_nodes) = {
            let mut state = self.state.write().await;

            let voice = state
                .voices
                .get(&id)
                .ok_or(Error::VoiceNotFound(id))?;

            let group = state
                .groups
                .get(&voice.config.group)
                .ok_or(Error::GroupNotFound(voice.config.group))?;

            // Merge default params with trigger params
            let mut merged_params = voice.config.params.clone();
            merged_params.extend(params.clone());

            // Set output bus to group's audio bus (for proper routing)
            merged_params.insert("out".to_string(), group.audio_bus.0 as f32);

            let synthdef = voice.config.synthdef.clone();
            let group_node_id = group.node_id;
            let polyphony = voice.config.polyphony as usize;
            let round_robin_count = voice.config.round_robin_count;
            let choke_group = voice.config.choke_group.clone();

            let node_id = state.alloc_node_id();

            // Handle choke groups: collect nodes to choke from other voices in same group
            let mut choke_nodes = Vec::new();
            if let Some(ref choke) = choke_group {
                for (voice_id, other_voice) in state.voices.iter_mut() {
                    // Skip the voice being triggered
                    if *voice_id == id {
                        continue;
                    }
                    // Check if in same choke group
                    if other_voice.config.choke_group.as_ref() == Some(choke) {
                        // Collect all active nodes to choke
                        choke_nodes.extend(other_voice.active_nodes.drain(..));
                        choke_nodes.extend(other_voice.note_nodes.drain().map(|(_, n)| n));
                    }
                }
            }

            // Update voice state
            let voice = state.voices.get_mut(&id).unwrap();
            voice.active_nodes.push(node_id);

            // Handle round-robin: add `rr` parameter and increment position
            if round_robin_count > 0 {
                merged_params.insert("rr".to_string(), voice.round_robin_position as f32);
                voice.round_robin_position = (voice.round_robin_position + 1) % round_robin_count;
            }

            // Collect nodes to free (polyphony management)
            let mut old_nodes = Vec::new();
            while voice.active_nodes.len() > polyphony {
                if let Some(old_node) = voice.active_nodes.first().copied() {
                    voice.active_nodes.remove(0);
                    old_nodes.push(old_node);
                }
            }

            (node_id, group_node_id, synthdef, merged_params, old_nodes, choke_nodes)
        };

        // Choke nodes from other voices in the same choke group (lock released)
        for choke_node in choke_nodes {
            let _ = self.backend.free_node(choke_node).await;
        }

        // Create synth in backend
        self.backend
            .create_synth(
                &synthdef,
                node_id,
                group_node_id,
                AddAction::Tail,
                &merged_params,
            )
            .await
            .map_err(Error::backend)?;

        // Free old nodes (polyphony limit)
        for old_node in old_nodes {
            let _ = self.backend.free_node(old_node).await;
        }

        Ok(())
    }

    async fn stop(&self, id: VoiceId) -> Result<()> {
        let nodes_to_free = {
            let mut state = self.state.write().await;

            let voice = state
                .voices
                .get_mut(&id)
                .ok_or(Error::VoiceNotFound(id))?;

            let nodes: Vec<NodeId> = voice.active_nodes.drain(..).collect();
            voice.note_nodes.clear();
            nodes
        };

        // Free all active synth nodes (lock released)
        for node_id in nodes_to_free {
            let _ = self.backend.free_node(node_id).await;
        }

        Ok(())
    }

    async fn note_on(&self, id: VoiceId, note: u8, velocity: f32) -> Result<()> {
        // Gather info and allocate node while holding lock
        let (node_id, group_node_id, synthdef, params, old_node) = {
            let mut state = self.state.write().await;

            let voice = state
                .voices
                .get(&id)
                .ok_or(Error::VoiceNotFound(id))?;

            let group = state
                .groups
                .get(&voice.config.group)
                .ok_or(Error::GroupNotFound(voice.config.group))?;

            let synthdef = voice.config.synthdef.clone();
            let group_node_id = group.node_id;

            // Build params with note info
            let mut params = voice.config.params.clone();
            params.insert("freq".to_string(), midi_to_freq(note));
            params.insert("amp".to_string(), velocity);
            params.insert("gate".to_string(), 1.0);

            // Set output bus to group's audio bus (for proper routing)
            params.insert("out".to_string(), group.audio_bus.0 as f32);

            let node_id = state.alloc_node_id();

            // Update voice state
            let voice = state.voices.get_mut(&id).unwrap();

            // If note already playing, collect it for cleanup
            let old_node = voice.note_nodes.remove(&note);

            // Track note -> node mapping
            voice.note_nodes.insert(note, node_id);

            (node_id, group_node_id, synthdef, params, old_node)
        };

        // Free old node if any (lock released)
        if let Some(old) = old_node {
            let _ = self.backend.free_node(old).await;
        }

        // Create synth
        self.backend
            .create_synth(
                &synthdef,
                node_id,
                group_node_id,
                AddAction::Tail,
                &params,
            )
            .await
            .map_err(Error::backend)?;

        Ok(())
    }

    async fn note_off(&self, id: VoiceId, note: u8) -> Result<()> {
        let node_to_release = {
            let mut state = self.state.write().await;

            let voice = state
                .voices
                .get_mut(&id)
                .ok_or(Error::VoiceNotFound(id))?;

            voice.note_nodes.remove(&note)
        };

        // Release the note (lock released)
        if let Some(node_id) = node_to_release {
            self.backend
                .set_param(node_id, "gate", 0.0)
                .await
                .map_err(Error::backend)?;
        }

        Ok(())
    }

    async fn mute(&self, id: VoiceId, muted: bool) -> Result<()> {
        let mut state = self.state.write().await;

        let voice = state
            .voices
            .get_mut(&id)
            .ok_or(Error::VoiceNotFound(id))?;

        voice.config.muted = muted;

        Ok(())
    }

    async fn set_param(&self, id: VoiceId, param: &str, value: f32) -> Result<()> {
        // Get all active nodes and update the default param value
        let nodes: Vec<NodeId> = {
            let mut state = self.state.write().await;

            let voice = state
                .voices
                .get_mut(&id)
                .ok_or(Error::VoiceNotFound(id))?;

            // Update the default param value for future triggers
            voice.config.params.insert(param.to_string(), value);

            // Collect all active nodes (both trigger nodes and note nodes)
            let mut nodes: Vec<NodeId> = voice.active_nodes.clone();
            nodes.extend(voice.note_nodes.values().copied());
            nodes
        };

        // Set param on all active synths (lock released)
        for node_id in nodes {
            let _ = self.backend.set_param(node_id, param, value).await;
        }

        Ok(())
    }
}

/// Convert MIDI note number to frequency in Hz.
fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}
