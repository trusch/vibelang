//! SynthDefs handler implementation.

use crate::backend::Backend;
use crate::compat::RwLock;
use crate::state::State;
use crate::synthdefs::generate_builtins;
use crate::traits::SynthDefs;
use crate::{Error, Result, SynthDefRejection};
use async_trait::async_trait;
use std::sync::Arc;

/// Handler for synthdef operations.
pub struct SynthDefsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

pub struct SynthDefBlob {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SynthDefPreflightReport {
    pub loaded: Vec<String>,
    pub rejected: Vec<SynthDefRejection>,
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
        let builtins: Vec<_> = generate_builtins()
            .into_iter()
            .map(|(name, data)| SynthDefBlob { name, data })
            .collect();
        let count = builtins.len();
        let report = self.preflight_load(builtins).await?;

        if !report.rejected.is_empty() {
            return Err(Error::SynthDefPreflightFailed {
                rejected: report.rejected,
            });
        }

        // Single barrier sync for the whole batch: each def's load was already
        // confirmed by its own `/done`, so this only guarantees ordering before
        // the rest of startup runs — replacing the ~N per-def syncs that used to
        // dominate startup wall-clock (~85 ms each × dozens of builtins).
        self.backend.sync().await.map_err(Error::backend)?;

        tracing::info!("Loaded {} built-in synthdefs from vibelang-dsp", count);
        Ok(())
    }

    pub async fn preflight_load(
        &self,
        synthdefs: impl IntoIterator<Item = SynthDefBlob>,
    ) -> Result<SynthDefPreflightReport> {
        let mut report = SynthDefPreflightReport::default();

        for SynthDefBlob { name, data } in synthdefs {
            match self.load_one(&name, &data).await {
                Ok(()) => {
                    tracing::debug!("Loaded synthdef: {}", name);
                    report.loaded.push(name);
                }
                Err(err) => {
                    if let Some(rejection) = err.as_synthdef_rejection() {
                        tracing::warn!(
                            "SynthDef '{}' rejected during preflight: {}",
                            rejection.name,
                            rejection.reason
                        );
                        report.rejected.push(rejection);
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        Ok(report)
    }

    async fn load_one(&self, name: &str, data: &[u8]) -> Result<()> {
        tracing::debug!(
            "SynthDefsHandler: loading synthdef '{}' ({} bytes)",
            name,
            data.len()
        );

        // `load_synthdef` already awaits the server's `/done d_recv` reply, so
        // the def is fully loaded and usable once this returns — no per-def
        // `/sync` round-trip is needed here. Bulk callers (`load_builtins`)
        // issue a single barrier sync after the whole batch instead of paying
        // one round-trip per def; the single-def message path (runtime
        // `SynthDefMessage::Load`) relies on the same `/done` confirmation.
        self.backend
            .load_synthdef(name, data)
            .await
            .map_err(|err| Error::synthdef_load(name, &err))?;

        tracing::debug!("SynthDefsHandler: '{}' loaded, registering in state", name);

        let outputs = vibelang_dsp::get_synthdef_outputs(name);
        let inputs = vibelang_dsp::get_synthdef_inputs(name).unwrap_or_default();

        let input_route_nodes_to_free = {
            let mut state = self.state.write().await;
            state.synthdefs.insert(name.to_string());

            let voice_ids: Vec<_> = state
                .voices
                .iter()
                .filter_map(|(voice_id, voice)| {
                    (voice.config.synthdef == name).then_some(*voice_id)
                })
                .collect();
            if let Some(outputs) = outputs {
                if voice_ids.is_empty() {
                    state.synthdef_outputs.insert(name.to_string(), outputs);
                }
            }
            if voice_ids.is_empty() {
                state.synthdef_inputs.insert(name.to_string(), inputs);
                Vec::new()
            } else {
                // Snapshot the old input set ONCE: the first voice's
                // reconcile overwrites `state.synthdef_inputs[name]`, so a
                // per-voice registry lookup would make every later voice
                // sharing this synthdef compare new-vs-new and skip its own
                // bus/route teardown.
                let old_inputs = state.synthdef_inputs(name);
                let mut current_input_routes = std::mem::take(&mut state.input_routes);
                let mut nodes = Vec::new();
                for voice_id in voice_ids {
                    let reconcile = crate::reload::reconcile_voice_input_ports_from(
                        &mut state,
                        voice_id,
                        &old_inputs,
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
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> SynthDefs for SynthDefsHandler<B> {
    async fn load(&self, name: &str, data: &[u8]) -> Result<()> {
        self.load_one(name, data).await
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
    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    use std::collections::HashMap;
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

    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    struct RejectingBackend {
        rejected: HashMap<String, String>,
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

    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    #[async_trait::async_trait]
    impl Backend for RejectingBackend {
        type Error = crate::backends::ScsynthError;

        async fn load_synthdef(
            &self,
            name: &str,
            _data: &[u8],
        ) -> std::result::Result<(), Self::Error> {
            if let Some(reason) = self.rejected.get(name) {
                return Err(crate::backends::ScsynthError::SynthDefRejected {
                    name: name.to_string(),
                    reason: reason.clone(),
                    missing_ugen: Some("Changed".to_string()),
                });
            }
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

    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    #[tokio::test]
    async fn rejected_synthdef_load_does_not_register_state() {
        let name = "rejected_input_manifest_synth";
        let outputs = vec![OutputPort {
            name: "out".to_string(),
            channels: 1,
            rate: PortRate::Ar,
        }];
        let inputs = vec![InputPort::ar("carrier", 2)];
        vibelang_dsp::register_synthdef_outputs(name.to_string(), outputs);
        vibelang_dsp::register_synthdef_inputs(name.to_string(), inputs);

        let backend = Arc::new(RejectingBackend {
            rejected: HashMap::from([(
                name.to_string(),
                "exception in GraphDef_Recv: UGen 'Changed' not installed.".to_string(),
            )]),
        });
        let state = Arc::new(RwLock::new(State::default()));
        let handler = SynthDefsHandler::new(backend, state.clone());

        let err = handler.load(name, b"fake").await.unwrap_err();
        assert!(matches!(err, Error::SynthDefRejected { .. }));

        let state = state.read().await;
        assert!(!state.synthdefs.contains(name));
        assert!(!state.synthdef_outputs.contains_key(name));
        assert!(!state.synthdef_inputs.contains_key(name));
    }

    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    #[tokio::test]
    async fn preflight_load_aggregates_rejected_synthdefs() {
        let backend = Arc::new(RejectingBackend {
            rejected: HashMap::from([
                (
                    "bad_one".to_string(),
                    "exception in GraphDef_Recv: UGen 'Changed' not installed.".to_string(),
                ),
                (
                    "bad_two".to_string(),
                    "exception in GraphDef_Recv: UGen 'Missing' not installed.".to_string(),
                ),
            ]),
        });
        let state = Arc::new(RwLock::new(State::default()));
        let handler = SynthDefsHandler::new(backend, state.clone());

        let report = handler
            .preflight_load([
                SynthDefBlob {
                    name: "ok".to_string(),
                    data: b"ok".to_vec(),
                },
                SynthDefBlob {
                    name: "bad_one".to_string(),
                    data: b"bad1".to_vec(),
                },
                SynthDefBlob {
                    name: "bad_two".to_string(),
                    data: b"bad2".to_vec(),
                },
            ])
            .await
            .unwrap();

        assert_eq!(report.loaded, vec!["ok"]);
        assert_eq!(
            report
                .rejected
                .iter()
                .map(|rejection| rejection.name.as_str())
                .collect::<Vec<_>>(),
            vec!["bad_one", "bad_two"]
        );

        let state = state.read().await;
        assert!(state.synthdefs.contains("ok"));
        assert!(!state.synthdefs.contains("bad_one"));
        assert!(!state.synthdefs.contains("bad_two"));
    }
}
