use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::VoicesHandler;
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, Voices,
};
use vibelang_dsp::{OutputPort, PortRate};

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
    creates: Mutex<Vec<ParamMap>>,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn create_params(&self) -> Vec<ParamMap> {
        self.creates.lock().unwrap().clone()
    }
}

#[async_trait]
impl Backend for MockBackend {
    type Error = MockError;

    async fn load_synthdef(&self, _name: &str, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn create_synth(
        &self,
        _def: &str,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.lock().unwrap().push(params.clone());
        Ok(())
    }

    async fn create_group(
        &self,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn free_node(&self, _node: NodeId) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn set_param(&self, _node: NodeId, _param: &str, _value: f32) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn map_param_to_bus(
        &self,
        _node: NodeId,
        _param: &str,
        _bus: u32,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn load_buffer(&self, _id: BufferId, _path: &Path) -> Result<BufferInfo, Self::Error> {
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
    ) -> Result<BufferInfo, Self::Error> {
        Ok(BufferInfo {
            frames,
            channels,
            sample_rate: 0.0,
        })
    }

    async fn write_buffer(&self, _id: BufferId, _path: &Path) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn free_buffer(&self, _id: BufferId) -> Result<(), Self::Error> {
        Ok(())
    }

    fn current_time(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
}

fn register_synthdef_param_defaults(name: &str, params: &[(&str, f32)]) {
    let ir = vibelang_dsp::GraphIR {
        name: name.to_string(),
        constants: Vec::new(),
        params: params
            .iter()
            .enumerate()
            .map(|(index, (name, value))| vibelang_dsp::ParamSpec {
                name: (*name).to_string(),
                default: vec![*value],
                index,
                lag_ms: None,
            })
            .collect(),
        nodes: Vec::new(),
        out_bus: 0,
    };
    vibelang_dsp::register_synthdef_ir(name.to_string(), ir);
}

async fn setup(
    synthdef: &str,
    ports: Vec<OutputPort>,
) -> (
    VoicesHandler<MockBackend>,
    Arc<MockBackend>,
    Arc<RwLock<State>>,
) {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = VoicesHandler::new(backend.clone(), state.clone());

    {
        let mut state = state.write().await;
        state.synthdefs.insert(synthdef.to_string());
        state.synthdef_outputs.insert(synthdef.to_string(), ports);
        state.groups.insert(
            GroupId::new(1),
            GroupState {
                id: GroupId::new(1),
                name: "group".to_string(),
                parent: None,
                node_id: NodeId(100),
                audio_bus: vibelang_core::BusId(16),
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );
    }

    (handler, backend, state)
}

fn ar_port(name: &str) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels: 1,
        rate: PortRate::Ar,
    }
}

#[tokio::test]
async fn multi_output_ports_override_out_params_without_opt_in() {
    let synthdef = "owned_output_bus_params_multi";
    let (handler, backend, state) = setup(synthdef, vec![ar_port("a"), ar_port("b")]).await;
    register_synthdef_param_defaults(synthdef, &[("out0", 0.0), ("out1", 0.0)]);

    let voice_id = VoiceId::new(1);
    handler
        .create(
            voice_id,
            VoiceConfig::new("multi", synthdef, GroupId::new(1)),
        )
        .await
        .unwrap();

    let output_buses = {
        let state = state.read().await;
        state
            .voices
            .get(&voice_id)
            .unwrap()
            .output_buses
            .iter()
            .map(|(_, bus)| bus.raw())
            .collect::<Vec<_>>()
    };
    assert_eq!(output_buses.len(), 2);
    assert!(output_buses.iter().all(|bus| *bus != 0));

    handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

    let creates = backend.create_params();
    assert_eq!(creates.len(), 1);
    assert_eq!(creates[0].get("out0"), Some(&(output_buses[0] as f32)));
    assert_eq!(creates[0].get("out1"), Some(&(output_buses[1] as f32)));
}

#[tokio::test]
async fn user_set_out_param_wins_over_auto_allocation() {
    let synthdef = "owned_output_bus_params_single_out";
    let (handler, backend, _state) = setup(synthdef, vec![ar_port("signal")]).await;
    register_synthdef_param_defaults(synthdef, &[("out", 0.0)]);

    let voice_id = VoiceId::new(1);
    let mut config = VoiceConfig::new("single", synthdef, GroupId::new(1));
    config.params.insert("out".to_string(), 99.0);
    handler.create(voice_id, config).await.unwrap();

    handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

    let creates = backend.create_params();
    assert_eq!(creates.len(), 1);
    assert_eq!(creates[0].get("out"), Some(&99.0));
}
