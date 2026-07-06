//! Integration tests for the kr-port → hardware-routed-group rejection.
//!
//! Audio routing through a hw-output group goes via
//! `system_link_audio[_mono]`, which reads the group's audio bus with
//! `In.ar`. Feeding a kr-rate output port into that path produces
//! undefined output (typically silence or stuck DC at the jack — exactly
//! the symptom that the May-4 CV-via-ADAT debug session surfaced with
//! `cv_clock` declaring `.output_kr("out")` in a `group(...).output(N)`
//! mono group).
//!
//! [`RoutesHandler::finalize`] must reject these route additions with a
//! clear `Error::InvalidConfig` so the misuse stops being a silent
//! failure at the jack.
//!
//! Three scenarios:
//!   1. `kr_port_to_hw_routed_group_rejected` — synthdef declares an
//!      `output_kr("out")` port; the voice's group is pinned to a
//!      hardware bus (`output_bus = Some(_)`). `finalize` must return
//!      `Err(InvalidConfig(...))` with a message naming the voice, port,
//!      group, and the suggested fixes.
//!   2. `ar_port_to_hw_routed_group_accepted` — same wiring shape but
//!      the port is ar-rate. `finalize` must succeed and spawn the
//!      mixer.
//!   3. `kr_port_to_param_route_accepted` — kr port routed via
//!      `RouteDest::Param { ... }` (the legitimate kr destination). The
//!      validation must not fire — `finalize` succeeds, no mixer is
//!      spawned (Param routes are handled by `finalize_params`), and the
//!      voice's group having `output_bus = Some(_)` is irrelevant
//!      because the rejection is gated on `RouteDest::Group`.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{RouteDest, RouteMap, RoutesHandler};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, BusId, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, VoiceRole, VoiceState,
};
use vibelang_dsp::{OutputPort, PortRate};

// =========================================================================
// Mock backend — captures create_synth so tests can assert finalize spawned
// (or didn't spawn) the per-port mixer.
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
}

struct MockBackend {
    creates: AtomicU32,
    create_log: Mutex<Vec<CreateCall>>,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            creates: AtomicU32::new(0),
            create_log: Mutex::new(Vec::new()),
        })
    }
    fn creates(&self) -> u32 {
        self.creates.load(Ordering::Relaxed)
    }
    fn create_log(&self) -> Vec<CreateCall> {
        self.create_log.lock().unwrap().clone()
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
        _params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.fetch_add(1, Ordering::Relaxed);
        self.create_log.lock().unwrap().push(CreateCall {
            def: def.to_string(),
            node,
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

/// Insert a group, optionally pinned to a hardware output via
/// `(output_bus, output_channels)`. `None` produces a non-hw group.
async fn insert_group(
    state: &Arc<RwLock<State>>,
    group_id: GroupId,
    name: &str,
    hardware: Option<(u32, u32)>,
) {
    let mut s = state.write().await;
    let node = s.alloc_node_id().unwrap();
    let bus = s.alloc_audio_bus(2).unwrap();
    let (output_bus, output_channels) = match hardware {
        Some((b, ch)) => (Some(b), Some(ch)),
        None => (None, None),
    };
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
            output_bus,
            output_channels,
        },
    );
}

/// Insert one voice. kr ports allocate from the control-bus pool; ar ports
/// allocate from the audio-bus pool. Mirrors the post-VoicesHandler shape.
async fn insert_voice(
    state: &Arc<RwLock<State>>,
    voice_id: VoiceId,
    voice_group: GroupId,
    voice_name: &str,
    synthdef: &str,
    ports: &[OutputPort],
) {
    let mut s = state.write().await;
    s.synthdefs.insert(synthdef.to_string());
    s.synthdef_outputs
        .insert(synthdef.to_string(), ports.to_vec());

    let mut output_buses = Vec::with_capacity(ports.len());
    for p in ports {
        let bus = match p.rate {
            PortRate::Ar => s.alloc_audio_bus(p.channels).unwrap(),
            PortRate::Kr | PortRate::Tr => BusId::new(s.alloc_control_bus().unwrap().raw()),
        };
        output_buses.push((p.name.clone(), bus));
    }

    s.voices.insert(
        voice_id,
        VoiceState {
            id: voice_id,
            config: VoiceConfig::new(voice_name, synthdef, voice_group),
            role: VoiceRole::Audible,
            active_nodes: Vec::new(),
            note_nodes: std::collections::HashMap::new(),
            round_robin_position: 0,
            pending_params: std::collections::HashMap::new(),
            output_buses,
            input_buses: Vec::new(),
        },
    );
}

/// Build a default-routes-equivalent route map for one (voice, port) → dest.
fn one_route(voice: VoiceId, port: &str, dest: RouteDest) -> RouteMap {
    let mut m = RouteMap::new();
    m.insert((voice, port.to_string()), vec![dest]);
    m
}

// =========================================================================
// (1) kr-port → hw-routed group is rejected with a clear error.
// =========================================================================

#[tokio::test]
async fn kr_port_to_hw_routed_group_rejected() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));

    // Mono CV group pinned to hardware bus 2 — the cv_clock-on-ES-3 shape.
    let hw_group = GroupId::new(1);
    insert_group(&state, hw_group, "cv1", Some((2, 1))).await;

    // Voice declares a single kr output port — like cv_clock.
    let voice = VoiceId::new(101);
    insert_voice(
        &state,
        voice,
        hw_group,
        "cv_clock_inst",
        "cv_clock_test",
        &[kr_port("out")],
    )
    .await;

    let handler = RoutesHandler::new(backend.clone(), state.clone());
    let new_routes = one_route(voice, "out", RouteDest::Group(hw_group));
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &new_routes);

    let err = handler
        .finalize(&diff)
        .await
        .expect_err("kr-port → hw-routed-group must be rejected");
    let msg = err.to_string();

    // The message must name the voice, the port, the group, and point at
    // the legit alternatives.
    assert!(
        msg.contains("cv_clock_inst"),
        "error names the voice — got: {msg}"
    );
    assert!(msg.contains("'out'"), "error names the port — got: {msg}");
    assert!(msg.contains("kr-rate"), "error names the rate — got: {msg}");
    assert!(msg.contains("'cv1'"), "error names the group — got: {msg}");
    assert!(
        msg.contains("`.output(...)`"),
        "error suggests ar-port fix — got: {msg}"
    );
    assert!(
        msg.contains(".to_param"),
        "error suggests .to_param fix — got: {msg}"
    );

    // No mixer was spawned — finalize bailed before the addition loop.
    assert_eq!(
        backend.creates(),
        0,
        "no synth must be spawned when validation fails"
    );

    // No route_synths entry was inserted either.
    let s = state.read().await;
    assert!(
        s.route_synths.is_empty(),
        "route_synths must stay empty when finalize bails"
    );
}

// =========================================================================
// (2) ar-port → hw-routed group is accepted (same wiring, ar instead of kr).
// =========================================================================

#[tokio::test]
async fn ar_port_to_hw_routed_group_accepted() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));

    let hw_group = GroupId::new(1);
    insert_group(&state, hw_group, "cv1", Some((2, 1))).await;

    let voice = VoiceId::new(102);
    insert_voice(
        &state,
        voice,
        hw_group,
        "lead",
        "lead_synth_test",
        &[ar_port("out", 1)],
    )
    .await;

    let handler = RoutesHandler::new(backend.clone(), state.clone());
    let new_routes = one_route(voice, "out", RouteDest::Group(hw_group));
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &new_routes);

    handler
        .finalize(&diff)
        .await
        .expect("ar-port → hw-routed-group is the legit case and must succeed");

    // The mono port mixer was spawned — wiring is intact.
    let creates = backend.create_log();
    assert!(
        creates.iter().any(|c| c.def == "port_to_group_link_1"),
        "mono port mixer must be spawned for the ar route — got: {creates:?}"
    );
}

// =========================================================================
// (3) kr-port → Param route is accepted (the legit kr destination).
// =========================================================================

#[tokio::test]
async fn kr_port_to_param_route_accepted() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));

    // Voice group is hw-routed — irrelevant to Param routes (they bypass
    // RouteDest::Group entirely), so the rejection must not fire.
    let hw_group = GroupId::new(1);
    insert_group(&state, hw_group, "cv1", Some((2, 1))).await;

    let src = VoiceId::new(201);
    insert_voice(
        &state,
        src,
        hw_group,
        "modulator",
        "kr_src_test",
        &[kr_port("env")],
    )
    .await;

    let tgt = VoiceId::new(202);
    insert_voice(
        &state,
        tgt,
        hw_group,
        "target",
        "kr_tgt_test",
        &[ar_port("out", 2)],
    )
    .await;

    let handler = RoutesHandler::new(backend.clone(), state.clone());
    let new_routes = one_route(
        src,
        "env",
        RouteDest::Param {
            voice_id: tgt,
            param_name: "cutoff".to_string(),
        },
    );
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &new_routes);

    handler
        .finalize(&diff)
        .await
        .expect("kr → Param is the legit kr destination — must succeed");

    // Param routes are handled by `finalize_params`, not `finalize`. The
    // mixer-synth path short-circuits, so no synth is created here.
    assert_eq!(
        backend.creates(),
        0,
        "Param routes do not spawn mixer synths in finalize"
    );
}

// =========================================================================
// (4) kr-port → non-hw sub-group is also rejected.
// =========================================================================

#[tokio::test]
async fn kr_port_to_non_hw_group_also_rejected() {
    // A non-hw sub-group still feeds an audio path: its mix bus folds into
    // the parent group via `system_link_audio`, which reads with `In.ar`.
    // kr data lands as DC bias / undefined output and leaks into the parent
    // chain — the same class of silent failure as the hw-group case, just
    // one step removed from the jack. The validator must reject it too.
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));

    // Sub-group with NO output_bus — purely an internal mix bus.
    let sub_group = GroupId::new(1);
    insert_group(&state, sub_group, "leads_fx", None).await;

    let voice = VoiceId::new(103);
    insert_voice(
        &state,
        voice,
        sub_group,
        "lfo_inst",
        "cv_lfo_test",
        &[kr_port("out")],
    )
    .await;

    let handler = RoutesHandler::new(backend.clone(), state.clone());
    let new_routes = one_route(voice, "out", RouteDest::Group(sub_group));
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &new_routes);

    let err = handler
        .finalize(&diff)
        .await
        .expect_err("kr-port → non-hw sub-group must also be rejected");
    let msg = err.to_string();

    assert!(
        msg.contains("lfo_inst"),
        "error names the voice — got: {msg}"
    );
    assert!(msg.contains("'out'"), "error names the port — got: {msg}");
    assert!(msg.contains("kr-rate"), "error names the rate — got: {msg}");
    assert!(
        msg.contains("'leads_fx'"),
        "error names the group — got: {msg}"
    );
    assert!(
        msg.contains(".to_param"),
        "error still suggests .to_param fix — got: {msg}"
    );

    // No mixer was spawned and no route_synths entry was inserted.
    assert_eq!(
        backend.creates(),
        0,
        "no synth must be spawned when validation fails"
    );
    let s = state.read().await;
    assert!(
        s.route_synths.is_empty(),
        "route_synths must stay empty when finalize bails"
    );
}
