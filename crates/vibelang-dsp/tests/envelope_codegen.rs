use rhai::{Dynamic, Engine, FnPtr};
use vibelang_dsp::helpers::{EnvGenParam, EnvelopeBuilder};
use vibelang_dsp::{encode_synthdef, GraphIR, Input, NodeRef, SynthDef, UGenNode};

/// Encoded-synthdef goldens captured on the commit *before* envelope levels
/// became `EnvGenParam` (vibelang main @ 861d376). Any envelope whose levels
/// are all numeric must keep encoding to exactly these bytes: widening the
/// level type is a compile-time change only, with no effect on the constant
/// table, its ordering, or the emitted EnvGen inputs.
mod goldens {
    pub const NUMERIC_ADSR: &str = "534367660000000200010c6e756d657269635f616473720000000b3f80000000000000400000004080000040400000bf8000003c23d70a3f4ccccd3e4ccccd3f00000043dc0000000000010000000000000001036f7574000000000000000707436f6e74726f6c0100000000000000010000010244430200000001000000010000ffffffff000000000206456e7647656e02000000190000000100000000000100000000ffffffff00000000ffffffff00000001ffffffff00000000ffffffff00000002ffffffff00000001ffffffff00000003ffffffff00000004ffffffff00000005ffffffff00000000ffffffff00000006ffffffff00000000ffffffff00000000ffffffff00000007ffffffff00000008ffffffff00000000ffffffff00000000ffffffff00000007ffffffff00000001ffffffff00000000ffffffff00000000ffffffff00000001ffffffff00000009ffffffff00000000ffffffff00000000020653696e4f73630200000002000000010000ffffffff0000000affffffff00000001020c42696e6172794f705547656e020000000200000001000200000003000000000000000200000000020450616e3202000000030000000200000000000400000000ffffffff00000001ffffffff000000000202034f757402000000030000000000000000000000000000000000050000000000000005000000010000";

    pub const NUMERIC_ASR: &str = "534367660000000200010b6e756d657269635f617372000000083f8000000000000040000000bf8000003f3333333c23d70a3e99999a43dc0000000000010000000000000001036f7574000000000000000707436f6e74726f6c0100000000000000010000010244430200000001000000010000ffffffff000000000206456e7647656e02000000110000000100000000000100000000ffffffff00000000ffffffff00000001ffffffff00000000ffffffff00000001ffffffff00000001ffffffff00000002ffffffff00000000ffffffff00000003ffffffff00000004ffffffff00000005ffffffff00000000ffffffff00000000ffffffff00000001ffffffff00000006ffffffff00000000ffffffff00000000020653696e4f73630200000002000000010000ffffffff00000007ffffffff00000001020c42696e6172794f705547656e020000000200000001000200000003000000000000000200000000020450616e3202000000030000000200000000000400000000ffffffff00000001ffffffff000000000202034f757402000000030000000000000000000000000000000000050000000000000005000000010000";

    pub const NUMERIC_PERC: &str = "534367660000000200010c6e756d657269635f70657263000000073f8000000000000040000000bf8000003ba3d70a3ecccccd43dc0000000000010000000000000001036f7574000000000000000707436f6e74726f6c0100000000000000010000010244430200000001000000010000ffffffff000000000206456e7647656e02000000110000000100000000000100000000ffffffff00000000ffffffff00000001ffffffff00000000ffffffff00000001ffffffff00000001ffffffff00000002ffffffff00000003ffffffff00000003ffffffff00000000ffffffff00000004ffffffff00000000ffffffff00000000ffffffff00000001ffffffff00000005ffffffff00000000ffffffff00000000020653696e4f73630200000002000000010000ffffffff00000006ffffffff00000001020c42696e6172794f705547656e020000000200000001000200000003000000000000000200000000020450616e3202000000030000000200000000000400000000ffffffff00000001ffffffff000000000202034f757402000000030000000000000000000000000000000000050000000000000005000000010000";

    pub const NUMERIC_ENV_CTOR: &str = "53436766000000020001106e756d657269635f656e765f63746f72000000093f8000000000000040400000bf8000003c23d70a3e99999a3dcccccd3ecccccd43dc0000000000010000000000000001036f7574000000000000000707436f6e74726f6c0100000000000000010000010244430200000001000000010000ffffffff000000000206456e7647656e02000000150000000100000000000100000000ffffffff00000000ffffffff00000001ffffffff00000000ffffffff00000001ffffffff00000001ffffffff00000002ffffffff00000003ffffffff00000003ffffffff00000000ffffffff00000004ffffffff00000000ffffffff00000000ffffffff00000005ffffffff00000006ffffffff00000000ffffffff00000000ffffffff00000001ffffffff00000007ffffffff00000000ffffffff00000000020653696e4f73630200000002000000010000ffffffff00000008ffffffff00000001020c42696e6172794f705547656e020000000200000001000200000003000000000000000200000000020450616e3202000000030000000200000000000400000000ffffffff00000001ffffffff000000000202034f757402000000030000000000000000000000000000000000050000000000000005000000010000";

    pub const PARAM_TIMES_NUMERIC_LEVELS: &str = "534367660000000200011a706172616d5f74696d65735f6e756d657269635f6c6576656c73000000093f80000000000000400000004080000040400000bf8000003f4ccccd3e4ccccd43dc0000000000033c23d70a3e4ccccd00000000000000030361746b000000000372656c00000001036f7574000000020000000707436f6e74726f6c01000000000000000300000101010244430200000001000000010000ffffffff000000000206456e7647656e02000000190000000100000000000100000000ffffffff00000000ffffffff00000001ffffffff00000000ffffffff00000002ffffffff00000001ffffffff00000003ffffffff00000004ffffffff00000005ffffffff000000000000000000000000ffffffff00000000ffffffff00000000ffffffff00000006ffffffff00000007ffffffff00000000ffffffff00000000ffffffff00000006ffffffff00000001ffffffff00000000ffffffff00000000ffffffff000000010000000000000001ffffffff00000000ffffffff00000000020653696e4f73630200000002000000010000ffffffff00000008ffffffff00000001020c42696e6172794f705547656e020000000200000001000200000003000000000000000200000000020450616e3202000000030000000200000000000400000000ffffffff00000001ffffffff000000000202034f757402000000030000000000000000000000000002000000050000000000000005000000010000";
}

fn body_closure(body: &str) -> FnPtr {
    Engine::new().eval(body).expect("parse synth body closure")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn assert_encodes_to_golden(name: &str, params: &[(&str, f64)], body: &str, golden: &str) {
    let ir = build_voice_graph(name, params, body);
    let bytes = encode_synthdef(&ir).expect("encode synthdef");
    assert_eq!(
        hex(&bytes),
        golden,
        "{} must encode byte-identically to the pre-change baseline",
        name
    );
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
        .asr(
            Dynamic::from(attack),
            Dynamic::from(1.0_f64),
            Dynamic::from(0.2_f64),
        )
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
fn all_numeric_envelopes_encode_byte_identically() {
    assert_encodes_to_golden(
        "numeric_adsr",
        &[],
        "|| { let e = envelope().adsr(0.01, 0.2, 0.8, 0.5).cleanup_on_finish().build(); sin_osc_ar(440.0, 0.0) * e }",
        goldens::NUMERIC_ADSR,
    );
    assert_encodes_to_golden(
        "numeric_asr",
        &[],
        "|| { let e = envelope().asr(0.01, 0.7, 0.3).build(); sin_osc_ar(440.0, 0.0) * e }",
        goldens::NUMERIC_ASR,
    );
    assert_encodes_to_golden(
        "numeric_perc",
        &[],
        "|| { let e = envelope().perc(0.005, 0.4).build(); sin_osc_ar(440.0, 0.0) * e }",
        goldens::NUMERIC_PERC,
    );
    assert_encodes_to_golden(
        "numeric_env_ctor",
        &[],
        "|| { let e = Env([0.0, 1.0, 0.3, 0.0], [0.01, 0.1, 0.4], 1.0); \
              let g = NewEnvGenBuilder(e, dc_ar(1.0)).build(); \
              sin_osc_ar(440.0, 0.0) * g }",
        goldens::NUMERIC_ENV_CTOR,
    );
}

/// Widening levels must not disturb the envelopes that already mixed
/// control-rate times with numeric levels.
#[test]
fn param_times_with_numeric_levels_encode_byte_identically() {
    assert_encodes_to_golden(
        "param_times_numeric_levels",
        &[("atk", 0.01), ("rel", 0.2)],
        "|atk, rel| { let e = envelope().adsr(atk, 0.2, 0.8, rel).cleanup_on_finish().build(); \
                      sin_osc_ar(440.0, 0.0) * e }",
        goldens::PARAM_TIMES_NUMERIC_LEVELS,
    );
}

/// The exact shape that used to fail with
/// `Function not found: adsr (EnvelopeBuilder, NodeRef x4)`.
///
/// ADSR EnvGen input layout: 5 fixed inputs (gate, level_scale, level_bias,
/// time_scale, done_action), then 4 envelope header inputs, then 4 inputs per
/// stage (level, time, shape, curve). The sustain level is the stage-1 and
/// stage-2 level, i.e. inputs 13 and 17.
#[test]
fn param_driven_adsr_sustain_references_control() {
    let ir = build_voice_graph(
        "param_driven_adsr_env",
        &[("atk", 0.01), ("dec", 0.2), ("sus", 0.8), ("rel", 0.5)],
        "|atk, dec, sus, rel| {
            let env = envelope()
                .adsr(atk, dec, sus, rel)
                .cleanup_on_finish()
                .build();
            sin_osc_ar(440.0, 0.0) * env
        }",
    );

    let env = env_gen_node(&ir);
    assert_control_input(&env.inputs[10], 0); // attack time
    assert_control_input(&env.inputs[13], 2); // sustain level (decay target)
    assert_control_input(&env.inputs[14], 1); // decay time
    assert_control_input(&env.inputs[17], 2); // sustain level (hold)
    assert_control_input(&env.inputs[22], 3); // release time
    encode_synthdef(&ir).expect("param-driven sustain should encode");
}

/// A param sustain reaches the graph through the standalone `.sustain()`
/// setter too, not only through the four-argument `.adsr()` form.
#[test]
fn param_driven_sustain_setter_references_control() {
    let ir = build_voice_graph(
        "param_sustain_setter_env",
        &[("sus", 0.6)],
        "|sus| {
            let env = envelope()
                .attack(\"10ms\")
                .decay(\"100ms\")
                .sustain(sus)
                .release(\"200ms\")
                .build();
            sin_osc_ar(440.0, 0.0) * env
        }",
    );

    let env = env_gen_node(&ir);
    assert_control_input(&env.inputs[13], 0);
    assert_control_input(&env.inputs[17], 0);
    encode_synthdef(&ir).expect("param sustain via setter should encode");
}

/// ASR carries its sustain as the stage-0 level, i.e. input 9.
#[test]
fn param_driven_asr_sustain_references_control() {
    let ir = build_voice_graph(
        "param_driven_asr_sustain_env",
        &[("sus", 0.9)],
        "|sus| {
            let env = envelope().asr(\"10ms\", sus, \"200ms\").build();
            sin_osc_ar(440.0, 0.0) * env
        }",
    );

    let env = env_gen_node(&ir);
    assert_control_input(&env.inputs[9], 0);
    encode_synthdef(&ir).expect("param-driven ASR sustain should encode");
}

/// The `Env(levels, times, curve)` array constructor takes a control source
/// in the levels array, symmetrically with the times array.
#[test]
fn env_array_constructor_accepts_control_source_level() {
    let ir = build_voice_graph(
        "env_ctor_control_level",
        &[("lvl", 0.5)],
        "|lvl| {
            let e = Env([0.0, lvl, 0.0], [0.01, 0.4], 1.0);
            let g = NewEnvGenBuilder(e, dc_ar(1.0)).build();
            sin_osc_ar(440.0, 0.0) * g
        }",
    );

    let env = env_gen_node(&ir);
    assert_control_input(&env.inputs[9], 0);
    encode_synthdef(&ir).expect("control-source Env level should encode");
}

/// A control source as the *initial* level is the one position that is not a
/// stage level; it is EnvGen input 5.
#[test]
fn env_array_constructor_accepts_control_source_initial_level() {
    let ir = build_voice_graph(
        "env_ctor_control_init_level",
        &[("init", 0.25)],
        "|init| {
            let e = Env([init, 1.0, 0.0], [0.01, 0.4], 1.0);
            let g = NewEnvGenBuilder(e, dc_ar(1.0)).build();
            sin_osc_ar(440.0, 0.0) * g
        }",
    );

    let env = env_gen_node(&ir);
    assert_control_input(&env.inputs[5], 0);
    encode_synthdef(&ir).expect("control-source initial level should encode");
}

/// Integer levels used to be swallowed by a `try_cast::<f64>().unwrap_or(0.0)`,
/// so `Env([0, 1, 0], ...)` silently rendered as an all-zero (silent)
/// envelope. Levels now parse integers the way times always have.
#[test]
fn integer_env_levels_are_not_silently_zero() {
    let ir = build_voice_graph(
        "integer_env_levels",
        &[],
        "|| {
            let e = Env([0, 1, 0], [0.01, 0.4], 1.0);
            let g = NewEnvGenBuilder(e, dc_ar(1.0)).build();
            sin_osc_ar(440.0, 0.0) * g
        }",
    );

    let env = env_gen_node(&ir);
    assert_constant(&env.inputs[9], 1.0);
    encode_synthdef(&ir).expect("integer Env levels should encode");
}

#[test]
fn invalid_dynamic_sustain_errors_instead_of_defaulting() {
    let err = EnvelopeBuilder::new()
        .sustain(Dynamic::from("loud"))
        .build()
        .expect_err("invalid sustain should fail loudly");
    let msg = err.to_string();
    assert!(
        msg.contains("sustain") && msg.contains("Expected number or control source"),
        "error should mention the invalid sustain value, got: {}",
        msg
    );
}

#[test]
fn builder_preserves_node_ref_sustain_in_env() {
    let sustain = NodeRef::new(7);
    let env = EnvelopeBuilder::new()
        .adsr(
            Dynamic::from(0.01_f64),
            Dynamic::from(0.2_f64),
            Dynamic::from(sustain),
            Dynamic::from(0.5_f64),
        )
        .determine_envelope();

    // ADSR levels are [0, peak, sustain, sustain, 0].
    for index in [2, 3] {
        match &env.levels[index] {
            EnvGenParam::Node(node) => assert_eq!(*node, sustain),
            other => panic!("expected Node sustain level, got {:?}", other),
        }
    }
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
