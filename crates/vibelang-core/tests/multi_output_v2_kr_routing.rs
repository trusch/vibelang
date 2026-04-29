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
//!   5. **Per-port FX chain (1 + 3 FX)** —
//!      [`per_port_fx_chain_one_and_three_fx_chain_in_order`]. A 1-FX
//!      route's signal arrives post-FX (`port → fx → bus → link → group`),
//!      and a 3-FX chain's signal flows through every FX in declared
//!      order (`port → fx[0] → bus[0] → fx[1] → bus[1] → fx[2] → bus[2]
//!      → link → group`). Asserts on the spawned synths' `__fx_bus_in`/
//!      `__fx_bus_out` params plus the link mixer's `in_bus`.
//!
//!   6. **Mixed fx_chain + to_param** —
//!      [`to_param_route_with_fx_chain_bypasses_fx_pipeline`]. Documents
//!      the v2 behaviour: FX only applies to ar audio routes; a `Param`
//!      destination bypasses the entire fx_chain pipeline (no FX synth
//!      spawned, no link mixer spawned, no intermediate buses allocated).
//!      v3 may revisit.
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
    ParamRoute, ParamRouteDiff, Route, RouteDest, RouteDiff, RouteMap, RoutesHandler,
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
struct CreateCall {
    def: String,
    node: NodeId,
    in_bus: f32,
    out_bus: f32,
    fx_in: Option<f32>,
    fx_out: Option<f32>,
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
            fx_in: params.get("__fx_bus_in").copied(),
            fx_out: params.get("__fx_bus_out").copied(),
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
            PortRate::Kr => BusId::new(s.alloc_control_bus().raw()),
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

    let env_bus = port_bus_raw(&state, src, "env").await;

    let mut diff = ParamRouteDiff::default();
    diff.additions.push(ParamRoute {
        source_voice: src,
        source_port: "env".to_string(),
        target_voice: tgt,
        target_param: "cutoff".to_string(),
    });

    handler.finalize_params(&diff).await.unwrap();

    let maps = backend.map_log();
    assert_eq!(maps.len(), 2, "one /n_map per active target node");
    let want_a = MapCall {
        node: target_active[0],
        param: "cutoff".to_string(),
        bus: env_bus,
    };
    let want_b = MapCall {
        node: target_active[1],
        param: "cutoff".to_string(),
        bus: env_bus,
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

    // Applied baseline recorded so voice teardown can find the mapping.
    let baseline = state.read().await.param_routes.clone();
    let entries = baseline
        .get(&(src, "env".to_string()))
        .expect("source key recorded");
    assert_eq!(entries.as_slice(), &[(tgt, "cutoff".to_string())]);

    // Param routes do not spawn audio-mixer synths.
    assert_eq!(backend.creates(), 0);
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

    let env_bus = port_bus_raw(&state, src, "env").await;

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

    handler.finalize_params(&diff).await.unwrap();

    let maps = backend.map_log();
    assert_eq!(maps.len(), 2, "one /n_map per fan-out target");
    assert!(maps.contains(&MapCall {
        node: node_a,
        param: "cutoff".to_string(),
        bus: env_bus,
    }));
    assert!(maps.contains(&MapCall {
        node: node_b,
        param: "pitch".to_string(),
        bus: env_bus,
    }));

    // Both fan-out pairs recorded under the same source key.
    let entries = state
        .read()
        .await
        .param_routes
        .get(&(src, "env".to_string()))
        .cloned()
        .expect("source key recorded");
    assert_eq!(entries.len(), 2);
    assert!(entries.contains(&(tgt_a, "cutoff".to_string())));
    assert!(entries.contains(&(tgt_b, "pitch".to_string())));

    assert_eq!(backend.creates(), 0);
}

// =========================================================================
// (5) Per-port FX chain: 1-FX route signal arrives post-FX; 3-FX chain
// signal flows through every FX in declared order.
// =========================================================================

#[tokio::test]
async fn per_port_fx_chain_one_and_three_fx_chain_in_order() {
    // Two voices on the same group:
    //   v1: ar port "even" with 1-FX chain ["reverb"] → group
    //       Wiring: even_bus → reverb → bus[0] → link → group_bus
    //   v2: ar port "sine" with 3-FX chain ["a","b","c"] → group
    //       Wiring: sine_bus → a → bus[0] → b → bus[1] → c → bus[2] →
    //               link → group_bus
    //
    // Asserts on backend.create_log:
    //   - For v1: 1 FX synth + 1 link mixer (2 creates).
    //     FX reads from even_bus, writes to bus[0]; link reads bus[0].
    //   - For v2: 3 FX synths + 1 link mixer (4 creates), spawned in
    //     declared order, with each FX's __fx_bus_out matching the next
    //     FX's __fx_bus_in. Link mixer reads from the last FX's out_bus.
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let voice_group = GroupId::new(1);
    let dest_group = GroupId::new(2);
    insert_group(&state, voice_group, "vg").await;
    let dest_bus = insert_group(&state, dest_group, "dg").await;

    let v1 = VoiceId::new(50);
    let v2 = VoiceId::new(51);

    // v1: stereo ar port "even" with 1-FX chain.
    insert_voice(
        &state,
        v1,
        voice_group,
        "fx_one_synth",
        &[ar_port("even", 2)],
        vec![],
    )
    .await;
    // v2: stereo ar port "sine" with 3-FX chain.
    insert_voice(
        &state,
        v2,
        voice_group,
        "fx_three_synth",
        &[ar_port("sine", 2)],
        vec![],
    )
    .await;

    // Register all FX synthdef names — `spawn_route` validates each entry
    // before allocating any intermediate buses or node IDs.
    {
        let mut s = state.write().await;
        s.synthdefs.insert("reverb".to_string());
        s.synthdefs.insert("fx_a".to_string());
        s.synthdefs.insert("fx_b".to_string());
        s.synthdefs.insert("fx_c".to_string());
    }

    let even_bus = port_bus_raw(&state, v1, "even").await as f32;
    let sine_bus = port_bus_raw(&state, v2, "sine").await as f32;

    let mut diff = RouteDiff::default();
    diff.additions.push(Route {
        voice_id: v1,
        port_name: "even".to_string(),
        dest: RouteDest::Group(dest_group),
        fx_chain: vec!["reverb".to_string()],
    });
    diff.additions.push(Route {
        voice_id: v2,
        port_name: "sine".to_string(),
        dest: RouteDest::Group(dest_group),
        fx_chain: vec!["fx_a".to_string(), "fx_b".to_string(), "fx_c".to_string()],
    });

    handler.finalize(&diff).await.unwrap();

    // Total: 2 (v1) + 4 (v2) = 6 spawns.
    assert_eq!(backend.creates(), 6, "1+1 for v1, 3+1 for v2");
    assert_eq!(backend.frees(), 0, "no removals on pure addition");

    let creates = backend.create_log();

    // ---- v1: 1-FX chain --------------------------------------------------
    let v1_fx: Vec<&CreateCall> = creates.iter().filter(|c| c.def == "reverb").collect();
    assert_eq!(v1_fx.len(), 1, "exactly one reverb spawn");
    let v1_fx = v1_fx[0];
    assert_eq!(
        v1_fx.fx_in,
        Some(even_bus),
        "reverb reads from the port bus directly (chain head reuses port_bus)",
    );
    let v1_inter = v1_fx
        .fx_out
        .expect("FX writes to an intermediate bus");

    // The link mixer for v1 reads from the FX's output bus and writes to
    // the destination group bus.
    let v1_link: Vec<&CreateCall> = creates
        .iter()
        .filter(|c| c.def == "port_to_group_link_2")
        .filter(|c| c.in_bus == v1_inter)
        .collect();
    assert_eq!(v1_link.len(), 1, "exactly one link mixer reads from v1's FX out");
    assert_eq!(v1_link[0].out_bus, dest_bus as f32);

    // ---- v2: 3-FX chain -------------------------------------------------
    // Spawned in declared order: fx_a → fx_b → fx_c, each writing to a fresh
    // intermediate bus and reading from the previous link's output (with
    // fx_a reading directly from sine_bus).
    let v2_creates: Vec<&CreateCall> = creates
        .iter()
        .filter(|c| ["fx_a", "fx_b", "fx_c"].contains(&c.def.as_str()))
        .collect();
    assert_eq!(v2_creates.len(), 3);

    let fx_a = v2_creates
        .iter()
        .find(|c| c.def == "fx_a")
        .expect("fx_a spawned");
    let fx_b = v2_creates
        .iter()
        .find(|c| c.def == "fx_b")
        .expect("fx_b spawned");
    let fx_c = v2_creates
        .iter()
        .find(|c| c.def == "fx_c")
        .expect("fx_c spawned");

    assert_eq!(
        fx_a.fx_in,
        Some(sine_bus),
        "fx_a reads directly from the port bus (chain head)",
    );
    let bus_a = fx_a.fx_out.expect("fx_a writes to bus[0]");
    assert_eq!(
        fx_b.fx_in,
        Some(bus_a),
        "fx_b reads from fx_a's output bus — declared order preserved",
    );
    let bus_b = fx_b.fx_out.expect("fx_b writes to bus[1]");
    assert_eq!(
        fx_c.fx_in,
        Some(bus_b),
        "fx_c reads from fx_b's output bus — declared order preserved",
    );
    let bus_c = fx_c.fx_out.expect("fx_c writes to bus[2]");

    // Spawn order: declared. With FreeListAllocator's monotonic-then-recycle
    // semantics on a fresh state, fx_a's node id < fx_b's < fx_c's.
    assert!(
        fx_a.node.raw() < fx_b.node.raw() && fx_b.node.raw() < fx_c.node.raw(),
        "FX synths spawned in declared order — node ids monotonic",
    );

    // The intermediate buses are pairwise distinct (no aliasing).
    assert_ne!(bus_a, bus_b);
    assert_ne!(bus_b, bus_c);
    assert_ne!(bus_a, bus_c);

    // The link mixer for v2 reads from fx_c's output and writes to dest_bus.
    let v2_link: Vec<&CreateCall> = creates
        .iter()
        .filter(|c| c.def == "port_to_group_link_2")
        .filter(|c| c.in_bus == bus_c)
        .collect();
    assert_eq!(v2_link.len(), 1);
    assert_eq!(v2_link[0].out_bus, dest_bus as f32);

    // route_fx_synths records every FX spawn in order so finalize() can
    // free everything on later removal.
    let s = state.read().await;
    let v1_fx_nodes = s
        .route_fx_synths
        .get(&(v1, "even".to_string()))
        .expect("v1 fx_synths recorded");
    assert_eq!(v1_fx_nodes.len(), 1);
    let v2_fx_nodes = s
        .route_fx_synths
        .get(&(v2, "sine".to_string()))
        .expect("v2 fx_synths recorded");
    assert_eq!(v2_fx_nodes.len(), 3);
    // Same monotonic order as the spawn log.
    assert_eq!(
        v2_fx_nodes,
        &vec![fx_a.node, fx_b.node, fx_c.node],
        "fx_synths recorded in declared order",
    );
}

// =========================================================================
// (6) Mixed: route with both fx_chain AND to_param.
//
// Documented v2 behaviour: FX only applies to ar audio routes; a Param
// destination bypasses the entire fx_chain pipeline. A `Route` whose dest
// is `Param { ... }` and whose `fx_chain` is non-empty must produce no FX
// synth, no link mixer, and allocate no intermediate buses — `finalize_params`
// then handles the param mapping through `/n_map` independently of the
// audio-mixer pipeline.
//
// v3 may revisit (FX on a copy of the kr signal); see the v2 plan §3.5
// and the Story 9 ticket for the rationale.
// =========================================================================

#[tokio::test]
async fn to_param_route_with_fx_chain_bypasses_fx_pipeline() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let voice_group = GroupId::new(1);
    insert_group(&state, voice_group, "vg").await;

    let src = VoiceId::new(60);
    let tgt = VoiceId::new(61);
    insert_voice(&state, src, voice_group, SRC_SYNTH, &[kr_port("env")], vec![]).await;
    let target_node = {
        let mut s = state.write().await;
        s.alloc_node_id()
    };
    insert_voice(
        &state,
        tgt,
        voice_group,
        TGT_SYNTH,
        &[ar_port("out", 2)],
        vec![target_node],
    )
    .await;

    // Snapshot the audio-bus allocator's outstanding count so we can prove
    // the Param route did NOT allocate any intermediate buses.
    let buses_before = state.read().await.audio_buses.allocated_count();

    // Even with a non-empty fx_chain, a Param dest must bypass the audio
    // pipeline. Pretend the user wrote `voice.output("env").fx(["reverb"])
    // .to_param(target, "cutoff")` and the diff machinery somehow surfaced
    // that as a Route with fx_chain — finalize() must skip it cleanly.
    let mut audio_diff = RouteDiff::default();
    audio_diff.additions.push(Route {
        voice_id: src,
        port_name: "env".to_string(),
        dest: RouteDest::Param {
            voice_id: tgt,
            param_name: "cutoff".to_string(),
        },
        fx_chain: vec!["reverb".to_string()],
    });

    handler.finalize(&audio_diff).await.unwrap();

    assert_eq!(
        backend.creates(),
        0,
        "Param dest must spawn no FX synth and no link mixer",
    );
    assert_eq!(backend.frees(), 0);
    assert!(
        state
            .read()
            .await
            .route_fx_synths
            .get(&(src, "env".to_string()))
            .is_none(),
        "no FX synth recorded for a Param-dest route",
    );
    assert!(
        state
            .read()
            .await
            .route_fx_buses
            .get(&(src, "env".to_string()))
            .is_none(),
        "no FX intermediate buses allocated for a Param-dest route",
    );
    assert_eq!(
        state.read().await.audio_buses.allocated_count(),
        buses_before,
        "audio bus allocator outstanding count unchanged — FX bypassed",
    );

    // Param routing still works through finalize_params — independent of
    // the audio-mixer pipeline that just bypassed the chain.
    let env_bus = port_bus_raw(&state, src, "env").await;
    let mut param_diff = ParamRouteDiff::default();
    param_diff.additions.push(ParamRoute {
        source_voice: src,
        source_port: "env".to_string(),
        target_voice: tgt,
        target_param: "cutoff".to_string(),
    });
    handler.finalize_params(&param_diff).await.unwrap();

    let maps = backend.map_log();
    assert_eq!(maps.len(), 1);
    assert_eq!(
        maps[0],
        MapCall {
            node: target_node,
            param: "cutoff".to_string(),
            bus: env_bus,
        },
        "Param route mapped via /n_map even though the audio path bypassed FX",
    );
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

    // Pretend the runtime already finalized a Param route on a prior reload.
    {
        let mut s = state.write().await;
        s.param_routes.insert(
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

    // The kr-side route is still in state.param_routes.
    let entries = state
        .read()
        .await
        .param_routes
        .get(&(src, "env".to_string()))
        .cloned()
        .expect("param route preserved across body-only reload");
    assert_eq!(entries, vec![(tgt, "cutoff".to_string())]);

    // RouteMap stays empty — kr ports do not populate it.
    assert!(routes.is_empty());
}

