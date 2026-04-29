//! Integration tests for the reload reconciler's port-set diff (Story 14a).
//!
//! These tests exercise the public seam between [`reconcile_voice_ports`]
//! (Story 10) and [`RoutesHandler::finalize`] (Story 6b) — the same two
//! pieces the runtime calls back-to-back during a script reload. They
//! cover the three cases the user-facing reload path has to handle:
//!
//! 1. **Port set unchanged across a body edit** — every existing route
//!    survives byte-for-byte and `finalize` is a no-op, i.e. no mixer
//!    synth is freed and no new mixer is spawned. That is what "no audio
//!    interruption" means at this layer: the running mixer node-ids in
//!    [`State::route_synths`] are not touched.
//!
//! 2. **Port removed** — the matching route is dropped from the route map
//!    (and reported back through [`PortReconcile::dropped_routes`] as the
//!    payload the caller is contractually required to surface as a
//!    user-facing warning), the diff produced against the prior route
//!    snapshot contains a `removals` entry for the dropped port, and
//!    `finalize` frees the orphan mixer synth so no node is left reading
//!    from the freed bus.
//!
//! 3. **Port renamed** — surfaces as the union of the previous two cases:
//!    the old name's route is dropped and its mixer is freed; the new
//!    name comes up `Muted` (silent) so `finalize` does not spawn a
//!    mixer for it until the script declares an explicit destination.
//!
//! Every test only reads through the public crate API — it does not touch
//! `handlers/`, `reload/`, or `runtime/`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::handlers::{RouteDest, RouteMap, RoutesHandler};
use vibelang_core::reload::reconcile_voice_ports;
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, GroupState, NodeId, ParamMap, State,
    VoiceConfig, VoiceId, VoiceState,
};
use vibelang_dsp::OutputPort;

// =========================================================================
// Mock backend — captures create_synth / free_node so we can assert on
// the exact set of mixer mutations finalize emitted.
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
#[allow(dead_code)] // fields are inspected via Debug if assertions fail
struct CreateCall {
    def: String,
    node: NodeId,
}

struct MockBackend {
    creates: AtomicU32,
    frees: AtomicU32,
    create_log: Mutex<Vec<CreateCall>>,
    free_log: Mutex<Vec<NodeId>>,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            creates: AtomicU32::new(0),
            frees: AtomicU32::new(0),
            create_log: Mutex::new(Vec::new()),
            free_log: Mutex::new(Vec::new()),
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
    fn free_log(&self) -> Vec<NodeId> {
        self.free_log.lock().unwrap().clone()
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

    async fn free_node(&self, node: NodeId) -> Result<(), Self::Error> {
        self.frees.fetch_add(1, Ordering::Relaxed);
        self.free_log.lock().unwrap().push(node);
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
// Test harness — set up a State that already has a voice with `ports`,
// route entries pointing at `dest_group`, and *live* mixer node-ids
// recorded in `state.route_synths` (one per port). This mirrors what the
// runtime looks like just before a reload re-runs the reconciler.
// =========================================================================

const SYNTH: &str = "voice_synth";

fn port(name: &str, channels: u8) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels,
        rate: vibelang_dsp::PortRate::Ar,
    }
}

struct Harness {
    handler: RoutesHandler<MockBackend>,
    backend: Arc<MockBackend>,
    state: Arc<RwLock<State>>,
    voice: VoiceId,
    dest_group: GroupId,
    /// node-id allocated for each port's pre-existing mixer synth, in the
    /// same order as `ports` — used by tests to assert which nodes were
    /// freed / kept after the reconcile.
    mixer_nodes: Vec<(String, NodeId)>,
    routes_before: RouteMap,
}

async fn setup(ports: &[OutputPort]) -> Harness {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let handler = RoutesHandler::new(backend.clone(), state.clone());

    let voice = VoiceId::new(7);
    let voice_group = GroupId::new(1);
    let dest_group = GroupId::new(2);

    let mut routes_before: RouteMap = HashMap::new();
    let mut mixer_nodes: Vec<(String, NodeId)> = Vec::new();

    {
        let mut s = state.write().await;

        s.synthdefs.insert(SYNTH.to_string());
        s.synthdef_outputs
            .insert(SYNTH.to_string(), ports.to_vec());

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

        let dest_group_node = s.alloc_node_id();
        let dest_group_bus = s.alloc_audio_bus(2);
        s.groups.insert(
            dest_group,
            GroupState {
                id: dest_group,
                name: "dg".to_string(),
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

        let mut output_buses = Vec::with_capacity(ports.len());
        for p in ports {
            output_buses.push((p.name.clone(), s.alloc_audio_bus(p.channels)));
        }
        s.voices.insert(
            voice,
            VoiceState {
                id: voice,
                config: VoiceConfig::new("v", SYNTH, voice_group),
                active_nodes: Vec::new(),
                note_nodes: HashMap::new(),
                round_robin_position: 0,
                pending_params: HashMap::new(),
                output_buses,
            },
        );

        for p in ports {
            let key = (voice, p.name.clone());
            routes_before.insert(key.clone(), RouteDest::Group(dest_group));
            // Pretend `finalize` already ran on a prior reload and these
            // mixer synths are alive. We just need a unique node id per
            // port so tests can identify which ones get freed.
            let node = s.alloc_node_id();
            s.route_synths.insert(key, node);
            mixer_nodes.push((p.name.clone(), node));
        }
    }

    Harness {
        handler,
        backend,
        state,
        voice,
        dest_group,
        mixer_nodes,
        routes_before,
    }
}

// =========================================================================
// (1) Port set unchanged across a body edit — routes intact, no audio
// interruption.
// =========================================================================

#[tokio::test]
async fn port_set_unchanged_across_body_edit_keeps_routes_intact() {
    // Four ports stay identical across the reload — only the synthdef
    // *body* changed. Every route entry must survive byte-for-byte and
    // `finalize` must not free or recreate any mixer synth (the live
    // node-ids in `state.route_synths` stay put — that is what guarantees
    // no audio interruption at the routing layer).
    let ports = vec![port("out", 2), port("a", 1), port("b", 1), port("c", 1)];
    let h = setup(&ports).await;

    let mut routes = h.routes_before.clone();
    let live_nodes_before: HashMap<(VoiceId, String), NodeId> =
        h.state.read().await.route_synths.clone();

    let outcome = {
        let mut s = h.state.write().await;
        reconcile_voice_ports(&mut s, h.voice, &ports, &mut routes)
    };

    assert!(
        outcome.diff.is_unchanged(),
        "port set is identical, diff must be empty"
    );
    assert!(outcome.dropped_routes.is_empty());
    assert!(outcome.default_muted.is_empty());

    // Route map left untouched.
    assert_eq!(routes, h.routes_before);

    // The diff against the *previous* route snapshot is empty, so finalize
    // is the no-op path: nothing freed, nothing created.
    let diff = RoutesHandler::<MockBackend>::diff(&h.routes_before, &routes);
    assert!(diff.is_empty(), "no route additions/removals/changes");

    h.handler.finalize(&diff).await.unwrap();
    assert_eq!(
        h.backend.creates(),
        0,
        "no mixer respawn for body-only edit"
    );
    assert_eq!(h.backend.frees(), 0, "no mixer torn down for body-only edit");

    // Live mixer node-ids unchanged — the running synths are still bound
    // to the same buses, so audio keeps flowing without interruption.
    let live_nodes_after: HashMap<(VoiceId, String), NodeId> =
        h.state.read().await.route_synths.clone();
    assert_eq!(live_nodes_before, live_nodes_after);
}

// =========================================================================
// (2) Port removed — route dropped, warning payload reported, no orphan
// mixer synth left after finalize.
// =========================================================================

#[tokio::test]
async fn port_removed_drops_route_warns_and_finalize_frees_orphan_mixer() {
    // Voice has two ports. One of them (`send`) is dropped from the
    // synthdef on reload. We expect:
    //
    //   - `reconcile_voice_ports` to remove the `send` route from the
    //     route map and surface `(voice, "send")` in
    //     `outcome.dropped_routes` — that is the payload the caller logs
    //     as a user-facing warning.
    //   - The diff against the prior route snapshot to contain a single
    //     `removal` for the dropped port.
    //   - `finalize` to free the `send` mixer node so no synth is left
    //     reading from the freed bus.
    //   - The `out` mixer to be untouched (its node-id stays in
    //     `state.route_synths` and the backend never frees it).
    let old_ports = vec![port("out", 2), port("send", 1)];
    let h = setup(&old_ports).await;

    let send_mixer_node = h
        .mixer_nodes
        .iter()
        .find(|(n, _)| n == "send")
        .map(|(_, id)| *id)
        .expect("send mixer pre-populated");
    let out_mixer_node = h
        .mixer_nodes
        .iter()
        .find(|(n, _)| n == "out")
        .map(|(_, id)| *id)
        .expect("out mixer pre-populated");

    let mut routes = h.routes_before.clone();
    let new_ports = vec![port("out", 2)];

    let outcome = {
        let mut s = h.state.write().await;
        reconcile_voice_ports(&mut s, h.voice, &new_ports, &mut routes)
    };

    // Reconcile reports the dropped port back to the caller — this is
    // what gets surfaced as a "route on removed port `send` dropped"
    // warning to the user.
    assert_eq!(outcome.diff.removed, vec![port("send", 1)]);
    assert!(
        outcome.diff.added.is_empty(),
        "no new ports — pure removal"
    );
    assert_eq!(
        outcome.dropped_routes,
        vec![(h.voice, "send".to_string())],
        "dropped-route payload identifies the warning target",
    );

    // Route map: `send` gone, `out` untouched.
    assert!(!routes.contains_key(&(h.voice, "send".to_string())));
    assert_eq!(
        routes.get(&(h.voice, "out".to_string())),
        Some(&RouteDest::Group(h.dest_group)),
    );

    // Diff drives finalize: one removal, no additions, no changes.
    let diff = RoutesHandler::<MockBackend>::diff(&h.routes_before, &routes);
    assert_eq!(diff.removals.len(), 1);
    assert_eq!(diff.removals[0].voice_id, h.voice);
    assert_eq!(diff.removals[0].port_name, "send");
    assert!(diff.additions.is_empty());
    assert!(diff.changes.is_empty());

    h.handler.finalize(&diff).await.unwrap();

    // The orphan mixer was freed — no synth left reading from the freed
    // bus chunk. Story 6b's `finalize` cleanup verified.
    assert_eq!(h.backend.frees(), 1, "exactly one mixer freed");
    assert!(
        h.backend.free_log().contains(&send_mixer_node),
        "the send mixer node id was freed",
    );
    assert!(
        !h.backend.free_log().contains(&out_mixer_node),
        "the out mixer must not be freed",
    );
    // No new mixer synth spawned — pure removal.
    assert_eq!(h.backend.creates(), 0);

    // `state.route_synths`: `send` entry purged, `out` entry preserved.
    let route_synths = h.state.read().await.route_synths.clone();
    assert!(!route_synths.contains_key(&(h.voice, "send".to_string())));
    assert_eq!(
        route_synths.get(&(h.voice, "out".to_string())),
        Some(&out_mixer_node),
    );
}

// =========================================================================
// (3) Port renamed — drop+warn semantics on the old name, new name silent
// (Muted) until the script re-routes it.
// =========================================================================

#[tokio::test]
async fn port_renamed_drops_old_warns_and_new_name_is_silent() {
    // `cv_old` is the only port and is routed to the destination group.
    // After reload the synthdef declares `cv_new` instead. We treat
    // rename as remove-plus-add, so:
    //
    //   - The old name's route is dropped and surfaced via
    //     `dropped_routes` (warning payload).
    //   - The new name comes up `Muted` (silent default for non-`out`
    //     ports) and is reported via `default_muted`.
    //   - The diff vs. the prior route snapshot contains both a removal
    //     for the old name and an addition for the new name.
    //   - `finalize` frees the old mixer (no orphan node) and creates
    //     *no* new mixer for the silent default — so the new port is
    //     truly silent until the user re-routes it.
    let old_ports = vec![port("cv_old", 1)];
    let h = setup(&old_ports).await;

    let cv_old_mixer_node = h
        .mixer_nodes
        .iter()
        .find(|(n, _)| n == "cv_old")
        .map(|(_, id)| *id)
        .expect("cv_old mixer pre-populated");

    let mut routes = h.routes_before.clone();
    let new_ports = vec![port("cv_new", 1)];

    let outcome = {
        let mut s = h.state.write().await;
        reconcile_voice_ports(&mut s, h.voice, &new_ports, &mut routes)
    };

    assert_eq!(outcome.diff.removed, vec![port("cv_old", 1)]);
    assert_eq!(outcome.diff.added, vec![port("cv_new", 1)]);
    assert_eq!(
        outcome.dropped_routes,
        vec![(h.voice, "cv_old".to_string())],
        "drop+warn payload identifies the old name",
    );
    assert_eq!(
        outcome.default_muted,
        vec![(h.voice, "cv_new".to_string())],
        "new name comes up silent by default",
    );

    // Route map: old gone, new is Muted.
    assert!(!routes.contains_key(&(h.voice, "cv_old".to_string())));
    assert_eq!(
        routes.get(&(h.voice, "cv_new".to_string())),
        Some(&RouteDest::Muted),
    );

    // Diff drives finalize: one removal (cv_old) and one addition
    // (cv_new → Muted, which finalize will short-circuit on).
    let diff = RoutesHandler::<MockBackend>::diff(&h.routes_before, &routes);
    assert_eq!(diff.removals.len(), 1);
    assert_eq!(diff.removals[0].port_name, "cv_old");
    assert_eq!(diff.additions.len(), 1);
    assert_eq!(diff.additions[0].port_name, "cv_new");
    assert_eq!(diff.additions[0].dest, RouteDest::Muted);

    h.handler.finalize(&diff).await.unwrap();

    // Old mixer freed.
    assert_eq!(h.backend.frees(), 1);
    assert_eq!(h.backend.free_log(), vec![cv_old_mixer_node]);

    // New name is Muted, so finalize must not spawn any mixer synth for
    // it — the new port is silent until the script declares an explicit
    // destination.
    assert_eq!(
        h.backend.creates(),
        0,
        "muted default must not create a mixer",
    );
    assert!(
        h.backend.create_log().is_empty(),
        "no port_to_group_link_* spawn for the renamed-silent port",
    );

    // `state.route_synths`: cv_old purged, no entry for cv_new (Muted
    // routes don't allocate a mixer node).
    let route_synths = h.state.read().await.route_synths.clone();
    assert!(!route_synths.contains_key(&(h.voice, "cv_old".to_string())));
    assert!(!route_synths.contains_key(&(h.voice, "cv_new".to_string())));
}
