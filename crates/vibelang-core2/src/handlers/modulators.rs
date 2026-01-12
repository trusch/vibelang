//! Modulators handler implementation.
//!
//! Modulators are control-rate synthdefs that output to control buses.
//! They can be used to modulate voice parameters (LFOs, envelopes, etc.).
//!
//! Modulator synths are placed in a special modulator group that runs
//! before voice groups, ensuring control signals are ready when voices
//! are triggered.

use crate::backend::{AddAction, Backend};
use crate::compat::RwLock;
use crate::state::{ModulatorState, State};
use crate::traits::{ModulatorConfig, Modulators};
use crate::types::{ModulatorId, NodeId};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// Handler for modulator operations.
pub struct ModulatorsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

impl<B: Backend> ModulatorsHandler<B> {
    /// Create a new modulators handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }

    /// Ensure the modulator group exists.
    ///
    /// The modulator group contains all modulator synths and runs before
    /// voice groups in the node graph. This ensures control signals are
    /// available when voices are triggered.
    pub async fn ensure_modulator_group(&self) -> Result<NodeId> {
        let group_node = {
            let mut state = self.state.write().await;
            if let Some(node) = state.modulator_group {
                return Ok(node);
            }

            // Allocate a node ID for the modulator group
            let node_id = state.alloc_node_id();
            state.modulator_group = Some(node_id);
            node_id
        };

        // Create the group at the head of the root group (node 0)
        // This ensures modulators run before everything else
        self.backend
            .create_group(group_node, NodeId::new(0), AddAction::Head)
            .await
            .map_err(Error::backend)?;

        tracing::debug!(
            "Created modulator group {:?} at head of root",
            group_node
        );

        Ok(group_node)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Modulators for ModulatorsHandler<B> {
    async fn create(&self, id: ModulatorId, config: ModulatorConfig) -> Result<()> {
        // Ensure modulator group exists
        let modulator_group = self.ensure_modulator_group().await?;

        // Gather info while holding lock
        let (node_id, control_bus, synthdef) = {
            let mut state = self.state.write().await;

            if state.modulators.contains_key(&id) {
                return Err(Error::ModulatorExists(id));
            }

            // Verify the synthdef exists
            if !state.synthdefs.contains(&config.synthdef) {
                return Err(Error::SynthDefNotFound(config.synthdef.clone()));
            }

            // Allocate resources
            let node_id = state.alloc_node_id();
            let control_bus = state.control_buses.allocate();

            // Store state
            state.modulators.insert(
                id,
                ModulatorState {
                    id,
                    config: config.clone(),
                    control_bus,
                    synth_node: node_id,
                },
            );

            (node_id, control_bus, config.synthdef.clone())
        };

        // Build params with control bus output
        let mut full_params = config.params.clone();
        full_params.insert("out".to_string(), control_bus.raw() as f32);

        tracing::debug!(
            "Creating modulator {:?} (node={:?}) on control bus {} with synthdef '{}'",
            id,
            node_id,
            control_bus.raw(),
            synthdef
        );

        // Create modulator synth in the modulator group
        self.backend
            .create_synth(&synthdef, node_id, modulator_group, AddAction::Tail, &full_params)
            .await
            .map_err(Error::backend)?;

        tracing::info!(
            "Modulator '{}' ({:?}) created on control bus {}",
            config.name,
            id,
            control_bus.raw()
        );

        Ok(())
    }

    async fn delete(&self, id: ModulatorId) -> Result<()> {
        let node_to_free = {
            let mut state = self.state.write().await;
            let modulator = state
                .modulators
                .remove(&id)
                .ok_or(Error::ModulatorNotFound(id))?;
            modulator.synth_node
        };

        // Free the modulator node
        self.backend
            .free_node(node_to_free)
            .await
            .map_err(Error::backend)?;

        tracing::debug!("Deleted modulator {:?}", id);

        Ok(())
    }

    async fn set_param(&self, id: ModulatorId, param: &str, value: f32) -> Result<()> {
        let node_id = {
            let mut state = self.state.write().await;

            let modulator = state
                .modulators
                .get_mut(&id)
                .ok_or(Error::ModulatorNotFound(id))?;

            // Update state
            modulator.config.params.insert(param.to_string(), value);
            modulator.synth_node
        };

        // Send to backend
        self.backend
            .set_param(node_id, param, value)
            .await
            .map_err(Error::backend)?;

        Ok(())
    }
}

/// Get the control bus ID for a modulator.
///
/// This is used when creating voices with modulation to determine
/// which control bus to map to the voice parameter.
pub fn get_modulator_control_bus(state: &State, id: ModulatorId) -> Option<u32> {
    state
        .modulators
        .get(&id)
        .map(|m| m.control_bus.raw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BufferInfo;
    use crate::compat::Instant;
    use crate::types::{BufferId, ParamMap};
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

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
        synths_created: AtomicU32,
        groups_created: AtomicU32,
        nodes_freed: AtomicU32,
        params_set: AtomicU32,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                synths_created: AtomicU32::new(0),
                groups_created: AtomicU32::new(0),
                nodes_freed: AtomicU32::new(0),
                params_set: AtomicU32::new(0),
            }
        }

        fn synths_created(&self) -> u32 {
            self.synths_created.load(Ordering::Relaxed)
        }

        fn groups_created(&self) -> u32 {
            self.groups_created.load(Ordering::Relaxed)
        }

        fn nodes_freed(&self) -> u32 {
            self.nodes_freed.load(Ordering::Relaxed)
        }

        fn params_set(&self) -> u32 {
            self.params_set.load(Ordering::Relaxed)
        }
    }

    #[async_trait::async_trait]
    impl Backend for MockBackend {
        type Error = MockError;

        async fn load_synthdef(&self, _name: &str, _data: &[u8]) -> std::result::Result<(), Self::Error> {
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
            self.synths_created.fetch_add(1, Ordering::Relaxed);
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

        async fn run_node(&self, _node: NodeId, _running: bool) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn set_param(&self, _node: NodeId, _param: &str, _value: f32) -> std::result::Result<(), Self::Error> {
            self.params_set.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn load_buffer(&self, _id: BufferId, _path: &Path) -> std::result::Result<BufferInfo, Self::Error> {
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

        async fn write_buffer(&self, _id: BufferId, _path: &Path) -> std::result::Result<(), Self::Error> {
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

    fn create_handler() -> (ModulatorsHandler<MockBackend>, Arc<MockBackend>, Arc<RwLock<State>>) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = ModulatorsHandler::new(backend.clone(), state.clone());
        (handler, backend, state)
    }

    async fn setup_state_with_synthdef(state: &Arc<RwLock<State>>) {
        let mut state_write = state.write().await;
        state_write.synthdefs.insert("lfo_sine".to_string());
        state_write.synthdefs.insert("envelope_follower".to_string());
    }

    // =========================================================================
    // Modulator Create Tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_modulator() {
        let (handler, backend, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let modulator_id = ModulatorId::new(1);
        let config = ModulatorConfig::new("test_lfo", "lfo_sine");

        let result = handler.create(modulator_id, config).await;

        assert!(result.is_ok(), "Creating modulator should succeed");
        assert_eq!(backend.groups_created(), 1, "Modulator group should be created");
        assert_eq!(backend.synths_created(), 1, "One synth should be created");

        let state_read = state.read().await;
        assert!(state_read.modulators.contains_key(&modulator_id));
        assert!(state_read.modulator_group.is_some());
    }

    #[tokio::test]
    async fn test_create_modulator_duplicate_fails() {
        let (handler, _, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let modulator_id = ModulatorId::new(1);
        let config = ModulatorConfig::new("test_lfo", "lfo_sine");

        handler.create(modulator_id, config.clone()).await.unwrap();
        let result = handler.create(modulator_id, config).await;

        assert!(result.is_err(), "Duplicate modulator should fail");
    }

    #[tokio::test]
    async fn test_create_modulator_synthdef_not_found() {
        let (handler, _, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let modulator_id = ModulatorId::new(1);
        let config = ModulatorConfig::new("test_lfo", "nonexistent_synthdef");

        let result = handler.create(modulator_id, config).await;

        assert!(result.is_err(), "Should fail with non-existent synthdef");
    }

    #[tokio::test]
    async fn test_create_modulator_with_params() {
        let (handler, backend, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let modulator_id = ModulatorId::new(1);
        let config = ModulatorConfig::new("test_lfo", "lfo_sine")
            .with_param("rate", 4.0)
            .with_param("lo", 200.0)
            .with_param("hi", 2000.0);

        let result = handler.create(modulator_id, config).await;

        assert!(result.is_ok());
        assert_eq!(backend.synths_created(), 1);

        let state_read = state.read().await;
        let modulator = state_read.modulators.get(&modulator_id).unwrap();
        assert_eq!(*modulator.config.params.get("rate").unwrap(), 4.0);
        assert_eq!(*modulator.config.params.get("lo").unwrap(), 200.0);
        assert_eq!(*modulator.config.params.get("hi").unwrap(), 2000.0);
    }

    // =========================================================================
    // Modulator Delete Tests
    // =========================================================================

    #[tokio::test]
    async fn test_delete_modulator() {
        let (handler, backend, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let modulator_id = ModulatorId::new(1);
        let config = ModulatorConfig::new("test_lfo", "lfo_sine");

        handler.create(modulator_id, config).await.unwrap();
        let result = handler.delete(modulator_id).await;

        assert!(result.is_ok(), "Deleting modulator should succeed");
        assert_eq!(backend.nodes_freed(), 1, "Node should be freed");

        let state_read = state.read().await;
        assert!(!state_read.modulators.contains_key(&modulator_id));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_modulator_fails() {
        let (handler, _, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let result = handler.delete(ModulatorId::new(999)).await;
        assert!(result.is_err(), "Deleting non-existent modulator should fail");
    }

    // =========================================================================
    // Modulator Set Param Tests
    // =========================================================================

    #[tokio::test]
    async fn test_set_param() {
        let (handler, backend, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let modulator_id = ModulatorId::new(1);
        let config = ModulatorConfig::new("test_lfo", "lfo_sine");

        handler.create(modulator_id, config).await.unwrap();
        let result = handler.set_param(modulator_id, "rate", 8.0).await;

        assert!(result.is_ok(), "Setting param should succeed");
        assert_eq!(backend.params_set(), 1, "Param should be set on backend");

        let state_read = state.read().await;
        let modulator = state_read.modulators.get(&modulator_id).unwrap();
        assert_eq!(*modulator.config.params.get("rate").unwrap(), 8.0);
    }

    #[tokio::test]
    async fn test_set_param_nonexistent_modulator_fails() {
        let (handler, _, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let result = handler.set_param(ModulatorId::new(999), "rate", 1.0).await;
        assert!(result.is_err(), "Setting param on non-existent modulator should fail");
    }

    // =========================================================================
    // Control Bus Tests
    // =========================================================================

    #[tokio::test]
    async fn test_modulator_allocates_unique_control_buses() {
        let (handler, _, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let config1 = ModulatorConfig::new("lfo1", "lfo_sine");
        let config2 = ModulatorConfig::new("lfo2", "lfo_sine");

        handler.create(ModulatorId::new(1), config1).await.unwrap();
        handler.create(ModulatorId::new(2), config2).await.unwrap();

        let state_read = state.read().await;
        let mod1 = state_read.modulators.get(&ModulatorId::new(1)).unwrap();
        let mod2 = state_read.modulators.get(&ModulatorId::new(2)).unwrap();

        assert_ne!(
            mod1.control_bus.raw(),
            mod2.control_bus.raw(),
            "Modulators should have different control buses"
        );
    }

    #[tokio::test]
    async fn test_get_modulator_control_bus() {
        let (handler, _, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        let modulator_id = ModulatorId::new(1);
        let config = ModulatorConfig::new("test_lfo", "lfo_sine");

        handler.create(modulator_id, config).await.unwrap();

        let state_read = state.read().await;
        let control_bus = get_modulator_control_bus(&state_read, modulator_id);

        assert!(control_bus.is_some(), "Should find control bus for existing modulator");
        assert!(control_bus.unwrap() >= 1000, "Control bus should be >= 1000");
    }
}
