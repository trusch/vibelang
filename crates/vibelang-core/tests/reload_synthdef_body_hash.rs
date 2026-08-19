//! Reload coverage for synthdef BODY edits (same name, same params).
//!
//! Product invariant: reload must equal cold boot. Before content-hash
//! tracking, editing a synthdef's body between reloads went undetected —
//! the diff compares voice configs by synthdef NAME, so live voices kept
//! playing the old compiled graph. The script-eval layer now records a
//! content hash of the encoded SCgf bytes per deployed synthdef
//! (`ScriptState::synthdef_hashes`), the runtime snapshots it on every
//! applied reload (`State::script_synthdef_hashes`), and a hash mismatch
//! structurally recreates dependent voices (spawn-before-release) and
//! effects.
//!
//! The /d_recv re-send itself is NOT driven by the diff: every script
//! evaluation re-runs `define_synthdef(...).body(...)`, whose deploy
//! callback queues a `SynthDefMessage::Load` ahead of the reload apply on
//! the same runtime channel. The first test pins that ordering contract at
//! the message level (Load lands on the backend before the reload runs).

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vibelang_core::reload::{calculate_diff, EffectConfig, GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, EffectId, GroupId, NodeId, ParamMap, ReloadMessage,
    Runtime, SynthDefMessage, VoiceConfig, VoiceId,
};

// =========================================================================
// Mock backend — records load_synthdef / create_synth / free_node /
// set_param traffic.
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
    creates: AtomicU32,
    load_log: Mutex<Vec<String>>,
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

    fn creates(&self) -> u32 {
        self.state.creates.load(Ordering::Relaxed)
    }

    fn loads_for(&self, name: &str) -> usize {
        self.state
            .load_log
            .lock()
            .unwrap()
            .iter()
            .filter(|n| n.as_str() == name)
            .count()
    }

    fn free_log(&self) -> Vec<NodeId> {
        self.state.free_log.lock().unwrap().clone()
    }

    fn sets_for_param(&self, param: &str) -> Vec<(NodeId, f32)> {
        self.state
            .set_param_log
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, p, _)| p == param)
            .map(|(n, _, v)| (*n, *v))
            .collect()
    }
}

#[async_trait]
impl Backend for MockBackend {
    type Error = MockError;

    async fn load_synthdef(&self, name: &str, _data: &[u8]) -> Result<(), Self::Error> {
        self.state.load_log.lock().unwrap().push(name.to_string());
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
        self.state.creates.fetch_add(1, Ordering::Relaxed);
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
const FX: EffectId = EffectId(20);

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

/// Group + one voice on `synthdef`, with the given body-content hash and
/// voice params.
fn voice_script(synthdef: &str, hash: u64, voice_params: &[(&str, f32)]) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            ..Default::default()
        },
    );
    let mut config = VoiceConfig::new("v", synthdef, GROUP);
    config.params = voice_params
        .iter()
        .map(|(name, value)| (name.to_string(), *value))
        .collect();
    script.add_voice(VOICE, config);
    // Keep the voice continuously running so it owns a sounding node across
    // reloads (and phase_trigger_running_voices does not stop it).
    script.running_voices.insert(VOICE);
    script.synthdef_hashes.insert(synthdef.to_string(), hash);
    script
}

/// Group + one effect on `synthdef`, with the given body-content hash.
fn effect_script(synthdef: &str, hash: u64) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            effects: vec![FX],
            ..Default::default()
        },
    );
    script.add_effect(
        FX,
        EffectConfig {
            name: String::new(),
            group: GROUP,
            synthdef: synthdef.to_string(),
            params: ParamMap::new(),
        },
    );
    script.synthdef_hashes.insert(synthdef.to_string(), hash);
    script
}

/// Register a gated synthdef IR so `voice_is_gated` sees a `gate` param —
/// the structural recreate path then gate-releases old nodes
/// (spawn-before-release) instead of hard-freeing them.
fn register_gated_synthdef(name: &str) {
    let ir = vibelang_dsp::GraphIR {
        name: name.to_string(),
        constants: Vec::new(),
        params: [("gate", 1.0f32), ("cutoff", 1200.0)]
            .iter()
            .enumerate()
            .map(|(index, (param, value))| vibelang_dsp::ParamSpec {
                name: (*param).to_string(),
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

/// The single sounding node of the running voice.
async fn sounding_node(runtime: &Runtime<MockBackend>) -> NodeId {
    let state = runtime.state().read().await;
    let nodes = &state.voices[&VOICE].active_nodes;
    assert_eq!(nodes.len(), 1, "expected exactly one sounding node");
    nodes[0]
}

async fn diff_is_clean(runtime: &Runtime<MockBackend>, script: &ScriptState) -> bool {
    let state = runtime.state().read().await;
    !calculate_diff(&state, script, &state.current_routes).has_changes()
}

// =========================================================================
// (a) Body-changed synthdef: /d_recv re-sent + dependent voice recreated,
// and the resulting config equals a cold boot of the new script.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn body_changed_synthdef_recreates_dependent_voice() {
    const SYNTH: &str = "body_hash_recreate_synth";
    register_gated_synthdef(SYNTH);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH).await;

    apply(&mut runtime, voice_script(SYNTH, 0xAAAA, &[])).await;
    let sounding = sounding_node(&runtime).await;

    // Script re-evaluation re-deploys the edited synthdef via the deploy
    // callback BEFORE the reload apply lands (same runtime channel, FIFO).
    // Simulate that ordering and pin it: the /d_recv-equivalent must reach
    // the backend again.
    let loads_before = backend.loads_for(SYNTH);
    load_synthdef(&mut runtime, SYNTH).await;
    assert_eq!(
        backend.loads_for(SYNTH),
        loads_before + 1,
        "re-defined synthdef must be re-sent to the backend before the reload"
    );

    // Same name, same params — only the body-content hash changed.
    apply(&mut runtime, voice_script(SYNTH, 0xBBBB, &[])).await;

    // The voice was structurally recreated: spawn-before-release gate-releases
    // the old sounding node.
    assert!(
        backend.sets_for_param("gate").contains(&(sounding, 0.0)),
        "body change must recreate the voice (old node gate-released): {:?}",
        backend.sets_for_param("gate")
    );
    let state = runtime.state().read().await;
    assert!(
        !state.voices[&VOICE].active_nodes.contains(&sounding),
        "old node must be detached from the recreated voice"
    );
    assert_eq!(
        state.script_synthdef_hashes.get(SYNTH),
        Some(&0xBBBB),
        "applied reload must snapshot the new body hash"
    );
    drop(state);

    // Reload == cold boot: the hot-reloaded runtime converges with the new
    // script, and its voice config equals a fresh runtime cold-booted on it.
    assert!(
        diff_is_clean(&runtime, &voice_script(SYNTH, 0xBBBB, &[])).await,
        "diff must be clean after the body-change reload"
    );
    let cold_backend = MockBackend::new();
    let mut cold = Runtime::new(cold_backend.clone());
    load_synthdef(&mut cold, SYNTH).await;
    apply(&mut cold, voice_script(SYNTH, 0xBBBB, &[])).await;
    let hot_config = runtime.state().read().await.voices[&VOICE].config.clone();
    let cold_config = cold.state().read().await.voices[&VOICE].config.clone();
    assert_eq!(
        hot_config, cold_config,
        "hot-reloaded voice config must equal cold boot"
    );
    assert_eq!(
        runtime.state().read().await.script_synthdef_hashes,
        cold.state().read().await.script_synthdef_hashes,
        "hash snapshots must match cold boot"
    );
}

// =========================================================================
// (b) Unchanged body: params-only edits must NOT recreate (no churn).
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn unchanged_body_params_only_change_does_not_recreate() {
    const SYNTH: &str = "body_hash_stable_synth";
    register_gated_synthdef(SYNTH);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH).await;

    apply(
        &mut runtime,
        voice_script(SYNTH, 0xCAFE, &[("cutoff", 800.0)]),
    )
    .await;
    let sounding = sounding_node(&runtime).await;

    // Same hash, different param value: in-place update only.
    apply(
        &mut runtime,
        voice_script(SYNTH, 0xCAFE, &[("cutoff", 900.0)]),
    )
    .await;

    assert!(
        !backend.sets_for_param("gate").contains(&(sounding, 0.0)),
        "params-only change must not gate-release (no structural recreate): {:?}",
        backend.sets_for_param("gate")
    );
    assert!(
        backend
            .sets_for_param("cutoff")
            .contains(&(sounding, 900.0)),
        "changed param must be applied in place to the running node: {:?}",
        backend.sets_for_param("cutoff")
    );
    let state = runtime.state().read().await;
    assert!(
        state.voices[&VOICE].active_nodes.contains(&sounding),
        "the sounding node must survive a params-only reload"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn unchanged_body_identical_script_is_a_full_noop() {
    const SYNTH: &str = "body_hash_noop_synth";

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH).await;

    let script = voice_script(SYNTH, 0xF00D, &[("cutoff", 800.0)]);
    apply(&mut runtime, script.clone()).await;

    let creates_before = backend.creates();
    let frees_before = backend.free_log().len();
    apply(&mut runtime, script.clone()).await;

    assert!(
        diff_is_clean(&runtime, &script).await,
        "identical script (same hash) must diff clean"
    );
    assert_eq!(backend.creates(), creates_before, "no synth respawn");
    assert_eq!(backend.free_log().len(), frees_before, "no node teardown");
}

/// A hash appearing for the first time (unknown → known) must not recreate:
/// missing entries on either side mean "unknown, assume unchanged".
#[tokio::test(flavor = "current_thread")]
async fn hash_appearing_for_the_first_time_does_not_recreate() {
    const SYNTH: &str = "body_hash_first_seen_synth";
    register_gated_synthdef(SYNTH);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH).await;

    // First apply carries NO hash (e.g. pre-hash-tracking state).
    let mut no_hash = voice_script(SYNTH, 0, &[]);
    no_hash.synthdef_hashes.clear();
    apply(&mut runtime, no_hash).await;
    let sounding = sounding_node(&runtime).await;

    // Second apply introduces the hash.
    apply(&mut runtime, voice_script(SYNTH, 0xD00D, &[])).await;

    assert!(
        !backend.sets_for_param("gate").contains(&(sounding, 0.0)),
        "unknown→known hash transition must not recreate the voice"
    );
    let state = runtime.state().read().await;
    assert!(state.voices[&VOICE].active_nodes.contains(&sounding));
}

// =========================================================================
// (a, effects) Body-changed define_fx synthdef recreates the effect node.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn body_changed_fx_synthdef_recreates_effect_node() {
    const FX_SYNTH: &str = "body_hash_fx_synth";

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, FX_SYNTH).await;

    apply(&mut runtime, effect_script(FX_SYNTH, 0x1111)).await;
    let old_node = runtime
        .state()
        .read()
        .await
        .effects
        .get(&FX)
        .expect("effect exists")
        .node_id;

    apply(&mut runtime, effect_script(FX_SYNTH, 0x2222)).await;

    let new_node = runtime
        .state()
        .read()
        .await
        .effects
        .get(&FX)
        .expect("effect exists")
        .node_id;
    assert_ne!(
        old_node, new_node,
        "body change must recreate the effect node"
    );
    // The old node's teardown is grace-deferred (50ms fade-out before the
    // /n_free); the immediate observable is the mix=0 fade on the old node.
    assert!(
        backend.sets_for_param("mix").contains(&(old_node, 0.0)),
        "old effect node must be faded out for removal: {:?}",
        backend.sets_for_param("mix")
    );

    // And a same-hash re-apply is a no-op.
    let node_before = new_node;
    apply(&mut runtime, effect_script(FX_SYNTH, 0x2222)).await;
    assert_eq!(
        runtime
            .state()
            .read()
            .await
            .effects
            .get(&FX)
            .unwrap()
            .node_id,
        node_before,
        "unchanged hash must not recreate the effect"
    );
}
