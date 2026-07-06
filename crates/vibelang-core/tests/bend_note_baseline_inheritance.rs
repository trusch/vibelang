use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{
    ParamRoute, ParamRouteDiff, ParamRouteTarget, RoutesHandler, VoicesHandler,
};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, BusId, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, VoiceRole, VoiceState,
};
use vibelang_dsp::{OutputPort, PortRate};

const SOURCE_SYNTH: &str = "bend_note_baseline_lfo";
const TARGET_SYNTH: &str = "bend_note_baseline_voice";

#[derive(Debug)]
struct MockError;

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}

impl std::error::Error for MockError {}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
struct MapCall {
    node: NodeId,
    param: String,
    bus: u32,
}

struct MockBackend {
    creates: Mutex<Vec<CreateCall>>,
    sets: Mutex<Vec<SetCall>>,
    maps: Mutex<Vec<MapCall>>,
    groups: AtomicU32,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            creates: Mutex::new(Vec::new()),
            sets: Mutex::new(Vec::new()),
            maps: Mutex::new(Vec::new()),
            groups: AtomicU32::new(0),
        })
    }

    fn creates(&self) -> Vec<CreateCall> {
        self.creates.lock().unwrap().clone()
    }

    fn sets(&self) -> Vec<SetCall> {
        self.sets.lock().unwrap().clone()
    }

    fn maps(&self) -> Vec<MapCall> {
        self.maps.lock().unwrap().clone()
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
        _target: NodeId,
        _action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.lock().unwrap().push(CreateCall {
            def: def.to_string(),
            node,
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
        self.groups.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn free_node(&self, _node: NodeId) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn set_param(&self, node: NodeId, param: &str, value: f32) -> Result<(), Self::Error> {
        self.sets.lock().unwrap().push(SetCall {
            node,
            param: param.to_string(),
            value,
        });
        Ok(())
    }

    async fn map_param_to_bus(
        &self,
        node: NodeId,
        param: &str,
        bus: u32,
    ) -> Result<(), Self::Error> {
        self.maps.lock().unwrap().push(MapCall {
            node,
            param: param.to_string(),
            bus,
        });
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

async fn insert_group(state: &Arc<RwLock<State>>, group_id: GroupId) {
    let mut s = state.write().await;
    let node = s.alloc_node_id().unwrap();
    let bus = s.alloc_audio_bus(2).unwrap();
    s.groups.insert(
        group_id,
        GroupState {
            id: group_id,
            name: "main".to_string(),
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
}

async fn insert_voice(
    state: &Arc<RwLock<State>>,
    voice_id: VoiceId,
    group_id: GroupId,
    name: &str,
    synthdef: &str,
    ports: &[OutputPort],
    params: ParamMap,
) {
    let mut s = state.write().await;
    s.synthdefs.insert(synthdef.to_string());
    s.synthdef_outputs
        .insert(synthdef.to_string(), ports.to_vec());

    let mut output_buses = Vec::with_capacity(ports.len());
    for port in ports {
        let bus = match port.rate {
            PortRate::Ar => s.alloc_audio_bus(port.channels).unwrap(),
            PortRate::Kr | PortRate::Tr => BusId::new(s.alloc_control_bus().unwrap().raw()),
        };
        output_buses.push((port.name.clone(), bus));
    }

    let mut config = VoiceConfig::new(name, synthdef, group_id);
    config.params = params;

    s.voices.insert(
        voice_id,
        VoiceState {
            id: voice_id,
            config,
            role: VoiceRole::Audible,
            active_nodes: Vec::new(),
            note_nodes: HashMap::new(),
            round_robin_position: 0,
            pending_params: HashMap::new(),
            output_buses,
            input_buses: Vec::new(),
        },
    );
}

#[tokio::test]
async fn bend_routed_note_on_inherits_merged_param_as_summer_baseline() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let routes = RoutesHandler::new(backend.clone(), state.clone());
    let voices = VoicesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    let source = VoiceId::new(10);
    let target = VoiceId::new(20);
    insert_group(&state, group).await;
    insert_voice(
        &state,
        source,
        group,
        "lfo",
        SOURCE_SYNTH,
        &[kr_port("out")],
        ParamMap::new(),
    )
    .await;

    let mut defaults = ParamMap::new();
    defaults.insert("cutoff".to_string(), 700.0);
    insert_voice(
        &state,
        target,
        group,
        "lead",
        TARGET_SYNTH,
        &[ar_port("out", 2)],
        defaults,
    )
    .await;

    let mut bend_diff = ParamRouteDiff::default();
    bend_diff.additions.push(ParamRoute {
        source_voice: source,
        source_port: "out".to_string(),
        target: ParamRouteTarget::Voice(target),
        target_param: "cutoff".to_string(),
    });
    routes
        .finalize_params(
            &ParamRouteDiff::default(),
            &bend_diff,
            &ParamRouteDiff::default(),
        )
        .await
        .unwrap();

    let (summer_node, summer_bus) = {
        let s = state.read().await;
        let summer = s
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
            .expect("BEND route should create a summer");
        (summer.node, summer.bus.raw())
    };

    let mut per_note = ParamMap::new();
    per_note.insert("cutoff".to_string(), 1337.0);
    voices
        .note_on_with_params(target, 60, 0.8, &per_note)
        .await
        .unwrap();

    let creates = backend.creates();
    let note_create = creates
        .iter()
        .find(|call| call.def == TARGET_SYNTH)
        .expect("note_on should create the target synth");
    let note_node = note_create.node;
    assert_eq!(note_create.params.get("cutoff"), Some(&1337.0));

    assert!(
        backend.maps().iter().any(|call| {
            call.node == note_node && call.param == "cutoff" && call.bus == summer_bus
        }),
        "spawned note should inherit the active BEND /n_map"
    );
    assert!(
        backend.sets().iter().any(|call| {
            call.node == summer_node && call.param == "baseline" && call.value == 1337.0
        }),
        "spawned note should forward its merged cutoff value to the BEND summer baseline"
    );
}
