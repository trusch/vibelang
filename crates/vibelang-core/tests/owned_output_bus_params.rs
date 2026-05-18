use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{
    merge_default_routes, InputRouteMap, InputRouteSrc, RouteDest, RouteMap, RoutesHandler,
    VoicesHandler,
};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, Voices,
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

#[derive(Default)]
struct MockBackend {
    creates: Mutex<Vec<CreateCall>>,
    sets: Mutex<Vec<SetCall>>,
}

#[derive(Clone, Debug)]
struct CreateCall {
    def: String,
    node: NodeId,
    params: ParamMap,
}

#[derive(Clone, Debug, PartialEq)]
struct SetCall {
    node: NodeId,
    param: String,
    value: f32,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn create_params(&self) -> Vec<ParamMap> {
        self.creates
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.params.clone())
            .collect()
    }

    fn create_calls(&self) -> Vec<CreateCall> {
        self.creates.lock().unwrap().clone()
    }

    fn set_calls(&self) -> Vec<SetCall> {
        self.sets.lock().unwrap().clone()
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
        self.creates.lock().unwrap().push(CreateCall {
            def: _def.to_string(),
            node: _node,
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
        self.sets.lock().unwrap().push(SetCall {
            node: _node,
            param: _param.to_string(),
            value: _value,
        });
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
        let group_bus = state.alloc_audio_bus(2);
        state.synthdefs.insert(synthdef.to_string());
        state.synthdef_outputs.insert(synthdef.to_string(), ports);
        state.groups.insert(
            GroupId::new(1),
            GroupState {
                id: GroupId::new(1),
                name: "group".to_string(),
                parent: None,
                node_id: NodeId(100),
                audio_bus: group_bus,
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

async fn setup_empty() -> (
    VoicesHandler<MockBackend>,
    RoutesHandler<MockBackend>,
    Arc<MockBackend>,
    Arc<RwLock<State>>,
) {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let voices = VoicesHandler::new(backend.clone(), state.clone());
    let routes = RoutesHandler::new(backend.clone(), state.clone());

    {
        let mut state = state.write().await;
        let group_bus = state.alloc_audio_bus(2);
        state.groups.insert(
            GroupId::new(1),
            GroupState {
                id: GroupId::new(1),
                name: "group".to_string(),
                parent: None,
                node_id: NodeId(100),
                audio_bus: group_bus,
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );
    }

    (voices, routes, backend, state)
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
async fn legacy_implicit_out_writes_to_owned_output_bus() {
    let synthdef = "owned_output_bus_params_legacy_out";
    let (handler, backend, state) = setup(synthdef, vec![ar_port("out")]).await;
    register_synthdef_param_defaults(synthdef, &[("out", 0.0)]);

    let voice_id = VoiceId::new(1);
    handler
        .create(
            voice_id,
            VoiceConfig::new("legacy", synthdef, GroupId::new(1)),
        )
        .await
        .unwrap();

    let output_bus = {
        let state = state.read().await;
        state.voices.get(&voice_id).unwrap().output_buses[0].1.raw()
    };
    let group_bus = {
        let state = state.read().await;
        state.groups.get(&GroupId::new(1)).unwrap().audio_bus.raw()
    };

    handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

    let creates = backend.create_params();
    assert_eq!(creates.len(), 1);
    assert_eq!(creates[0].get("out"), Some(&(output_bus as f32)));
    assert_ne!(
        creates[0].get("out"),
        Some(&(group_bus as f32)),
        "legacy source must not write directly to the group bus"
    );
}

#[tokio::test]
async fn legacy_default_route_mixes_owned_output_bus_once() {
    let synthdef = "owned_output_bus_params_legacy_default_route";
    let (handler, routes, backend, state) = setup_empty().await;
    register_synthdef_param_defaults(synthdef, &[("out", 0.0)]);

    {
        let mut state = state.write().await;
        state.synthdefs.insert(synthdef.to_string());
    }

    let voice_id = VoiceId::new(1);
    handler
        .create(
            voice_id,
            VoiceConfig::new("legacy", synthdef, GroupId::new(1)),
        )
        .await
        .unwrap();

    let output_bus = {
        let state = state.read().await;
        state.voices.get(&voice_id).unwrap().output_buses[0].1.raw()
    };
    let group_bus = {
        let state = state.read().await;
        state.groups.get(&GroupId::new(1)).unwrap().audio_bus.raw()
    };

    handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

    let merged = {
        let state = state.read().await;
        merge_default_routes(&RouteMap::new(), &state.default_routes)
    };
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &merged);
    routes.finalize(&diff).await.unwrap();

    let calls = backend.create_calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].def, synthdef);
    assert_eq!(calls[0].params.get("out"), Some(&(output_bus as f32)));

    let route_calls: Vec<_> = calls
        .iter()
        .filter(|call| call.def == "port_to_group_link_2")
        .collect();
    assert_eq!(
        route_calls.len(),
        1,
        "default output should be one owned-bus mixer, not a doubled direct write"
    );
    assert_eq!(
        route_calls[0].params.get("in_bus"),
        Some(&(output_bus as f32))
    );
    assert_eq!(
        route_calls[0].params.get("out_bus"),
        Some(&(group_bus as f32))
    );
}

#[tokio::test]
async fn muted_legacy_out_does_not_leak_group_bus_audio() {
    let synthdef = "owned_output_bus_params_legacy_muted";
    let (handler, routes, backend, state) = setup_empty().await;
    register_synthdef_param_defaults(synthdef, &[("out", 0.0)]);

    {
        let mut state = state.write().await;
        state.synthdefs.insert(synthdef.to_string());
    }

    let voice_id = VoiceId::new(1);
    handler
        .create(
            voice_id,
            VoiceConfig::new("legacy", synthdef, GroupId::new(1)),
        )
        .await
        .unwrap();

    let output_bus = {
        let state = state.read().await;
        state.voices.get(&voice_id).unwrap().output_buses[0].1.raw()
    };
    let group_bus = {
        let state = state.read().await;
        state.groups.get(&GroupId::new(1)).unwrap().audio_bus.raw()
    };

    handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

    let mut user_routes = RouteMap::new();
    user_routes.insert((voice_id, "out".to_string()), vec![RouteDest::Muted]);
    let merged = {
        let state = state.read().await;
        merge_default_routes(&user_routes, &state.default_routes)
    };
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &merged);
    routes.finalize(&diff).await.unwrap();

    let calls = backend.create_calls();
    assert_eq!(calls.len(), 1, "muted route must not spawn an output mixer");
    assert_eq!(calls[0].def, synthdef);
    assert_eq!(calls[0].params.get("out"), Some(&(output_bus as f32)));
    assert_ne!(
        calls[0].params.get("out"),
        Some(&(group_bus as f32)),
        "muted legacy source must not dry-write into the group bus"
    );
}

#[tokio::test]
async fn named_input_link_reads_legacy_sources_owned_output_bus() {
    let source_synth = "owned_output_bus_params_legacy_named_source";
    let target_synth = "owned_output_bus_params_named_target";
    let (handler, routes, backend, state) = setup_empty().await;
    register_synthdef_param_defaults(source_synth, &[("out", 0.0)]);
    register_synthdef_param_defaults(target_synth, &[]);

    {
        let mut state = state.write().await;
        state.synthdefs.insert(source_synth.to_string());
        state.synthdefs.insert(target_synth.to_string());
        state
            .synthdef_inputs
            .insert(target_synth.to_string(), vec![InputPort::ar("carrier", 2)]);
        state
            .synthdef_outputs
            .insert(target_synth.to_string(), Vec::new());
    }

    let source_id = VoiceId::new(1);
    let target_id = VoiceId::new(2);
    handler
        .create(
            source_id,
            VoiceConfig::new("source", source_synth, GroupId::new(1)),
        )
        .await
        .unwrap();
    handler
        .create(
            target_id,
            VoiceConfig::new("target", target_synth, GroupId::new(1)),
        )
        .await
        .unwrap();

    let source_bus = {
        let state = state.read().await;
        state.voices.get(&source_id).unwrap().output_buses[0]
            .1
            .raw()
    };

    handler.trigger(source_id, &ParamMap::new()).await.unwrap();

    let desired = InputRouteMap::from([(
        (target_id, "carrier".to_string()),
        vec![InputRouteSrc::Voice(source_id, "out".to_string())],
    )]);
    routes.finalize_input_routes(&desired).await.unwrap();

    let calls = backend.create_calls();
    let source_create = calls.iter().find(|call| call.def == source_synth).unwrap();
    let input_link = calls
        .iter()
        .find(|call| call.def == "input_link_2")
        .unwrap();

    assert_eq!(source_create.params.get("out"), Some(&(source_bus as f32)));
    assert_eq!(input_link.params.get("in_bus"), Some(&(source_bus as f32)));
}

#[tokio::test]
async fn live_named_input_legacy_source_uses_owned_bus_without_muted_dry_route() {
    let source_synth = "owned_output_bus_params_live_legacy_named_source";
    let target_synth = "owned_output_bus_params_live_named_target";
    let (handler, routes, backend, state) = setup_empty().await;
    register_synthdef_param_defaults(source_synth, &[("out", 0.0)]);
    register_synthdef_param_defaults(target_synth, &[]);

    {
        let mut state = state.write().await;
        state.synthdefs.insert(source_synth.to_string());
        state.synthdefs.insert(target_synth.to_string());
        state
            .synthdef_inputs
            .insert(target_synth.to_string(), vec![InputPort::ar("carrier", 2)]);
        state
            .synthdef_outputs
            .insert(target_synth.to_string(), Vec::new());
    }

    let source_id = VoiceId::new(1);
    let target_id = VoiceId::new(2);
    handler
        .create(
            source_id,
            VoiceConfig::new("source", source_synth, GroupId::new(1)),
        )
        .await
        .unwrap();
    handler
        .create(
            target_id,
            VoiceConfig::new("target", target_synth, GroupId::new(1)),
        )
        .await
        .unwrap();

    let (source_bus, group_bus) = {
        let state = state.read().await;
        (
            state.voices.get(&source_id).unwrap().output_buses[0]
                .1
                .raw(),
            state.groups.get(&GroupId::new(1)).unwrap().audio_bus.raw(),
        )
    };

    handler.trigger(target_id, &ParamMap::new()).await.unwrap();
    handler.trigger(source_id, &ParamMap::new()).await.unwrap();

    let mut user_routes = RouteMap::new();
    user_routes.insert((source_id, "out".to_string()), vec![RouteDest::Muted]);
    let merged = {
        let state = state.read().await;
        merge_default_routes(&user_routes, &state.default_routes)
    };
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &merged);
    routes.finalize(&diff).await.unwrap();

    let desired = InputRouteMap::from([(
        (target_id, "carrier".to_string()),
        vec![InputRouteSrc::Voice(source_id, "out".to_string())],
    )]);
    routes.finalize_input_routes(&desired).await.unwrap();

    let calls = backend.create_calls();
    let target_create = calls.iter().find(|call| call.def == target_synth).unwrap();
    let source_create = calls.iter().find(|call| call.def == source_synth).unwrap();
    let input_link = calls
        .iter()
        .find(|call| call.def == "input_link_2")
        .unwrap();

    let target_input_bus = {
        let state = state.read().await;
        state.voices.get(&target_id).unwrap().input_buses[0].1.raw()
    };

    assert_eq!(source_create.params.get("out"), Some(&(source_bus as f32)));
    assert_ne!(
        source_create.params.get("out"),
        Some(&(group_bus as f32)),
        "legacy source must not dry-write to its group when its output route is muted"
    );
    assert_eq!(input_link.params.get("in_bus"), Some(&(source_bus as f32)));
    assert_eq!(
        input_link.params.get("out_bus"),
        Some(&(target_input_bus as f32))
    );
    assert_eq!(
        backend.set_calls(),
        vec![SetCall {
            node: target_create.node,
            param: "__in0".to_string(),
            value: target_input_bus as f32,
        }]
    );
    assert!(
        calls.iter().all(|call| call.def != "port_to_group_link_2"),
        "muted source output must not spawn a dry output mixer"
    );
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
