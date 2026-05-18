//! Regression test for the `voice.on(sample)` multi-sample bug.
//!
//! Bug: every note-on path in `VoicesHandler` used to clone
//! `voice.config.params` — which carries a hardcoded `bufnum=0` written
//! by the Rhai layer because the script doesn't know the runtime
//! `BufferId` — and never overrode it before sending `/s_new`. Single-
//! sample scripts worked by accident (the lone sample is allocated
//! buffer 0). Any script with two or more samples played back the
//! first one for every voice.
//!
//! Fix: each of the three note-on paths in `voices.rs` (note_on,
//! trigger, note_on_with_params) now writes
//! `params["bufnum"] = sample_info.buffer_id.0 as f32` after the
//! existing `sample_id`/`sample_info` lookup, overriding the stale
//! script-side 0 with the live runtime buffer id.
//!
//! This test exercises both `note_on` and `trigger` end-to-end with
//! two voices pointed at two distinct samples (allocated to distinct
//! buffer ids) and asserts the recorded `/s_new` calls carry distinct
//! `bufnum` values that match each voice's sample.

use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::RwLock;
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, GroupState, NodeId, ParamMap, SampleId,
    SampleInfo, State, VoiceConfig, VoiceId, VoiceRole, VoiceState, Voices,
};

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
    bufnum: Option<f32>,
}

struct MockBackend {
    create_log: Mutex<Vec<CreateCall>>,
}

impl MockBackend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            create_log: Mutex::new(Vec::new()),
        })
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
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.create_log.lock().unwrap().push(CreateCall {
            def: def.to_string(),
            bufnum: params.get("bufnum").copied(),
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

/// Pre-populate a sample-playback voice in `state`. Mirrors the shape the
/// Rhai layer leaves on the voice config when `voice("v").on(sample)` runs:
/// `sample_id` is set, and `params["bufnum"]` is the script-side hardcoded
/// `0.0` that the runtime trigger paths must override at note-on.
fn install_sample_voice(
    state: &mut State,
    voice_id: VoiceId,
    name: &str,
    group: GroupId,
    sample_id: SampleId,
) {
    let mut config = VoiceConfig::new(name, "playbuf_one_shot", group);
    config.sample_id = Some(sample_id);
    // Simulate the Rhai layer's hardcoded bufnum=0 in voice.config.params:
    // see crates/vibelang-rhai/src/api/voice.rs::on_sample. The runtime
    // must override this with the real sample_info.buffer_id at every
    // note-on — that's the bug under test.
    config.params.insert("bufnum".to_string(), 0.0);

    state.voices.insert(
        voice_id,
        VoiceState {
            id: voice_id,
            config,
            role: VoiceRole::Audible,
            active_nodes: Vec::new(),
            note_nodes: Default::default(),
            round_robin_position: 0,
            pending_params: Default::default(),
            output_buses: Vec::new(),
            input_buses: Vec::new(),
        },
    );
}

fn install_sample(state: &mut State, sample_id: SampleId, buffer_id: BufferId, name: &str) {
    state.samples.insert(
        sample_id,
        SampleInfo {
            id: sample_id,
            buffer_id,
            path: PathBuf::from(format!("/tmp/{}.wav", name)),
            duration_secs: 1.0,
            sample_rate: 48_000.0,
            channels: 1,
            detected_bpm: None,
        },
    );
}

/// Two voices, two samples, two distinct buffer ids — assert each voice's
/// `/s_new` carries the bufnum matching its own sample, not a shared 0.
#[tokio::test]
async fn note_on_with_two_samples_emits_distinct_bufnums() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let voices = vibelang_core::handlers::VoicesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    let buf_a = BufferId::new(0);
    let buf_b = BufferId::new(1);
    let sample_a = SampleId::new(10);
    let sample_b = SampleId::new(11);
    let voice_a = VoiceId::new(100);
    let voice_b = VoiceId::new(101);

    {
        let mut s = state.write().await;
        let group_node = s.alloc_node_id();
        let group_bus = s.alloc_audio_bus(2);
        s.groups.insert(
            group,
            GroupState {
                id: group,
                name: "g".to_string(),
                parent: None,
                node_id: group_node,
                audio_bus: group_bus,
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );

        install_sample(&mut s, sample_a, buf_a, "a");
        install_sample(&mut s, sample_b, buf_b, "b");
        install_sample_voice(&mut s, voice_a, "va", group, sample_a);
        install_sample_voice(&mut s, voice_b, "vb", group, sample_b);
    }

    voices.note_on(voice_a, 36, 1.0).await.unwrap();
    voices.note_on(voice_b, 37, 1.0).await.unwrap();

    let calls = backend.create_log();
    assert_eq!(calls.len(), 2, "exactly one /s_new per note-on");
    assert!(
        calls.iter().all(|c| c.def == "playbuf_one_shot"),
        "both calls go through the sample synthdef path",
    );

    let bufnum_a = calls[0].bufnum.expect("voice A bufnum was sent");
    let bufnum_b = calls[1].bufnum.expect("voice B bufnum was sent");

    assert_eq!(
        bufnum_a, buf_a.0 as f32,
        "voice A must trigger with its own sample's buffer id, \
         not the script-side hardcoded 0",
    );
    assert_eq!(
        bufnum_b, buf_b.0 as f32,
        "voice B must trigger with its own sample's buffer id",
    );
    assert_ne!(
        bufnum_a, bufnum_b,
        "the two voices must receive distinct bufnums — that's the regression",
    );
}

/// Same scenario via the generic `trigger()` path (used by patterns and
/// melodies, not just direct MIDI). Same invariant: each voice receives
/// its own sample's bufnum.
#[tokio::test]
async fn trigger_with_two_samples_emits_distinct_bufnums() {
    let backend = MockBackend::new();
    let state = Arc::new(RwLock::new(State::default()));
    let voices = vibelang_core::handlers::VoicesHandler::new(backend.clone(), state.clone());

    let group = GroupId::new(1);
    let buf_a = BufferId::new(0);
    let buf_b = BufferId::new(1);
    let sample_a = SampleId::new(20);
    let sample_b = SampleId::new(21);
    let voice_a = VoiceId::new(200);
    let voice_b = VoiceId::new(201);

    {
        let mut s = state.write().await;
        let group_node = s.alloc_node_id();
        let group_bus = s.alloc_audio_bus(2);
        s.groups.insert(
            group,
            GroupState {
                id: group,
                name: "g".to_string(),
                parent: None,
                node_id: group_node,
                audio_bus: group_bus,
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );

        install_sample(&mut s, sample_a, buf_a, "a");
        install_sample(&mut s, sample_b, buf_b, "b");
        install_sample_voice(&mut s, voice_a, "va", group, sample_a);
        install_sample_voice(&mut s, voice_b, "vb", group, sample_b);
    }

    let empty = ParamMap::new();
    voices.trigger(voice_a, &empty).await.unwrap();
    voices.trigger(voice_b, &empty).await.unwrap();

    let calls = backend.create_log();
    assert_eq!(calls.len(), 2);

    let bufnum_a = calls[0].bufnum.expect("voice A bufnum was sent");
    let bufnum_b = calls[1].bufnum.expect("voice B bufnum was sent");
    assert_eq!(bufnum_a, buf_a.0 as f32);
    assert_eq!(bufnum_b, buf_b.0 as f32);
    assert_ne!(bufnum_a, bufnum_b);
}
