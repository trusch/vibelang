//! Reload port-reconcile coverage for MULTIPLE voices sharing one synthdef.
//!
//! Regression: the old-port snapshot used to be read live from
//! `state.synthdef_outputs` inside each per-voice reconcile, but the first
//! voice's reconcile overwrites that registry entry with the new set — so
//! every later voice sharing the synthdef compared new-vs-new, no-opped,
//! and kept its stale buses/routes (first-voice-wins). The runtime now
//! snapshots the old port set once per pending reconcile and every
//! dependent voice reconciles against it.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vibelang_core::reload::{GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, NodeId, ParamMap, ReloadMessage, Runtime,
    SynthDefMessage, VoiceConfig, VoiceId,
};
use vibelang_dsp::{OutputPort, PortRate};

// =========================================================================
// Minimal mock backend.
// =========================================================================

#[derive(Debug)]
struct MockError;

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}

impl std::error::Error for MockError {}

#[derive(Clone, Default)]
struct MockBackend {
    _log: Arc<Mutex<Vec<String>>>,
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
// Fixtures
// =========================================================================

const GROUP: GroupId = GroupId(1);
const VOICE_A: VoiceId = VoiceId(10);
const VOICE_B: VoiceId = VoiceId(11);

fn port(name: &str, channels: u8) -> OutputPort {
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

/// Group + two voices sharing `synthdef`.
fn two_voice_script(synthdef: &str) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            ..Default::default()
        },
    );
    script.add_voice(VOICE_A, VoiceConfig::new("va", synthdef, GROUP));
    script.add_voice(VOICE_B, VoiceConfig::new("vb", synthdef, GROUP));
    script
}

async fn port_names(runtime: &Runtime<MockBackend>, voice: VoiceId) -> Vec<String> {
    runtime.state().read().await.voices[&voice]
        .output_buses
        .iter()
        .map(|(name, _)| name.clone())
        .collect()
}

// =========================================================================
// Tests
// =========================================================================

/// Rename a port on a synthdef shared by two voices: BOTH voices must drop
/// the old port's bus and pick up the new one — not just the first voice
/// the reconcile happens to visit.
#[tokio::test(flavor = "current_thread")]
async fn shared_synthdef_port_rename_reconciles_every_dependent_voice() {
    const SYNTH: &str = "shared_ports_rename_synth";
    vibelang_dsp::register_synthdef_outputs(SYNTH.to_string(), vec![port("out", 2), port("cv", 1)]);

    let backend = MockBackend::default();
    let mut runtime = Runtime::new(backend);
    load_synthdef(&mut runtime, SYNTH).await;
    apply(&mut runtime, two_voice_script(SYNTH)).await;

    for voice in [VOICE_A, VOICE_B] {
        assert_eq!(
            port_names(&runtime, voice).await,
            vec!["out".to_string(), "cv".to_string()],
            "voice {:?} starts with the old port set",
            voice
        );
    }

    // Script edit renames `cv` → `cv_gain`; the re-run define_synthdef call
    // updates the dsp-side registry before the reload applies.
    vibelang_dsp::register_synthdef_outputs(
        SYNTH.to_string(),
        vec![port("out", 2), port("cv_gain", 1)],
    );
    apply(&mut runtime, two_voice_script(SYNTH)).await;

    for voice in [VOICE_A, VOICE_B] {
        let names = port_names(&runtime, voice).await;
        assert!(
            names.contains(&"cv_gain".to_string()),
            "voice {:?} must own a bus for the renamed port (got {:?})",
            voice,
            names
        );
        assert!(
            !names.contains(&"cv".to_string()),
            "voice {:?} must have dropped the old port's bus (got {:?})",
            voice,
            names
        );
    }

    let state = runtime.state().read().await;
    assert_eq!(
        state.synthdef_outputs(SYNTH),
        vec![port("out", 2), port("cv_gain", 1)],
        "registered port set reflects the new shape"
    );
}

/// Rate-flip variant (ar → kr) on a shared synthdef: the second voice must
/// also swap its bus to the control-bus range and lose the stale ar bus.
#[tokio::test(flavor = "current_thread")]
async fn shared_synthdef_rate_flip_swaps_buses_on_every_dependent_voice() {
    const SYNTH: &str = "shared_ports_rate_flip_synth";
    vibelang_dsp::register_synthdef_outputs(
        SYNTH.to_string(),
        vec![port("out", 2), port("env", 1)],
    );

    let backend = MockBackend::default();
    let mut runtime = Runtime::new(backend);
    load_synthdef(&mut runtime, SYNTH).await;
    apply(&mut runtime, two_voice_script(SYNTH)).await;

    // Flip `env` to control rate.
    vibelang_dsp::register_synthdef_outputs(
        SYNTH.to_string(),
        vec![port("out", 2), kr_port("env")],
    );
    apply(&mut runtime, two_voice_script(SYNTH)).await;

    let state = runtime.state().read().await;
    for voice in [VOICE_A, VOICE_B] {
        let env_bus = state.voices[&voice]
            .output_buses
            .iter()
            .find(|(name, _)| name == "env")
            .map(|(_, bus)| *bus)
            .unwrap_or_else(|| panic!("voice {:?} lost its env bus", voice));
        assert!(
            env_bus.raw() >= 1000,
            "voice {:?} env must hold a control-bus id after the ar→kr flip (got {})",
            voice,
            env_bus.raw()
        );
    }
    let env_registered = state
        .synthdef_outputs(SYNTH)
        .into_iter()
        .find(|p| p.name == "env")
        .expect("env registered");
    assert_eq!(env_registered.rate, PortRate::Kr);
}
