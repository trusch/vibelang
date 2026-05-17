use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{InputRouteMap, InputRouteSrc, RoutesHandler, VoicesHandler};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, BusId, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, Voices,
};
use vibelang_dsp::{InputPort, OutputPort, PortRate};

const SOURCE: &str = "spectraphon_chain_source";
const ANALYZER: &str = "spectraphon_chain_mag_writer";
const READER: &str = "spectraphon_chain_mag_reader";

#[derive(Debug)]
struct MockError;

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}

impl std::error::Error for MockError {}

#[derive(Clone, Debug)]
struct CreateCall {
    def: String,
    node: NodeId,
    target: NodeId,
    action: AddAction,
    params: ParamMap,
}

#[derive(Default)]
struct MockBackend {
    creates: Mutex<Vec<CreateCall>>,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn creates(&self) -> Vec<CreateCall> {
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
        def: &str,
        node: NodeId,
        target: NodeId,
        action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.lock().unwrap().push(CreateCall {
            def: def.to_string(),
            node,
            target,
            action,
            params: params.clone(),
        });
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

fn ar_output(name: &str) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels: 1,
        rate: PortRate::Ar,
    }
}

fn ar_input(name: &str) -> InputPort {
    InputPort {
        name: name.to_string(),
        channels: 1,
        rate: PortRate::Ar,
    }
}

fn register_params(name: &str, params: &[(&str, f32)]) {
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

async fn setup_state(state: &Arc<RwLock<State>>) {
    let mut state = state.write().await;
    state.groups.insert(
        GroupId::new(1),
        GroupState {
            id: GroupId::new(1),
            name: "rack".to_string(),
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

    for synthdef in [SOURCE, ANALYZER, READER] {
        state.synthdefs.insert(synthdef.to_string());
    }
    state
        .synthdef_outputs
        .insert(SOURCE.to_string(), vec![ar_output("out")]);
    state
        .synthdef_inputs
        .insert(ANALYZER.to_string(), vec![ar_input("analyze")]);
    state
        .synthdef_outputs
        .insert(ANALYZER.to_string(), Vec::new());
    state
        .synthdef_outputs
        .insert(READER.to_string(), vec![ar_output("out")]);
}

#[tokio::test]
async fn analyzer_input_route_feeds_writer_and_reader_shared_mag_buffer() {
    register_params(SOURCE, &[("out0", 0.0)]);
    register_params(
        ANALYZER,
        &[
            (&vibelang_dsp::builder::input_bus_param_name(0), 0.0),
            ("mag_buf", 0.0),
        ],
    );
    register_params(READER, &[("out0", 0.0), ("mag_buf", 0.0)]);

    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    setup_state(&state).await;

    let voices = VoicesHandler::new(backend.clone(), state.clone());
    let routes = RoutesHandler::new(backend.clone(), state.clone());

    let source = VoiceId::new(1);
    let analyzer = VoiceId::new(2);
    let reader = VoiceId::new(3);
    let mag_buf = 42.0;

    voices
        .create(source, VoiceConfig::new("source", SOURCE, GroupId::new(1)))
        .await
        .unwrap();

    let mut analyzer_config = VoiceConfig::new("analyzer", ANALYZER, GroupId::new(1));
    analyzer_config
        .params
        .insert("mag_buf".to_string(), mag_buf);
    voices.create(analyzer, analyzer_config).await.unwrap();

    let mut reader_config = VoiceConfig::new("reader", READER, GroupId::new(1));
    reader_config.params.insert("mag_buf".to_string(), mag_buf);
    voices.create(reader, reader_config).await.unwrap();

    let source_out_bus = {
        let state = state.read().await;
        state
            .voices
            .get(&source)
            .unwrap()
            .output_buses
            .iter()
            .find(|(name, _)| name == "out")
            .unwrap()
            .1
    };

    let desired = InputRouteMap::from([(
        (analyzer, "analyze".to_string()),
        vec![InputRouteSrc::Voice(source, "out".to_string())],
    )]);
    routes.finalize_input_routes(&desired).await.unwrap();

    let analyzer_input_bus = {
        let state = state.read().await;
        state
            .voices
            .get(&analyzer)
            .unwrap()
            .input_buses
            .iter()
            .find(|(name, _)| name == "analyze")
            .unwrap()
            .1
    };

    let input_link = backend
        .creates()
        .into_iter()
        .find(|call| call.def == "input_link_1")
        .expect("input route should materialize as input_link_1");
    assert_eq!(
        input_link.params.get("in_bus"),
        Some(&(source_out_bus.raw() as f32))
    );
    assert_eq!(
        input_link.params.get("out_bus"),
        Some(&(analyzer_input_bus.raw() as f32))
    );

    voices.trigger(analyzer, &ParamMap::new()).await.unwrap();
    voices.trigger(reader, &ParamMap::new()).await.unwrap();

    let creates = backend.creates();
    let analyzer_create = creates
        .iter()
        .find(|call| call.def == ANALYZER)
        .expect("analyzer synth should be spawned");
    assert_eq!(analyzer_create.target, input_link.node);
    assert_eq!(analyzer_create.action, AddAction::After);
    assert_eq!(
        analyzer_create
            .params
            .get(&vibelang_dsp::builder::input_bus_param_name(0)),
        Some(&(analyzer_input_bus.raw() as f32))
    );
    assert_eq!(analyzer_create.params.get("mag_buf"), Some(&mag_buf));

    let reader_create = creates
        .iter()
        .find(|call| call.def == READER)
        .expect("mag-buffer reader synth should be spawned");
    assert_eq!(reader_create.params.get("mag_buf"), Some(&mag_buf));
}
