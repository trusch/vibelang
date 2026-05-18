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
use vibelang_core::handlers::{InputRouteSrc, ParamRouteTarget};
use vibelang_core::message::{ReloadMessage, SynthDefMessage, VoiceMessage};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, NodeId, ParamMap, Runtime, VoiceId, VoiceRole,
};
use vibelang_dsp::{get_synthdef_outputs, GraphIR, InputPort, OutputPort, ParamSpec, PortRate};
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParamMapCall {
    node: NodeId,
    param: String,
    bus: u32,
}

#[derive(Debug, Default)]
struct RecordingBackend {
    creates: Mutex<Vec<SynthCreate>>,
    frees: Mutex<Vec<NodeId>>,
    maps: Mutex<Vec<ParamMapCall>>,
    fail_next_input_link_create: AtomicBool,
}

impl RecordingBackend {
    fn synth_creates(&self) -> Vec<SynthCreate> {
        self.creates.lock().unwrap().clone()
    }

    fn freed_nodes(&self) -> Vec<NodeId> {
        self.frees.lock().unwrap().clone()
    }

    fn param_maps(&self) -> Vec<ParamMapCall> {
        self.maps.lock().unwrap().clone()
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
        node: NodeId,
        param: &str,
        bus: u32,
    ) -> Result<(), Self::Error> {
        self.maps.lock().unwrap().push(ParamMapCall {
            node,
            param: param.to_string(),
            bus,
        });
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

fn register_synthdef_params(name: &str, params: &[(&str, f32)]) {
    let param_specs: Vec<ParamSpec> = params
        .iter()
        .enumerate()
        .map(|(index, (param, default))| ParamSpec {
            name: (*param).to_string(),
            default: vec![*default],
            index,
            lag_ms: None,
        })
        .collect();
    vibelang_dsp::register_synthdef_ir(
        name.to_string(),
        GraphIR {
            name: name.to_string(),
            constants: Vec::new(),
            params: param_specs,
            nodes: Vec::new(),
            out_bus: 0,
        },
    );
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

fn execute_script_state(script: &str) -> vibelang_core::reload::ScriptState {
    ScriptEngine::new()
        .execute(script)
        .expect("script must execute")
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

fn ar_out(name: &str, channels: u8) -> OutputPort {
    OutputPort {
        name: name.to_string(),
        channels,
        rate: PortRate::Ar,
    }
}

fn named_ar_out(name: &str, channels: u8) -> Vec<OutputPort> {
    vec![ar_out(name, channels)]
}

fn multi_ar_out(ports: &[(&str, u8)]) -> Vec<OutputPort> {
    ports
        .iter()
        .map(|(name, channels)| ar_out(name, *channels))
        .collect()
}

fn kr_out(name: &str) -> Vec<OutputPort> {
    vec![OutputPort {
        name: name.to_string(),
        channels: 1,
        rate: PortRate::Kr,
    }]
}

fn tr_out(name: &str) -> Vec<OutputPort> {
    vec![OutputPort {
        name: name.to_string(),
        channels: 1,
        rate: PortRate::Tr,
    }]
}

async fn load_cv_param_fixture(
    runtime: &mut Runtime<RecordingBackend>,
    source_synth: &str,
    target_synth: &str,
) {
    load_registered_synthdef(runtime, source_synth, kr_out("out"), Vec::new()).await;
    register_synthdef_params(target_synth, &[("freq", 220.0), ("amp", 0.2)]);
    load_registered_synthdef(runtime, target_synth, stereo_out(), Vec::new()).await;
}

fn active_voice_node(runtime_state: &vibelang_core::State, voice_name: &str) -> NodeId {
    let voice_id = VoiceId::new(fnv1a_id(voice_name));
    runtime_state
        .voices
        .get(&voice_id)
        .unwrap_or_else(|| panic!("voice {voice_name} should exist"))
        .active_nodes[0]
}

#[test]
fn script_target_first_from_voice_port_records_named_source_port() {
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_tf_src".to_string(),
        named_ar_out("left", 1),
    );

    let state = execute_script_state(
        r#"
        let source = voice("named_input_port_tf_source").synth("named_input_port_tf_src");
        let target = voice("named_input_port_tf_target").synth("named_input_port_tf_target_synth");
        target.input("ch1_a").from(source, "left");
    "#,
    );

    let source = VoiceId::new(fnv1a_id("named_input_port_tf_source"));
    let target = VoiceId::new(fnv1a_id("named_input_port_tf_target"));
    assert_eq!(
        state.input_routes.get(&(target, "ch1_a".to_string())),
        Some(&vec![InputRouteSrc::Voice(source, "left".to_string())])
    );
}

#[test]
fn script_source_first_to_input_records_named_source_port() {
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_sf_src".to_string(),
        named_ar_out("left", 1),
    );

    let state = execute_script_state(
        r#"
        let source = voice("named_input_port_sf_source").synth("named_input_port_sf_src");
        let target = voice("named_input_port_sf_target").synth("named_input_port_sf_target_synth");
        source.output("left").to_input(target, "ch1_a");
    "#,
    );

    let source = VoiceId::new(fnv1a_id("named_input_port_sf_source"));
    let target = VoiceId::new(fnv1a_id("named_input_port_sf_target"));
    assert_eq!(
        state.input_routes.get(&(target, "ch1_a".to_string())),
        Some(&vec![InputRouteSrc::Voice(source, "left".to_string())])
    );
}

#[test]
fn script_target_first_and_source_first_named_input_routes_are_equivalent() {
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_equiv_src".to_string(),
        named_ar_out("left", 1),
    );

    let target_first = execute_script_state(
        r#"
        let source = voice("named_input_port_equiv_source").synth("named_input_port_equiv_src");
        let target = voice("named_input_port_equiv_target").synth("named_input_port_equiv_target_synth");
        target.input("ch1_a").from(source, "left");
    "#,
    );
    let source_first = execute_script_state(
        r#"
        let source = voice("named_input_port_equiv_source").synth("named_input_port_equiv_src");
        let target = voice("named_input_port_equiv_target").synth("named_input_port_equiv_target_synth");
        source.output("left").to_input(target, "ch1_a");
    "#,
    );

    assert_eq!(target_first.input_routes, source_first.input_routes);
    assert_eq!(
        target_first.input_route_order,
        source_first.input_route_order
    );
}

#[test]
fn script_target_first_from_voice_port_replaces_prior_source() {
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_replace_a_src".to_string(),
        named_ar_out("left", 1),
    );
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_replace_b_src".to_string(),
        named_ar_out("right", 1),
    );

    let state = execute_script_state(
        r#"
        let a = voice("named_input_port_replace_a").synth("named_input_port_replace_a_src");
        let b = voice("named_input_port_replace_b").synth("named_input_port_replace_b_src");
        let target = voice("named_input_port_replace_target").synth("named_input_port_replace_target_synth");
        target.input("ch1_a").from(a, "left");
        target.input("ch1_a").from(b, "right");
    "#,
    );

    let b = VoiceId::new(fnv1a_id("named_input_port_replace_b"));
    let target = VoiceId::new(fnv1a_id("named_input_port_replace_target"));
    assert_eq!(
        state.input_routes.get(&(target, "ch1_a".to_string())),
        Some(&vec![InputRouteSrc::Voice(b, "right".to_string())])
    );
}

#[test]
fn script_mixed_named_input_shapes_replace_last_writer_wins() {
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_mixed_a_src".to_string(),
        named_ar_out("left", 1),
    );
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_mixed_b_src".to_string(),
        named_ar_out("right", 1),
    );

    let state = execute_script_state(
        r#"
        let a = voice("named_input_port_mixed_a").synth("named_input_port_mixed_a_src");
        let b = voice("named_input_port_mixed_b").synth("named_input_port_mixed_b_src");
        let target = voice("named_input_port_mixed_target").synth("named_input_port_mixed_target_synth");
        target.input("ch1_a").from(a, "left");
        b.output("right").to_input(target, "ch1_a");
    "#,
    );

    let b = VoiceId::new(fnv1a_id("named_input_port_mixed_b"));
    let target = VoiceId::new(fnv1a_id("named_input_port_mixed_target"));
    assert_eq!(
        state.input_routes.get(&(target, "ch1_a".to_string())),
        Some(&vec![InputRouteSrc::Voice(b, "right".to_string())])
    );
}

#[test]
fn script_to_input_rejects_kr_and_tr_source_ports() {
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_reject_kr_src".to_string(),
        kr_out("env"),
    );
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_reject_tr_src".to_string(),
        tr_out("gate"),
    );

    let kr_err = ScriptEngine::new()
        .execute(
            r#"
        let source = voice("named_input_port_reject_kr").synth("named_input_port_reject_kr_src");
        let target = voice("named_input_port_reject_kr_target").synth("named_input_port_reject_target_synth");
        source.output("env").to_input(target, "ch1_a");
    "#,
        )
        .expect_err("kr source ports must not feed named inputs");
    let kr_msg = kr_err.to_string();
    assert!(kr_msg.contains("to_input"), "msg = {kr_msg}");
    assert!(kr_msg.contains("kr-rate"), "msg = {kr_msg}");
    assert!(kr_msg.contains("'env'"), "msg = {kr_msg}");

    let tr_err = ScriptEngine::new()
        .execute(
            r#"
        let source = voice("named_input_port_reject_tr").synth("named_input_port_reject_tr_src");
        let target = voice("named_input_port_reject_tr_target").synth("named_input_port_reject_target_synth");
        source.output("gate").to_input(target, "ch1_a");
    "#,
        )
        .expect_err("tr source ports must not feed named inputs");
    let tr_msg = tr_err.to_string();
    assert!(tr_msg.contains("to_input"), "msg = {tr_msg}");
    assert!(tr_msg.contains("tr-rate"), "msg = {tr_msg}");
    assert!(tr_msg.contains("'gate'"), "msg = {tr_msg}");
}

#[test]
fn script_to_input_rejects_muted_source_port() {
    vibelang_dsp::register_synthdef_outputs(
        "named_input_port_muted_src".to_string(),
        named_ar_out("left", 1),
    );

    let err = ScriptEngine::new()
        .execute(
            r#"
        let source = voice("named_input_port_muted_source").synth("named_input_port_muted_src");
        let target = voice("named_input_port_muted_target").synth("named_input_port_muted_target_synth");
        source.output("left").mute().to_input(target, "ch1_a");
    "#,
        )
        .expect_err("muted source ports must not feed named inputs");
    let msg = err.to_string();
    assert!(msg.contains("to_input"), "msg = {msg}");
    assert!(msg.contains("muted"), "msg = {msg}");
    assert!(msg.contains("'left'"), "msg = {msg}");
}

#[tokio::test]
async fn migrated_stdlib_lfo_synthdef_routes_to_target_param() {
    let lfo_source = include_str!("../../vibelang-std/stdlib/cv/lfo/lfo_sine.vibe");
    vibelang_dsp::set_deploy_callback(|_| Ok(()));
    ScriptEngine::new()
        .execute(lfo_source)
        .expect("migrated stdlib lfo_sine synthdef should register");
    let source_outputs =
        get_synthdef_outputs("lfo_sine").expect("lfo_sine should declare output metadata");
    assert!(
        source_outputs
            .iter()
            .any(|port| port.name == "out" && port.rate == PortRate::Kr),
        "migrated lfo_sine must expose a kr `out` port: {source_outputs:?}",
    );

    let mut runtime = Runtime::new(RecordingBackend::default());
    load_registered_synthdef(&mut runtime, "lfo_sine", source_outputs, Vec::new()).await;
    register_synthdef_params("migrated_cv_target", &[("freq", 220.0), ("amp", 0.2)]);
    load_registered_synthdef(&mut runtime, "migrated_cv_target", stereo_out(), Vec::new()).await;

    let script = r#"
        let lfo = voice("migrated_cv_lfo")
            .synth("lfo_sine")
            .group("migrated_cv")
            .set_param("rate", 0.5)
            .set_param("lo", -0.25)
            .set_param("hi", 0.25)
            .run();
        let target = voice("migrated_cv_target_voice")
            .synth("migrated_cv_target")
            .group("migrated_cv")
            .param("freq", 220.0)
            .param("amp", 0.2)
            .run();
        lfo.output("out").to_param(target, "freq").scale(80.0).offset(440.0);
    "#;
    apply_script(&mut runtime, script).await;

    let (target_node, summer_bus) = {
        let state = runtime.state().read().await;
        let target = VoiceId::new(fnv1a_id("migrated_cv_target_voice"));
        let target_node = active_voice_node(&state, "migrated_cv_target_voice");
        let summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
            .expect("migrated CV source should create a live param summer");
        assert_eq!(summer.sources.len(), 1);
        (target_node, summer.bus.raw())
    };

    assert!(
        runtime.backend().param_maps().contains(&ParamMapCall {
            node: target_node,
            param: "freq".to_string(),
            bus: summer_bus,
        }),
        "migrated stdlib CV source route must emit a backend /n_map-equivalent call",
    );

    let summers: Vec<_> = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == "param_kr_modulate_1")
        .collect();
    assert_eq!(summers.len(), 1);
    assert_eq!(summers[0].params.get("baseline"), Some(&0.0));
    assert_eq!(summers[0].params.get("scale_a"), Some(&80.0));
    assert_eq!(summers[0].params.get("offset_a"), Some(&440.0));
}

#[tokio::test]
async fn script_source_first_cv_to_param_materializes_live_map_and_summer() {
    let mut runtime = Runtime::new(RecordingBackend::default());
    load_cv_param_fixture(
        &mut runtime,
        "cv_param_src_first_source",
        "cv_param_src_first_target",
    )
    .await;

    let script = r#"
        let lfo = voice("cv_param_src_first_lfo")
            .synth("cv_param_src_first_source")
            .group("cv_param_src_first")
            .run();
        let target = voice("cv_param_src_first_sine")
            .synth("cv_param_src_first_target")
            .group("cv_param_src_first")
            .param("freq", 220.0)
            .param("amp", 0.2)
            .run();
        lfo.output("out").to_param(target, "freq").scale(700.0).offset(110.0);
    "#;
    apply_script(&mut runtime, script).await;

    let (target_node, summer_bus) = {
        let state = runtime.state().read().await;
        let target = VoiceId::new(fnv1a_id("cv_param_src_first_sine"));
        let target_node = active_voice_node(&state, "cv_param_src_first_sine");
        let summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
            .expect("source-first SET should create a live param summer");
        assert_eq!(summer.sources.len(), 1);
        (target_node, summer.bus.raw())
    };

    assert!(
        runtime.backend().param_maps().contains(&ParamMapCall {
            node: target_node,
            param: "freq".to_string(),
            bus: summer_bus,
        }),
        "source-first SET route must emit a backend /n_map-equivalent call"
    );

    let summers: Vec<_> = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == "param_kr_modulate_1")
        .collect();
    assert_eq!(summers.len(), 1);
    assert_eq!(summers[0].params.get("baseline"), Some(&0.0));
    assert_eq!(summers[0].params.get("scale_a"), Some(&700.0));
    assert_eq!(summers[0].params.get("offset_a"), Some(&110.0));
}

#[tokio::test]
async fn script_source_first_cv_to_param_maps_target_nodes_spawned_after_reload() {
    let mut runtime = Runtime::new(RecordingBackend::default());
    load_cv_param_fixture(
        &mut runtime,
        "cv_param_late_spawn_source",
        "cv_param_late_spawn_target",
    )
    .await;

    let script = r#"
        let lfo = voice("cv_param_late_spawn_lfo")
            .synth("cv_param_late_spawn_source")
            .group("cv_param_late_spawn")
            .run();
        let target = voice("cv_param_late_spawn_sine")
            .synth("cv_param_late_spawn_target")
            .group("cv_param_late_spawn")
            .param("freq", 220.0)
            .param("amp", 0.2);
        lfo.output("out").to_param(target, "freq").scale(700.0).offset(110.0);
    "#;
    apply_script(&mut runtime, script).await;

    let target = VoiceId::new(fnv1a_id("cv_param_late_spawn_sine"));
    let (summer_bus, initial_summer_count) = {
        let state = runtime.state().read().await;
        assert!(
            state
                .voices
                .get(&target)
                .expect("target voice exists")
                .active_nodes
                .is_empty(),
            "target starts untriggered so only the later node can receive this map"
        );
        let summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
            .expect("source-first SET route should materialize a reusable summer");
        assert_eq!(summer.sources[0].scale, 700.0);
        assert_eq!(summer.sources[0].offset, 110.0);
        (
            summer.bus.raw(),
            runtime
                .backend()
                .synth_creates()
                .iter()
                .filter(|create| create.def == "param_kr_modulate_1")
                .count(),
        )
    };

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

    let new_node = {
        let state = runtime.state().read().await;
        *state
            .voices
            .get(&target)
            .expect("target voice exists")
            .active_nodes
            .last()
            .expect("trigger should spawn a node")
    };
    assert_eq!(
        runtime
            .backend()
            .synth_creates()
            .iter()
            .filter(|create| create.def == "param_kr_modulate_1")
            .count(),
        initial_summer_count,
        "late trigger should reuse the existing voice-target-scoped summer"
    );
    assert!(runtime.backend().param_maps().contains(&ParamMapCall {
        node: new_node,
        param: "freq".to_string(),
        bus: summer_bus,
    }));
}

#[tokio::test]
async fn script_target_first_cv_to_param_materializes_map_and_conflicts_with_set() {
    let mut runtime = Runtime::new(RecordingBackend::default());
    load_cv_param_fixture(
        &mut runtime,
        "cv_param_tgt_first_source",
        "cv_param_tgt_first_target",
    )
    .await;

    let script = r#"
        let lfo = voice("cv_param_tgt_first_lfo")
            .synth("cv_param_tgt_first_source")
            .group("cv_param_tgt_first")
            .run();
        let target = voice("cv_param_tgt_first_sine")
            .synth("cv_param_tgt_first_target")
            .group("cv_param_tgt_first")
            .param("freq", 330.0)
            .param("amp", 0.2)
            .run();
        target.param("freq").modulate_by(lfo, "out").scale(50.0).offset(5.0);
    "#;
    apply_script(&mut runtime, script).await;

    let (target_node, summer_bus) = {
        let state = runtime.state().read().await;
        let target = VoiceId::new(fnv1a_id("cv_param_tgt_first_sine"));
        let target_node = active_voice_node(&state, "cv_param_tgt_first_sine");
        let summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
            .expect("target-first BEND should create a live param summer");
        assert_eq!(summer.sources.len(), 1);
        (target_node, summer.bus.raw())
    };
    assert!(runtime.backend().param_maps().contains(&ParamMapCall {
        node: target_node,
        param: "freq".to_string(),
        bus: summer_bus,
    }));

    let summers: Vec<_> = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .filter(|create| create.def == "param_kr_modulate_1")
        .collect();
    assert_eq!(summers.len(), 1);
    assert_eq!(
        summers[0].params.get("baseline"),
        Some(&330.0),
        "BEND summer should add to the target param baseline"
    );
    assert_eq!(summers[0].params.get("scale_a"), Some(&50.0));
    assert_eq!(summers[0].params.get("offset_a"), Some(&5.0));

    let conflict = r#"
        let lfo = voice("cv_param_tgt_first_lfo_conflict")
            .synth("cv_param_tgt_first_source");
        let target = voice("cv_param_tgt_first_sine_conflict")
            .synth("cv_param_tgt_first_target");
        lfo.output("out").to_param(target, "freq");
        target.param("freq").modulate_by(lfo, "out");
    "#;
    let err = ScriptEngine::new()
        .execute(conflict)
        .expect_err("SET and BEND on the same target param must conflict");
    let msg = err.to_string();
    assert!(
        msg.contains("to_param") && msg.contains("modulate_by"),
        "cross-verb conflict should name both authoring forms, got: {msg}"
    );
}

#[tokio::test]
async fn script_source_first_fan_out_materializes_two_summers_and_maps() {
    let mut runtime = Runtime::new(RecordingBackend::default());
    load_cv_param_fixture(
        &mut runtime,
        "cv_param_fanout_source",
        "cv_param_fanout_target",
    )
    .await;

    let script = r#"
        let lfo = voice("cv_param_fanout_lfo")
            .synth("cv_param_fanout_source")
            .group("cv_param_fanout")
            .run();
        let left = voice("cv_param_fanout_left")
            .synth("cv_param_fanout_target")
            .group("cv_param_fanout")
            .param("freq", 180.0)
            .run();
        let right = voice("cv_param_fanout_right")
            .synth("cv_param_fanout_target")
            .group("cv_param_fanout")
            .param("freq", 360.0)
            .run();
        lfo.output("out").to_param(left, "freq").scale(300.0).offset(100.0);
        lfo.output("out").to_param(right, "freq").scale(450.0).offset(120.0);
    "#;
    apply_script(&mut runtime, script).await;

    let expected_maps = {
        let state = runtime.state().read().await;
        let left = VoiceId::new(fnv1a_id("cv_param_fanout_left"));
        let right = VoiceId::new(fnv1a_id("cv_param_fanout_right"));
        let left_bus = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(left), "freq".to_string()))
            .expect("left fan-out target should have a summer")
            .bus
            .raw();
        let right_bus = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(right), "freq".to_string()))
            .expect("right fan-out target should have a summer")
            .bus
            .raw();
        vec![
            ParamMapCall {
                node: active_voice_node(&state, "cv_param_fanout_left"),
                param: "freq".to_string(),
                bus: left_bus,
            },
            ParamMapCall {
                node: active_voice_node(&state, "cv_param_fanout_right"),
                param: "freq".to_string(),
                bus: right_bus,
            },
        ]
    };

    let maps = runtime.backend().param_maps();
    for expected in expected_maps {
        assert!(
            maps.contains(&expected),
            "fan-out should map each target param independently; missing {expected:?}"
        );
    }
    assert_eq!(
        runtime
            .backend()
            .synth_creates()
            .iter()
            .filter(|create| create.def == "param_kr_modulate_1")
            .count(),
        2,
        "one source fanning out to two target params should spawn two summers"
    );
}

#[tokio::test]
async fn script_target_first_fan_in_shrinks_without_entity_changes() {
    let mut runtime = Runtime::new(RecordingBackend::default());
    load_registered_synthdef(
        &mut runtime,
        "cv_param_fanin_source",
        kr_out("out"),
        Vec::new(),
    )
    .await;
    register_synthdef_params("cv_param_fanin_target", &[("freq", 220.0)]);
    load_registered_synthdef(
        &mut runtime,
        "cv_param_fanin_target",
        stereo_out(),
        Vec::new(),
    )
    .await;

    let two_sources = r#"
        let a = voice("cv_param_fanin_a")
            .synth("cv_param_fanin_source")
            .group("cv_param_fanin")
            .run();
        let b = voice("cv_param_fanin_b")
            .synth("cv_param_fanin_source")
            .group("cv_param_fanin")
            .run();
        let target = voice("cv_param_fanin_sine")
            .synth("cv_param_fanin_target")
            .group("cv_param_fanin")
            .param("freq", 220.0)
            .run();
        target.param("freq")
            .modulate_by(a, "out").scale(30.0).offset(0.0)
            .modulate_by(b, "out").scale(40.0).offset(0.0);
    "#;
    apply_script(&mut runtime, two_sources).await;

    let first_summer = {
        let state = runtime.state().read().await;
        let target = VoiceId::new(fnv1a_id("cv_param_fanin_sine"));
        let summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
            .expect("two BEND sources should create a summer");
        assert_eq!(summer.sources.len(), 2);
        summer.node
    };
    assert_eq!(
        runtime
            .backend()
            .synth_creates()
            .iter()
            .filter(|create| create.def == "param_kr_modulate_2")
            .count(),
        1
    );
    let fan_in_create = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .rev()
        .find(|create| create.def == "param_kr_modulate_2")
        .expect("fan-in should create a modulate_2 summer");
    assert_eq!(fan_in_create.params.get("scale_a"), Some(&30.0));
    assert_eq!(fan_in_create.params.get("scale_b"), Some(&40.0));

    let one_source = r#"
        let a = voice("cv_param_fanin_a")
            .synth("cv_param_fanin_source")
            .group("cv_param_fanin")
            .run();
        let _b = voice("cv_param_fanin_b")
            .synth("cv_param_fanin_source")
            .group("cv_param_fanin")
            .run();
        let target = voice("cv_param_fanin_sine")
            .synth("cv_param_fanin_target")
            .group("cv_param_fanin")
            .param("freq", 220.0)
            .run();
        target.param("freq").modulate_by(a, "out").scale(30.0).offset(0.0);
    "#;
    apply_script(&mut runtime, one_source).await;

    {
        let state = runtime.state().read().await;
        let target = VoiceId::new(fnv1a_id("cv_param_fanin_sine"));
        let summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
            .expect("shrunk BEND route should still have one summer");
        assert_eq!(summer.sources.len(), 1);
        assert_ne!(
            summer.node, first_summer,
            "route-only shrink should respawn arity-1 summer"
        );
    }
    assert!(
        runtime.backend().freed_nodes().contains(&first_summer),
        "shrinking N=2 to N=1 should free the stale N=2 summer"
    );
    assert_eq!(
        runtime
            .backend()
            .synth_creates()
            .iter()
            .filter(|create| create.def == "param_kr_modulate_1")
            .count(),
        1
    );
}

#[tokio::test]
async fn script_route_only_to_param_change_bypasses_no_change_fast_path() {
    let mut runtime = Runtime::new(RecordingBackend::default());
    load_cv_param_fixture(
        &mut runtime,
        "cv_param_route_only_source",
        "cv_param_route_only_target",
    )
    .await;

    let freq_route = r#"
        let lfo = voice("cv_param_route_only_lfo")
            .synth("cv_param_route_only_source")
            .group("cv_param_route_only")
            .run();
        let target = voice("cv_param_route_only_sine")
            .synth("cv_param_route_only_target")
            .group("cv_param_route_only")
            .param("freq", 220.0)
            .param("amp", 0.2)
            .run();
        lfo.output("out").to_param(target, "freq").scale(500.0).offset(100.0);
    "#;
    apply_script(&mut runtime, freq_route).await;

    let freq_summer = {
        let state = runtime.state().read().await;
        let target = VoiceId::new(fnv1a_id("cv_param_route_only_sine"));
        state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
            .expect("initial freq route should have a summer")
            .node
    };

    let amp_route = r#"
        let lfo = voice("cv_param_route_only_lfo")
            .synth("cv_param_route_only_source")
            .group("cv_param_route_only")
            .run();
        let target = voice("cv_param_route_only_sine")
            .synth("cv_param_route_only_target")
            .group("cv_param_route_only")
            .param("freq", 220.0)
            .param("amp", 0.2)
            .run();
        lfo.output("out").to_param(target, "amp").scale(0.4).offset(0.1);
    "#;
    apply_script(&mut runtime, amp_route).await;

    let (target_node, amp_bus) = {
        let state = runtime.state().read().await;
        let target = VoiceId::new(fnv1a_id("cv_param_route_only_sine"));
        assert!(
            !state
                .param_summers
                .contains_key(&(ParamRouteTarget::Voice(target), "freq".to_string())),
            "route-only reload should remove the stale freq route"
        );
        let amp_bus = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "amp".to_string()))
            .expect("route-only reload should add the new amp route")
            .bus
            .raw();
        (
            active_voice_node(&state, "cv_param_route_only_sine"),
            amp_bus,
        )
    };

    let maps = runtime.backend().param_maps();
    assert!(
        maps.contains(&ParamMapCall {
            node: target_node,
            param: "freq".to_string(),
            bus: u32::MAX,
        }),
        "route-only reload should unmap the removed freq target"
    );
    assert!(
        maps.contains(&ParamMapCall {
            node: target_node,
            param: "amp".to_string(),
            bus: amp_bus,
        }),
        "route-only reload should map the newly-added amp target"
    );
    assert!(
        runtime.backend().freed_nodes().contains(&freq_summer),
        "route-only reload should free the removed route's stale summer"
    );
}

#[tokio::test]
async fn script_route_only_param_shaping_change_respawns_summer() {
    let mut runtime = Runtime::new(RecordingBackend::default());
    load_cv_param_fixture(
        &mut runtime,
        "cv_param_shape_reload_source",
        "cv_param_shape_reload_target",
    )
    .await;

    let scale_700 = r#"
        let lfo = voice("cv_param_shape_reload_lfo")
            .synth("cv_param_shape_reload_source")
            .group("cv_param_shape_reload")
            .run();
        let target = voice("cv_param_shape_reload_sine")
            .synth("cv_param_shape_reload_target")
            .group("cv_param_shape_reload")
            .param("freq", 220.0)
            .run();
        lfo.output("out").to_param(target, "freq").scale(700.0).offset(110.0);
    "#;
    apply_script(&mut runtime, scale_700).await;

    let first_summer = {
        let state = runtime.state().read().await;
        let target = VoiceId::new(fnv1a_id("cv_param_shape_reload_sine"));
        state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
            .expect("initial shaped route should have a summer")
            .node
    };

    let scale_500 = r#"
        let lfo = voice("cv_param_shape_reload_lfo")
            .synth("cv_param_shape_reload_source")
            .group("cv_param_shape_reload")
            .run();
        let target = voice("cv_param_shape_reload_sine")
            .synth("cv_param_shape_reload_target")
            .group("cv_param_shape_reload")
            .param("freq", 220.0)
            .run();
        lfo.output("out").to_param(target, "freq").scale(500.0).offset(110.0);
    "#;
    apply_script(&mut runtime, scale_500).await;

    assert!(
        runtime.backend().freed_nodes().contains(&first_summer),
        "route-only shaping reload should free the stale summer"
    );
    let latest = runtime
        .backend()
        .synth_creates()
        .into_iter()
        .rev()
        .find(|create| create.def == "param_kr_modulate_1")
        .expect("shaping reload should create a replacement summer");
    assert_eq!(latest.params.get("scale_a"), Some(&500.0));
    assert_eq!(latest.params.get("offset_a"), Some(&110.0));
}

#[tokio::test]
async fn script_output_route_only_reload_materializes_backend_change() {
    let mut runtime = Runtime::new(RecordingBackend::default());
    load_registered_synthdef(
        &mut runtime,
        "output_route_only_synth",
        stereo_out(),
        Vec::new(),
    )
    .await;

    let to_group = r#"
        let g = group("output_route_only_bus");
        let v = voice("output_route_only_voice")
            .synth("output_route_only_synth")
            .group("output_route_only_bus")
            .run();
        v.output("out").to(g);
    "#;
    apply_script(&mut runtime, to_group).await;
    let first_route_count = runtime
        .backend()
        .synth_creates()
        .iter()
        .filter(|create| create.def == "port_to_group_link_2")
        .count();
    assert_eq!(first_route_count, 1);

    let to_main = r#"
        let _g = group("output_route_only_bus");
        let v = voice("output_route_only_voice")
            .synth("output_route_only_synth")
            .group("output_route_only_bus")
            .run();
        v.output("out").to_main();
    "#;
    apply_script(&mut runtime, to_main).await;

    let creates = runtime.backend().synth_creates();
    let route_creates: Vec<_> = creates
        .iter()
        .filter(|create| create.def == "port_to_group_link_2")
        .collect();
    assert_eq!(
        route_creates.len(),
        2,
        "output-route-only reload should spawn the replacement route mixer"
    );
    assert_eq!(
        route_creates.last().unwrap().params.get("out_bus"),
        Some(&0.0)
    );
}

#[tokio::test]
async fn script_muted_kr_route_does_not_unsuppress_modulation_only_audio_default() {
    let mut runtime = Runtime::new(RecordingBackend::default());
    load_registered_synthdef(
        &mut runtime,
        "mod_only_mixed_source",
        vec![
            OutputPort {
                name: "audio".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            },
            OutputPort {
                name: "env".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            },
            OutputPort {
                name: "dummy".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            },
        ],
        Vec::new(),
    )
    .await;
    register_synthdef_params("mod_only_target", &[("freq", 220.0)]);
    load_registered_synthdef(&mut runtime, "mod_only_target", stereo_out(), Vec::new()).await;

    let script = r#"
        let src = voice("mod_only_mixed_src")
            .synth("mod_only_mixed_source")
            .group("mod_only")
            .run();
        let target = voice("mod_only_target_voice")
            .synth("mod_only_target")
            .group("mod_only")
            .param("freq", 220.0)
            .run();
        src.output("env").to_param(target, "freq").scale(700.0);
        src.output("dummy").mute();
    "#;
    apply_script(&mut runtime, script).await;

    let (source_audio_bus, target, target_node, summer_bus) = {
        let state = runtime.state().read().await;
        let src = VoiceId::new(fnv1a_id("mod_only_mixed_src"));
        let target = VoiceId::new(fnv1a_id("mod_only_target_voice"));
        let src_voice = state.voices.get(&src).expect("source voice should exist");
        assert_eq!(
            src_voice.role,
            VoiceRole::ModulatorOnly,
            "only kr routes plus a muted kr dummy should derive ModulatorOnly"
        );
        let source_audio_bus = src_voice
            .output_buses
            .iter()
            .find(|(name, _)| name == "audio")
            .map(|(_, bus)| bus.raw() as f32)
            .expect("source audio bus should exist");
        let summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
            .expect("param-route summer should remain materialized");
        assert_eq!(summer.sources.len(), 1);
        (
            source_audio_bus,
            target,
            active_voice_node(&state, "mod_only_target_voice"),
            summer.bus.raw(),
        )
    };
    let leaked_default = runtime.backend().synth_creates().iter().any(|create| {
        create.def == "port_to_group_link_2"
            && create.params.get("in_bus") == Some(&source_audio_bus)
    });
    assert!(
        !leaked_default,
        "muting a kr dummy port must not make the implicit ar default audible"
    );
    assert!(
        runtime.backend().param_maps().contains(&ParamMapCall {
            node: target_node,
            param: "freq".to_string(),
            bus: summer_bus,
        }),
        "ModulatorOnly default suppression must not break the live param-route mapping for {target:?}"
    );
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
        multi_ar_out(&[("out", 2), ("wide_src", 2)]),
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
        target.input("wide").from(src, "wide_src");
    "#;
    apply_script(&mut runtime, script).await;

    let source = VoiceId::new(fnv1a_id("named_input_stereo_src"));
    let target = VoiceId::new(fnv1a_id("named_input_stereo_target"));
    let route = InputRouteSrc::Voice(source, "wide_src".to_string());

    let (default_source_bus, named_source_bus) = {
        let state = runtime.state().read().await;
        let source_voice = state.voices.get(&source).expect("source voice exists");
        let default_source_bus = source_voice
            .output_buses
            .iter()
            .find(|(name, _)| name == "out")
            .map(|(_, bus)| *bus)
            .expect("default source bus exists");
        let named_source_bus = source_voice
            .output_buses
            .iter()
            .find(|(name, _)| name == "wide_src")
            .map(|(_, bus)| *bus)
            .expect("named source bus exists");
        assert_ne!(
            default_source_bus, named_source_bus,
            "fixture must prove non-default source-port selection"
        );
        (default_source_bus, named_source_bus)
    };

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
    assert_eq!(
        stereo_links[0].params.get("in_bus"),
        Some(&(named_source_bus.raw() as f32)),
        "named input link should read from the selected non-default source port"
    );
    assert_ne!(
        stereo_links[0].params.get("in_bus"),
        Some(&(default_source_bus.raw() as f32)),
        "named input link must not silently fall back to the default out port"
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
