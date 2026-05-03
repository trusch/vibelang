//! Groups handler implementation.

use crate::backend::{AddAction, Backend};
use crate::compat::RwLock;
use crate::state::{GroupState, State};
use crate::traits::Groups;
use crate::types::{BusId, GroupId, NodeId, ParamMap};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

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
        let groups_to_link: Vec<(GroupId, NodeId, BusId, BusId, Option<u32>)> = {
            let state = self.state.read().await;

            state
                .groups
                .values()
                .filter(|g| g.link_synth_node_id.is_none())
                .map(|g| {
                    // Determine output bus:
                    // 1. Explicit output_bus override → route to hardware bus directly
                    // 2. Parent group → route to parent's audio bus
                    // 3. Root group → route to bus 0 (main stereo output)
                    let out_bus = if let Some(hw_bus) = g.output_bus {
                        BusId::new(hw_bus)
                    } else {
                        g.parent
                            .and_then(|p| state.groups.get(&p))
                            .map(|pg| pg.audio_bus)
                            .unwrap_or(BusId::new(0))
                    };

                    (g.id, g.node_id, g.audio_bus, out_bus, g.output_channels)
                })
                .collect()
        };

        // Create link synths for each group
        let num_groups = groups_to_link.len();
        for (group_id, group_node_id, in_bus, out_bus, output_channels) in groups_to_link {
            // Allocate node ID for link synth
            let link_node_id = {
                let mut state = self.state.write().await;
                state.alloc_node_id()
            };

            // Create the link synth
            // It reads from in_bus and writes to out_bus
            // Note: system_link_audio uses "inbus" and "outbus" parameter names
            let mut params = ParamMap::new();
            params.insert("inbus".to_string(), in_bus.0 as f32);
            params.insert("outbus".to_string(), out_bus.0 as f32);
            params.insert("amp".to_string(), 1.0);

            // Pick the link-synth variant based on the group's hardware
            // channel count: Some(1) → mono mixdown variant; Some(2) or
            // None (the implicit-stereo default) → the stereo default.
            let link_synth_name = match output_channels {
                Some(1) => "system_link_audio_mono",
                _ => "system_link_audio",
            };

            self.backend
                .create_synth(
                    link_synth_name,
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

            tracing::info!(
                "Created link synth for group {} (node_id={}, inbus={}, outbus={})",
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Groups for GroupsHandler<B> {
    async fn create(&self, id: GroupId, name: &str, parent: Option<GroupId>) -> Result<()> {
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
                name: name.to_string(),
                parent,
                node_id,
                audio_bus,
                link_synth_node_id: None, // Created later by FinalizeGroups
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );

        tracing::info!(
            "Created group {} '{}' (node_id={}, audio_bus={}) - voices should output to bus {}",
            id.0,
            name,
            node_id.0,
            audio_bus.0,
            audio_bus.0
        );

        Ok(())
    }

    async fn delete(&self, id: GroupId) -> Result<()> {
        let group = {
            let mut state = self.state.write().await;
            let group = state.groups.remove(&id).ok_or(Error::GroupNotFound(id))?;
            state.free_node_id(group.node_id);
            if let Some(link_id) = group.link_synth_node_id {
                state.free_node_id(link_id);
            }
            group
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

            let group = state.groups.get_mut(&id).ok_or(Error::GroupNotFound(id))?;

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

        let group = state.groups.get_mut(&id).ok_or(Error::GroupNotFound(id))?;

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
            let group = state.groups.get_mut(&id).ok_or(Error::GroupNotFound(id))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BufferInfo;
    use crate::compat::Instant;
    use crate::types::BufferId;
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    // =========================================================================
    // Mock Backend for Testing
    // =========================================================================

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock error")
        }
    }

    impl std::error::Error for MockError {}

    struct MockBackend {
        groups_created: AtomicU32,
        synths_created: AtomicU32,
        nodes_freed: AtomicU32,
        params_set: AtomicU32,
        run_node_calls: AtomicU32,
        synth_names: Mutex<Vec<String>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                groups_created: AtomicU32::new(0),
                synths_created: AtomicU32::new(0),
                nodes_freed: AtomicU32::new(0),
                params_set: AtomicU32::new(0),
                run_node_calls: AtomicU32::new(0),
                synth_names: Mutex::new(Vec::new()),
            }
        }

        fn groups_created(&self) -> u32 {
            self.groups_created.load(Ordering::Relaxed)
        }

        fn synths_created(&self) -> u32 {
            self.synths_created.load(Ordering::Relaxed)
        }

        fn nodes_freed(&self) -> u32 {
            self.nodes_freed.load(Ordering::Relaxed)
        }

        fn params_set(&self) -> u32 {
            self.params_set.load(Ordering::Relaxed)
        }

        fn run_node_calls(&self) -> u32 {
            self.run_node_calls.load(Ordering::Relaxed)
        }

        fn synth_names(&self) -> Vec<String> {
            self.synth_names.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl Backend for MockBackend {
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
            def: &str,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
            _params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            self.synths_created.fetch_add(1, Ordering::Relaxed);
            self.synth_names.lock().unwrap().push(def.to_string());
            Ok(())
        }

        async fn create_group(
            &self,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
        ) -> std::result::Result<(), Self::Error> {
            self.groups_created.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn free_node(&self, _node: NodeId) -> std::result::Result<(), Self::Error> {
            self.nodes_freed.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn run_node(
            &self,
            _node: NodeId,
            _running: bool,
        ) -> std::result::Result<(), Self::Error> {
            self.run_node_calls.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn set_param(
            &self,
            _node: NodeId,
            _param: &str,
            _value: f32,
        ) -> std::result::Result<(), Self::Error> {
            self.params_set.fetch_add(1, Ordering::Relaxed);
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

        async fn map_param_to_bus(
            &self,
            _node: NodeId,
            _param: &str,
            _bus: u32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        fn current_time(&self) -> Instant {
            Instant::now()
        }
    }

    // =========================================================================
    // Helper Functions
    // =========================================================================

    fn create_handler() -> (
        GroupsHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = GroupsHandler::new(backend.clone(), state.clone());
        (handler, backend, state)
    }

    // =========================================================================
    // Group Creation Tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_group() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        let result = handler.create(group_id, "TestGroup", None).await;

        assert!(result.is_ok(), "Creating group should succeed");
        assert_eq!(
            backend.groups_created(),
            1,
            "One group should be created in backend"
        );

        let state_read = state.read().await;
        assert!(state_read.groups.contains_key(&group_id));
        let group = state_read.groups.get(&group_id).unwrap();
        assert_eq!(group.name, "TestGroup");
        assert!(group.parent.is_none());
    }

    #[tokio::test]
    async fn test_create_group_duplicate_fails() {
        let (handler, _, _state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.create(group_id, "TestGroup2", None).await;
        assert!(result.is_err(), "Duplicate group creation should fail");
    }

    #[tokio::test]
    async fn test_create_group_with_parent() {
        let (handler, backend, state) = create_handler();

        let parent_id = GroupId::new(1);
        let child_id = GroupId::new(2);

        handler.create(parent_id, "Parent", None).await.unwrap();
        let result = handler.create(child_id, "Child", Some(parent_id)).await;

        assert!(result.is_ok(), "Creating child group should succeed");
        assert_eq!(backend.groups_created(), 2);

        let state_read = state.read().await;
        let child = state_read.groups.get(&child_id).unwrap();
        assert_eq!(child.parent, Some(parent_id));
    }

    #[tokio::test]
    async fn test_create_group_with_nonexistent_parent_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler
            .create(GroupId::new(1), "Child", Some(GroupId::new(999)))
            .await;
        assert!(result.is_err(), "Should fail with non-existent parent");
    }

    #[tokio::test]
    async fn test_create_group_allocates_node_and_bus() {
        let (handler, _, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();

        // Node ID should be > 0 (0 is reserved for root)
        assert!(group.node_id.0 > 0, "Node ID should be allocated");
        // Audio bus should be in the group bus range (>= 16)
        assert!(
            group.audio_bus.0 >= 16,
            "Audio bus should be in group range"
        );
    }

    // =========================================================================
    // Group Deletion Tests
    // =========================================================================

    #[tokio::test]
    async fn test_delete_group() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.delete(group_id).await;
        assert!(result.is_ok(), "Deleting group should succeed");
        assert_eq!(backend.nodes_freed(), 1, "Node should be freed");

        let state_read = state.read().await;
        assert!(!state_read.groups.contains_key(&group_id));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_group_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler.delete(GroupId::new(999)).await;
        assert!(result.is_err(), "Deleting non-existent group should fail");
    }

    // =========================================================================
    // Group Set Param Tests
    // =========================================================================

    #[tokio::test]
    async fn test_set_param() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.set_param(group_id, "gain", 0.8).await;
        assert!(result.is_ok(), "Setting param should succeed");
        assert_eq!(backend.params_set(), 1);

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert_eq!(*group.params.get("gain").unwrap(), 0.8);
    }

    #[tokio::test]
    async fn test_set_param_nonexistent_group_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler.set_param(GroupId::new(999), "gain", 0.5).await;
        assert!(
            result.is_err(),
            "Setting param on non-existent group should fail"
        );
    }

    // =========================================================================
    // Group Mute Tests
    // =========================================================================

    #[tokio::test]
    async fn test_mute_group() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.mute(group_id, true).await;
        assert!(result.is_ok(), "Muting group should succeed");
        assert_eq!(backend.run_node_calls(), 1, "run_node should be called");

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(group.muted);
    }

    #[tokio::test]
    async fn test_unmute_group() {
        let (handler, _, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        handler.mute(group_id, true).await.unwrap();
        handler.mute(group_id, false).await.unwrap();

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(!group.muted);
    }

    #[tokio::test]
    async fn test_mute_nonexistent_group_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler.mute(GroupId::new(999), true).await;
        assert!(result.is_err());
    }

    // =========================================================================
    // Group Solo Tests
    // =========================================================================

    #[tokio::test]
    async fn test_solo_group() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.solo(group_id, true).await;
        assert!(result.is_ok(), "Soloing group should succeed");
        assert!(backend.run_node_calls() >= 1, "run_node should be called");

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(group.soloed);
    }

    #[tokio::test]
    async fn test_unsolo_group() {
        let (handler, _, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        handler.solo(group_id, true).await.unwrap();
        handler.solo(group_id, false).await.unwrap();

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(!group.soloed);
    }

    #[tokio::test]
    async fn test_solo_nonexistent_group_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler.solo(GroupId::new(999), true).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_solo_affects_multiple_groups() {
        let (handler, backend, _state) = create_handler();

        let group1 = GroupId::new(1);
        let group2 = GroupId::new(2);

        handler.create(group1, "Group1", None).await.unwrap();
        handler.create(group2, "Group2", None).await.unwrap();

        // Solo group1 - should update run state for both groups
        handler.solo(group1, true).await.unwrap();

        // Should have called run_node for both groups
        assert!(
            backend.run_node_calls() >= 2,
            "run_node should be called for all groups"
        );
    }

    // =========================================================================
    // Finalize Tests
    // =========================================================================

    #[tokio::test]
    async fn test_finalize_creates_link_synths() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.finalize().await;
        assert!(result.is_ok(), "Finalize should succeed");
        assert_eq!(backend.synths_created(), 1, "Link synth should be created");

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(
            group.link_synth_node_id.is_some(),
            "Link synth node ID should be set"
        );
    }

    #[tokio::test]
    async fn test_finalize_multiple_groups() {
        let (handler, backend, _state) = create_handler();

        let group1 = GroupId::new(1);
        let group2 = GroupId::new(2);

        handler.create(group1, "Group1", None).await.unwrap();
        handler.create(group2, "Group2", None).await.unwrap();

        handler.finalize().await.unwrap();

        assert_eq!(backend.synths_created(), 2, "Link synth for each group");
    }

    #[tokio::test]
    async fn test_finalize_default_spawns_stereo_link_synth() {
        // Default group (output_channels = None, output_bus = None) keeps the
        // legacy stereo behaviour: spawn `system_link_audio`.
        let (handler, backend, _state) = create_handler();
        handler
            .create(GroupId::new(1), "stereo", None)
            .await
            .unwrap();
        handler.finalize().await.unwrap();
        assert_eq!(backend.synth_names(), vec!["system_link_audio".to_string()]);
    }

    #[tokio::test]
    async fn test_finalize_mono_group_spawns_mono_link_synth() {
        // Mono-routed group (output_channels = Some(1), output_bus = Some(2))
        // dispatches to the mixdown variant.
        let (handler, backend, state) = create_handler();
        let group_id = GroupId::new(1);
        handler.create(group_id, "mono", None).await.unwrap();

        // Mirror what the Rhai layer (Task C) will do once it sets both
        // fields together: mono output to hardware bus 2.
        {
            let mut state = state.write().await;
            let group = state.groups.get_mut(&group_id).unwrap();
            group.output_bus = Some(2);
            group.output_channels = Some(1);
        }

        handler.finalize().await.unwrap();
        assert_eq!(
            backend.synth_names(),
            vec!["system_link_audio_mono".to_string()],
            "output_channels=Some(1) must spawn the mono mixdown variant"
        );
    }

    #[tokio::test]
    async fn test_finalize_stereo_group_with_output_bus_spawns_stereo() {
        // Stereo-routed group (output_channels = Some(2), output_bus = Some(2))
        // keeps the existing `system_link_audio` synthdef.
        let (handler, backend, state) = create_handler();
        let group_id = GroupId::new(1);
        handler.create(group_id, "stereo_hw", None).await.unwrap();
        {
            let mut state = state.write().await;
            let group = state.groups.get_mut(&group_id).unwrap();
            group.output_bus = Some(2);
            group.output_channels = Some(2);
        }

        handler.finalize().await.unwrap();
        assert_eq!(backend.synth_names(), vec!["system_link_audio".to_string()]);
    }

    #[tokio::test]
    async fn test_finalize_idempotent() {
        let (handler, backend, _state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        // Finalize twice
        handler.finalize().await.unwrap();
        handler.finalize().await.unwrap();

        // Should only create one link synth (the second finalize is a no-op)
        assert_eq!(
            backend.synths_created(),
            1,
            "Should only create link synth once"
        );
    }

    // =========================================================================
    // Multiple Operations Tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_delete_create() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);

        handler.create(group_id, "TestGroup", None).await.unwrap();
        handler.delete(group_id).await.unwrap();
        handler.create(group_id, "TestGroup2", None).await.unwrap();

        assert_eq!(backend.groups_created(), 2);
        assert_eq!(backend.nodes_freed(), 1);

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert_eq!(group.name, "TestGroup2");
    }

    #[tokio::test]
    async fn test_multiple_params() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        handler.set_param(group_id, "gain", 0.8).await.unwrap();
        handler.set_param(group_id, "pan", -0.5).await.unwrap();
        handler.set_param(group_id, "filter", 0.6).await.unwrap();

        assert_eq!(backend.params_set(), 3);

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert_eq!(*group.params.get("gain").unwrap(), 0.8);
        assert_eq!(*group.params.get("pan").unwrap(), -0.5);
        assert_eq!(*group.params.get("filter").unwrap(), 0.6);
    }
}
