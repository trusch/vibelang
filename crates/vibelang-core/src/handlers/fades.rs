//! Fades handler implementation.

use crate::backend::Backend;
use crate::compat::{Duration, Instant, RwLock};
use crate::state::{ActiveFade, State};
use crate::traits::{FadeConfig, FadeTarget, Fades};
use crate::types::NodeId;
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Minimum interval between backend emissions per (target, param) while a
/// fade is in flight. The link synth smooths each step with a ~30 ms Lag
/// (see `vibelang-dsp::system_synthdefs::routing::DECLICK_LAG_S`), so
/// emitting every ~5 ms is audibly identical to emitting at tick rate
/// (500-1000 Hz) at a fraction of the /n_set traffic. The final value of a
/// completed fade is always emitted regardless of this limit.
const MIN_EMIT_INTERVAL: Duration = Duration::from_millis(5);

/// Handler for fade operations.
pub struct FadesHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Last backend emission per (target, param), for rate limiting.
    last_emit: Mutex<HashMap<(FadeTarget, String), Instant>>,
}

/// Collected fade data for processing.
struct FadeData {
    index: usize,
    target: FadeTarget,
    param: String,
    value: f32,
    is_complete: bool,
}

/// Info needed to update a parameter after collecting from state.
struct FadeUpdate {
    nodes: Vec<NodeId>,
    param: String,
    value: f32,
}

impl<B: Backend> FadesHandler<B> {
    /// Create a new fades handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self {
            backend,
            state,
            last_emit: Mutex::new(HashMap::new()),
        }
    }

    /// Decide whether to emit to the backend for this (target, param) now.
    ///
    /// Completed fades always emit (the fade must land exactly on its target
    /// value); in-flight fades emit at most once per [`MIN_EMIT_INTERVAL`].
    /// Records `now` as the last emission time when emitting.
    fn should_emit(
        &self,
        target: &FadeTarget,
        param: &str,
        now: Instant,
        is_complete: bool,
    ) -> bool {
        let mut last_emit = self.last_emit.lock().unwrap();
        let key = (target.clone(), param.to_string());
        if is_complete {
            last_emit.remove(&key);
            return true;
        }
        match last_emit.get(&key) {
            Some(prev) if now.duration_since(*prev) < MIN_EMIT_INTERVAL => false,
            _ => {
                last_emit.insert(key, now);
                true
            }
        }
    }

    /// Process active fades.
    ///
    /// Called by the runtime's tick loop to update fading parameters.
    pub async fn tick(&self) {
        // Phase 1: Collect fade data (read-only from active_fades)
        let fade_data: Vec<FadeData> = {
            let state = self.state.read().await;
            let now = self.backend.current_time();
            let tempo = state.tempo;

            state
                .active_fades
                .iter()
                .enumerate()
                .map(|(i, fade)| FadeData {
                    index: i,
                    target: fade.config.target.clone(),
                    param: fade.config.param.clone(),
                    value: fade.current_value(now, tempo),
                    is_complete: fade.is_complete(now, tempo),
                })
                .collect()
        };

        // Phase 2: Apply updates to state and collect node IDs
        let now = self.backend.current_time();
        let updates = {
            let mut state = self.state.write().await;
            let mut updates = Vec::new();
            let mut completed_indices = Vec::new();

            for data in fade_data {
                // Collect nodes to update and update state
                let nodes = match &data.target {
                    FadeTarget::Group(id) => {
                        if let Some(group) = state.groups.get_mut(id) {
                            group.params.insert(data.param.clone(), data.value);
                            // amp/pan flow through the link synth (same
                            // dispatch as GroupsHandler::set_param): fading
                            // the raw group node would broadcast /n_set to
                            // every child — gain applied twice (link amp ×
                            // per-note amp) and each note's velocity-derived
                            // amp permanently overwritten.
                            let node = if matches!(data.param.as_str(), "amp" | "pan") {
                                group.link_synth_node_id.unwrap_or(group.node_id)
                            } else {
                                group.node_id
                            };
                            vec![node]
                        } else {
                            vec![]
                        }
                    }
                    FadeTarget::Voice(id) => {
                        if let Some(voice) = state.voices.get_mut(id) {
                            voice.config.params.insert(data.param.clone(), data.value);
                            voice.active_nodes.clone()
                        } else {
                            vec![]
                        }
                    }
                    FadeTarget::Effect(id) => {
                        if let Some(effect) = state.effects.get_mut(id) {
                            effect.params.insert(data.param.clone(), data.value);
                            vec![effect.node_id]
                        } else {
                            vec![]
                        }
                    }
                    FadeTarget::Pattern(id) => {
                        // Pattern fades affect FUTURE triggers, not live nodes,
                        // so there is nothing to /n_set here (hence the empty
                        // node list and no emission rate-limiting below).
                        //
                        // The per-tick update is a single float insert into the
                        // pattern's `fade_overlay`; `content` is left pristine.
                        // The pattern trigger path merges this overlay on top of
                        // each step at trigger time, reproducing the old
                        // "value stamped on every step" behaviour without the
                        // ~30k allocations/sec of rebuilding the whole content
                        // Arc every 2 ms. On completion the final value is
                        // flushed into `content` exactly once so post-fade
                        // content is byte-identical to the old implementation
                        // and a subsequent reload diff sees the correct
                        // end-state.
                        if let Some(pattern) = state.patterns.get_mut(id) {
                            if data.is_complete {
                                pattern.write_param_to_all_steps(&data.param, data.value);
                                pattern.fade_overlay.remove(&data.param);
                            } else {
                                pattern.fade_overlay.insert(data.param.clone(), data.value);
                            }
                        }
                        vec![] // No live nodes to update
                    }
                    FadeTarget::Melody(_) => {
                        vec![] // Melody parameters are applied when notes trigger
                    }
                };

                if data.is_complete {
                    tracing::debug!(
                        "Fade completed on {:?}/{}: final value={}",
                        &data.target,
                        &data.param,
                        data.value
                    );
                    completed_indices.push(data.index);
                }

                // Rate-limit backend emission: with the Lag smoothing inside
                // the link synth a fade stays audibly smooth at ~200 Hz, so
                // there is no need to flood /n_set at tick rate.
                if !nodes.is_empty()
                    && self.should_emit(&data.target, &data.param, now, data.is_complete)
                {
                    updates.push(FadeUpdate {
                        nodes,
                        param: data.param,
                        value: data.value,
                    });
                }
            }

            // Remove completed fades (in reverse order to preserve indices)
            for i in completed_indices.into_iter().rev() {
                state.active_fades.remove(i);
            }

            updates
        };

        // Phase 3: Send updates to backend (lock released)
        for update in updates {
            for node_id in update.nodes {
                let _ = self
                    .backend
                    .set_param(node_id, &update.param, update.value)
                    .await;
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Fades for FadesHandler<B> {
    async fn fade(&self, config: FadeConfig) -> Result<()> {
        let mut state = self.state.write().await;

        // Get the starting value
        let start_value = config.from.unwrap_or_else(|| {
            match &config.target {
                FadeTarget::Group(id) => state
                    .groups
                    .get(id)
                    .and_then(|g| g.params.get(&config.param).copied())
                    .unwrap_or(0.0),
                FadeTarget::Voice(id) => state
                    .voices
                    .get(id)
                    .and_then(|v| v.config.params.get(&config.param).copied())
                    .unwrap_or(0.0),
                FadeTarget::Pattern(id) => state
                    .patterns
                    .get(id)
                    .and_then(|p| {
                        // An in-flight fade keeps the live value in the overlay
                        // (content stays pristine), so a fade restarted
                        // mid-flight captures the current interpolated value as
                        // its start — matching the old code where `content`
                        // held that value. With no active overlay, fall back to
                        // the pristine first-step param.
                        p.fade_overlay.get(&config.param).copied().or_else(|| {
                            p.content
                                .steps
                                .first()
                                .and_then(|s| s.params.get(&config.param).copied())
                        })
                    })
                    .unwrap_or(0.0),
                FadeTarget::Melody(_) => 0.0, // Melodies don't have persistent params
                FadeTarget::Effect(id) => state
                    .effects
                    .get(id)
                    .and_then(|e| e.params.get(&config.param).copied())
                    .unwrap_or(0.0),
            }
        });

        // Cancel any existing fade on the same target/param
        state
            .active_fades
            .retain(|f| f.config.target != config.target || f.config.param != config.param);

        // Reset the emission rate limiter so the new fade's first tick emits
        // immediately.
        self.last_emit
            .lock()
            .unwrap()
            .remove(&(config.target.clone(), config.param.clone()));

        // Add the new fade
        state.active_fades.push(ActiveFade {
            config,
            start_time: self.backend.current_time(),
            start_value,
        });

        Ok(())
    }

    async fn cancel(&self, target: &FadeTarget, param: &str) -> Result<()> {
        let mut state = self.state.write().await;

        state
            .active_fades
            .retain(|f| &f.config.target != target || f.config.param != param);

        // Freeze a cancelled pattern fade at its last overlaid value by baking
        // it into `content` once, then drop the transient overlay entry. This
        // matches the old code, where `content` retained whatever the last
        // fade tick stamped at cancel time; without the flush the value would
        // vanish and future triggers would revert to the pre-fade content.
        if let FadeTarget::Pattern(id) = target {
            if let Some(pattern) = state.patterns.get_mut(id) {
                if let Some(value) = pattern.fade_overlay.remove(param) {
                    pattern.write_param_to_all_steps(param, value);
                }
            }
        }

        self.last_emit
            .lock()
            .unwrap()
            .remove(&(target.clone(), param.to_string()));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{AddAction, BufferInfo};
    use crate::state::GroupState;
    use crate::types::{BufferId, BusId, GroupId, ParamMap};
    use std::path::Path;

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock error")
        }
    }

    impl std::error::Error for MockError {}

    #[derive(Default)]
    struct MockBackend {
        set_param_calls: Mutex<Vec<(NodeId, String, f32)>>,
    }

    impl MockBackend {
        fn set_param_calls(&self) -> Vec<(NodeId, String, f32)> {
            self.set_param_calls.lock().unwrap().clone()
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
            node: NodeId,
            param: &str,
            value: f32,
        ) -> std::result::Result<(), Self::Error> {
            self.set_param_calls
                .lock()
                .unwrap()
                .push((node, param.to_string(), value));
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

    const GROUP_NODE: NodeId = NodeId(10);
    const LINK_NODE: NodeId = NodeId(77);

    /// Build a handler whose state contains one group. `with_link` controls
    /// whether the group's link synth is spawned.
    async fn handler_with_group(
        with_link: bool,
    ) -> (FadesHandler<MockBackend>, Arc<MockBackend>, GroupId) {
        let backend = Arc::new(MockBackend::default());
        let mut state = State::default();
        state.tempo = 120.0;

        let group_id = GroupId::new(1);
        state.groups.insert(
            group_id,
            GroupState {
                id: group_id,
                name: "g".to_string(),
                parent: None,
                node_id: GROUP_NODE,
                audio_bus: BusId::new(16),
                link_synth_node_id: with_link.then_some(LINK_NODE),
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );

        let state = Arc::new(RwLock::new(state));
        let handler = FadesHandler::new(backend.clone(), state);
        (handler, backend, group_id)
    }

    /// An instantly-complete fade (0 beats) emits its final value once.
    #[tokio::test]
    async fn test_group_amp_fade_targets_link_synth() {
        let (handler, backend, group_id) = handler_with_group(true).await;

        handler
            .fade(FadeConfig::group(group_id, "amp", 0.5, 0.0))
            .await
            .unwrap();
        handler.tick().await;

        assert_eq!(
            backend.set_param_calls(),
            vec![(LINK_NODE, "amp".to_string(), 0.5)],
            "group amp fades must dispatch to the link synth, not the group node"
        );
    }

    #[tokio::test]
    async fn test_group_non_amp_fade_targets_group_node() {
        let (handler, backend, group_id) = handler_with_group(true).await;

        handler
            .fade(FadeConfig::group(group_id, "cutoff", 800.0, 0.0))
            .await
            .unwrap();
        handler.tick().await;

        assert_eq!(
            backend.set_param_calls(),
            vec![(GROUP_NODE, "cutoff".to_string(), 800.0)],
            "non-amp/pan group fades keep the group-node broadcast convention"
        );
    }

    #[tokio::test]
    async fn test_group_amp_fade_falls_back_to_group_node_without_link() {
        let (handler, backend, group_id) = handler_with_group(false).await;

        handler
            .fade(FadeConfig::group(group_id, "amp", 0.5, 0.0))
            .await
            .unwrap();
        handler.tick().await;

        assert_eq!(
            backend.set_param_calls(),
            vec![(GROUP_NODE, "amp".to_string(), 0.5)],
            "without a link synth the group node is the only available target"
        );
    }

    #[tokio::test]
    async fn test_fade_emission_is_rate_limited() {
        let (handler, backend, group_id) = handler_with_group(true).await;

        // Long-running fade (100 beats at 120 BPM = 50 s) — never completes
        // during the test, so every emission goes through the rate limiter.
        handler
            .fade(FadeConfig::group(group_id, "amp", 1.0, 100.0))
            .await
            .unwrap();

        // Back-to-back ticks (far less than 5 ms apart): only one emission.
        handler.tick().await;
        handler.tick().await;
        assert_eq!(
            backend.set_param_calls().len(),
            1,
            "second tick within MIN_EMIT_INTERVAL must be suppressed"
        );

        // After the interval passes, the next tick emits again.
        std::thread::sleep(std::time::Duration::from_millis(6));
        handler.tick().await;
        assert_eq!(
            backend.set_param_calls().len(),
            2,
            "tick after MIN_EMIT_INTERVAL must emit"
        );
    }

    #[tokio::test]
    async fn test_completed_fade_emits_final_value_and_is_removed() {
        let (handler, backend, group_id) = handler_with_group(true).await;

        handler
            .fade(FadeConfig::group(group_id, "amp", 0.25, 0.0))
            .await
            .unwrap();
        handler.tick().await;
        // Fade completed on the first tick: exact target value, fade removed.
        assert_eq!(
            backend.set_param_calls(),
            vec![(LINK_NODE, "amp".to_string(), 0.25)]
        );

        handler.tick().await;
        assert_eq!(
            backend.set_param_calls().len(),
            1,
            "completed fades must not keep emitting"
        );
    }
}
