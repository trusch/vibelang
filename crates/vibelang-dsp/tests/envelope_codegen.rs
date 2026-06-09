use rhai::{Dynamic, Engine, FnPtr};
use vibelang_dsp::helpers::{EnvGenParam, EnvelopeBuilder};
use vibelang_dsp::{encode_synthdef, GraphIR, Input, NodeRef, SynthDef, UGenNode};

fn body_closure(body: &str) -> FnPtr {
    Engine::new().eval(body).expect("parse synth body closure")
}

fn build_voice_graph(name: &str, params: &[(&str, f64)], body: &str) -> GraphIR {
    let mut synthdef = SynthDef::new(name.to_string());
    for (name, default) in params {
        synthdef.arg_f((*name).to_string(), *default);
    }
    synthdef
        .build_body_closure(body_closure(body))
        .expect("build synthdef graph")
}

fn env_gen_node(ir: &GraphIR) -> &UGenNode {
    ir.nodes
        .iter()
        .find(|node| node.name == "EnvGen")
        .expect("EnvGen node")
}

fn assert_constant(input: &Input, expected: f32) {
    match input {
        Input::Constant(actual) => assert!(
            (actual - expected).abs() < 1e-6,
            "expected constant {}, got {}",
            expected,
            actual
        ),
        other => panic!("expected Constant({}), got {:?}", expected, other),
    }
}

fn assert_control_input(input: &Input, expected_slot: u32) {
    match input {
        Input::Node {
            node_id,
            output_index,
        } => {
            assert_eq!(*node_id, 0, "control params should read from Control node");
            assert_eq!(*output_index, expected_slot);
        }
        other => panic!(
            "expected Control input slot {}, got {:?}",
            expected_slot, other
        ),
    }
}

#[test]
fn node_ref_attack_time_emits_envgen_node_input() {
    let ir = build_voice_graph(
        "node_ref_attack_env",
        &[("atk", 0.02)],
        "|atk| envelope().asr(atk, 1.0, \"100ms\").cleanup_on_finish().build()",
    );

    let env = env_gen_node(&ir);
    assert_control_input(&env.inputs[10], 0);
    assert_constant(&env.inputs[14], 0.1);
    encode_synthdef(&ir).expect("node-backed EnvGen time should encode");
}

#[test]
fn constant_and_humantime_times_stay_constants() {
    let ir = build_voice_graph(
        "constant_env_times",
        &[],
        "|| envelope().asr(0.01, 1.0, \"100ms\").build()",
    );

    let env = env_gen_node(&ir);
    assert_constant(&env.inputs[10], 0.01);
    assert_constant(&env.inputs[14], 0.1);
    encode_synthdef(&ir).expect("constant EnvGen times should encode");
}

#[test]
fn builder_preserves_node_ref_attack_in_env() {
    let attack = NodeRef::new(42);
    let env = EnvelopeBuilder::new()
        .asr(Dynamic::from(attack), 1.0, Dynamic::from(0.2_f64))
        .determine_envelope();

    match &env.times[0] {
        EnvGenParam::Node(node) => assert_eq!(*node, attack),
        other => panic!("expected Node attack time, got {:?}", other),
    }
}

#[test]
fn invalid_dynamic_time_errors_instead_of_defaulting() {
    let err = EnvelopeBuilder::new()
        .attack(Dynamic::from(true))
        .build()
        .expect_err("invalid attack should fail loudly");
    let msg = err.to_string();
    assert!(
        msg.contains("attack") && msg.contains("Expected number or duration string"),
        "error should mention the invalid attack value, got: {}",
        msg
    );
}

#[test]
fn param_driven_asr_attack_and_release_reference_controls() {
    let ir = build_voice_graph(
        "param_driven_asr_env",
        &[("atk", 0.01), ("rel", 0.2)],
        "|atk, rel| {
            let env = envelope()
                .asr(atk, 1.0, rel)
                .cleanup_on_finish()
                .build();
            sin_osc_ar(440.0, 0.0) * env
        }",
    );

    let env = env_gen_node(&ir);
    assert_control_input(&env.inputs[10], 0);
    assert_control_input(&env.inputs[14], 1);
    encode_synthdef(&ir).expect("param-driven EnvGen times should encode");
}
