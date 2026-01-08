//! Groups handler implementation.

use crate::backend::{AddAction, Backend};
use crate::state::{GroupState, State};
use crate::traits::Groups;
use crate::types::{BusId, GroupId, NodeId, ParamMap};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handler for group operations.
pub struct GroupsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

impl<B: Backend> GroupsHandler<B> {
    /// Create a new groups handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }

    /// Finalize all groups by creating link synths for audio routing.
    ///
    /// This creates `system_link_audio` synths that route audio from each
    /// group's audio bus to its parent's bus (or bus 0 for the main output).
    pub async fn finalize(&self) -> Result<()> {
        // Collect groups that need link synths
        let groups_to_link: Vec<(GroupId, NodeId, BusId, BusId)> = {
            let state = self.state.read().await;

            state
                .groups
                .values()
                .filter(|g| g.link_synth_node_id.is_none())
                .map(|g| {
                    // Determine output bus: parent's bus or bus 0 (main output)
                    let out_bus = g
                        .parent
                        .and_then(|p| state.groups.get(&p))
                        .map(|pg| pg.audio_bus)
                        .unwrap_or(BusId::new(0));

                    (g.id, g.node_id, g.audio_bus, out_bus)
                })
                .collect()
        };

        // Create link synths for each group
        let num_groups = groups_to_link.len();
        for (group_id, group_node_id, in_bus, out_bus) in groups_to_link {
            // Allocate node ID for link synth
            let link_node_id = {
                let mut state = self.state.write().await;
                state.alloc_node_id()
            };

            // Create the link synth
            // It reads from in_bus and writes to out_bus
            let mut params = ParamMap::new();
            params.insert("in".to_string(), in_bus.0 as f32);
            params.insert("out".to_string(), out_bus.0 as f32);
            params.insert("amp".to_string(), 1.0);

            self.backend
                .create_synth(
                    "system_link_audio",
                    link_node_id,
                    group_node_id,
                    AddAction::Tail, // Add at tail of the group
                    &params,
                )
                .await
                .map_err(Error::backend)?;

            // Update state with link synth node ID
            {
                let mut state = self.state.write().await;
                if let Some(group) = state.groups.get_mut(&group_id) {
                    group.link_synth_node_id = Some(link_node_id);
                }
            }

            tracing::debug!(
                "Created link synth for group {} (node_id={}, in_bus={}, out_bus={})",
                group_id.0,
                link_node_id.0,
                in_bus.0,
                out_bus.0
            );
        }

        tracing::info!("Finalized {} groups with link synths", num_groups);
        Ok(())
    }
}

#[async_trait]
impl<B: Backend> Groups for GroupsHandler<B> {
    async fn create(&self, id: GroupId, parent: Option<GroupId>) -> Result<()> {
        let mut state = self.state.write().await;

        if state.groups.contains_key(&id) {
            return Err(Error::GroupExists(id));
        }

        // Allocate node ID and audio bus
        let node_id = state.alloc_node_id();
        let audio_bus = state.alloc_bus_id();

        // Determine target for placement
        let (target, action) = if let Some(parent_id) = parent {
            let parent_state = state
                .groups
                .get(&parent_id)
                .ok_or(Error::GroupNotFound(parent_id))?;
            (parent_state.node_id, AddAction::Tail)
        } else {
            // Add to root group (node 0)
            (NodeId::new(0), AddAction::Tail)
        };

        // Create group in backend
        self.backend
            .create_group(node_id, target, action)
            .await
            .map_err(Error::backend)?;

        // Store state
        state.groups.insert(
            id,
            GroupState {
                id,
                parent,
                node_id,
                audio_bus,
                link_synth_node_id: None, // Created later by FinalizeGroups
                muted: false,
                soloed: false,
                params: ParamMap::new(),
            },
        );

        tracing::debug!(
            "Created group {} (node_id={}, audio_bus={})",
            id.0,
            node_id.0,
            audio_bus.0
        );

        Ok(())
    }

    async fn delete(&self, id: GroupId) -> Result<()> {
        let group = {
            let mut state = self.state.write().await;
            state.groups.remove(&id).ok_or(Error::GroupNotFound(id))?
        };

        // Free the link synth first if it exists
        if let Some(link_node_id) = group.link_synth_node_id {
            let _ = self.backend.free_node(link_node_id).await;
        }

        // Free the group node (this also frees all child nodes)
        self.backend
            .free_node(group.node_id)
            .await
            .map_err(Error::backend)?;

        tracing::debug!("Deleted group {} (node_id={})", id.0, group.node_id.0);

        Ok(())
    }

    async fn set_param(&self, id: GroupId, param: &str, value: f32) -> Result<()> {
        let (target_node_id, link_synth_node_id) = {
            let mut state = self.state.write().await;

            let group = state
                .groups
                .get_mut(&id)
                .ok_or(Error::GroupNotFound(id))?;

            // Update state
            group.params.insert(param.to_string(), value);

            (group.node_id, group.link_synth_node_id)
        };

        // For amp/pan parameters, target the link synth if it exists
        // This is where audio actually flows through
        let target = if matches!(param, "amp" | "pan") {
            link_synth_node_id.unwrap_or(target_node_id)
        } else {
            target_node_id
        };

        // Send to backend
        self.backend
            .set_param(target, param, value)
            .await
            .map_err(Error::backend)?;

        Ok(())
    }

    async fn mute(&self, id: GroupId, muted: bool) -> Result<()> {
        let mut state = self.state.write().await;

        let group = state
            .groups
            .get_mut(&id)
            .ok_or(Error::GroupNotFound(id))?;

        group.muted = muted;

        // Run/pause the group node
        self.backend
            .run_node(group.node_id, !muted)
            .await
            .map_err(Error::backend)?;

        Ok(())
    }

    async fn solo(&self, id: GroupId, solo: bool) -> Result<()> {
        // Collect data about what to do, then release lock before backend calls
        let updates: Vec<(NodeId, bool)> = {
            let mut state = self.state.write().await;

            // Update the target group's soloed state
            let group = state
                .groups
                .get_mut(&id)
                .ok_or(Error::GroupNotFound(id))?;
            group.soloed = solo;

            // Check if ANY group is now soloed
            let any_soloed = state.groups.values().any(|g| g.soloed);

            // Determine run state for each group
            // If any group is soloed: run only soloed groups (unless muted)
            // If no groups are soloed: run groups based on their mute state
            state
                .groups
                .values()
                .map(|g| {
                    let should_run = if any_soloed {
                        // Solo mode: only run if soloed AND not muted
                        g.soloed && !g.muted
                    } else {
                        // Normal mode: run if not muted
                        !g.muted
                    };
                    (g.node_id, should_run)
                })
                .collect()
        };

        // Apply updates to backend (lock released)
        for (node_id, should_run) in updates {
            self.backend
                .run_node(node_id, should_run)
                .await
                .map_err(Error::backend)?;
        }

        tracing::debug!("Group {} solo set to {}", id.0, solo);
        Ok(())
    }
}
