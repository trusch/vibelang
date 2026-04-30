//! Integration tests for Story 5's count-based default-route installation
//! at voice create (Story 14c).
//!
//! These tests exercise the full public seam between
//! [`VoicesHandler::create`] (Story 5: installs `state.default_routes` from
//! the synthdef's [`vibelang_dsp::OutputPort`] set, applying the
//! count-based rules) and the route reconcile pipeline that
//! [`crate::runtime::Runtime::apply_reload`] runs in Phase 4.7
//! (merge_default_routes → RoutesHandler::diff → RoutesHandler::finalize).
//!
//! Rather than reaching into `state.default_routes` and stopping there,
//! every test runs the same merge → diff → finalize sequence the runtime
//! uses on a script reload, so we are asserting end-to-end on the
//! observable backend effect: which `port_to_group_link_<n>` mixer synths
//! are spawned, against which buses.
//!
//! The five count-based scenarios covered (one test each):
//!
//!   1. **1 mono port** → `port_to_group_link_1` mixer (mono dup-pan) into
//!      the voice's own group bus.
//!   2. **1 stereo port** → `port_to_group_link_2` mixer (straight L/R)
//!      into the voice's own group bus.
//!   3. **2 mono ports** → two `port_to_group_link_1` mixers, both into
//!      the voice's group bus (dual-mono summed).
//!   4. **4 mono ports** → only the first two ports get default mixers
//!      (the 2-port rule); ports[2..4] have no `state.default_routes`
//!      entry and `finalize` spawns no mixer for them — i.e. silent.
//!   5. **Explicit user route on port[0]** wins over the default — the
//!      mixer for that port is bound to the user's destination group, not
//!      the voice group. Combined with rules 1–4 above, this proves
//!      `voice.output(0).to(g)` is not overridden by default insertion
//!      (the `entry().or_insert()` semantics in `VoicesHandler::create`
//!      plus the merge step in apply_reload).
//!
//! Tests only read through the public crate API — they do not touch
//! `handlers/`, `reload/`, or `runtime/`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{
    merge_default_routes, RouteDest, RouteMap, RoutesHandler, VoicesHandler,
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
        _node: NodeId,
        _param: &str,
        _bus: u32,
    ) -> Result<(), Self::Error> {
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
// Harness — register a synthdef + ports, install a voice group, and (for
// scenario 5) optionally a separate destination group for an explicit
// user route. Returns wired handlers plus shared state so tests can drive
// the merge → diff → finalize pipeline themselves.
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

fn port(name: &str, channels: u8) -> OutputPort {
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

/// Run the same merge → diff → finalize sequence the runtime executes in
/// `Runtime::apply_reload` Phase 4.7. `user_routes` carries any explicit
/// `voice.output(...).to(...)` entries the script supplied; for the four
/// pure-default scenarios this is empty.
async fn merge_diff_finalize(h: &Harness, user_routes: &RouteMap) -> RouteMap {
    let merged = {
        let s = h.state.read().await;
        merge_default_routes(user_routes, &s.default_routes)
    };
    let diff = RoutesHandler::<MockBackend>::diff(&RouteMap::new(), &merged);
    h.routes.finalize(&diff).await.unwrap();
    merged
}

/// Look up a voice's per-port input bus id (its allocated audio bus).
async fn port_in_bus(state: &Arc<RwLock<State>>, voice: VoiceId, port_name: &str) -> u32 {
    let s = state.read().await;
    s.voices
        .get(&voice)
        .expect("voice exists")
        .output_buses
        .iter()
        .find(|(n, _)| n == port_name)
        .map(|(_, b)| b.0)
        .expect("port bus allocated")
}

// =========================================================================
// (1) 1 mono port → pan-mono into the voice's group (port_to_group_link_1).
// =========================================================================

#[tokio::test]
async fn create_voice_one_mono_port_default_routes_pan2_to_group() {
    let h = setup(&[port("out", 1)]).await;

    let voice = VoiceId::new(101);
    h.voices
        .create(voice, VoiceConfig::new("v", SYNTH, h.voice_group))
        .await
        .unwrap();

    // The default-routes installer recorded one entry pointing at the voice
    // group — that's the input to the merge step.
    {
        let s = h.state.read().await;
        assert_eq!(s.default_routes.len(), 1, "exactly one default route");
        assert_eq!(
            s.default_routes[&(voice, "out".to_string())],
            vec![RouteDest::Group(h.voice_group)],
        );
    }

    let _ = merge_diff_finalize(&h, &RouteMap::new()).await;

    assert_eq!(h.backend.creates(), 1, "one mixer synth spawned");
    assert_eq!(h.backend.frees(), 0);

    let creates = h.backend.create_log();
    assert_eq!(
        creates[0].def, "port_to_group_link_1",
        "1-channel port → mono dup-pan synthdef variant",
    );
    assert_eq!(
        creates[0].out_bus, h.voice_group_bus_id as f32,
        "mixer writes into the voice's own group bus (pan2 to group L/R)",
    );
    assert_eq!(
        creates[0].in_bus,
        port_in_bus(&h.state, voice, "out").await as f32,
        "mixer reads from the port's allocated audio bus",
    );
}

// =========================================================================
// (2) 1 stereo port → straight L/R into the voice's group
// (port_to_group_link_2).
// =========================================================================

#[tokio::test]
async fn create_voice_one_stereo_port_default_routes_straight_to_group() {
    let h = setup(&[port("out", 2)]).await;

    let voice = VoiceId::new(102);
    h.voices
        .create(voice, VoiceConfig::new("v", SYNTH, h.voice_group))
        .await
        .unwrap();

    {
        let s = h.state.read().await;
        assert_eq!(s.default_routes.len(), 1);
        assert_eq!(
            s.default_routes[&(voice, "out".to_string())],
            vec![RouteDest::Group(h.voice_group)],
        );
    }

    let _ = merge_diff_finalize(&h, &RouteMap::new()).await;

    assert_eq!(h.backend.creates(), 1);
    let creates = h.backend.create_log();
    assert_eq!(
        creates[0].def, "port_to_group_link_2",
        "2-channel port → straight L/R synthdef variant",
    );
    assert_eq!(creates[0].out_bus, h.voice_group_bus_id as f32);
    assert_eq!(
        creates[0].in_bus,
        port_in_bus(&h.state, voice, "out").await as f32,
    );
}

// =========================================================================
// (3) 2 mono ports → port[0] and port[1] both default-routed into group
// (dual-mono summed). Two `port_to_group_link_1` mixers spawned.
// =========================================================================

#[tokio::test]
async fn create_voice_two_mono_ports_default_routes_both_to_group() {
    let h = setup(&[port("L", 1), port("R", 1)]).await;

    let voice = VoiceId::new(103);
    h.voices
        .create(voice, VoiceConfig::new("v", SYNTH, h.voice_group))
        .await
        .unwrap();

    {
        let s = h.state.read().await;
        assert_eq!(s.default_routes.len(), 2, "both ports default-routed");
        assert_eq!(
            s.default_routes[&(voice, "L".to_string())],
            vec![RouteDest::Group(h.voice_group)],
        );
        assert_eq!(
            s.default_routes[&(voice, "R".to_string())],
            vec![RouteDest::Group(h.voice_group)],
        );
    }

    let _ = merge_diff_finalize(&h, &RouteMap::new()).await;

    assert_eq!(
        h.backend.creates(),
        2,
        "one mixer per port — dual-mono sums at the group bus",
    );
    let creates = h.backend.create_log();
    let l_in = port_in_bus(&h.state, voice, "L").await as f32;
    let r_in = port_in_bus(&h.state, voice, "R").await as f32;

    // Order isn't guaranteed (HashMap iteration), so collect (in_bus, def, out_bus)
    // tuples and assert the set.
    let mut got: Vec<(String, f32, f32)> = creates
        .iter()
        .map(|c| (c.def.clone(), c.in_bus, c.out_bus))
        .collect();
    got.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    let mut want = vec![
        ("port_to_group_link_1".to_string(), l_in, h.voice_group_bus_id as f32),
        ("port_to_group_link_1".to_string(), r_in, h.voice_group_bus_id as f32),
    ];
    want.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    assert_eq!(got, want);
}

// =========================================================================
// (4) 4 mono ports → only the first two get default mixers via the 2-port
// rule; ports[2..4] are silent (no RouteDest entry, no mixer spawned).
// =========================================================================

#[tokio::test]
async fn create_voice_four_mono_ports_only_first_two_default_routed_rest_silent() {
    // Order matters: the count-based rule defaults the first two ports
    // ("sine", "sub"), regardless of channel widths of later ports.
    let h = setup(&[
        port("sine", 1),
        port("sub", 1),
        port("odd", 1),
        port("even", 1),
    ])
    .await;

    let voice = VoiceId::new(104);
    h.voices
        .create(voice, VoiceConfig::new("v", SYNTH, h.voice_group))
        .await
        .unwrap();

    // Only the first two ports show up in default_routes; "odd" and "even"
    // have NO entry — that absence is the silence guarantee.
    {
        let s = h.state.read().await;
        assert_eq!(
            s.default_routes.len(),
            2,
            "only the first two ports get count-based defaults"
        );
        assert!(s.default_routes.contains_key(&(voice, "sine".to_string())));
        assert!(s.default_routes.contains_key(&(voice, "sub".to_string())));
        assert!(
            !s.default_routes.contains_key(&(voice, "odd".to_string())),
            "ports beyond the first two stay un-routed"
        );
        assert!(!s.default_routes.contains_key(&(voice, "even".to_string())));
    }

    let _ = merge_diff_finalize(&h, &RouteMap::new()).await;

    // Exactly two mixers spawned — for "sine" and "sub". Ports "odd" and
    // "even" produce no `create_synth` call, so their audio buses are
    // never read by any mixer ⇒ silent.
    assert_eq!(
        h.backend.creates(),
        2,
        "ports[0..2] get mixers, ports[2..4] are silent",
    );

    let creates = h.backend.create_log();
    let sine_in = port_in_bus(&h.state, voice, "sine").await as f32;
    let sub_in = port_in_bus(&h.state, voice, "sub").await as f32;
    let odd_in = port_in_bus(&h.state, voice, "odd").await as f32;
    let even_in = port_in_bus(&h.state, voice, "even").await as f32;

    let in_buses: Vec<f32> = creates.iter().map(|c| c.in_bus).collect();
    assert!(in_buses.contains(&sine_in), "sine port has a mixer");
    assert!(in_buses.contains(&sub_in), "sub port has a mixer");
    assert!(
        !in_buses.contains(&odd_in),
        "odd port (port[2]) must be silent — no mixer reads its bus",
    );
    assert!(
        !in_buses.contains(&even_in),
        "even port (port[3]) must be silent — no mixer reads its bus",
    );

    // route_synths registry mirrors the same shape: only sine/sub recorded.
    let registry = h.state.read().await.route_synths.clone();
    assert_eq!(registry.len(), 2);
    let g = RouteDest::Group(h.voice_group);
    assert!(registry.contains_key(&(voice, "sine".to_string(), g.clone())));
    assert!(registry.contains_key(&(voice, "sub".to_string(), g.clone())));
    assert!(!registry.contains_key(&(voice, "odd".to_string(), g.clone())));
    assert!(!registry.contains_key(&(voice, "even".to_string(), g)));
}

// =========================================================================
// (5) Explicit user route on port[0] not overridden by default insertion.
//
// User script wrote `voice.output(0).to(g_explicit)`. After voice create
// installs the count-based default for "out" (= voice group), the merge
// step (`merge_default_routes(user, defaults)`) lets the user entry win.
// finalize must spawn the mixer pointed at `g_explicit.audio_bus`, not at
// the voice group bus.
// =========================================================================

#[tokio::test]
async fn explicit_user_route_on_port_zero_not_overridden_by_default() {
    let h = setup(&[port("out", 2)]).await;

    // Allocate a *separate* destination group representing `to(g_explicit)`.
    let dest_group = GroupId::new(99);
    let dest_group_bus_id = {
        let mut s = h.state.write().await;
        let dest_group_node = s.alloc_node_id();
        let dest_group_bus = s.alloc_audio_bus(2);
        s.groups.insert(
            dest_group,
            GroupState {
                id: dest_group,
                name: "explicit".to_string(),
                parent: None,
                node_id: dest_group_node,
                audio_bus: dest_group_bus,
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
            },
        );
        dest_group_bus.0
    };
    assert_ne!(
        dest_group_bus_id, h.voice_group_bus_id,
        "explicit destination must be a different bus than the voice group, \
         otherwise this test cannot distinguish override from no-op",
    );

    let voice = VoiceId::new(105);
    h.voices
        .create(voice, VoiceConfig::new("v", SYNTH, h.voice_group))
        .await
        .unwrap();

    // Sanity check: the default installer pointed "out" at the voice group.
    {
        let s = h.state.read().await;
        assert_eq!(
            s.default_routes[&(voice, "out".to_string())],
            vec![RouteDest::Group(h.voice_group)],
            "default for stereo port[0] is the voice's own group",
        );
    }

    // Build the script-supplied explicit user route — the moral equivalent
    // of `voice.output(0).to(g_explicit)` once it lands in `new_state.routes`.
    let mut user_routes = RouteMap::new();
    user_routes.insert(
        (voice, "out".to_string()),
        vec![RouteDest::Group(dest_group)],
    );

    let merged = merge_diff_finalize(&h, &user_routes).await;

    // The merged map carries the user's explicit destination — the default
    // is shadowed but `state.default_routes` is left intact (so a later
    // reload that drops the user route falls back cleanly to the default).
    assert_eq!(
        merged[&(voice, "out".to_string())],
        vec![RouteDest::Group(dest_group)],
        "user's explicit `voice.output(0).to(g_explicit)` wins over default",
    );
    assert_eq!(
        h.state.read().await.default_routes[&(voice, "out".to_string())],
        vec![RouteDest::Group(h.voice_group)],
        "default_routes itself is not mutated by the merge — it just loses at merge time",
    );

    // Backend: exactly one mixer, bound to the *explicit* destination bus
    // — never to the voice group bus. That is the assertion that proves
    // the default insertion does not override.
    assert_eq!(h.backend.creates(), 1);
    let creates = h.backend.create_log();
    assert_eq!(creates[0].def, "port_to_group_link_2");
    assert_eq!(
        creates[0].out_bus, dest_group_bus_id as f32,
        "mixer targets the explicit destination group, not the voice group",
    );
    assert_ne!(
        creates[0].out_bus, h.voice_group_bus_id as f32,
        "default insertion did NOT silently re-bind the user's explicit route",
    );
    assert_eq!(
        creates[0].in_bus,
        port_in_bus(&h.state, voice, "out").await as f32,
    );
}

// Suppress warnings about HashMap import in case rustc's unused-import
// detector flips on a future refactor.
#[allow(dead_code)]
fn _hash_map_marker(_: HashMap<u32, u32>) {}
