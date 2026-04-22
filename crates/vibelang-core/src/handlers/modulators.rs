//! Modulators handler implementation.
//!
//! Modulators are control-rate synthdefs that output to control buses.
//! They can be used to modulate voice parameters (LFOs, envelopes, etc.).
//!
//! Modulator synths are placed in a special modulator group that runs
//! before voice groups, ensuring control signals are ready when voices
//! are triggered.
//!
//! Nested modulation is supported: modulator parameters can be modulated
//! by other modulators. Circular dependencies are detected and rejected.

use crate::backend::{AddAction, Backend};
use crate::compat::RwLock;
use crate::state::{ModulatorState, State};
use crate::traits::{ModulatorConfig, Modulators};
use crate::types::{ModulatorId, NodeId};
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// =============================================================================
// Dependency Graph Utilities
// =============================================================================

/// Build a dependency graph from modulator configurations.
///
/// Returns an adjacency list: modulator_id -> list of modulators it depends on.
pub fn build_dependency_graph(
    modulators: &HashMap<ModulatorId, ModulatorConfig>,
) -> HashMap<ModulatorId, Vec<ModulatorId>> {
    modulators
        .iter()
        .map(|(id, config)| (*id, config.modulations.values().cloned().collect()))
        .collect()
}

/// Detect circular dependencies in the modulator graph using DFS.
///
/// Returns Some(cycle_description) if a cycle exists, None otherwise.
pub fn detect_cycle(deps: &HashMap<ModulatorId, Vec<ModulatorId>>) -> Option<String> {
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    for &start in deps.keys() {
        if !visited.contains(&start) {
            if let Some(cycle) =
                dfs_detect_cycle(start, deps, &mut visited, &mut rec_stack, &mut path)
            {
                return Some(cycle);
            }
        }
    }
    None
}

fn dfs_detect_cycle(
    node: ModulatorId,
    deps: &HashMap<ModulatorId, Vec<ModulatorId>>,
    visited: &mut HashSet<ModulatorId>,
    rec_stack: &mut HashSet<ModulatorId>,
    path: &mut Vec<ModulatorId>,
) -> Option<String> {
    visited.insert(node);
    rec_stack.insert(node);
    path.push(node);

    if let Some(neighbors) = deps.get(&node) {
        for &neighbor in neighbors {
            if !visited.contains(&neighbor) {
                if let Some(cycle) = dfs_detect_cycle(neighbor, deps, visited, rec_stack, path) {
                    return Some(cycle);
                }
            } else if rec_stack.contains(&neighbor) {
                // Found a cycle - build description
                let cycle_start_idx = path.iter().position(|&n| n == neighbor).unwrap_or(0);
                let cycle_nodes: Vec<String> = path[cycle_start_idx..]
                    .iter()
                    .map(|id| format!("{}", id.raw()))
                    .collect();
                return Some(format!(
                    "modulators form a cycle: {} -> {}",
                    cycle_nodes.join(" -> "),
                    neighbor.raw()
                ));
            }
        }
    }

    path.pop();
    rec_stack.remove(&node);
    None
}

/// Topologically sort modulators so dependencies come before dependents.
///
/// The input graph is `node -> [dependencies]` (what the node depends on).
/// Output is ordered so each node comes after all its dependencies.
#[allow(dead_code)]
pub fn topological_sort(deps: &HashMap<ModulatorId, Vec<ModulatorId>>) -> Result<Vec<ModulatorId>> {
    // Count how many dependencies each node has (nodes it depends on)
    let mut dep_count: HashMap<ModulatorId, usize> = deps
        .iter()
        .map(|(&id, dependencies)| (id, dependencies.len()))
        .collect();

    // Start with nodes that have no dependencies
    let mut queue: Vec<ModulatorId> = dep_count
        .iter()
        .filter(|(_, &count)| count == 0)
        .map(|(&id, _)| id)
        .collect();

    // Build reverse graph: for each node, who depends on it?
    let mut dependents: HashMap<ModulatorId, Vec<ModulatorId>> = HashMap::new();
    for (&node, dependencies) in deps {
        for &dep in dependencies {
            dependents.entry(dep).or_default().push(node);
        }
    }

    let mut result = Vec::new();

    while let Some(node) = queue.pop() {
        result.push(node);

        // For each node that depends on this one, reduce their dep count
        if let Some(dependent_list) = dependents.get(&node) {
            for &dependent in dependent_list {
                if let Some(count) = dep_count.get_mut(&dependent) {
                    *count -= 1;
                    if *count == 0 {
                        queue.push(dependent);
                    }
                }
            }
        }
    }

    if result.len() != deps.len() {
        // Cycle detected (should have been caught by detect_cycle, but be safe)
        Err(Error::CircularModulationDependency(
            "cycle detected during topological sort".to_string(),
        ))
    } else {
        Ok(result)
    }
}

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

        tracing::debug!("Created modulator group {:?} at head of root", group_node);

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
        let (node_id, control_bus, synthdef, modulations_to_apply, target_node, add_action) = {
            let mut state = self.state.write().await;

            if state.modulators.contains_key(&id) {
                return Err(Error::ModulatorExists(id));
            }

            // Verify the synthdef exists
            if !state.synthdefs.contains(&config.synthdef) {
                return Err(Error::SynthDefNotFound(config.synthdef.clone()));
            }

            // Check for circular dependencies if this modulator has modulations
            if !config.modulations.is_empty() {
                // Build a test config map including the new modulator
                let mut test_configs: HashMap<ModulatorId, ModulatorConfig> = state
                    .modulators
                    .iter()
                    .map(|(id, m)| (*id, m.config.clone()))
                    .collect();
                test_configs.insert(id, config.clone());

                let deps = build_dependency_graph(&test_configs);
                if let Some(cycle) = detect_cycle(&deps) {
                    return Err(Error::CircularModulationDependency(cycle));
                }
            }

            // Collect modulations to apply (param_name, control_bus)
            let mut modulations_to_apply: Vec<(String, u32)> = Vec::new();
            for (param_name, source_mod_id) in &config.modulations {
                if let Some(source_mod) = state.modulators.get(source_mod_id) {
                    modulations_to_apply.push((param_name.clone(), source_mod.control_bus.raw()));
                } else {
                    return Err(Error::ModulatorNotFound(*source_mod_id));
                }
            }

            // Determine node placement based on dependencies
            let (target_node, add_action) = if config.modulations.is_empty() {
                // No dependencies - place at tail of modulator group
                (modulator_group, AddAction::Tail)
            } else {
                // Find the "latest" dependency node (furthest in execution order)
                // Place this modulator after all its dependencies
                let mut latest_node: Option<NodeId> = None;
                for source_id in config.modulations.values() {
                    if let Some(source_mod) = state.modulators.get(source_id) {
                        // Use the last dependency's node as target
                        // (In a perfect implementation we'd track actual node order)
                        latest_node = Some(source_mod.synth_node);
                    }
                }
                match latest_node {
                    Some(node) => (node, AddAction::After),
                    None => (modulator_group, AddAction::Tail),
                }
            };

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

            (
                node_id,
                control_bus,
                config.synthdef.clone(),
                modulations_to_apply,
                target_node,
                add_action,
            )
        };

        // Build params with control bus output
        let mut full_params = config.params.clone();
        full_params.insert("out".to_string(), control_bus.raw() as f32);

        tracing::debug!(
            "Creating modulator {:?} (node={:?}) on control bus {} with synthdef '{}', modulations: {:?}",
            id,
            node_id,
            control_bus.raw(),
            synthdef,
            modulations_to_apply
        );

        // Create modulator synth with proper ordering
        self.backend
            .create_synth(&synthdef, node_id, target_node, add_action, &full_params)
            .await
            .map_err(Error::backend)?;

        // Apply modulations: map parameters to control buses from source modulators
        for (param_name, control_bus) in &modulations_to_apply {
            self.backend
                .map_param_to_bus(node_id, param_name, *control_bus)
                .await
                .map_err(Error::backend)?;
            tracing::debug!(
                "Mapped modulator {:?} param '{}' to control bus {}",
                id,
                param_name,
                control_bus
            );
        }

        tracing::info!(
            "Modulator '{}' ({:?}) created on control bus {}{}",
            config.name,
            id,
            control_bus.raw(),
            if modulations_to_apply.is_empty() {
                String::new()
            } else {
                format!(" with {} modulated params", modulations_to_apply.len())
            }
        );

        Ok(())
    }

    async fn delete(&self, id: ModulatorId) -> Result<()> {
        let (node_to_free, control_bus) = {
            let mut state = self.state.write().await;
            let modulator = state
                .modulators
                .remove(&id)
                .ok_or(Error::ModulatorNotFound(id))?;
            state.free_node_id(modulator.synth_node);
            state.control_buses.free(modulator.control_bus);
            (modulator.synth_node, modulator.control_bus)
        };

        // Free the modulator node
        self.backend
            .free_node(node_to_free)
            .await
            .map_err(Error::backend)?;

        tracing::debug!(
            "Deleted modulator {:?} (node={:?}, control_bus={})",
            id,
            node_to_free,
            control_bus.raw()
        );

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
    state.modulators.get(&id).map(|m| m.control_bus.raw())
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

        async fn load_synthdef(
            &self,
            _name: &str,
            _data: &[u8],
        ) -> std::result::Result<(), Self::Error> {
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

        async fn run_node(
            &self,
            _node: NodeId,
            _running: bool,
        ) -> std::result::Result<(), Self::Error> {
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
        ModulatorsHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = ModulatorsHandler::new(backend.clone(), state.clone());
        (handler, backend, state)
    }

    async fn setup_state_with_synthdef(state: &Arc<RwLock<State>>) {
        let mut state_write = state.write().await;
        state_write.synthdefs.insert("lfo_sine".to_string());
        state_write
            .synthdefs
            .insert("envelope_follower".to_string());
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
        assert_eq!(
            backend.groups_created(),
            1,
            "Modulator group should be created"
        );
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
        assert!(
            result.is_err(),
            "Deleting non-existent modulator should fail"
        );
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
        assert!(
            result.is_err(),
            "Setting param on non-existent modulator should fail"
        );
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

        assert!(
            control_bus.is_some(),
            "Should find control bus for existing modulator"
        );
        assert!(
            control_bus.unwrap() >= 1000,
            "Control bus should be >= 1000"
        );
    }

    // =========================================================================
    // Nested Modulation Tests
    // =========================================================================

    #[tokio::test]
    async fn test_nested_modulation_basic() {
        let (handler, backend, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        // Create the source modulator first (rate_mod)
        let rate_mod_id = ModulatorId::new(1);
        let rate_mod_config = ModulatorConfig::new("rate_mod", "lfo_sine")
            .with_param("rate", 0.1)
            .with_param("lo", 0.5)
            .with_param("hi", 8.0);
        handler.create(rate_mod_id, rate_mod_config).await.unwrap();

        // Create main modulator that depends on rate_mod
        let main_mod_id = ModulatorId::new(2);
        let main_mod_config = ModulatorConfig::new("main_lfo", "lfo_sine")
            .with_param("lo", 200.0)
            .with_param("hi", 2000.0)
            .with_modulation("rate", rate_mod_id);
        let result = handler.create(main_mod_id, main_mod_config).await;

        assert!(
            result.is_ok(),
            "Creating modulator with nested modulation should succeed"
        );
        assert_eq!(backend.synths_created(), 2, "Two synths should be created");

        let state_read = state.read().await;
        let main_mod = state_read.modulators.get(&main_mod_id).unwrap();
        assert_eq!(main_mod.config.modulations.len(), 1);
        assert_eq!(main_mod.config.modulations.get("rate"), Some(&rate_mod_id));
    }

    #[tokio::test]
    async fn test_nested_modulation_source_not_found() {
        let (handler, _, state) = create_handler();
        setup_state_with_synthdef(&state).await;

        // Try to create modulator with dependency on non-existent modulator
        let main_mod_id = ModulatorId::new(1);
        let nonexistent_id = ModulatorId::new(999);
        let main_mod_config =
            ModulatorConfig::new("main_lfo", "lfo_sine").with_modulation("rate", nonexistent_id);

        let result = handler.create(main_mod_id, main_mod_config).await;

        assert!(
            result.is_err(),
            "Should fail when source modulator doesn't exist"
        );
    }

    // =========================================================================
    // Circular Dependency Detection Tests
    // =========================================================================

    #[test]
    fn test_detect_cycle_no_cycle() {
        let mut deps = HashMap::new();
        deps.insert(ModulatorId::new(1), vec![]);
        deps.insert(ModulatorId::new(2), vec![ModulatorId::new(1)]);
        deps.insert(ModulatorId::new(3), vec![ModulatorId::new(2)]);

        let cycle = detect_cycle(&deps);
        assert!(cycle.is_none(), "Should not detect cycle in linear chain");
    }

    #[test]
    fn test_detect_cycle_simple_cycle() {
        let mut deps = HashMap::new();
        deps.insert(ModulatorId::new(1), vec![ModulatorId::new(2)]);
        deps.insert(ModulatorId::new(2), vec![ModulatorId::new(1)]);

        let cycle = detect_cycle(&deps);
        assert!(cycle.is_some(), "Should detect simple A -> B -> A cycle");
    }

    #[test]
    fn test_detect_cycle_self_loop() {
        let mut deps = HashMap::new();
        deps.insert(ModulatorId::new(1), vec![ModulatorId::new(1)]);

        let cycle = detect_cycle(&deps);
        assert!(cycle.is_some(), "Should detect self-loop");
    }

    #[test]
    fn test_detect_cycle_complex_cycle() {
        let mut deps = HashMap::new();
        deps.insert(ModulatorId::new(1), vec![ModulatorId::new(2)]);
        deps.insert(ModulatorId::new(2), vec![ModulatorId::new(3)]);
        deps.insert(ModulatorId::new(3), vec![ModulatorId::new(1)]);

        let cycle = detect_cycle(&deps);
        assert!(cycle.is_some(), "Should detect A -> B -> C -> A cycle");
    }

    #[test]
    fn test_topological_sort_linear() {
        let mut deps = HashMap::new();
        deps.insert(ModulatorId::new(1), vec![]);
        deps.insert(ModulatorId::new(2), vec![ModulatorId::new(1)]);
        deps.insert(ModulatorId::new(3), vec![ModulatorId::new(2)]);

        let result = topological_sort(&deps);
        assert!(result.is_ok(), "Should succeed for acyclic graph");

        let sorted = result.unwrap();
        assert_eq!(sorted.len(), 3);

        // Verify order: dependencies come before dependents
        let pos1 = sorted
            .iter()
            .position(|&id| id == ModulatorId::new(1))
            .unwrap();
        let pos2 = sorted
            .iter()
            .position(|&id| id == ModulatorId::new(2))
            .unwrap();
        let pos3 = sorted
            .iter()
            .position(|&id| id == ModulatorId::new(3))
            .unwrap();
        assert!(pos1 < pos2, "mod1 should come before mod2");
        assert!(pos2 < pos3, "mod2 should come before mod3");
    }

    #[test]
    fn test_topological_sort_diamond() {
        // Diamond: 1 -> 2, 1 -> 3, 2 -> 4, 3 -> 4
        let mut deps = HashMap::new();
        deps.insert(ModulatorId::new(1), vec![]);
        deps.insert(ModulatorId::new(2), vec![ModulatorId::new(1)]);
        deps.insert(ModulatorId::new(3), vec![ModulatorId::new(1)]);
        deps.insert(
            ModulatorId::new(4),
            vec![ModulatorId::new(2), ModulatorId::new(3)],
        );

        let result = topological_sort(&deps);
        assert!(result.is_ok(), "Should succeed for diamond graph");

        let sorted = result.unwrap();
        let pos1 = sorted
            .iter()
            .position(|&id| id == ModulatorId::new(1))
            .unwrap();
        let pos2 = sorted
            .iter()
            .position(|&id| id == ModulatorId::new(2))
            .unwrap();
        let pos3 = sorted
            .iter()
            .position(|&id| id == ModulatorId::new(3))
            .unwrap();
        let pos4 = sorted
            .iter()
            .position(|&id| id == ModulatorId::new(4))
            .unwrap();

        assert!(
            pos1 < pos2 && pos1 < pos3,
            "mod1 should come before mod2 and mod3"
        );
        assert!(
            pos2 < pos4 && pos3 < pos4,
            "mod2 and mod3 should come before mod4"
        );
    }

    #[test]
    fn test_build_dependency_graph() {
        let mut modulators = HashMap::new();
        modulators.insert(
            ModulatorId::new(1),
            ModulatorConfig::new("mod1", "lfo_sine"),
        );
        modulators.insert(
            ModulatorId::new(2),
            ModulatorConfig::new("mod2", "lfo_sine").with_modulation("rate", ModulatorId::new(1)),
        );

        let deps = build_dependency_graph(&modulators);

        assert_eq!(deps.get(&ModulatorId::new(1)).unwrap().len(), 0);
        assert_eq!(deps.get(&ModulatorId::new(2)).unwrap().len(), 1);
        assert!(deps
            .get(&ModulatorId::new(2))
            .unwrap()
            .contains(&ModulatorId::new(1)));
    }
}
