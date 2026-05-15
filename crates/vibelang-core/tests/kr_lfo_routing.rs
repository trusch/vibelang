//! Task D — kr-output LFO routing acceptance tests.
//!
//! The new `cv_lfo_*_kr` stdlib synthdefs (Task D of the Modulator-class
//! voices epic) declare a single `.output_kr("out")` port. They are intended
//! purely as modulation sources, wired into target params via
//! `.to_param` / `.modulate_by`. Two contracts must hold:
//!
//!   1. `kr_lfo_to_param_via_modulate_by` — wiring an `cv_lfo_sine_kr` voice
//!      to a target voice's param via `.modulate_by` (target-first BEND)
//!      installs a working kr-bus → `/n_map` path. No validator error.
//!   2. `kr_lfo_to_audio_group_rejected` — attempting `.to(group)` on the
//!      kr LFO is rejected by Task A's tightened kr-port validator with a
//!      message naming the voice, the port, the kr rate, and the group.
//!
//! Both tests drive the public seam through `RoutesHandler` against a mock
//! backend, mirroring the style of `route_kr_to_hw_group_rejected.rs` and
//! `multi_output_v2_kr_routing.rs`. The synthdef shape is registered to
//! match the actual `cv_lfo_sine_kr.vibe` body (single kr "out" port);
//! that shape is what `vibelang-std/src/lib.rs::cv_lfo_kr_synthdefs_*` test
//! pins for the on-disk file.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{
    ParamRoute, ParamRouteDiff, ParamRouteTarget, RouteDest, RouteMap, RoutesHandler,
};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, BusId, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, VoiceState,
};
use vibelang_dsp::{OutputPort, PortRate};

const KR_LFO_SYNTH: &str = "cv_lfo_sine_kr";
const TGT_SYNTH: &str = "kr_lfo_routing_tgt";

// =========================================================================
// Mock backend — captures `create_synth`, `free_node`, `map_param_to_bus`.
// =========================================================================

#[derive(Debug)]
struct MockError;
impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}
impl std::error::Error for MockError {}

#[derive(Debug, Clone, PartialEq)]
struct MapCall {
    node: NodeId,
    param: String,
    bus: u32,
}

struct MockBackend {
    creates: AtomicU32,
    map_log: Mutex<Vec<MapCall>>,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            creates: AtomicU32::new(0),
            map_log: Mutex::new(Vec::new()),
        })
    }
    fn creates(&self) -> u32 {
        self.creates.load(Ordering::Relaxed)
    }
    fn map_log(&self) -> Vec<MapCall> {
        self.map_log.lock().unwrap().clone()
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
        _params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.fetch_add(1, Ordering::Relaxed);
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
        node: NodeId,
        param: &str,
        bus: u32,
    ) -> Result<(), Self::Error> {
        self.map_log.lock().unwrap().push(MapCall {
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

// =========================================================================
// Helpers
// =========================================================================

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

async fn insert_group(state: &Arc<RwLock<State>>, group_id: GroupId, name: &str) {
    let mut s = state.write().await;
    let node = s.alloc_node_id();
    let bus = s.alloc_audio_bus(2);
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
}

async fn insert_voice(
    state: &Arc<RwLock<State>>,
    voice_id: VoiceId,
    voice_group: GroupId,
    voice_name: &str,
    synthdef: &str,
    ports: &[OutputPort],
    active_nodes: Vec<NodeId>,
) {
    let mut s = state.write().await;
    s.synthdefs.insert(synthdef.to_string());
    s.synthdef_outputs
        .insert(synthdef.to_string(), ports.to_vec());

    let mut output_buses = Vec::with_capacity(ports.len());
    for p in ports {
        let bus = match p.rate {
            PortRate::Ar => s.alloc_audio_bus(p.channels),
            PortRate::Kr | PortRate::Tr => BusId::new(s.alloc_control_bus().raw()),
        };
        output_buses.push((p.name.clone(), bus));
    }

    s.voices.insert(
        voice_id,
        VoiceState {
            id: voice_id,
            config: VoiceConfig::new(voice_name, synthdef, voice_group),
            active_nodes,
            note_nodes: HashMap::new(),
            round_robin_position: 0,
            pending_params: HashMap::new(),
            output_buses,
            input_buses: Vec::new(),
        },
    );
}

fn one_route(voice: VoiceId, port: &str, dest: RouteDest) -> RouteMap {
    let mut m = RouteMap::new();
    m.insert((voice, port.to_string()), vec![dest]);
    m
}

// =========================================================================
// (1) kr LFO routed via .modulate_by (target-first BEND) — kr-bus path
//     installs cleanly, /n_map fires for every active target node, no
//     validator error.
// =========================================================================

#[tokio::test]
async fn kr_lfo_to_param_via_modulate_by() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    insert_group(&state, group, "leads").await;

    // Source: a `cv_lfo_sine_kr` voice. Single kr "out" port matches the
    // shape declared by `stdlib/cv/lfo/cv_lfo_sine_kr.vibe`.
    let lfo = VoiceId::new(101);
    insert_voice(
        &state,
        lfo,
        group,
        "lfo_inst",
        KR_LFO_SYNTH,
        &[kr_port("out")],
        vec![],
    )
    .await;

    // Target: a sounding voice with one active synth node — modulate_by
    // must `/n_map` that node's `cutoff` param to the kr summer's bus.
    let target_node = {
        let mut s = state.write().await;
        s.alloc_node_id()
    };
    let tgt = VoiceId::new(102);
    insert_voice(
        &state,
        tgt,
        group,
        "lead",
        TGT_SYNTH,
        &[ar_port("out", 2)],
        vec![target_node],
    )
    .await;

    // BEND: target-first `.modulate_by(lfo, "out")` for cutoff.
    let mut bend_diff = ParamRouteDiff::default();
    bend_diff.additions.push(ParamRoute {
        source_voice: lfo,
        source_port: "out".to_string(),
        target: ParamRouteTarget::Voice(tgt),
        target_param: "cutoff".to_string(),
    });

    handler
        .finalize_params(
            &ParamRouteDiff::default(),
            &bend_diff,
            &ParamRouteDiff::default(),
        )
        .await
        .expect("kr LFO → param via modulate_by must succeed (legit kr destination)");

    // The summer for `(tgt, "cutoff")` is registered, and the target's
    // active node is /n_map'd to the summer's intermediate bus.
    let s = state.read().await;
    let summer_bus = s
        .param_summers
        .get(&(ParamRouteTarget::Voice(tgt), "cutoff".to_string()))
        .expect("summer registered for (tgt, cutoff) under BEND")
        .bus
        .raw();
    drop(s);

    let maps = backend.map_log();
    assert_eq!(
        maps.len(),
        1,
        "exactly one /n_map for the single active target node — got {:?}",
        maps,
    );
    assert_eq!(
        maps[0],
        MapCall {
            node: target_node,
            param: "cutoff".to_string(),
            bus: summer_bus,
        },
        "/n_map must point at the kr summer's intermediate bus",
    );

    // BEND map is source-keyed: `(source_voice, source_port)` →
    // [(target, target_param), ...].  The route entry must show up under
    // the LFO's `out` port pointing at `(tgt, "cutoff")`.
    let bend_entries = state
        .read()
        .await
        .param_routes_bend
        .get(&(lfo, "out".to_string()))
        .cloned()
        .expect("BEND route recorded under source key");
    assert_eq!(
        bend_entries.as_slice(),
        &[(ParamRouteTarget::Voice(tgt), "cutoff".to_string())],
    );
}

// =========================================================================
// (2) kr LFO routed to an audio group via `.to(group)` — Task A's
//     tightened validator must reject with a clear error naming the
//     voice, port, kr rate, and group.
// =========================================================================

#[tokio::test]
async fn kr_lfo_to_audio_group_rejected() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));

    // Mix-bus group (no hardware pin) — still feeds an audio path through
    // its parent's `system_link_audio`, which reads with `In.ar`. The kr
    // LFO would land as DC bias / undefined output. Task A's validator
    // catches this at script-load time.
    let group = GroupId::new(1);
    insert_group(&state, group, "leads_fx").await;

    let lfo = VoiceId::new(201);
    insert_voice(
        &state,
        lfo,
        group,
        "sine_lfo",
        KR_LFO_SYNTH,
        &[kr_port("out")],
        vec![],
    )
    .await;

    let handler = RoutesHandler::new(backend.clone(), state.clone());
    let new_routes = one_route(lfo, "out", RouteDest::Group(group));
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &new_routes);

    let err = handler
        .finalize(&diff)
        .await
        .expect_err("kr LFO → audio group must be rejected by Task A's validator");
    let msg = err.to_string();

    assert!(
        msg.contains("sine_lfo"),
        "error names the voice — got: {msg}",
    );
    assert!(msg.contains("'out'"), "error names the port — got: {msg}");
    assert!(msg.contains("kr-rate"), "error names the rate — got: {msg}");
    assert!(
        msg.contains("'leads_fx'"),
        "error names the group — got: {msg}",
    );
    assert!(
        msg.contains(".to_param"),
        "error suggests .to_param fix — got: {msg}",
    );

    // No mixer was spawned and no route_synths entry was inserted.
    assert_eq!(
        backend.creates(),
        0,
        "no synth must be spawned when validation fails",
    );
    let s = state.read().await;
    assert!(
        s.route_synths.is_empty(),
        "route_synths must stay empty when finalize bails",
    );
}
