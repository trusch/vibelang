//! Integration tests for the mono-hardware-output group form
//! (`group("name").output(N)` in Rhai).
//!
//! Covers the seam shipped across mono-group-A/B/C:
//!
//! * Task A — `system_link_audio_mono` synthdef variant that takes a
//!   stereo group bus and writes the L+R sum (no halving) to a single
//!   hardware output bus.
//! * Task B — `GroupConfig.output_channels: Option<u32>` carries the
//!   channel-count intent through to [`GroupsHandler::finalize`], which
//!   dispatches `Some(1) → "system_link_audio_mono"` vs.
//!   `Some(2) | None → "system_link_audio"`.
//! * Task C — Rhai surface: `group("cv1").output(2)` (int form, mono)
//!   sets `output_bus = Some(2)` + `output_channels = Some(1)`.
//!   `group("g").output([2])` is sugar for the same, and
//!   `group("g").output([4, 5])` keeps the existing stereo behaviour.
//!
//! These tests assert against the steady state — no reload across a
//! mono ↔ stereo flip; that's covered by Task D's reload-diff tests.
//!
//! Three scenarios:
//!   1. `mono_group_writes_to_declared_bus_only` — single mono group,
//!      `output(2)`, mono CV synthdef in it. Finalize spawns
//!      `system_link_audio_mono` with `inbus = group.audio_bus`,
//!      `outbus = 2`. Bus 3 stays free (the mono variant only emits to
//!      the declared bus).
//!   2. `mono_and_stereo_groups_coexist` — both forms in one State;
//!      finalize spawns `system_link_audio_mono` for the mono group
//!      and `system_link_audio` for the stereo group, each writing to
//!      the right hardware bus.
//!   3. `stereo_synth_in_mono_group_sums` — a 2-channel synthdef body
//!      routed via the count-based default into a mono group. The
//!      port-side mixer is the stereo variant
//!      (`port_to_group_link_2`), and the mono group still spawns
//!      `system_link_audio_mono` to do the L+R sum-no-halve at the
//!      group → hardware boundary. Verifies the wiring doesn't reject
//!      the script — the gain-hot caveat itself is covered by Task A's
//!      synthdef test.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{
    merge_default_routes, GroupsHandler, RouteDest, RouteMap, RoutesHandler, VoicesHandler,
};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, Voices,
};
use vibelang_dsp::OutputPort;

// =========================================================================
// Mock backend — captures `create_synth` calls so tests can assert on the
// (def, in_bus, out_bus) tuple set finalize emitted.
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
    #[allow(dead_code)]
    node: NodeId,
    in_bus: f32,
    out_bus: f32,
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
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.fetch_add(1, Ordering::Relaxed);
        // Group link synths use `inbus`/`outbus`; per-port mixer synths
        // use `in_bus`/`out_bus`. Capture either so a single CreateCall
        // shape works for both.
        let in_bus = params
            .get("in_bus")
            .copied()
            .or_else(|| params.get("inbus").copied())
            .unwrap_or(-1.0);
        let out_bus = params
            .get("out_bus")
            .copied()
            .or_else(|| params.get("outbus").copied())
            .unwrap_or(-1.0);
        self.create_log.lock().unwrap().push(CreateCall {
            def: def.to_string(),
            node,
            in_bus,
            out_bus,
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
        rate: vibelang_dsp::PortRate::Ar,
    }
}

/// Insert a group with a freshly-allocated stereo audio bus and node id,
/// optionally pinned to a hardware output via `(output_bus, output_channels)`.
/// Mirrors the post-Rhai-finalize `GroupState` shape Task C produces.
async fn insert_group(
    state: &Arc<RwLock<State>>,
    group_id: GroupId,
    name: &str,
    hardware: Option<(u32, u32)>,
) -> u32 {
    let mut s = state.write().await;
    let node = s.alloc_node_id();
    let bus = s.alloc_audio_bus(2);
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
    bus.0
}

const CV_SYNTH_MONO: &str = "cv_lfo_mono_test";
const STEREO_SYNTH: &str = "stereo_voice_test";

// =========================================================================
// (1) Mono group — single mono CV port, finalize spawns
// `system_link_audio_mono` writing the declared hardware bus.
// =========================================================================

#[tokio::test]
async fn mono_group_writes_to_declared_bus_only() {
    // Mono CV-style group pinned to hardware bus 2 (1-indexed JACK
    // channel 3). Captures the `group("cv1").output(2)` Rhai surface
    // post-Task-C: `output_bus = Some(2)`, `output_channels = Some(1)`.
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));

    let group_id = GroupId::new(1);
    let group_in_bus = insert_group(&state, group_id, "cv1", Some((2, 1))).await;

    // A mono CV synthdef declared into the group — exercises the same
    // shape any stdlib `cv_lfo_*` would produce after voice-create.
    {
        let mut s = state.write().await;
        s.synthdefs.insert(CV_SYNTH_MONO.to_string());
        s.synthdef_outputs
            .insert(CV_SYNTH_MONO.to_string(), vec![ar_port("out", 1)]);
    }

    let voices = VoicesHandler::new(backend.clone(), state.clone());
    let voice = VoiceId::new(101);
    voices
        .create(voice, VoiceConfig::new("v", CV_SYNTH_MONO, group_id))
        .await
        .unwrap();

    // Drive the route side first — the count-based default for the
    // single mono port spawns `port_to_group_link_1` writing into the
    // group's audio bus. This isn't the assertion target, but it
    // matches what the runtime does pre-`GroupsHandler::finalize`.
    let routes = RoutesHandler::new(backend.clone(), state.clone());
    let merged = {
        let s = state.read().await;
        merge_default_routes(&RouteMap::new(), &s.default_routes)
    };
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &merged);
    routes.finalize(&diff).await.unwrap();

    // Now the group → hardware link. This is the assertion under test.
    let groups = GroupsHandler::new(backend.clone(), state.clone());
    groups.finalize().await.unwrap();

    // Filter out the per-port mixer; only the link synth carries the
    // mono dispatch we care about.
    let creates = backend.create_log();
    let link_synths: Vec<&CreateCall> = creates
        .iter()
        .filter(|c| c.def.starts_with("system_link_audio"))
        .collect();
    assert_eq!(link_synths.len(), 1, "exactly one group link synth");
    assert_eq!(
        link_synths[0].def, "system_link_audio_mono",
        "output_channels=Some(1) must dispatch to the mono variant",
    );
    assert_eq!(
        link_synths[0].in_bus, group_in_bus as f32,
        "link reads from the group's stereo mix bus",
    );
    assert_eq!(
        link_synths[0].out_bus, 2.0,
        "link writes to the declared hardware bus only — bus 3 stays free",
    );

    // The full create count: 1 port mixer + 1 link synth.
    assert_eq!(backend.creates(), 2);
}

// =========================================================================
// (2) Mono and stereo groups in one State — each dispatches to the right
// link variant.
// =========================================================================

#[tokio::test]
async fn mono_and_stereo_groups_coexist() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));

    let mono_group = GroupId::new(1);
    let stereo_group = GroupId::new(2);

    let mono_in_bus = insert_group(&state, mono_group, "cv1", Some((2, 1))).await;
    let stereo_in_bus = insert_group(&state, stereo_group, "main", Some((4, 2))).await;

    let groups = GroupsHandler::new(backend.clone(), state.clone());
    groups.finalize().await.unwrap();

    let creates = backend.create_log();
    assert_eq!(creates.len(), 2, "one link synth per group");

    let mono_call = creates
        .iter()
        .find(|c| c.def == "system_link_audio_mono")
        .expect("mono link synth spawned");
    assert_eq!(mono_call.in_bus, mono_in_bus as f32);
    assert_eq!(mono_call.out_bus, 2.0);

    let stereo_call = creates
        .iter()
        .find(|c| c.def == "system_link_audio")
        .expect("stereo link synth spawned");
    assert_eq!(stereo_call.in_bus, stereo_in_bus as f32);
    assert_eq!(
        stereo_call.out_bus, 4.0,
        "stereo group writes to the L of its declared pair (R = L+1 implicit)",
    );

    // No cross-contamination: neither link writes to the other's
    // hardware bus.
    assert_ne!(mono_call.out_bus, stereo_call.out_bus);
}

// =========================================================================
// (3) Stereo synth body in a mono group — wiring doesn't reject; the
// per-port mixer keeps both channels (`port_to_group_link_2`), the group
// link does the L+R sum at the hardware boundary
// (`system_link_audio_mono`).
// =========================================================================

#[tokio::test]
async fn stereo_synth_in_mono_group_sums() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));

    // Mono group pinned to hardware bus 2.
    let group_id = GroupId::new(1);
    let group_in_bus = insert_group(&state, group_id, "cv1", Some((2, 1))).await;

    // Stereo synthdef (one 2-channel port) declared into the mono group —
    // the count-based default routes its single port to the group bus.
    {
        let mut s = state.write().await;
        s.synthdefs.insert(STEREO_SYNTH.to_string());
        s.synthdef_outputs
            .insert(STEREO_SYNTH.to_string(), vec![ar_port("out", 2)]);
    }

    let voices = VoicesHandler::new(backend.clone(), state.clone());
    let voice = VoiceId::new(202);
    voices
        .create(voice, VoiceConfig::new("v", STEREO_SYNTH, group_id))
        .await
        .unwrap();

    // The default route installer must have produced exactly one
    // (voice, "out") → Group(group_id) entry.
    {
        let s = state.read().await;
        assert_eq!(s.default_routes.len(), 1);
        assert_eq!(
            s.default_routes[&(voice, "out".to_string())],
            vec![RouteDest::Group(group_id)],
            "stereo port routes to the mono group at the default-routes layer",
        );
    }

    // Drive the route → group finalize sequence the runtime would.
    let routes = RoutesHandler::new(backend.clone(), state.clone());
    let merged = {
        let s = state.read().await;
        merge_default_routes(&RouteMap::new(), &s.default_routes)
    };
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &merged);
    routes.finalize(&diff).await.unwrap();

    let groups = GroupsHandler::new(backend.clone(), state.clone());
    groups.finalize().await.unwrap();

    // Port-side mixer: `port_to_group_link_2` (still 2 channels at this
    // boundary; the mono fold happens further down).
    let creates = backend.create_log();
    let port_mixer = creates
        .iter()
        .find(|c| c.def.starts_with("port_to_group_link_"))
        .expect("port mixer spawned for the default-routed stereo port");
    assert_eq!(
        port_mixer.def, "port_to_group_link_2",
        "stereo synthdef body keeps its 2-channel mixer — no premature mono fold",
    );
    assert_eq!(
        port_mixer.out_bus, group_in_bus as f32,
        "mixer writes into the mono group's stereo mix bus",
    );

    // Group link: the L+R sum-no-halve to the declared hardware bus.
    let link_synth = creates
        .iter()
        .find(|c| c.def.starts_with("system_link_audio"))
        .expect("group link synth spawned");
    assert_eq!(
        link_synth.def, "system_link_audio_mono",
        "the mono fold lives in the group link variant — Task A's synthdef",
    );
    assert_eq!(link_synth.in_bus, group_in_bus as f32);
    assert_eq!(link_synth.out_bus, 2.0);

    // The whole pipeline succeeded — script wiring accepted, no error.
    // Total creates: 1 port mixer + 1 group link.
    assert_eq!(backend.creates(), 2);
}
