//! Effects handler implementation.
//!
//! Effects process audio passing through a group's audio bus. They use:
//! - `In.ar(bus)` to read from the group's bus
//! - `ReplaceOut(bus, ...)` to write processed audio back to the same bus
//!
//! The synthdef must have `__fx_bus_in` and `__fx_bus_out` parameters,
//! which are automatically added by `define_fx()` in vibelang-dsp.

use crate::backend::{AddAction, Backend};
use crate::state::{EffectState, State};
use crate::traits::Effects;
use crate::types::{EffectId, GroupId, NodeId, ParamMap};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handler for effect operations.
pub struct EffectsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

impl<B: Backend> EffectsHandler<B> {
    /// Create a new effects handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }
}

#[async_trait]
impl<B: Backend> Effects for EffectsHandler<B> {
    async fn add(
        &self,
        id: EffectId,
        group: GroupId,
        synthdef: &str,
        params: &ParamMap,
    ) -> Result<()> {
        // Gather info while holding lock
        let (node_id, audio_bus, target_node, add_action) = {
            let mut state = self.state.write().await;

            if state.effects.contains_key(&id) {
                return Err(Error::EffectExists(id));
            }

            let group_state = state.groups.get(&group).ok_or(Error::GroupNotFound(group))?;
            let group_node_id = group_state.node_id;
            let audio_bus = group_state.audio_bus;
            let link_synth_node_id = group_state.link_synth_node_id;

            let node_id = state.alloc_node_id();

            // Determine placement: effects should run AFTER voices but BEFORE link synth
            // - If link synth exists: insert BEFORE it
            // - Otherwise: add to TAIL of group
            let (target_node, add_action): (NodeId, AddAction) =
                if let Some(link_node) = link_synth_node_id {
                    (link_node, AddAction::Before)
                } else {
                    (group_node_id, AddAction::Tail)
                };

            // Store state (do this before releasing lock)
            state.effects.insert(
                id,
                EffectState {
                    id,
                    group,
                    synthdef: synthdef.to_string(),
                    node_id,
                    audio_bus,
                    params: params.clone(),
                },
            );

            (node_id, audio_bus, target_node, add_action)
        };

        // Build params with bus routing
        // Effects created with define_fx() have hidden __fx_bus_in and __fx_bus_out params
        let mut full_params = params.clone();
        full_params.insert("__fx_bus_in".to_string(), audio_bus.0 as f32);
        full_params.insert("__fx_bus_out".to_string(), audio_bus.0 as f32);

        tracing::debug!(
            "Creating effect {:?} (node={:?}) on bus {} with action {:?} relative to {:?}",
            id,
            node_id,
            audio_bus.0,
            add_action,
            target_node
        );

        // Create effect synth (lock released)
        self.backend
            .create_synth(synthdef, node_id, target_node, add_action, &full_params)
            .await
            .map_err(Error::backend)?;

        Ok(())
    }

    async fn remove(&self, id: EffectId) -> Result<()> {
        let node_to_free = {
            let mut state = self.state.write().await;
            let effect = state.effects.remove(&id).ok_or(Error::EffectNotFound(id))?;
            effect.node_id
        };

        // Free the effect node (lock released)
        self.backend
            .free_node(node_to_free)
            .await
            .map_err(Error::backend)?;

        Ok(())
    }

    async fn set_param(&self, id: EffectId, param: &str, value: f32) -> Result<()> {
        let node_id = {
            let mut state = self.state.write().await;

            let effect = state
                .effects
                .get_mut(&id)
                .ok_or(Error::EffectNotFound(id))?;

            // Update state
            effect.params.insert(param.to_string(), value);
            effect.node_id
        };

        // Send to backend (lock released)
        self.backend
            .set_param(node_id, param, value)
            .await
            .map_err(Error::backend)?;

        Ok(())
    }
}
