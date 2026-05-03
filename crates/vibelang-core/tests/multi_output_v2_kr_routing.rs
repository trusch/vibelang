//! Multi-output v2 Story 9 — integration tests for kr routing + per-port FX.
//!
//! Drives the public seam between
//! [`vibelang_core::handlers::RoutesHandler`] (`finalize` for audio routes +
//! per-port FX chain, `finalize_params` for CV→param /n_map) and
//! [`vibelang_core::reload::reconcile_voice_ports`] (port-set diff + bus
//! free/alloc).
//!
//! Five of the seven Story 9 scenarios live here (the two Rhai-surface error
//! cases land in
//! `crates/vibelang-rhai/tests/multi_output_v2_to_param_rhai.rs`).  Each
//! integration test sets up a `State` directly through the public crate API,
//! then drives the handler against a mock backend that records every
//! `create_synth`, `free_node`, and `map_param_to_bus` call so the test can
//! assert the precise mutation set the runtime would have produced.
//!
//! ## Scenarios — Story 9 ticket bullets ↔ tests in this file
//!
//!   1. **CV-to-param: kr-port voice drives target voice's param** —
//!      [`cv_to_param_kr_drives_target_param_via_n_map`]. Asserts that
//!      `finalize_params` issues `/n_map` on every active synth node of the
//!      target voice, mapping the named param to the source voice's
//!      control bus. That is the unit-level proof of "target audibly
//!      responds": every running synth on the target now reads the param
//!      from the source's kr bus rather than its synthdef default.
//!
//!   2. **Multiple `.to_param` routes from one source** —
//!      [`multiple_to_param_routes_from_one_source`]. One source kr port,
//!      two distinct targets/params. Asserts `finalize_params` emits two
//!      `/n_map` calls and `state.param_routes` carries the fan-out.
//!
//!   7. **Reload preserves kr routes when port rates unchanged** —
//!      [`reload_preserves_kr_param_routes_when_port_rates_unchanged`].
//!      A body-only edit on a synthdef that owns a kr port whose
//!      `Param` route is already installed must keep the route in
//!      `state.param_routes` byte-for-byte; the reconcile diff is
//!      unchanged and `dropped_param_routes` is empty.
//!
//! Tests touch the public crate API only — no `handlers/`, `reload/`, or
//! `runtime/` internal references.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{
    ParamRoute, ParamRouteDiff, RouteMap, RoutesHandler,
};
use vibelang_core::reload::reconcile_voice_ports;
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, BusId, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, VoiceState,
};
use vibelang_dsp::{OutputPort, PortRate};

// =========================================================================
// Mock backend — captures create_synth / free_node / map_param_to_bus so
// tests can assert on the exact mutation set the handler emitted.
// =========================================================================

#[derive(Debug)]
struct MockError;
impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}
impl std::error::Error for MockError {}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CreateCall {
    def: String,
    node: NodeId,
    in_bus: f32,
    out_bus: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct MapCall {
    node: NodeId,
    param: String,
    bus: u32,
}

struct MockBackend {
    creates: AtomicU32,
    frees: AtomicU32,
    create_log: Mutex<Vec<CreateCall>>,
    map_log: Mutex<Vec<MapCall>>,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            creates: AtomicU32::new(0),
            frees: AtomicU32::new(0),
            create_log: Mutex::new(Vec::new()),
            map_log: Mutex::new(Vec::new()),
        })
    }
    fn creates(&self) -> u32 {
        self.creates.load(Ordering::Relaxed)
    }
    fn frees(&self) -> u32 {
        self.frees.load(Ordering::Relaxed)
    }
    #[allow(dead_code)]
    fn create_log(&self) -> Vec<CreateCall> {
        self.create_log.lock().unwrap().clone()
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
        def: &str,
        node: NodeId,
        _target: NodeId,
        _action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.fetch_add(1, Ordering::Relaxed);
        self.create_log.lock().unwrap().push(CreateCall {
            def: def.to_string(),
            node,
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
        self.frees.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn set_param(
        &self,
        _node: NodeId,
        _param: &str,
        _value: f32,
    ) -> Result<(), Self::Error> {
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

    async fn load_buffer(
        &self,
        _id: BufferId,
        _path: &Path,
    ) -> Result<BufferInfo, Self::Error> {
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
// Setup helpers — build a State pre-populated with the voices, groups,
// synthdefs, and bus allocations the tests need.
// =========================================================================

const SRC_SYNTH: &str = "v2_kr_routing_src";
const TGT_SYNTH: &str = "v2_kr_routing_tgt";

/// Insert one voice into `state` with the listed `ports` and an optional
/// pre-populated `active_nodes` set (for the target voice path that
/// `finalize_params` walks). kr ports allocate from the control-bus pool;
/// ar ports allocate from the audio-bus pool. Returns the voice id.
async fn insert_voice(
    state: &Arc<RwLock<State>>,
    voice_id: VoiceId,
    voice_group: GroupId,
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
            config: VoiceConfig::new("v", synthdef, voice_group),
            active_nodes,
            note_nodes: HashMap::new(),
            round_robin_position: 0,
            pending_params: HashMap::new(),
            output_buses,
        },
    );
}

/// Insert a group with a freshly-allocated stereo audio bus and node id.
async fn insert_group(
    state: &Arc<RwLock<State>>,
    group_id: GroupId,
    name: &str,
) -> u32 {
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
    bus.0
}

/// Look up the source voice's port bus value (raw u32). Works for ar and kr
/// ports — both are stored in `output_buses` as `BusId`.
async fn port_bus_raw(state: &Arc<RwLock<State>>, voice: VoiceId, port: &str) -> u32 {
    let s = state.read().await;
    s.voices
        .get(&voice)
        .expect("voice exists")
        .output_buses
        .iter()
        .find(|(n, _)| n == port)
        .map(|(_, b)| b.raw())
        .expect("port bus allocated")
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

// =========================================================================
// (1) CV-to-param: kr-port voice drives target voice's param.
// =========================================================================

#[tokio::test]
async fn cv_to_param_kr_drives_target_param_via_n_map() {
    // Source voice owns one kr port "env". Target voice has two active synth
    // nodes (i.e. the voice is currently sounding two notes). Installing a
    // ParamRoute env → target.cutoff must result in two `/n_map` calls — one
    // per active node — pointing at the source's control bus. That is what
    // makes the target audibly respond: every running synth on the target
    // reads `cutoff` from the source's kr bus instead of its synthdef
    // default.
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let voice_group = GroupId::new(1);
    insert_group(&state, voice_group, "vg").await;

    let src = VoiceId::new(10);
    let tgt = VoiceId::new(11);

    insert_voice(&state, src, voice_group, SRC_SYNTH, &[kr_port("env")], vec![]).await;

    // Two active nodes on the target — `finalize_params` must map the param
    // on each.
    let target_active: Vec<NodeId> = {
        let mut s = state.write().await;
        let n0 = s.alloc_node_id();
        let n1 = s.alloc_node_id();
        vec![n0, n1]
    };
    insert_voice(
        &state,
        tgt,
        voice_group,
        TGT_SYNTH,
        &[ar_port("out", 2)],
        target_active.clone(),
    )
    .await;

    let _env_bus = port_bus_raw(&state, src, "env").await;

    let mut diff = ParamRouteDiff::default();
    diff.additions.push(ParamRoute {
        source_voice: src,
        source_port: "env".to_string(),
        target_voice: tgt,
        target_param: "cutoff".to_string(),
    });

    handler
        .finalize_params(&diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
        .await
        .unwrap();

    // Post-A1.a unification: SET routes go through a `param_kr_modulate_1`
    // summer with scale=1, offset=0, baseline=0 — `/n_map` targets the
    // summer's intermediate kr bus (functionally equivalent to the source
    // bus, since the summer is the identity at default scale/offset).
    let summer_bus = state
        .read()
        .await
        .param_summers
        .get(&(tgt, "cutoff".to_string()))
        .expect("summer registered for (tgt, cutoff)")
        .bus
        .raw();

    let maps = backend.map_log();
    assert_eq!(maps.len(), 2, "one /n_map per active target node");
    let want_a = MapCall {
        node: target_active[0],
        param: "cutoff".to_string(),
        bus: summer_bus,
    };
    let want_b = MapCall {
        node: target_active[1],
        param: "cutoff".to_string(),
        bus: summer_bus,
    };
    assert!(
        maps.contains(&want_a),
        "missing /n_map for first target node — got {:?}",
        maps
    );
    assert!(
        maps.contains(&want_b),
        "missing /n_map for second target node — got {:?}",
        maps
    );

    // Applied baseline recorded in the SET map so voice teardown can find
    // the mapping. (We're testing .to_param semantics here.)
    let baseline = state.read().await.param_routes_set.clone();
    let entries = baseline
        .get(&(src, "env".to_string()))
        .expect("source key recorded");
    assert_eq!(entries.as_slice(), &[(tgt, "cutoff".to_string())]);

    // Post-A1.a: SET routes spawn one `param_kr_modulate_1` summer per
    // (target, param) pair so `.scale/.offset` modifiers can apply
    // uniformly. No audio-mixer synths (those are for Group/Main routes).
    assert_eq!(backend.creates(), 1);
    assert_eq!(backend.frees(), 0);
}

// =========================================================================
// (2) Multiple .to_param routes from one source.
// =========================================================================

#[tokio::test]
async fn multiple_to_param_routes_from_one_source() {
    // One source kr port "env" fans out to two distinct targets/params.
    // After finalize_params: two /n_map calls (one per target's single
    // active node), and `state.param_routes[(src,"env")]` carries both
    // (target, param) pairs.
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let voice_group = GroupId::new(1);
    insert_group(&state, voice_group, "vg").await;

    let src = VoiceId::new(20);
    let tgt_a = VoiceId::new(21);
    let tgt_b = VoiceId::new(22);

    insert_voice(&state, src, voice_group, SRC_SYNTH, &[kr_port("env")], vec![]).await;

    let (node_a, node_b) = {
        let mut s = state.write().await;
        (s.alloc_node_id(), s.alloc_node_id())
    };
    insert_voice(
        &state,
        tgt_a,
        voice_group,
        TGT_SYNTH,
        &[ar_port("out", 2)],
        vec![node_a],
    )
    .await;
    insert_voice(
        &state,
        tgt_b,
        voice_group,
        TGT_SYNTH,
        &[ar_port("out", 2)],
        vec![node_b],
    )
    .await;

    let _env_bus = port_bus_raw(&state, src, "env").await;

    let mut diff = ParamRouteDiff::default();
    diff.additions.push(ParamRoute {
        source_voice: src,
        source_port: "env".to_string(),
        target_voice: tgt_a,
        target_param: "cutoff".to_string(),
    });
    diff.additions.push(ParamRoute {
        source_voice: src,
        source_port: "env".to_string(),
        target_voice: tgt_b,
        target_param: "pitch".to_string(),
    });

    handler
        .finalize_params(&diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
        .await
        .unwrap();

    // Each (target, param) gets its own param_kr_modulate_1 summer (post
    // A1.a unification). `/n_map` points at the per-target summer's bus.
    let s = state.read().await;
    let bus_a = s
        .param_summers
        .get(&(tgt_a, "cutoff".to_string()))
        .expect("summer for (tgt_a, cutoff)")
        .bus
        .raw();
    let bus_b = s
        .param_summers
        .get(&(tgt_b, "pitch".to_string()))
        .expect("summer for (tgt_b, pitch)")
        .bus
        .raw();
    drop(s);

    let maps = backend.map_log();
    assert_eq!(maps.len(), 2, "one /n_map per fan-out target");
    assert!(maps.contains(&MapCall {
        node: node_a,
        param: "cutoff".to_string(),
        bus: bus_a,
    }));
    assert!(maps.contains(&MapCall {
        node: node_b,
        param: "pitch".to_string(),
        bus: bus_b,
    }));

    // Both fan-out pairs recorded under the same source key in the SET map.
    let entries = state
        .read()
        .await
        .param_routes_set
        .get(&(src, "env".to_string()))
        .cloned()
        .expect("source key recorded");
    assert_eq!(entries.len(), 2);
    assert!(entries.contains(&(tgt_a, "cutoff".to_string())));
    assert!(entries.contains(&(tgt_b, "pitch".to_string())));

    // Post-A1.a: one summer per (target, param) — fan-out spawns 2 summers.
    assert_eq!(backend.creates(), 2);
}

// =========================================================================
// (7) Reload preserves kr param routes when port rates unchanged.
// =========================================================================

#[tokio::test]
async fn reload_preserves_kr_param_routes_when_port_rates_unchanged() {
    // A body-only edit on a synthdef that owns a kr port whose `Param` route
    // is already installed must keep the route in `state.param_routes`
    // byte-for-byte: reconcile_voice_ports' diff is unchanged and
    // `dropped_param_routes` is empty.
    let state = Arc::new(RwLock::new(State::default()));

    let voice_group = GroupId::new(1);
    insert_group(&state, voice_group, "vg").await;

    let src = VoiceId::new(70);
    let tgt = VoiceId::new(71);

    let ports = vec![kr_port("env")];
    insert_voice(
        &state,
        src,
        voice_group,
        SRC_SYNTH,
        &ports,
        vec![],
    )
    .await;
    insert_voice(
        &state,
        tgt,
        voice_group,
        TGT_SYNTH,
        &[ar_port("out", 2)],
        vec![],
    )
    .await;

    // Pretend the runtime already finalized a SET Param route on a prior reload.
    {
        let mut s = state.write().await;
        s.param_routes_set.insert(
            (src, "env".to_string()),
            vec![(tgt, "cutoff".to_string())],
        );
    }

    // RouteMap is what ar audio routes live in — kr ports never appear here.
    // The port reconciler still takes a `&mut RouteMap` so we pass an empty
    // one to mirror the runtime's call shape.
    let mut routes = RouteMap::new();

    let outcome = {
        let mut s = state.write().await;
        reconcile_voice_ports(&mut s, src, &ports, &mut routes)
    };

    assert!(
        outcome.diff.is_unchanged(),
        "identical port set → diff unchanged",
    );
    assert!(
        outcome.dropped_routes.is_empty(),
        "no ar routes to drop",
    );
    assert!(
        outcome.dropped_param_routes.is_empty(),
        "kr port unchanged — its Param route must not drop",
    );
    assert!(outcome.rate_changes.is_empty());

    // The kr-side route is still in state.param_routes_set.
    let entries = state
        .read()
        .await
        .param_routes_set
        .get(&(src, "env".to_string()))
        .cloned()
        .expect("param route preserved across body-only reload");
    assert_eq!(entries, vec![(tgt, "cutoff".to_string())]);

    // RouteMap stays empty — kr ports do not populate it.
    assert!(routes.is_empty());
}

