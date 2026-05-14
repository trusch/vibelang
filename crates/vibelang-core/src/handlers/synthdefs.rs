//! SynthDefs handler implementation.

use crate::backend::Backend;
use crate::compat::RwLock;
use crate::state::State;
use crate::synthdefs::generate_builtins;
use crate::traits::SynthDefs;
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// Handler for synthdef operations.
pub struct SynthDefsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

impl<B: Backend> SynthDefsHandler<B> {
    /// Create a new synthdefs handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }

    /// Load all built-in synthdefs.
    ///
    /// This should be called during runtime initialization to ensure
    /// essential synthdefs are available. Loads synthdefs from vibelang-dsp
    /// including sample voices, SFZ instruments, MIDI output, routing, and recording.
    pub async fn load_builtins(&self) -> Result<()> {
        let builtins = generate_builtins();
        let count = builtins.len();

        for (name, bytes) in builtins {
            self.load(&name, &bytes).await?;
            tracing::debug!("Loaded built-in synthdef: {}", name);
        }

        tracing::info!("Loaded {} built-in synthdefs from vibelang-dsp", count);
        Ok(())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> SynthDefs for SynthDefsHandler<B> {
    async fn load(&self, name: &str, data: &[u8]) -> Result<()> {
        tracing::debug!(
            "SynthDefsHandler: loading synthdef '{}' ({} bytes)",
            name,
            data.len()
        );

        // Load in backend
        self.backend
            .load_synthdef(name, data)
            .await
            .map_err(Error::backend)?;

        tracing::debug!("SynthDefsHandler: sent d_recv for '{}', now syncing", name);

        // Sync with backend to ensure synthdef is loaded before continuing.
        // This prevents race conditions where we try to create synths before
        // the synthdef is available.
        self.backend.sync().await.map_err(Error::backend)?;

        tracing::debug!(
            "SynthDefsHandler: sync completed for '{}', registered in state",
            name
        );

        let outputs = vibelang_dsp::get_synthdef_outputs(name);
        let inputs = vibelang_dsp::get_synthdef_inputs(name).unwrap_or_default();

        let input_route_nodes_to_free = {
            let mut state = self.state.write().await;
            state.synthdefs.insert(name.to_string());
            if let Some(outputs) = outputs {
                state.synthdef_outputs.insert(name.to_string(), outputs);
            }

            let voice_ids: Vec<_> = state
                .voices
                .iter()
                .filter_map(|(voice_id, voice)| {
                    (voice.config.synthdef == name).then_some(*voice_id)
                })
                .collect();
            if voice_ids.is_empty() {
                state.synthdef_inputs.insert(name.to_string(), inputs);
                Vec::new()
            } else {
                let mut current_input_routes = std::mem::take(&mut state.input_routes);
                let mut nodes = Vec::new();
                for voice_id in voice_ids {
                    let reconcile = crate::reload::reconcile_voice_input_ports(
                        &mut state,
                        voice_id,
                        &inputs,
                        &mut current_input_routes,
                    );
                    nodes.extend(reconcile.freed_route_nodes);
                }
                state.input_routes = current_input_routes;
                nodes
            }
        };
        for node_id in input_route_nodes_to_free {
            let _ = self.backend.free_node(node_id).await;
        }

        Ok(())
    }

    async fn is_loaded(&self, name: &str) -> bool {
        self.state.read().await.synthdefs.contains(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{AddAction, Backend, BufferInfo};
    use crate::compat::Instant;
    use crate::types::{BufferId, NodeId, ParamMap};
    use std::path::Path;
    use vibelang_dsp::{InputPort, OutputPort, PortRate};

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock error")
        }
    }

    impl std::error::Error for MockError {}

    struct MockBackend;

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
            _node: NodeId,
            _param: &str,
            _value: f32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn load_buffer(
            &self,
            _id: BufferId,
            _path: &Path,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames: 0,
                channels: 0,
                sample_rate: 0.0,
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
                sample_rate: 0.0,
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

    #[tokio::test]
    async fn input_route_synthdef_load_registers_input_manifest() {
        let backend = Arc::new(MockBackend);
        let state = Arc::new(RwLock::new(State::default()));
        let handler = SynthDefsHandler::new(backend, state.clone());

        let name = "input_manifest_synth";
        let outputs = vec![OutputPort {
            name: "out".to_string(),
            channels: 1,
            rate: PortRate::Ar,
        }];
        let inputs = vec![InputPort::ar("carrier", 2)];
        vibelang_dsp::register_synthdef_outputs(name.to_string(), outputs.clone());
        vibelang_dsp::register_synthdef_inputs(name.to_string(), inputs.clone());

        handler.load(name, b"fake").await.unwrap();

        let state = state.read().await;
        assert_eq!(state.synthdef_outputs(name), outputs);
        assert_eq!(state.synthdef_inputs(name), inputs);
    }
}
