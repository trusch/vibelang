//! Fades handler implementation.

use crate::backend::Backend;
use crate::state::{ActiveFade, State};
use crate::traits::{FadeConfig, FadeTarget, Fades};
use crate::types::NodeId;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
use crate::compat::RwLock;

/// Handler for fade operations.
pub struct FadesHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
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
        Self { backend, state }
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
                            vec![group.node_id]
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
                        if let Some(pattern) = state.patterns.get_mut(id) {
                            for step in &mut pattern.config.steps {
                                step.params.insert(data.param.clone(), data.value);
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
                        &data.target, &data.param, data.value
                    );
                    completed_indices.push(data.index);
                }

                if !nodes.is_empty() {
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
                        p.config
                            .steps
                            .first()
                            .and_then(|s| s.params.get(&config.param).copied())
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
        state.active_fades.retain(|f| {
            f.config.target != config.target || f.config.param != config.param
        });

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

        state.active_fades.retain(|f| {
            &f.config.target != target || f.config.param != param
        });

        Ok(())
    }
}
