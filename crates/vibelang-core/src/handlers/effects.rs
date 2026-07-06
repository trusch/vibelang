//! Effects handler implementation.
//!
//! Effects process audio passing through a group's audio bus. They use:
//! - `In.ar(bus)` to read from the group's bus
//! - `ReplaceOut(bus, ...)` to write processed audio back to the same bus
//!
//! The synthdef must have `__fx_bus_in` and `__fx_bus_out` parameters,
//! which are automatically added by `define_fx()` in vibelang-dsp.

use crate::backend::{AddAction, Backend};
use crate::compat::{Instant, RwLock};
use crate::state::{EffectState, State};
use crate::traits::Effects;
use crate::types::{EffectId, GroupId, NodeId, ParamMap};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::compat::Duration;

/// Grace period before freeing an effect node (in milliseconds).
/// This allows time for the mix parameter to fade to 0.
const EFFECT_GRACE_PERIOD_MS: u64 = 50;

/// Handler for effect operations.
pub struct EffectsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Effects pending removal after grace period.
    /// Each entry is (node_id, instant when removal was requested).
    pending_frees: Arc<RwLock<Vec<(NodeId, Instant)>>>,
}

impl<B: Backend> EffectsHandler<B> {
    /// Create a new effects handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self {
            backend,
            state,
            pending_frees: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Process pending effect frees.
    ///
    /// Called by the runtime's tick loop to free effects after their grace period.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn tick(&self) {
        let now = Instant::now();
        let grace_period = Duration::from_millis(EFFECT_GRACE_PERIOD_MS);

        // Collect nodes to free
        let nodes_to_free: Vec<NodeId> = {
            let mut pending = self.pending_frees.write().await;
            let mut to_free = Vec::new();
            let mut remaining = Vec::new();

            for (node_id, requested_at) in pending.drain(..) {
                if now.duration_since(requested_at) >= grace_period {
                    to_free.push(node_id);
                } else {
                    remaining.push((node_id, requested_at));
                }
            }

            *pending = remaining;
            to_free
        };

        // Free the nodes
        for node_id in nodes_to_free {
            tracing::debug!("Effect grace period elapsed, freeing node {:?}", node_id);
            let _ = self.backend.free_node(node_id).await;
            self.state.write().await.free_node_id(node_id);
        }
    }

    /// Process pending effect frees (WASM version - immediate free).
    #[cfg(target_arch = "wasm32")]
    pub async fn tick(&self) {
        // On WASM, just free immediately (no reliable timing)
        let nodes_to_free: Vec<NodeId> = {
            let mut pending = self.pending_frees.write().await;
            pending.drain(..).map(|(node_id, _)| node_id).collect()
        };

        for node_id in nodes_to_free {
            let _ = self.backend.free_node(node_id).await;
            self.state.write().await.free_node_id(node_id);
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
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

            // Verify the synthdef exists
            if !state.synthdefs.contains(synthdef) {
                return Err(Error::SynthDefNotFound(synthdef.to_string()));
            }

            let group_state = state
                .groups
                .get(&group)
                .ok_or(Error::GroupNotFound(group))?;
            let group_node_id = group_state.node_id;
            let audio_bus = group_state.audio_bus;
            let link_synth_node_id = group_state.link_synth_node_id;

            let node_id = state.alloc_node_id()?;

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
        // Effects created with define_fx() have hidden __fx_bus_in and __fx_bus_out params.
        // Note: group audio buses are stereo (2-channel). Effect synthdefs must read and
        // write 2 channels via In.ar/ReplaceOut.ar, or the unused channel goes silent.
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
            // Drain any param routes targeting this fx so the route registry
            // doesn't carry stale `Effect(id)` entries through the next
            // reload diff. Mirrors the voice-removal path
            // ([`State::take_voice_param_routes`]) — fx are never sources, so
            // there are no source-side entries to follow up on; the in-place
            // scrub is sufficient.
            state.take_effect_param_routes(id);
            effect.node_id
        };

        // Try to fade out by setting mix=0 (effects may or may not have this param)
        // This is best-effort - if the effect doesn't have a mix param, it's ignored
        tracing::debug!("Effect {:?}: setting mix=0 for fade-out before removal", id);
        let _ = self.backend.set_param(node_to_free, "mix", 0.0).await;

        // Schedule the free after grace period (allows fade-out to complete)
        {
            let mut pending = self.pending_frees.write().await;
            pending.push((node_to_free, Instant::now()));
            tracing::debug!(
                "Effect {:?}: scheduled for removal after {}ms grace period",
                id,
                EFFECT_GRACE_PERIOD_MS
            );
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BufferInfo;
    use crate::compat::Instant;
    use crate::state::GroupState;
    use crate::types::{BufferId, BusId};
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
        nodes_freed: AtomicU32,
        params_set: AtomicU32,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                synths_created: AtomicU32::new(0),
                nodes_freed: AtomicU32::new(0),
                params_set: AtomicU32::new(0),
            }
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
        EffectsHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = EffectsHandler::new(backend.clone(), state.clone());
        (handler, backend, state)
    }

    async fn setup_state_with_group(state: &Arc<RwLock<State>>) {
        let mut state_write = state.write().await;

        // Register the synthdef
        state_write.synthdefs.insert("reverb".to_string());
        state_write.synthdefs.insert("delay".to_string());

        // Create a group
        let group_id = GroupId::new(1);
        state_write.groups.insert(
            group_id,
            GroupState {
                id: group_id,
                name: "TestGroup".to_string(),
                parent: None,
                node_id: NodeId(100),
                audio_bus: BusId(16),
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );
    }

    // =========================================================================
    // Effect Add Tests
    // =========================================================================

    #[tokio::test]
    async fn test_add_effect() {
        let (handler, backend, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        let result = handler
            .add(effect_id, GroupId::new(1), "reverb", &ParamMap::new())
            .await;

        assert!(result.is_ok(), "Adding effect should succeed");
        assert_eq!(backend.synths_created(), 1, "One synth should be created");

        let state_read = state.read().await;
        assert!(state_read.effects.contains_key(&effect_id));
    }

    #[tokio::test]
    async fn test_add_effect_duplicate_fails() {
        let (handler, _, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        handler
            .add(effect_id, GroupId::new(1), "reverb", &ParamMap::new())
            .await
            .unwrap();

        let result = handler
            .add(effect_id, GroupId::new(1), "reverb", &ParamMap::new())
            .await;

        assert!(result.is_err(), "Duplicate effect should fail");
    }

    #[tokio::test]
    async fn test_add_effect_group_not_found() {
        let (handler, _, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        let result = handler
            .add(effect_id, GroupId::new(999), "reverb", &ParamMap::new())
            .await;

        assert!(result.is_err(), "Should fail with non-existent group");
    }

    #[tokio::test]
    async fn test_add_effect_synthdef_not_found() {
        let (handler, _, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        let result = handler
            .add(
                effect_id,
                GroupId::new(1),
                "nonexistent_fx",
                &ParamMap::new(),
            )
            .await;

        assert!(result.is_err(), "Should fail with non-existent synthdef");
    }

    #[tokio::test]
    async fn test_add_effect_with_params() {
        let (handler, backend, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        let mut params = ParamMap::new();
        params.insert("room".to_string(), 0.8);
        params.insert("mix".to_string(), 0.3);

        let result = handler
            .add(effect_id, GroupId::new(1), "reverb", &params)
            .await;

        assert!(result.is_ok());
        assert_eq!(backend.synths_created(), 1);

        let state_read = state.read().await;
        let effect = state_read.effects.get(&effect_id).unwrap();
        assert_eq!(*effect.params.get("room").unwrap(), 0.8);
        assert_eq!(*effect.params.get("mix").unwrap(), 0.3);
    }

    #[tokio::test]
    async fn test_add_multiple_effects_to_group() {
        let (handler, backend, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect1 = EffectId::new(1);
        let effect2 = EffectId::new(2);

        handler
            .add(effect1, GroupId::new(1), "reverb", &ParamMap::new())
            .await
            .unwrap();
        handler
            .add(effect2, GroupId::new(1), "delay", &ParamMap::new())
            .await
            .unwrap();

        assert_eq!(backend.synths_created(), 2, "Two effects should be created");

        let state_read = state.read().await;
        assert!(state_read.effects.contains_key(&effect1));
        assert!(state_read.effects.contains_key(&effect2));
    }

    // =========================================================================
    // Effect Remove Tests
    // =========================================================================

    #[tokio::test]
    async fn test_remove_effect() {
        let (handler, backend, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        handler
            .add(effect_id, GroupId::new(1), "reverb", &ParamMap::new())
            .await
            .unwrap();

        let result = handler.remove(effect_id).await;
        assert!(result.is_ok(), "Removing effect should succeed");

        // Effect removal uses a deferred grace period (50ms) for fade-out.
        // Wait for the grace period to elapse, then tick to process the free.
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        handler.tick().await;

        assert_eq!(
            backend.nodes_freed(),
            1,
            "Node should be freed after grace period"
        );

        let state_read = state.read().await;
        assert!(!state_read.effects.contains_key(&effect_id));
    }

    #[tokio::test]
    async fn test_remove_nonexistent_effect_fails() {
        let (handler, _, state) = create_handler();
        setup_state_with_group(&state).await;

        let result = handler.remove(EffectId::new(999)).await;
        assert!(result.is_err(), "Removing non-existent effect should fail");
    }

    // =========================================================================
    // Effect Set Param Tests
    // =========================================================================

    #[tokio::test]
    async fn test_set_param() {
        let (handler, backend, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        handler
            .add(effect_id, GroupId::new(1), "reverb", &ParamMap::new())
            .await
            .unwrap();

        let result = handler.set_param(effect_id, "room", 0.9).await;
        assert!(result.is_ok(), "Setting param should succeed");
        assert_eq!(backend.params_set(), 1, "Param should be set on backend");

        let state_read = state.read().await;
        let effect = state_read.effects.get(&effect_id).unwrap();
        assert_eq!(*effect.params.get("room").unwrap(), 0.9);
    }

    #[tokio::test]
    async fn test_set_param_nonexistent_effect_fails() {
        let (handler, _, state) = create_handler();
        setup_state_with_group(&state).await;

        let result = handler.set_param(EffectId::new(999), "room", 0.5).await;
        assert!(
            result.is_err(),
            "Setting param on non-existent effect should fail"
        );
    }

    #[tokio::test]
    async fn test_set_multiple_params() {
        let (handler, backend, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        handler
            .add(effect_id, GroupId::new(1), "reverb", &ParamMap::new())
            .await
            .unwrap();

        handler.set_param(effect_id, "room", 0.8).await.unwrap();
        handler.set_param(effect_id, "mix", 0.4).await.unwrap();
        handler.set_param(effect_id, "damp", 0.6).await.unwrap();

        assert_eq!(backend.params_set(), 3, "Three params should be set");

        let state_read = state.read().await;
        let effect = state_read.effects.get(&effect_id).unwrap();
        assert_eq!(*effect.params.get("room").unwrap(), 0.8);
        assert_eq!(*effect.params.get("mix").unwrap(), 0.4);
        assert_eq!(*effect.params.get("damp").unwrap(), 0.6);
    }

    // =========================================================================
    // Effect Placement Tests
    // =========================================================================

    #[tokio::test]
    async fn test_effect_stores_group_audio_bus() {
        let (handler, _, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        handler
            .add(effect_id, GroupId::new(1), "reverb", &ParamMap::new())
            .await
            .unwrap();

        let state_read = state.read().await;
        let effect = state_read.effects.get(&effect_id).unwrap();
        assert_eq!(
            effect.audio_bus,
            BusId(16),
            "Effect should use group's audio bus"
        );
    }

    #[tokio::test]
    async fn test_effect_stores_synthdef_name() {
        let (handler, _, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        handler
            .add(effect_id, GroupId::new(1), "reverb", &ParamMap::new())
            .await
            .unwrap();

        let state_read = state.read().await;
        let effect = state_read.effects.get(&effect_id).unwrap();
        assert_eq!(effect.synthdef, "reverb");
    }

    #[tokio::test]
    async fn test_effect_stores_group_id() {
        let (handler, _, state) = create_handler();
        setup_state_with_group(&state).await;

        let effect_id = EffectId::new(1);
        handler
            .add(effect_id, GroupId::new(1), "reverb", &ParamMap::new())
            .await
            .unwrap();

        let state_read = state.read().await;
        let effect = state_read.effects.get(&effect_id).unwrap();
        assert_eq!(effect.group, GroupId::new(1));
    }
}
