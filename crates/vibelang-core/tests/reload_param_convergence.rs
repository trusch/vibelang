//! Diff-correctness coverage for the reload param system.
//!
//! Product invariant: a hot reload must produce the same runtime state as
//! cold-booting the new script (cold boot literally is `apply_reload` from
//! empty state). These tests pin down three previously-broken corners:
//!
//! 1. **Grow-only params** — a param removed from the script must be reset
//!    to the synthdef's declared default on the running node (voice, group
//!    link synth, effect), not kept at its last value forever.
//! 2. **Live `set_param` leaking into the diff** — HTTP/MIDI tweaks write
//!    into the live params maps; the differ must compare the *script*
//!    config (tracked in `State::script_*_params` snapshots), so a live
//!    tweak neither keeps the entity perpetually "updated" nor snaps back
//!    on an unrelated save.
//! 3. **Group effects equality** — the runtime-side `GroupConfig` used for
//!    diffing must carry the group's real effect chain, so an unchanged
//!    effect-bearing group produces NO update actions on reload.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vibelang_core::reload::{calculate_diff, EffectConfig, GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, EffectId, EffectMessage, GroupId, GroupMessage,
    NodeId, ParamMap, ReloadMessage, Runtime, SynthDefMessage, VoiceConfig, VoiceId, VoiceMessage,
};

// =========================================================================
// Mock backend — captures set_param and create_synth traffic so tests can
// assert exactly which /n_set-equivalent calls a reload emitted.
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
    frees: AtomicU32,
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

    fn frees(&self) -> u32 {
        self.state.frees.load(Ordering::Relaxed)
    }

    fn set_param_log(&self) -> Vec<(NodeId, String, f32)> {
        self.state.set_param_log.lock().unwrap().clone()
    }

    fn sets_for_param(&self, param: &str) -> Vec<(NodeId, f32)> {
        self.set_param_log()
            .into_iter()
            .filter(|(_, p, _)| p == param)
            .map(|(n, _, v)| (n, v))
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

    async fn free_node(&self, _node: NodeId) -> Result<(), Self::Error> {
        self.state.frees.fetch_add(1, Ordering::Relaxed);
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

/// Register a synthdef IR in the process-global registry so
/// `get_synthdef_param_defaults` resolves declared defaults for it.
fn register_synthdef_defaults(name: &str, params: &[(&str, f32)]) {
    let ir = vibelang_dsp::GraphIR {
        name: name.to_string(),
        constants: Vec::new(),
        params: params
            .iter()
            .enumerate()
            .map(|(index, (name, value))| vibelang_dsp::ParamSpec {
                name: (*name).to_string(),
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

/// Same, but into the effect registry (what `define_fx` populates).
fn register_effect_defaults(name: &str, params: &[(&str, f32)]) {
    let ir = vibelang_dsp::GraphIR {
        name: name.to_string(),
        constants: Vec::new(),
        params: params
            .iter()
            .enumerate()
            .map(|(index, (name, value))| vibelang_dsp::ParamSpec {
                name: (*name).to_string(),
                default: vec![*value],
                index,
                lag_ms: None,
            })
            .collect(),
        nodes: Vec::new(),
        out_bus: 0,
    };
    vibelang_dsp::register_effect_ir(name.to_string(), ir);
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

fn params(pairs: &[(&str, f32)]) -> ParamMap {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), *value))
        .collect()
}

fn group_script(group_params: &[(&str, f32)]) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            params: params(group_params),
            ..Default::default()
        },
    );
    script
}

fn voice_script(synthdef: &str, voice_params: &[(&str, f32)]) -> ScriptState {
    let mut script = group_script(&[]);
    let mut config = VoiceConfig::new("v", synthdef, GROUP);
    config.params = params(voice_params);
    script.add_voice(VOICE, config);
    script
}

fn effect_script(synthdef: &str, fx_params: &[(&str, f32)]) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            params: params(&[("amp", 0.25)]),
            effects: vec![FX],
            ..Default::default()
        },
    );
    script.add_effect(
        FX,
        EffectConfig {
            group: GROUP,
            synthdef: synthdef.to_string(),
            params: params(fx_params),
        },
    );
    script
}

async fn link_node(runtime: &Runtime<MockBackend>) -> NodeId {
    runtime
        .state()
        .read()
        .await
        .groups
        .get(&GROUP)
        .expect("group exists")
        .link_synth_node_id
        .expect("group finalized")
}

/// Diff the runtime's current state against `script` through the public
/// `calculate_diff` seam — this is exactly what a reload would compute.
async fn diff_against(runtime: &Runtime<MockBackend>, script: &ScriptState) -> bool {
    let state = runtime.state().read().await;
    calculate_diff(&state, script, &state.current_routes).has_changes()
}

// =========================================================================
// (1) Grow-only params: removed script params reset to synthdef defaults.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn removed_voice_param_resets_to_synthdef_default() {
    const SYNTH: &str = "reload_param_convergence_voice_synth";
    register_synthdef_defaults(SYNTH, &[("cutoff", 1200.0), ("amp", 0.8)]);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH).await;

    // Cold boot with an explicit cutoff override.
    apply(&mut runtime, voice_script(SYNTH, &[("cutoff", 800.0)])).await;

    // Pretend a note is sounding so the reset is observable as backend
    // traffic, not just config state. The node id must come from the real
    // allocator: the reload's stop-inactive-voices phase frees it later.
    let sounding = {
        let mut state = runtime.state().write().await;
        let node = state.alloc_node_id().unwrap();
        state
            .voices
            .get_mut(&VOICE)
            .expect("voice exists")
            .active_nodes
            .push(node);
        node
    };

    // The script drops the cutoff override.
    apply(&mut runtime, voice_script(SYNTH, &[])).await;

    assert!(
        backend
            .sets_for_param("cutoff")
            .contains(&(sounding, 1200.0)),
        "removed param must be reset to the synthdef default on running nodes: {:?}",
        backend.sets_for_param("cutoff")
    );

    // The stored config matches a cold boot: no cutoff key at all, so
    // future triggers fall back to the synthdef default implicitly.
    let state = runtime.state().read().await;
    assert!(
        !state
            .voices
            .get(&VOICE)
            .unwrap()
            .config
            .params
            .contains_key("cutoff"),
        "removed param must not linger in the stored voice config"
    );
    drop(state);

    // And the diff converges: re-applying the same script is a no-op.
    assert!(
        !diff_against(&runtime, &voice_script(SYNTH, &[])).await,
        "state must converge with the script after the removal reload"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn removed_group_amp_resets_link_synth_to_default() {
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());

    apply(&mut runtime, group_script(&[("amp", 0.25)])).await;
    let link = link_node(&runtime).await;

    apply(&mut runtime, group_script(&[])).await;

    assert!(
        backend.sets_for_param("amp").contains(&(link, 1.0)),
        "removed group amp must be reset to the link synth default 1.0: {:?}",
        backend.sets_for_param("amp")
    );
    let state = runtime.state().read().await;
    assert!(
        !state.groups.get(&GROUP).unwrap().params.contains_key("amp"),
        "removed amp must not linger in the stored group params"
    );
    drop(state);

    assert!(
        !diff_against(&runtime, &group_script(&[])).await,
        "group must converge with the script after the removal reload"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn removed_group_param_without_default_warns_and_converges() {
    // Non-amp/pan group params broadcast to child synths — there is no
    // single default, so the runtime warns and skips the backend reset but
    // still converges the stored state so the diff goes quiet.
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());

    apply(&mut runtime, group_script(&[("drive", 0.6)])).await;
    let drive_sets_after_boot = backend.sets_for_param("drive").len();

    apply(&mut runtime, group_script(&[])).await;

    assert_eq!(
        backend.sets_for_param("drive").len(),
        drive_sets_after_boot,
        "no reset value exists for a broadcast group param — must not guess"
    );
    let state = runtime.state().read().await;
    assert!(
        !state
            .groups
            .get(&GROUP)
            .unwrap()
            .params
            .contains_key("drive"),
        "the stored key must still be dropped so the diff converges"
    );
    drop(state);
    assert!(!diff_against(&runtime, &group_script(&[])).await);
}

#[tokio::test(flavor = "current_thread")]
async fn removed_effect_param_resets_to_registered_default() {
    const FX_SYNTH: &str = "reload_param_convergence_fx";
    register_effect_defaults(FX_SYNTH, &[("room", 0.5), ("mix", 0.3)]);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, FX_SYNTH).await;

    apply(&mut runtime, effect_script(FX_SYNTH, &[("room", 0.9)])).await;
    let fx_node = runtime
        .state()
        .read()
        .await
        .effects
        .get(&FX)
        .expect("effect exists")
        .node_id;

    apply(&mut runtime, effect_script(FX_SYNTH, &[])).await;

    assert!(
        backend.sets_for_param("room").contains(&(fx_node, 0.5)),
        "removed effect param must be reset to its registered default: {:?}",
        backend.sets_for_param("room")
    );
    let state = runtime.state().read().await;
    assert!(
        !state
            .effects
            .get(&FX)
            .unwrap()
            .params
            .contains_key("room"),
        "removed param must not linger in the stored effect params"
    );
    drop(state);

    assert!(
        !diff_against(&runtime, &effect_script(FX_SYNTH, &[])).await,
        "effect must converge with the script after the removal reload"
    );
}

// =========================================================================
// (2) Live set_param tweaks stay out of the diffed script config.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn live_tweaked_unnamed_group_param_does_not_dirty_diff() {
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    let script = group_script(&[("amp", 0.25)]);

    apply(&mut runtime, script.clone()).await;

    // Live performance tweak on a param the script never names.
    runtime
        .send(
            GroupMessage::SetParam {
                id: GROUP,
                param: "wobble".to_string(),
                value: 0.3,
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    // The live key must not leak into the script-config comparison: the
    // diff against the unchanged script stays clean forever.
    assert!(
        !diff_against(&runtime, &script).await,
        "live tweak on an unnamed param must not keep the group perpetually updated"
    );
    let state = runtime.state().read().await;
    assert!(
        !state
            .script_group_params
            .get(&GROUP)
            .unwrap()
            .contains_key("wobble"),
        "live tweak must not survive into the tracked script config"
    );
    drop(state);

    // An unrelated save (identical script) is a no-op: no /n_set replay.
    let sets_before = backend.set_param_log().len();
    apply(&mut runtime, script.clone()).await;
    assert_eq!(
        backend.set_param_log().len(),
        sets_before,
        "unchanged script must not replay any params after a live tweak"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn live_tweaked_named_group_param_survives_unrelated_reload_but_script_edit_wins() {
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    let script = group_script(&[("amp", 0.25)]);

    apply(&mut runtime, script.clone()).await;
    let link = link_node(&runtime).await;

    // Live tweak the script-named amp.
    runtime
        .send(
            GroupMessage::SetParam {
                id: GROUP,
                param: "amp".to_string(),
                value: 0.9,
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    // Unrelated save: script amp unchanged (0.25) — the live value must
    // NOT be snapped back (this was the audible bug: every save reset
    // live-tweaked group amps because effect-bearing groups always diffed
    // as updated).
    let amp_sets_before = backend.sets_for_param("amp").len();
    apply(&mut runtime, script.clone()).await;
    assert_eq!(
        backend.sets_for_param("amp").len(),
        amp_sets_before,
        "unchanged script amp must not be re-applied over a live tweak"
    );
    let live_amp = runtime
        .state()
        .read()
        .await
        .groups
        .get(&GROUP)
        .unwrap()
        .params
        .get("amp")
        .copied();
    assert_eq!(live_amp, Some(0.9), "live tweak survives the unrelated save");

    // But an actual script edit to that param wins over the live tweak.
    apply(&mut runtime, group_script(&[("amp", 0.5)])).await;
    assert!(
        backend.sets_for_param("amp").contains(&(link, 0.5)),
        "script edit must re-assert the param over the live tweak: {:?}",
        backend.sets_for_param("amp")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn live_tweaked_unnamed_voice_and_effect_params_do_not_dirty_diff() {
    const SYNTH: &str = "reload_param_convergence_live_voice_synth";
    const FX_SYNTH: &str = "reload_param_convergence_live_fx";
    register_synthdef_defaults(SYNTH, &[("cutoff", 1200.0)]);
    register_effect_defaults(FX_SYNTH, &[("room", 0.5)]);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, SYNTH).await;
    load_synthdef(&mut runtime, FX_SYNTH).await;

    let mut script = effect_script(FX_SYNTH, &[("room", 0.7)]);
    let mut voice_config = VoiceConfig::new("v", SYNTH, GROUP);
    voice_config.params = params(&[("cutoff", 900.0)]);
    script.add_voice(VOICE, voice_config);

    apply(&mut runtime, script.clone()).await;

    runtime
        .send(
            VoiceMessage::SetParam {
                id: VOICE,
                param: "detune".to_string(),
                value: 0.02,
            }
            .into(),
        )
        .await
        .unwrap();
    runtime
        .send(
            EffectMessage::SetParam {
                id: FX,
                param: "shimmer".to_string(),
                value: 0.4,
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    assert!(
        !diff_against(&runtime, &script).await,
        "live voice/effect tweaks on unnamed params must not dirty the diff"
    );
}

// =========================================================================
// (3) Effect-bearing group: unchanged config is a reload no-op; changed
//     config still applies.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn unchanged_effect_bearing_group_produces_no_update_actions() {
    const FX_SYNTH: &str = "reload_param_convergence_noop_fx";
    register_effect_defaults(FX_SYNTH, &[("room", 0.5)]);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, FX_SYNTH).await;

    let script = effect_script(FX_SYNTH, &[("room", 0.7)]);
    apply(&mut runtime, script.clone()).await;

    // The diff itself must be empty — with the old `effects: Vec::new()`
    // placeholder every effect-bearing group diffed as updated on EVERY
    // reload, so has_changes() never early-outed for real scripts.
    {
        let state = runtime.state().read().await;
        let diff = calculate_diff(&state, &script, &state.current_routes);
        assert!(
            !diff.groups.has_changes(),
            "unchanged effect-bearing group must not diff as updated: {}",
            diff.summary()
        );
        assert!(
            !diff.has_changes(),
            "identical script must produce an empty diff: {}",
            diff.summary()
        );
    }

    // And the apply path emits zero backend traffic: no /n_set replay of
    // group params/mute/solo, no node churn.
    let sets_before = backend.set_param_log().len();
    let creates_before = backend.creates();
    let frees_before = backend.frees();

    apply(&mut runtime, script.clone()).await;

    assert_eq!(
        backend.set_param_log().len(),
        sets_before,
        "no /n_set replay on an unchanged reload"
    );
    assert_eq!(backend.creates(), creates_before, "no synth respawn");
    assert_eq!(backend.frees(), frees_before, "no node teardown");
}

#[tokio::test(flavor = "current_thread")]
async fn changed_effect_bearing_group_config_still_applies() {
    const FX_SYNTH: &str = "reload_param_convergence_changed_fx";
    register_effect_defaults(FX_SYNTH, &[("room", 0.5)]);

    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    load_synthdef(&mut runtime, FX_SYNTH).await;

    apply(&mut runtime, effect_script(FX_SYNTH, &[("room", 0.7)])).await;
    let link = link_node(&runtime).await;
    let fx_node = runtime.state().read().await.effects.get(&FX).unwrap().node_id;

    // Change the group amp and the effect room in one save.
    let mut changed = effect_script(FX_SYNTH, &[("room", 0.2)]);
    changed
        .groups
        .get_mut(&GROUP)
        .unwrap()
        .params
        .insert("amp".to_string(), 0.5);

    apply(&mut runtime, changed.clone()).await;

    assert!(
        backend.sets_for_param("amp").contains(&(link, 0.5)),
        "changed group amp must be applied to the link synth: {:?}",
        backend.sets_for_param("amp")
    );
    assert!(
        backend.sets_for_param("room").contains(&(fx_node, 0.2)),
        "changed effect param must be applied to the effect node: {:?}",
        backend.sets_for_param("room")
    );
    assert!(
        !diff_against(&runtime, &changed).await,
        "state must converge with the changed script"
    );
}
