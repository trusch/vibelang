//! Integration regression tests for script-authored named input routes.
//!
//! These tests execute Rhai scripts, apply the resulting `ScriptState` through
//! `Runtime::apply_reload` via `ReloadMessage::Apply`, then assert against the
//! committed core runtime state. Unit coverage already proves the Rhai surface
//! and dispatcher in isolation; this pins the seam between them.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use vibelang_core::compat::Instant;
use vibelang_core::handlers::InputRouteSrc;
use vibelang_core::message::{ReloadMessage, SynthDefMessage, VoiceMessage};
use vibelang_core::{AddAction, Backend, BufferId, BufferInfo, NodeId, ParamMap, Runtime, VoiceId};
use vibelang_dsp::{InputPort, OutputPort, PortRate};
use vibelang_rhai::ScriptEngine;

#[derive(Debug)]
struct MockError;

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock backend error")
    }
}

impl std::error::Error for MockError {}

#[derive(Clone, Debug)]
struct SynthCreate {
    def: String,
    params: ParamMap,
}

#[derive(Debug, Default)]
struct RecordingBackend {
    creates: Mutex<Vec<SynthCreate>>,
    frees: Mutex<Vec<NodeId>>,
    fail_next_input_link_create: AtomicBool,
}

impl RecordingBackend {
    fn synth_creates(&self) -> Vec<SynthCreate> {
        self.creates.lock().unwrap().clone()
    }

    fn freed_nodes(&self) -> Vec<NodeId> {
        self.frees.lock().unwrap().clone()
    }

    fn fail_next_input_link_create(&self) {
        self.fail_next_input_link_create
            .store(true, Ordering::Relaxed);
    }
}

#[async_trait]
impl Backend for RecordingBackend {
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
        if def.starts_with("input_link_")
            && self
                .fail_next_input_link_create
                .swap(false, Ordering::Relaxed)
        {
            return Err(MockError);
        }
        self.creates.lock().unwrap().push(SynthCreate {
            def: def.to_string(),
            params: params.clone(),
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
        self.frees.lock().unwrap().push(node);
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
            channels: 1,
            sample_rate: 44100.0,
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
            sample_rate: 44100.0,
        })
    }

    async fn write_buffer(&self, _id: BufferId, _path: &Path) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn free_buffer(&self, _id: BufferId) -> Result<(), Self::Error> {
        Ok(())
    }

    fn current_time(&self) -> Instant {
        Instant::now()
    }
}

fn fnv1a_id(name: &str) -> u32 {
    const FNV_OFFSET_BASIS: u32 = 2166136261;
    const FNV_PRIME: u32 = 16777619;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    if hash == 0 {
        1
    } else {
        hash
    }
}

async fn load_registered_synthdef(
    runtime: &mut Runtime<RecordingBackend>,
    name: &str,
    outputs: Vec<OutputPort>,
    inputs: Vec<InputPort>,
) {
    vibelang_dsp::register_synthdef_outputs(name.to_string(), outputs);
    vibelang_dsp::register_synthdef_inputs(name.to_string(), inputs);
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

async fn apply_script(runtime: &mut Runtime<RecordingBackend>, script: &str) {
    let mut engine = ScriptEngine::new();
    let state = engine.execute(script).expect("script must execute");
    runtime
        .send(ReloadMessage::Apply { state }.into())
        .await
        .unwrap();
    runtime.tick().await;
}

fn mono_out() -> Vec<OutputPort> {
    vec![OutputPort {
        name: "out".to_string(),
        channels: 1,
        rate: PortRate::Ar,
    }]
}

fn stereo_out() -> Vec<OutputPort> {
    vec![OutputPort {
        name: "out".to_string(),
        channels: 2,
        rate: PortRate::Ar,
    }]
}

#[tokio::test]
async fn script_named_input_route_replaces_and_disconnects_in_runtime_state() {
    let mut runtime = Runtime::new(RecordingBackend::default());

    load_registered_synthdef(&mut runtime, "named_input_src_mono", mono_out(), Vec::new()).await;
    load_registered_synthdef(
        &mut runtime,
        "named_input_target_mono",
        Vec::new(),
        vec![InputPort::ar("in", 1)],
    )
    .await;

    let src_a = VoiceId::new(fnv1a_id("named_input_src_a"));
    let src_b = VoiceId::new(fnv1a_id("named_input_src_b"));
    let target = VoiceId::new(fnv1a_id("named_input_target"));
    let key = (target, "in".to_string());
    let src_a_route = InputRouteSrc::Voice(src_a, "out".to_string());
    let src_b_route = InputRouteSrc::Voice(src_b, "out".to_string());

    let first = r#"
        let a = voice("named_input_src_a").synth("named_input_src_mono").group("named_input_rt");
        let _b = voice("named_input_src_b").synth("named_input_src_mono").group("named_input_rt");
        let target = voice("named_input_target").synth("named_input_target_mono").group("named_input_rt");
        target.input("in").from(a);
    "#;
    apply_script(&mut runtime, first).await;

    {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&key),
            Some(&vec![src_a_route.clone()])
        );
        assert!(state.input_route_synths.contains_key(&(
            target,
            "in".to_string(),
            src_a_route.clone()
        )));
        assert_eq!(
            state.input_route_synths.len(),
            1,
            "first reload should commit exactly one input link"
        );
    }

    let second = r#"
        let _a = voice("named_input_src_a").synth("named_input_src_mono").group("named_input_rt");
        let b = voice("named_input_src_b").synth("named_input_src_mono").group("named_input_rt");
        let target = voice("named_input_target").synth("named_input_target_mono").group("named_input_rt");
        target.input("in").from(b);
    "#;
    apply_script(&mut runtime, second).await;

    {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&key),
            Some(&vec![src_b_route.clone()])
        );
        assert!(!state
            .input_route_synths
            .contains_key(&(target, "in".to_string(), src_a_route)));
        assert!(state.input_route_synths.contains_key(&(
            target,
            "in".to_string(),
            src_b_route.clone()
        )));
        assert_eq!(
            state.input_route_synths.len(),
            1,
            "replacement must not accumulate stale source links"
        );
    }

    let third = r#"
        let _a = voice("named_input_src_a").synth("named_input_src_mono").group("named_input_rt");
        let _b = voice("named_input_src_b").synth("named_input_src_mono").group("named_input_rt");
        let target = voice("named_input_target").synth("named_input_target_mono").group("named_input_rt");
        target.input("in").disconnect();
    "#;
    apply_script(&mut runtime, third).await;

    {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&key),
            Some(&vec![InputRouteSrc::Silent])
        );
        assert!(!state
            .input_route_synths
            .contains_key(&(target, "in".to_string(), src_b_route)));
        assert!(state.input_route_synths.contains_key(&(
            target,
            "in".to_string(),
            InputRouteSrc::Silent
        )));
        assert_eq!(
            state.input_route_synths.len(),
            1,
            "disconnect replaces the prior source link with one silent link"
        );
    }

    let backend = runtime.backend();
    let input_links: Vec<_> = backend
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == "input_link_1")
        .collect();
    assert_eq!(
        input_links.len(),
        3,
        "initial source, replacement source, and disconnect each spawn one mono input link"
    );
    assert!(
        input_links
            .iter()
            .all(|create| create.params.contains_key("in_bus")
                && create.params.contains_key("out_bus")),
        "runtime-created input links must be wired with source and target buses"
    );
    assert_eq!(
        backend.freed_nodes().len(),
        2,
        "replacement and disconnect each free the previous input link"
    );
}

#[tokio::test]
async fn script_named_stereo_input_route_spawns_stereo_link() {
    let mut runtime = Runtime::new(RecordingBackend::default());

    load_registered_synthdef(
        &mut runtime,
        "named_input_src_stereo",
        stereo_out(),
        Vec::new(),
    )
    .await;
    load_registered_synthdef(
        &mut runtime,
        "named_input_target_stereo",
        Vec::new(),
        vec![InputPort::ar("wide", 2)],
    )
    .await;

    let script = r#"
        let src = voice("named_input_stereo_src").synth("named_input_src_stereo").group("named_input_stereo_rt");
        let target = voice("named_input_stereo_target").synth("named_input_target_stereo").group("named_input_stereo_rt");
        target.input("wide").from(src);
    "#;
    apply_script(&mut runtime, script).await;

    let source = VoiceId::new(fnv1a_id("named_input_stereo_src"));
    let target = VoiceId::new(fnv1a_id("named_input_stereo_target"));
    let route = InputRouteSrc::Voice(source, "out".to_string());

    {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&(target, "wide".to_string())),
            Some(&vec![route.clone()])
        );
        assert!(state
            .input_route_synths
            .contains_key(&(target, "wide".to_string(), route)));
    }

    let stereo_links: Vec<_> = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == "input_link_2")
        .collect();
    assert_eq!(
        stereo_links.len(),
        1,
        "stereo input port should route through input_link_2"
    );
}

#[tokio::test]
async fn script_declared_unpatched_input_defaults_to_silent_link() {
    let mut runtime = Runtime::new(RecordingBackend::default());

    load_registered_synthdef(
        &mut runtime,
        "named_input_unpatched_target",
        Vec::new(),
        vec![InputPort::ar("carrier", 1)],
    )
    .await;

    let script = r#"
        let _target = voice("named_input_unpatched_target_voice")
            .synth("named_input_unpatched_target")
            .group("named_input_unpatched_rt");
    "#;
    apply_script(&mut runtime, script).await;

    let target = VoiceId::new(fnv1a_id("named_input_unpatched_target_voice"));
    let key = (target, "carrier".to_string());

    let (silent_bus, target_bus) = {
        let state = runtime.state().read().await;
        assert_eq!(
            state.input_routes.get(&key),
            Some(&vec![InputRouteSrc::Silent]),
            "unpatched declared input should be materialized as a silent route"
        );
        assert!(state.input_route_synths.contains_key(&(
            target,
            "carrier".to_string(),
            InputRouteSrc::Silent
        )));

        let silent_bus = state.silent_ar_bus.expect("silent ar bus allocated");
        let target_bus = state.voices.get(&target).unwrap().input_buses[0].1;
        (silent_bus, target_bus)
    };

    let input_links: Vec<_> = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == "input_link_1")
        .collect();
    assert_eq!(input_links.len(), 1);
    assert_eq!(
        input_links[0].params.get("in_bus"),
        Some(&(silent_bus.raw() as f32))
    );
    assert_eq!(
        input_links[0].params.get("out_bus"),
        Some(&(target_bus.raw() as f32))
    );
    runtime
        .send(
            VoiceMessage::Trigger {
                id: target,
                params: ParamMap::new(),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    let target_creates: Vec<_> = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == "named_input_unpatched_target")
        .collect();
    assert_eq!(target_creates.len(), 1);
    assert_eq!(
        target_creates[0].params.get("__in0"),
        Some(&(target_bus.raw() as f32)),
        "triggered target synth should receive the allocated input bus, not the hidden -1 default"
    );
}

#[tokio::test]
async fn script_group_source_to_mono_named_input_is_rejected() {
    let mut runtime = Runtime::new(RecordingBackend::default());

    load_registered_synthdef(
        &mut runtime,
        "named_input_group_mono_target",
        Vec::new(),
        vec![InputPort::ar("carrier", 1)],
    )
    .await;

    let script = r#"
        let target = voice("named_input_group_mono_target_voice")
            .synth("named_input_group_mono_target")
            .group("named_input_group_rt");
        target.input("carrier").from_current_group();
    "#;
    apply_script(&mut runtime, script).await;

    let target = VoiceId::new(fnv1a_id("named_input_group_mono_target_voice"));
    let key = (target, "carrier".to_string());

    {
        let state = runtime.state().read().await;
        assert!(
            state.input_routes.get(&key).is_none(),
            "stereo group bus must not be materialized into a mono named input"
        );
        assert!(
            state.input_route_synths.is_empty(),
            "rejected group-to-mono route should not leave a live input link"
        );
    }

    let input_links: Vec<_> = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def.starts_with("input_link_"))
        .collect();
    assert!(
        input_links.is_empty(),
        "group-to-mono rejection should not spawn input_link_*"
    );
}

#[tokio::test]
async fn script_named_input_route_create_failure_retries_on_no_change_reload() {
    let mut runtime = Runtime::new(RecordingBackend::default());

    load_registered_synthdef(
        &mut runtime,
        "named_input_retry_src",
        mono_out(),
        Vec::new(),
    )
    .await;
    load_registered_synthdef(
        &mut runtime,
        "named_input_retry_target",
        Vec::new(),
        vec![InputPort::ar("carrier", 1)],
    )
    .await;

    let script = r#"
        let src = voice("named_input_retry_src_voice")
            .synth("named_input_retry_src")
            .group("named_input_retry_rt");
        let target = voice("named_input_retry_target_voice")
            .synth("named_input_retry_target")
            .group("named_input_retry_rt");
        target.input("carrier").from(src);
    "#;

    runtime.backend().fail_next_input_link_create();
    apply_script(&mut runtime, script).await;

    let source = VoiceId::new(fnv1a_id("named_input_retry_src_voice"));
    let target = VoiceId::new(fnv1a_id("named_input_retry_target_voice"));
    let route = InputRouteSrc::Voice(source, "out".to_string());
    let key = (target, "carrier".to_string());

    {
        let state = runtime.state().read().await;
        assert!(
            state.input_routes.get(&key).is_none(),
            "failed link creation should not advance the materialized input route map"
        );
        assert!(
            state.input_route_synths.is_empty(),
            "failed link creation should not leave a live route synth"
        );
    }

    apply_script(&mut runtime, script).await;

    {
        let state = runtime.state().read().await;
        assert_eq!(state.input_routes.get(&key), Some(&vec![route.clone()]));
        assert!(state
            .input_route_synths
            .contains_key(&(target, "carrier".to_string(), route)));
    }

    let input_links: Vec<_> = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == "input_link_1")
        .collect();
    assert_eq!(
        input_links.len(),
        1,
        "no-change reload should retry and eventually create the missing input link"
    );
}
