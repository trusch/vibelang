//! Gate-release teardown coverage (HL2).
//!
//! Product invariant: teardown must never click. Freeing a sounding synth
//! node with /n_free truncates its audio instantly; every teardown path
//! that can hit a sounding gated voice must instead send `gate=0` (letting
//! the release envelope tail out, with doneAction=2 self-freeing the node)
//! and defer the fallback /n_free, route-mixer frees, and bus/ID reclaim
//! past a release-derived grace period.
//!
//! These tests pin down, through the public reload seam and a mock backend
//! that records /n_set + /n_free traffic:
//!
//! 1. Deleting a gated voice sends `gate=0` on its sounding nodes and NO
//!    immediate /n_free.
//! 2. The voice's route mixer and audio bus are reclaimed only after the
//!    grace period elapses (driven by `Runtime::tick`).
//! 3. A structural voice recreate materializes the new voice (fresh buses,
//!    since the old ones are still held for the tail) before gate-releasing
//!    the old nodes.
//! 4. Deleting a group defers the recursive group-node free past the grace
//!    so gate-released children are not truncated.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vibelang_core::reload::{GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, NodeId, ParamMap, ReloadMessage, Runtime,
    SynthDefMessage, VoiceConfig, VoiceId,
};

// =========================================================================
// Mock backend — records /n_set and /n_free traffic with ordering.
// =========================================================================

#[derive(Debug)]
struct MockError;

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}

impl std::error::Error for MockError {}

#[derive(Default)]
struct BackendState {
    free_log: Mutex<Vec<NodeId>>,
    set_param_log: Mutex<Vec<(NodeId, String, f32)>>,
}

#[derive(Clone, Default)]
struct MockBackend {
    state: Arc<BackendState>,
}

impl MockBackend {
    fn new() -> Self {
        Self::default()
    }

    fn freed_nodes(&self) -> Vec<NodeId> {
        self.state.free_log.lock().unwrap().clone()
    }

    fn was_freed(&self, node: NodeId) -> bool {
        self.freed_nodes().contains(&node)
    }

    fn gate_zero_sets(&self) -> Vec<NodeId> {
        self.state
            .set_param_log
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, p, v)| p == "gate" && *v == 0.0)
            .map(|(n, _, _)| *n)
            .collect()
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
        self.state.free_log.lock().unwrap().push(node);
        Ok(())
    }

    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn set_param(&self, node: NodeId, param: &str, value: f32) -> Result<(), Self::Error> {
        self.state
            .set_param_log
            .lock()
            .unwrap()
            .push((node, param.to_string(), value));
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
// Fixtures
// =========================================================================

const GROUP: GroupId = GroupId(1);
const VOICE: VoiceId = VoiceId(10);

/// Release time declared on the gated test synthdefs. The runtime's reclaim
/// grace is `release + 100ms` margin, so tests wait `RELEASE_S + 0.2s`
/// before expecting the deferred frees.
const RELEASE_S: f32 = 0.05;

/// Register a gated synthdef IR (with `gate` + `release` params) in the
/// process-global registry so `voice_is_gated` / `voice_release_grace`
/// resolve it.
fn register_gated_synthdef(name: &str) {
    let params = [("gate", 1.0f32), ("release", RELEASE_S), ("amp", 0.8)];
    let ir = vibelang_dsp::GraphIR {
        name: name.to_string(),
        constants: Vec::new(),
        params: params
            .iter()
            .enumerate()
            .map(|(index, (pname, value))| vibelang_dsp::ParamSpec {
                name: (*pname).to_string(),
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

async fn load_synthdef(runtime: &mut Runtime<MockBackend>, name: &str) {
    runtime
        .send(
            SynthDefMessage::Load {
                name: name.to_string(),
                data: Vec::new(),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;
}

async fn apply(runtime: &mut Runtime<MockBackend>, script: ScriptState) {
    runtime
        .send(ReloadMessage::Apply { state: script }.into())
        .await
        .unwrap();
    runtime.tick().await;
}

fn group_script() -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            ..Default::default()
        },
    );
    script
}

fn voice_script(synthdef: &str) -> ScriptState {
    let mut script = group_script();
    script.add_voice(VOICE, VoiceConfig::new("v", synthdef, GROUP));
    script
}

/// Simulate a sounding note: push a real allocator-issued node ID into the
/// voice's active_nodes, exactly what a trigger would have tracked.
async fn simulate_sounding_node(runtime: &Runtime<MockBackend>) -> NodeId {
    let mut state = runtime.state().write().await;
    let node = state.alloc_node_id().unwrap();
    state
        .voices
        .get_mut(&VOICE)
        .expect("voice exists")
        .active_nodes
        .push(node);
    node
}

/// Wait past the release-derived grace and drive the runtime tick so the
/// deferred reclaim machinery runs.
async fn wait_grace_and_tick(runtime: &mut Runtime<MockBackend>) {
    tokio::time::sleep(std::time::Duration::from_secs_f32(RELEASE_S + 0.2)).await;
    runtime.tick().await;
}

// =========================================================================
// (1) + (2) Voice deletion: gate=0, no immediate free, deferred reclaim.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn deleted_gated_voice_gate_releases_and_defers_reclaim() {
    const SYNTH: &str = "teardown_gate_release_delete_synth";
    register_gated_synthdef(SYNTH);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH).await;
    apply(&mut runtime, voice_script(SYNTH)).await;

    let sounding = simulate_sounding_node(&runtime).await;
    let (voice_bus, mixer_nodes) = {
        let state = runtime.state().read().await;
        let bus = state
            .voices
            .get(&VOICE)
            .expect("voice exists")
            .output_buses
            .first()
            .expect("legacy stereo out bus")
            .1;
        let mixers: Vec<NodeId> = state
            .route_synths
            .iter()
            .filter(|((vid, _, _), _)| *vid == VOICE)
            .map(|(_, node)| *node)
            .collect();
        (bus, mixers)
    };
    assert!(
        !mixer_nodes.is_empty(),
        "default route should have spawned a mixer for the voice"
    );

    // Reload with the voice removed (group survives).
    apply(&mut runtime, group_script()).await;

    // gate=0 was sent to the sounding node; nothing was hard-freed.
    assert!(
        backend.gate_zero_sets().contains(&sounding),
        "delete of a gated voice must send gate=0 to its sounding nodes: {:?}",
        backend.gate_zero_sets()
    );
    assert!(
        !backend.was_freed(sounding),
        "no immediate /n_free for a gate-released node"
    );
    for mixer in &mixer_nodes {
        assert!(
            !backend.was_freed(*mixer),
            "route mixer must stay alive through the release tail"
        );
    }

    // The voice's audio bus must not be reallocated mid-release.
    let early_bus = {
        let mut state = runtime.state().write().await;
        state.alloc_audio_bus(2).unwrap()
    };
    assert_ne!(
        early_bus, voice_bus,
        "released voice's bus must not be reallocated before the grace"
    );

    // After the grace: fallback free of node + mixers, bus back in the pool.
    wait_grace_and_tick(&mut runtime).await;

    assert!(
        backend.was_freed(sounding),
        "fallback /n_free must arrive after the grace: {:?}",
        backend.freed_nodes()
    );
    for mixer in &mixer_nodes {
        assert!(
            backend.was_freed(*mixer),
            "route mixer must be freed after the grace"
        );
    }
    let reclaimed_bus = {
        let mut state = runtime.state().write().await;
        state.alloc_audio_bus(2).unwrap()
    };
    assert_eq!(
        reclaimed_bus, voice_bus,
        "voice bus must return to the free pool after the grace"
    );
}

// =========================================================================
// (3) Structural recreate: new voice materialized before the old release.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn recreate_materializes_new_voice_before_releasing_old() {
    const SYNTH_A: &str = "teardown_gate_release_recreate_a";
    const SYNTH_B: &str = "teardown_gate_release_recreate_b";
    register_gated_synthdef(SYNTH_A);
    register_gated_synthdef(SYNTH_B);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH_A).await;
    load_synthdef(&mut runtime, SYNTH_B).await;
    apply(&mut runtime, voice_script(SYNTH_A)).await;

    let sounding = simulate_sounding_node(&runtime).await;
    let old_bus = {
        let state = runtime.state().read().await;
        state
            .voices
            .get(&VOICE)
            .expect("voice exists")
            .output_buses
            .first()
            .expect("legacy stereo out bus")
            .1
    };

    // Synthdef change forces a structural recreate.
    apply(&mut runtime, voice_script(SYNTH_B)).await;

    // Old node: gate-released, not freed.
    assert!(
        backend.gate_zero_sets().contains(&sounding),
        "recreate must gate-release the old node"
    );
    assert!(
        !backend.was_freed(sounding),
        "no immediate /n_free on recreate"
    );

    // New voice exists with the new synthdef and FRESH buses — the old bus
    // is still held for the release tail, proving the new voice was
    // materialized while the old one was still sounding (spawn before
    // release, no gap and no bus sharing).
    {
        let state = runtime.state().read().await;
        let voice = state.voices.get(&VOICE).expect("voice recreated");
        assert_eq!(voice.config.synthdef, SYNTH_B);
        let new_bus = voice.output_buses.first().expect("out bus").1;
        assert_ne!(
            new_bus, old_bus,
            "recreated voice must get a fresh bus while the old release tail holds the old one"
        );
        assert!(
            voice.active_nodes.is_empty(),
            "old sounding nodes must not be carried into the new voice"
        );
    }

    // After the grace the old node and bus are reclaimed.
    wait_grace_and_tick(&mut runtime).await;
    assert!(
        backend.was_freed(sounding),
        "old node fallback-freed after the grace"
    );
    let reclaimed_bus = {
        let mut state = runtime.state().write().await;
        state.alloc_audio_bus(2).unwrap()
    };
    assert_eq!(
        reclaimed_bus, old_bus,
        "old voice bus must return to the pool after the grace"
    );
}

// =========================================================================
// (4) Group deletion: recursive free deferred past the release grace.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn deleted_group_free_is_deferred_past_child_release() {
    const SYNTH: &str = "teardown_gate_release_group_synth";
    register_gated_synthdef(SYNTH);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH).await;
    apply(&mut runtime, voice_script(SYNTH)).await;

    let sounding = simulate_sounding_node(&runtime).await;
    let (group_node, link_node) = {
        let state = runtime.state().read().await;
        let group = state.groups.get(&GROUP).expect("group exists");
        (group.node_id, group.link_synth_node_id)
    };

    // Reload to an empty script: voice AND group deleted.
    apply(&mut runtime, ScriptState::new()).await;

    assert!(
        backend.gate_zero_sets().contains(&sounding),
        "child voice node must be gate-released"
    );
    assert!(
        !backend.was_freed(group_node),
        "group node free would kill the releasing child — must be deferred"
    );
    if let Some(link) = link_node {
        assert!(
            !backend.was_freed(link),
            "link synth free is deferred with the group node"
        );
    }
    {
        let state = runtime.state().read().await;
        assert!(
            !state.groups.contains_key(&GROUP),
            "group is gone from the state immediately"
        );
    }

    wait_grace_and_tick(&mut runtime).await;
    assert!(
        backend.was_freed(group_node),
        "group node freed after the grace: {:?}",
        backend.freed_nodes()
    );
    if let Some(link) = link_node {
        assert!(backend.was_freed(link), "link synth freed after the grace");
    }
}

// =========================================================================
// Gateless synthdefs keep the immediate-free semantics.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn deleted_gateless_voice_is_freed_immediately() {
    const SYNTH: &str = "teardown_gate_release_gateless_synth";
    // Registered WITHOUT a `gate` param — no release envelope to trigger.
    let ir = vibelang_dsp::GraphIR {
        name: SYNTH.to_string(),
        constants: Vec::new(),
        params: vec![vibelang_dsp::ParamSpec {
            name: "amp".to_string(),
            default: vec![0.8],
            index: 0,
            lag_ms: None,
        }],
        nodes: Vec::new(),
        out_bus: 0,
    };
    vibelang_dsp::register_synthdef_ir(SYNTH.to_string(), ir);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH).await;
    apply(&mut runtime, voice_script(SYNTH)).await;

    let sounding = simulate_sounding_node(&runtime).await;

    apply(&mut runtime, group_script()).await;

    assert!(
        backend.was_freed(sounding),
        "gateless voices fall back to the immediate /n_free"
    );
    assert!(
        !backend.gate_zero_sets().contains(&sounding),
        "no gate=0 is sent to a synth without a gate param"
    );
}
