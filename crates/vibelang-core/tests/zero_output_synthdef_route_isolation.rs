//! Integration coverage for zero-output side-effect synthdefs and route
//! finalization abort semantics.
//!
//! These tests use local fixtures instead of the real Spectraphon stdlib files:
//! an analyzer-shaped synthdef declares an audio input, performs buffer-write
//! side effects, and declares `outputs([])`. The route assertions then run
//! through the same public handlers/runtime messages that script reloads use.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing_subscriber::layer::SubscriberExt;
use vibelang_core::handlers::{
    merge_default_routes, InputRouteMap, InputRouteSrc, RouteDest, RouteMap, RoutesHandler,
    VoicesHandler,
};
use vibelang_core::reload::{GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, GroupState, NodeId, ParamMap, ReloadMessage,
    Runtime, State, SynthDefMessage, VoiceConfig, VoiceId, Voices,
};
use vibelang_dsp::{InputPort, OutputPort, PortRate};

#[derive(Debug)]
struct MockError;

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}

impl std::error::Error for MockError {}

#[derive(Debug, Clone)]
struct CreateCall {
    def: String,
    #[allow(dead_code)]
    in_bus: f32,
    out_bus: f32,
}

#[derive(Default)]
struct BackendState {
    creates: AtomicU32,
    frees: AtomicU32,
    create_log: Mutex<Vec<CreateCall>>,
}

#[derive(Clone, Default)]
struct MockBackend {
    state: Arc<BackendState>,
}

impl MockBackend {
    fn new() -> Self {
        Self::default()
    }

    fn creates(&self) -> u32 {
        self.state.creates.load(Ordering::Relaxed)
    }

    fn frees(&self) -> u32 {
        self.state.frees.load(Ordering::Relaxed)
    }

    fn create_log(&self) -> Vec<CreateCall> {
        self.state.create_log.lock().unwrap().clone()
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
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.state.creates.fetch_add(1, Ordering::Relaxed);
        self.state.create_log.lock().unwrap().push(CreateCall {
            def: def.to_string(),
            in_bus: *params.get("in_bus").unwrap_or(&-1.0),
            out_bus: *params.get("out_bus").unwrap_or(&-1.0),
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
        self.state.frees.fetch_add(1, Ordering::Relaxed);
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

#[derive(Default, Clone)]
struct CapturedEvents {
    lines: Arc<Mutex<Vec<String>>>,
}

impl CapturedEvents {
    fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

struct CaptureLayer {
    sink: CapturedEvents,
}

impl<S> tracing_subscriber::Layer<S> for CaptureLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        struct Visitor<'a>(&'a mut String);

        impl tracing::field::Visit for Visitor<'_> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    let _ = std::fmt::Write::write_fmt(self.0, format_args!("{:?}", value));
                }
            }

            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "message" {
                    self.0.push_str(value);
                }
            }
        }

        let mut line = String::new();
        event.record(&mut Visitor(&mut line));
        self.sink.lines.lock().unwrap().push(line);
    }
}

fn install_tracing_capture() -> (CapturedEvents, tracing::dispatcher::DefaultGuard) {
    let captured = CapturedEvents::default();
    let subscriber = tracing_subscriber::registry().with(CaptureLayer {
        sink: captured.clone(),
    });
    let guard = tracing::subscriber::set_default(subscriber);
    (captured, guard)
}

const ANALYZER: &str = "zero_output_analyzer_route_isolation";
const PRODUCER: &str = "audio_producer_route_isolation";
const KR_SOURCE: &str = "kr_source_route_isolation";

fn ar_port(name: &str, channels: u8) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels,
        rate: PortRate::Ar,
    }
}

fn kr_port(name: &str) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels: 1,
        rate: PortRate::Kr,
    }
}

fn input_port(name: &str, channels: u8) -> InputPort {
    InputPort {
        name: name.to_string(),
        channels,
        rate: PortRate::Ar,
    }
}

fn register_analyzer_fixture() {
    vibelang_dsp::set_deploy_callback(|_| Ok(()));
    let mut engine = rhai::Engine::new();
    vibelang_dsp::register_dsp_api(&mut engine);
    let _ = engine
        .eval::<rhai::Dynamic>(
            r#"
            define_synthdef("zero_output_analyzer_route_isolation")
                .input("in", 2)
                .param("bufnum", 0.0)
                .outputs([])
                .body_map(|p| {
                    let buf = local_buf_ir(1, 64);
                    record_buf_ar(p.inputs["in"][0], buf, 0, 1.0, 0.0, 1.0, 1, 1, 0);
                    []
                });

            "#,
        )
        .expect("register analyzer fixture synthdefs");
}

async fn insert_group(state: &Arc<RwLock<State>>, group_id: GroupId, name: &str) -> (NodeId, u32) {
    let mut s = state.write().await;
    let node = s.alloc_node_id();
    let bus = s.alloc_audio_bus(2);
    let bus_id = bus.raw();
    s.groups.insert(
        group_id,
        GroupState {
            id: group_id,
            name: name.to_string(),
            parent: None,
            node_id: node,
            audio_bus: bus,
            link_synth_node_id: None,
            muted: false,
            soloed: false,
            params: ParamMap::new(),
            output_bus: None,
            output_channels: None,
        },
    );
    (node, bus_id)
}

async fn insert_synthdef_manifests(
    state: &Arc<RwLock<State>>,
    name: &str,
    outputs: Vec<OutputPort>,
    inputs: Vec<InputPort>,
) {
    let mut s = state.write().await;
    s.synthdefs.insert(name.to_string());
    s.synthdef_outputs.insert(name.to_string(), outputs);
    if !inputs.is_empty() {
        s.synthdef_inputs.insert(name.to_string(), inputs);
    }
}

fn one_route(voice: VoiceId, port: &str, dest: RouteDest) -> RouteMap {
    HashMap::from([((voice, port.to_string()), vec![dest])])
}

#[tokio::test]
async fn zero_output_analyzer_with_audio_input_allows_explicit_audio_routes() {
    register_analyzer_fixture();

    let analyzer_outputs =
        vibelang_dsp::get_synthdef_outputs(ANALYZER).expect("analyzer output manifest registered");
    let analyzer_inputs =
        vibelang_dsp::get_synthdef_inputs(ANALYZER).expect("analyzer input manifest registered");
    let producer_outputs = vec![ar_port("out", 2)];

    assert!(
        analyzer_outputs.is_empty(),
        "analyzer fixture must not need a fake noop output"
    );
    assert_eq!(analyzer_inputs, vec![input_port("in", 2)]);
    assert_eq!(producer_outputs, vec![ar_port("out", 2)]);

    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let voices = VoicesHandler::new(Arc::new(backend.clone()), state.clone());
    let routes = RoutesHandler::new(Arc::new(backend.clone()), state.clone());

    let rack_group = GroupId::new(1);
    let output_group = GroupId::new(2);
    insert_group(&state, rack_group, "rack").await;
    let (_, output_group_bus) = insert_group(&state, output_group, "output").await;
    insert_synthdef_manifests(&state, ANALYZER, analyzer_outputs, analyzer_inputs).await;
    insert_synthdef_manifests(&state, PRODUCER, producer_outputs, Vec::new()).await;

    let producer = VoiceId::new(10);
    let analyzer = VoiceId::new(11);
    voices
        .create(producer, VoiceConfig::new("producer", PRODUCER, rack_group))
        .await
        .unwrap();
    voices
        .create(analyzer, VoiceConfig::new("analyzer", ANALYZER, rack_group))
        .await
        .unwrap();

    {
        let s = state.read().await;
        let analyzer_state = s.voices.get(&analyzer).expect("analyzer voice");
        assert!(analyzer_state.output_buses.is_empty());
        assert!(
            !s.default_routes
                .keys()
                .any(|(voice_id, _)| *voice_id == analyzer),
            "zero-output analyzer must not install default output routes"
        );
    }

    let user_routes = one_route(producer, "out", RouteDest::Group(output_group));
    let merged = {
        let s = state.read().await;
        merge_default_routes(&user_routes, &s.default_routes)
    };
    assert_eq!(
        merged.get(&(producer, "out".to_string())),
        Some(&vec![RouteDest::Group(output_group)])
    );
    assert!(
        !merged.keys().any(|(voice_id, _)| *voice_id == analyzer),
        "zero-output analyzer must not contribute output routes"
    );

    let output_diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &merged);
    routes.finalize(&output_diff).await.unwrap();

    let input_routes = InputRouteMap::from([(
        (analyzer, "in".to_string()),
        vec![InputRouteSrc::Voice(producer, "out".to_string())],
    )]);
    routes.finalize_input_routes(&input_routes).await.unwrap();

    let creates = backend.create_log();
    assert!(
        creates.iter().any(|call| {
            call.def == "port_to_group_link_2" && call.out_bus == output_group_bus as f32
        }),
        "explicit producer output route must materialize beside analyzer: {creates:?}"
    );
    assert!(
        creates.iter().any(|call| call.def == "input_link_2"),
        "analyzer input route should materialize through input_link_2: {creates:?}"
    );

    let s = state.read().await;
    assert_eq!(s.route_synths.len(), 1);
    assert_eq!(s.input_route_synths.len(), 1);
    assert_eq!(
        s.voices.get(&analyzer).unwrap().input_buses.len(),
        1,
        "input route finalization allocates the analyzer input bus"
    );
}

#[tokio::test]
async fn unrouted_kr_output_does_not_default_route_or_block_valid_audio_route() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let voices = VoicesHandler::new(Arc::new(backend.clone()), state.clone());
    let routes = RoutesHandler::new(Arc::new(backend.clone()), state.clone());

    let rack_group = GroupId::new(1);
    let output_group = GroupId::new(2);
    insert_group(&state, rack_group, "rack").await;
    let (_, output_group_bus) = insert_group(&state, output_group, "output").await;
    insert_synthdef_manifests(&state, KR_SOURCE, vec![kr_port("env")], Vec::new()).await;
    insert_synthdef_manifests(&state, PRODUCER, vec![ar_port("out", 2)], Vec::new()).await;

    let kr_voice = VoiceId::new(20);
    let producer = VoiceId::new(21);
    voices
        .create(kr_voice, VoiceConfig::new("kr_src", KR_SOURCE, rack_group))
        .await
        .unwrap();
    voices
        .create(producer, VoiceConfig::new("producer", PRODUCER, rack_group))
        .await
        .unwrap();

    let user_routes = one_route(producer, "out", RouteDest::Group(output_group));
    let merged = {
        let s = state.read().await;
        assert!(
            !s.default_routes
                .contains_key(&(kr_voice, "env".to_string())),
            "kr outputs must not receive implicit audio defaults"
        );
        merge_default_routes(&user_routes, &s.default_routes)
    };
    assert!(
        !merged.contains_key(&(kr_voice, "env".to_string())),
        "unrouted kr port must stay absent from effective output routes"
    );

    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &merged);
    routes
        .finalize(&diff)
        .await
        .expect("valid audio route must not be blocked by an unrouted kr port");

    let creates = backend.create_log();
    assert_eq!(backend.creates(), 1);
    assert!(
        creates.iter().any(|call| {
            call.def == "port_to_group_link_2" && call.out_bus == output_group_bus as f32
        }),
        "valid explicit audio route should materialize: {creates:?}"
    );
}

async fn load_runtime_synthdef(
    runtime: &mut Runtime<MockBackend>,
    name: &str,
    outputs: Vec<OutputPort>,
) {
    vibelang_dsp::register_synthdef_outputs(name.to_string(), outputs);
    runtime
        .send(
            SynthDefMessage::Load {
                name: name.to_string(),
                data: Vec::new(),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;
}

fn script_with_audio_route(group: GroupId, audio_voice: VoiceId) -> ScriptState {
    let mut state = ScriptState::new();
    state.add_group(
        group,
        GroupConfig {
            name: "rack".to_string(),
            ..Default::default()
        },
    );
    state.add_voice(audio_voice, VoiceConfig::new("producer", PRODUCER, group));
    state.set_route(audio_voice, "out", RouteDest::Group(group));
    state
}

#[tokio::test(flavor = "current_thread")]
async fn route_finalize_error_aborts_reload_without_clean_completion_log() {
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_runtime_synthdef(&mut runtime, PRODUCER, vec![ar_port("out", 2)]).await;
    load_runtime_synthdef(&mut runtime, KR_SOURCE, vec![kr_port("env")]).await;

    let group = GroupId::new(1);
    let audio_voice = VoiceId::new(30);
    runtime
        .send(
            ReloadMessage::Apply {
                state: script_with_audio_route(group, audio_voice),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    let creates_before = backend.create_log();
    assert!(
        creates_before
            .iter()
            .any(|call| call.def == "port_to_group_link_2"),
        "initial valid route should have materialized: {creates_before:?}"
    );

    let (captured, _guard) = install_tracing_capture();
    let mut bad_reload = script_with_audio_route(group, audio_voice);
    let kr_voice = VoiceId::new(31);
    bad_reload.add_voice(kr_voice, VoiceConfig::new("kr_src", KR_SOURCE, group));
    bad_reload.set_route(kr_voice, "env", RouteDest::Group(group));

    runtime
        .send(ReloadMessage::Apply { state: bad_reload }.into())
        .await
        .unwrap();
    runtime.tick().await;

    let lines = captured.lines();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("routes.finalize failed") && line.contains("aborting")),
        "route-finalize validation failure should be logged as abort: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("Reload: complete")),
        "failed route reload must not be reported as clean success: {lines:?}"
    );
    assert_eq!(
        backend.create_log().len(),
        creates_before.len(),
        "bad route reload must not spawn additional route materialization"
    );
    assert_eq!(
        backend.frees(),
        0,
        "previous materialized route should remain live on abort"
    );
}
