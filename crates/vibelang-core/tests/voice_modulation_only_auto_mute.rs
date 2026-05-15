//! Heuristic auto-mute for modulation-only voices (Story B).
//!
//! Acceptance tests for [`suppress_modulation_only_defaults`]:
//! a voice used purely as a modulation source (its outputs only feed
//! `.to_param` / `.to_param_audio` / `.modulate_by` / `.to_trigger`)
//! should NOT have its count-based default `Group(voice_group)` mix
//! emitted by Phase 4.7. Without the suppression, an LFO voice's raw
//! waveform leaks into its surrounding group's audio bus.
//!
//! Each test runs the same suppress → merge → diff → finalize sequence
//! the runtime executes in `Runtime::apply_reload` Phase 4.7, so the
//! assertion target is the observable backend effect: which (if any)
//! `port_to_group_link_<n>` mixer synths get spawned, against which
//! buses.
//!
//! Three scenarios:
//!
//!   1. `voice_with_only_param_routes_not_mixed` — voice has a param
//!      route but no explicit `RouteDest::{Group,Main,Muted}` user
//!      route. Heuristic fires: NO mixer spawned. INFO log emitted.
//!   2. `voice_with_param_and_audio_routes_still_mixed` — voice has
//!      both `.to(group)` AND `.to_param(...)`. Heuristic does NOT
//!      fire (explicit user route present). One mixer spawned to the
//!      user's chosen destination.
//!   3. `voice_with_only_audio_routes_unaffected` — voice has no
//!      param routes (normal audio voice). Default group mix emits
//!      as before. Sanity: heuristic must not affect non-modulation
//!      voices.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing_subscriber::layer::SubscriberExt;
use vibelang_core::handlers::{
    merge_default_routes, suppress_modulation_only_defaults, ParamRouteMap, ParamRouteTarget,
    RouteDest, RouteMap, RoutesHandler, VoicesHandler,
};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, Voices,
};
use vibelang_dsp::OutputPort;

// =========================================================================
// Mock backend — captures `create_synth` and `free_node` so tests can
// assert the precise set of mixer-synth mutations finalize emitted.
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
    frees: AtomicU32,
    create_log: Mutex<Vec<CreateCall>>,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            creates: AtomicU32::new(0),
            frees: AtomicU32::new(0),
            create_log: Mutex::new(Vec::new()),
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
// Tracing capture — collects every `event!` message into a shared Vec so
// tests can assert that the heuristic's INFO line was emitted.
// =========================================================================

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
        impl<'a> tracing::field::Visit for Visitor<'a> {
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
        let mut buf = String::new();
        event.record(&mut Visitor(&mut buf));
        self.sink.lines.lock().unwrap().push(buf);
    }
}

/// Install a thread-local tracing subscriber that captures every event
/// into the returned [`CapturedEvents`]. The returned guard scopes the
/// subscriber to the calling thread for its lifetime, which is what we
/// want under `#[tokio::test(flavor = "current_thread")]`: events emitted
/// while the guard is alive land in the captured sink, and concurrent
/// tests on other threads see their own (empty) capture.
fn install_tracing_capture() -> (CapturedEvents, tracing::dispatcher::DefaultGuard) {
    let captured = CapturedEvents::default();
    let layer = CaptureLayer {
        sink: captured.clone(),
    };
    let subscriber = tracing_subscriber::registry().with(layer);
    let guard = tracing::subscriber::set_default(subscriber);
    (captured, guard)
}

// =========================================================================
// Harness — register a synthdef + ports, install a voice group + voice,
// and (where relevant) a separate destination group for explicit user
// routes. Returns wired handlers plus shared state so tests can drive
// the suppress → merge → diff → finalize pipeline themselves.
// =========================================================================

struct Harness {
    voices: VoicesHandler<MockBackend>,
    routes: RoutesHandler<MockBackend>,
    backend: Arc<MockBackend>,
    state: Arc<RwLock<State>>,
    voice_group: GroupId,
    voice_group_bus_id: u32,
}

const SYNTH: &str = "v_synth";

fn ar_port(name: &str, channels: u8) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels,
        rate: vibelang_dsp::PortRate::Ar,
    }
}

async fn setup(ports: &[OutputPort]) -> Harness {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let voices = VoicesHandler::new(backend.clone(), state.clone());
    let routes = RoutesHandler::new(backend.clone(), state.clone());

    let voice_group = GroupId::new(1);

    let voice_group_bus_id = {
        let mut s = state.write().await;
        s.synthdefs.insert(SYNTH.to_string());
        s.synthdef_outputs.insert(SYNTH.to_string(), ports.to_vec());

        let voice_group_node = s.alloc_node_id();
        let voice_group_bus = s.alloc_audio_bus(2);
        s.groups.insert(
            voice_group,
            GroupState {
                id: voice_group,
                name: "vg".to_string(),
                parent: None,
                node_id: voice_group_node,
                audio_bus: voice_group_bus,
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );
        voice_group_bus.0
    };

    Harness {
        voices,
        routes,
        backend,
        state,
        voice_group,
        voice_group_bus_id,
    }
}

/// Allocate a second group on the harness state (used as the explicit
/// destination for `.to(g)` user routes in test 2). Returns the new
/// group's id and its audio-bus index.
async fn add_group(state: &Arc<RwLock<State>>, id: u32, name: &str) -> (GroupId, u32) {
    let group_id = GroupId::new(id);
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
    (group_id, bus.0)
}

/// Run the same suppress → merge → diff → finalize sequence the runtime
/// executes in `Runtime::apply_reload` Phase 4.7. `param_routes_set` /
/// `_bend` / `_trigger` mirror the script's desired-state maps that
/// would be carried in [`reload::ScriptState`] at reload time.
async fn suppress_merge_diff_finalize(
    h: &Harness,
    user_routes: &RouteMap,
    set: &ParamRouteMap,
    bend: &ParamRouteMap,
    trigger: &ParamRouteMap,
) -> RouteMap {
    let merged = {
        let s = h.state.read().await;
        let filtered = suppress_modulation_only_defaults(
            &s.default_routes,
            user_routes,
            set,
            bend,
            trigger,
            |vid| s.voices.get(&vid).map(|v| v.config.name.clone()),
            |vid| {
                s.voices
                    .get(&vid)
                    .map(|v| v.config.modulator_only)
                    .unwrap_or(false)
            },
        );
        merge_default_routes(user_routes, &filtered)
    };
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &merged);
    h.routes.finalize(&diff).await.unwrap();
    merged
}

// =========================================================================
// (1) Voice with only param routes is not mixed — heuristic fires.
//
// LFO-style: voice has one ar output port and the script wires it
// into `target_voice.cutoff` via `.to_param`. There is no explicit
// `.to(g)` / `.to_main()` / `.mute()` user route on the voice. The
// count-based default would dump the LFO's waveform into the voice
// group's audio bus; the heuristic must drop that default before
// finalize spawns any mixer.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn voice_with_only_param_routes_not_mixed() {
    let h = setup(&[ar_port("out", 1)]).await;

    let voice = VoiceId::new(201);
    h.voices
        .create(voice, VoiceConfig::new("lfo", SYNTH, h.voice_group))
        .await
        .unwrap();

    // Sanity: the default-route installer recorded one entry into the
    // voice group — that's exactly what we expect the heuristic to drop.
    {
        let s = h.state.read().await;
        assert_eq!(
            s.default_routes[&(voice, "out".to_string())],
            vec![RouteDest::Group(h.voice_group)],
        );
    }

    // Script's desired state: one outgoing param route from the voice's
    // "out" port to a hypothetical target voice's "cutoff" param. The
    // target id need not be a real voice for this test — the heuristic
    // only cares about the *source* side.
    let target = VoiceId::new(999);
    let mut set = ParamRouteMap::new();
    set.insert(
        (voice, "out".to_string()),
        vec![(ParamRouteTarget::Voice(target), "cutoff".to_string())],
    );

    let (captured, _guard) = install_tracing_capture();
    let merged = suppress_merge_diff_finalize(
        &h,
        &RouteMap::new(),
        &set,
        &ParamRouteMap::new(),
        &ParamRouteMap::new(),
    )
    .await;
    let log_lines = captured.lines();

    // Default for ("voice", "out") was dropped, so the merged map is
    // empty (no user routes either) and finalize spawned nothing.
    assert!(
        merged.is_empty(),
        "modulation-only voice's default group mix must be suppressed; got {:?}",
        merged
    );
    assert_eq!(
        h.backend.creates(),
        0,
        "no port_to_group_link_* mixer should spawn for a modulation-only voice",
    );
    assert_eq!(h.backend.frees(), 0);

    // INFO log activation marker — the helper emits one line per
    // suppressed voice naming it and the count of outgoing param routes.
    let info_match = log_lines.iter().any(|l| {
        l.contains("Voice 'lfo'")
            && l.contains("skipping default audio routing")
            && l.contains("modulation-only")
            && l.contains("1 outgoing param routes")
    });
    assert!(
        info_match,
        "expected INFO line about modulation-only suppression for 'lfo'; got {:?}",
        log_lines,
    );
}

// =========================================================================
// (2) Voice with both param routes AND an explicit audio route is still
// mixed. The heuristic is per-voice: as soon as the script supplies any
// `RouteDest::{Group,Main,Muted}` entry against the voice the heuristic
// does not fire and the explicit route's mixer is spawned normally.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn voice_with_param_and_audio_routes_still_mixed() {
    let h = setup(&[ar_port("out", 1)]).await;

    let voice = VoiceId::new(202);
    h.voices
        .create(voice, VoiceConfig::new("dual", SYNTH, h.voice_group))
        .await
        .unwrap();

    // Allocate a second destination group — the test asserts the mixer
    // lands on this bus, not on the voice-group bus, so the two must
    // be distinguishable.
    let (dest_group, dest_group_bus_id) = add_group(&h.state, 99, "leads").await;
    assert_ne!(dest_group_bus_id, h.voice_group_bus_id);

    // User route: `voice.output("out").to(group("leads"))` — explicit.
    let mut user_routes = RouteMap::new();
    user_routes.insert(
        (voice, "out".to_string()),
        vec![RouteDest::Group(dest_group)],
    );

    // Plus an outgoing param route: `voice.output("out").to_param(target, "p")`.
    let target = VoiceId::new(998);
    let mut set = ParamRouteMap::new();
    set.insert(
        (voice, "out".to_string()),
        vec![(ParamRouteTarget::Voice(target), "p".to_string())],
    );

    let (captured, _guard) = install_tracing_capture();
    let _merged = suppress_merge_diff_finalize(
        &h,
        &user_routes,
        &set,
        &ParamRouteMap::new(),
        &ParamRouteMap::new(),
    )
    .await;
    let log_lines = captured.lines();

    // One mixer spawned, bound to the user's explicit destination bus.
    // The presence of `.to_param` does NOT prevent the audio route from
    // landing — the heuristic only fires when no explicit user route
    // exists for the voice.
    assert_eq!(h.backend.creates(), 1);
    let creates = h.backend.create_log();
    assert_eq!(creates[0].def, "port_to_group_link_1");
    assert_eq!(
        creates[0].out_bus, dest_group_bus_id as f32,
        "mixer must target the user's explicit `leads` group, not the voice group",
    );

    // Heuristic must NOT have logged for this voice.
    let info_match = log_lines
        .iter()
        .any(|l| l.contains("Voice 'dual'") && l.contains("modulation-only"));
    assert!(
        !info_match,
        "heuristic must not fire when the voice has an explicit user route; \
         got {:?}",
        log_lines,
    );
}

// =========================================================================
// (3) Voice with only audio routes (no param routes anywhere) is
// unaffected — the heuristic must be a no-op for normal voices.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn voice_with_only_audio_routes_unaffected() {
    let h = setup(&[ar_port("out", 1)]).await;

    let voice = VoiceId::new(203);
    h.voices
        .create(voice, VoiceConfig::new("synth", SYNTH, h.voice_group))
        .await
        .unwrap();

    // No param routes anywhere, no user routes — the count-based
    // default must come through the helper untouched.
    let (captured, _guard) = install_tracing_capture();
    let merged = suppress_merge_diff_finalize(
        &h,
        &RouteMap::new(),
        &ParamRouteMap::new(),
        &ParamRouteMap::new(),
        &ParamRouteMap::new(),
    )
    .await;
    let log_lines = captured.lines();

    assert_eq!(
        merged[&(voice, "out".to_string())],
        vec![RouteDest::Group(h.voice_group)],
        "default group mix must survive when the voice has no param routes",
    );
    assert_eq!(h.backend.creates(), 1);
    assert_eq!(h.backend.create_log()[0].def, "port_to_group_link_1");
    assert_eq!(
        h.backend.create_log()[0].out_bus,
        h.voice_group_bus_id as f32,
    );

    // No INFO log — heuristic did not fire.
    let info_match = log_lines
        .iter()
        .any(|l| l.contains("Voice 'synth'") && l.contains("modulation-only"));
    assert!(
        !info_match,
        "heuristic must stay silent for non-modulation voices; got {:?}",
        log_lines,
    );
}

// =========================================================================
// (4) Explicit `modulator_only()` flag — Story C.
//
// The flag forces suppression even when the heuristic conditions are not
// met. This test checks the simple case: voice has the flag set, no user
// routes, AND no outgoing param routes (the heuristic alone would NOT
// fire because there are no param routes — but the flag does).
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn voice_modulator_only_skips_audio() {
    let h = setup(&[ar_port("out", 1)]).await;

    let voice = VoiceId::new(204);
    let mut config = VoiceConfig::new("lfo_explicit", SYNTH, h.voice_group);
    config.modulator_only = true;
    h.voices.create(voice, config).await.unwrap();

    // Sanity: default-route installer recorded one entry into the voice
    // group — the flag must drop it before finalize spawns any mixer.
    {
        let s = h.state.read().await;
        assert_eq!(
            s.default_routes[&(voice, "out".to_string())],
            vec![RouteDest::Group(h.voice_group)],
        );
    }

    // No user routes, no param routes — the heuristic alone would NOT
    // fire here. Only the explicit flag triggers suppression.
    let (captured, _guard) = install_tracing_capture();
    let merged = suppress_merge_diff_finalize(
        &h,
        &RouteMap::new(),
        &ParamRouteMap::new(),
        &ParamRouteMap::new(),
        &ParamRouteMap::new(),
    )
    .await;
    let log_lines = captured.lines();

    assert!(
        merged.is_empty(),
        "modulator_only() voice's default group mix must be suppressed; got {:?}",
        merged
    );
    assert_eq!(
        h.backend.creates(),
        0,
        "no port_to_group_link_* mixer should spawn for a modulator_only() voice",
    );
    assert_eq!(h.backend.frees(), 0);

    // INFO log marker — explicit-flag wording, distinct from the
    // heuristic's "modulation-only" line.
    let info_match = log_lines.iter().any(|l| {
        l.contains("Voice 'lfo_explicit'")
            && l.contains("skipping default audio routing")
            && l.contains("explicit modulator_only() flag")
    });
    assert!(
        info_match,
        "expected INFO line about explicit-flag suppression for 'lfo_explicit'; got {:?}",
        log_lines,
    );
}

// =========================================================================
// (5) Explicit `modulator_only()` flag with an explicit audio route — the
// heuristic from Story B would NOT fire (because of the user route), but
// the flag must still drop the implicit default. Net result: the voice
// mixes only into the user's chosen destination, and the surrounding
// voice-group bus stays clean.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn voice_modulator_only_with_audio_route_still_skips_implicit() {
    let h = setup(&[ar_port("out", 1)]).await;

    let voice = VoiceId::new(205);
    let mut config = VoiceConfig::new("lfo_dual", SYNTH, h.voice_group);
    config.modulator_only = true;
    h.voices.create(voice, config).await.unwrap();

    // Allocate a separate destination group for the explicit user route.
    let (dest_group, dest_group_bus_id) = add_group(&h.state, 99, "rec").await;
    assert_ne!(dest_group_bus_id, h.voice_group_bus_id);

    // User route: `voice.output("out").to(group("rec"))`.
    let mut user_routes = RouteMap::new();
    user_routes.insert(
        (voice, "out".to_string()),
        vec![RouteDest::Group(dest_group)],
    );

    let (captured, _guard) = install_tracing_capture();
    let merged = suppress_merge_diff_finalize(
        &h,
        &user_routes,
        &ParamRouteMap::new(),
        &ParamRouteMap::new(),
        &ParamRouteMap::new(),
    )
    .await;
    let log_lines = captured.lines();

    // Merged map should contain ONLY the user's explicit destination —
    // the implicit `Group(voice_group)` default has been suppressed by
    // the flag.
    assert_eq!(
        merged.len(),
        1,
        "merged map should contain only the explicit user route; got {:?}",
        merged,
    );
    assert_eq!(
        merged[&(voice, "out".to_string())],
        vec![RouteDest::Group(dest_group)],
        "explicit `.to(rec)` must survive; implicit default must be dropped",
    );

    // Exactly one mixer spawned, bound to the user's `rec` group bus —
    // not the voice-group bus.
    assert_eq!(h.backend.creates(), 1);
    let creates = h.backend.create_log();
    assert_eq!(creates[0].def, "port_to_group_link_1");
    assert_eq!(
        creates[0].out_bus, dest_group_bus_id as f32,
        "mixer must target the user's explicit `rec` group, not the voice group",
    );

    // INFO log marker — flag-driven suppression, distinct from heuristic.
    let info_match = log_lines.iter().any(|l| {
        l.contains("Voice 'lfo_dual'")
            && l.contains("skipping default audio routing")
            && l.contains("explicit modulator_only() flag")
    });
    assert!(
        info_match,
        "expected INFO line about explicit-flag suppression for 'lfo_dual'; got {:?}",
        log_lines,
    );
}

// Suppress unused-import warnings about HashMap if a future refactor
// drops the only use site below.
#[allow(dead_code)]
fn _hash_map_marker(_: HashMap<u32, u32>) {}
