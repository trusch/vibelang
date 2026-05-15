use rhai::{Dynamic, Engine};
use vibelang_dsp::{
    clear_active_builder, encode_synthdef, register_dsp_api, set_active_builder, GraphBuilderInner,
    GraphIR,
};

fn build_via_rhai(script: &str) -> GraphIR {
    let mut engine = Engine::new();
    register_dsp_api(&mut engine);
    set_active_builder(GraphBuilderInner::new());
    let _: Dynamic = engine
        .eval(script)
        .unwrap_or_else(|e| panic!("rhai eval failed for {script:?}: {e}"));
    let builder = clear_active_builder().expect("active builder vanished");
    GraphIR::from_builder("pseudo_ugen_lowering".to_string(), builder)
}

fn node_names(ir: &GraphIR) -> Vec<&str> {
    ir.nodes.iter().map(|node| node.name.as_str()).collect()
}

fn assert_no_encoded_name(ir: &GraphIR, name: &str) {
    let bytes = encode_synthdef(ir).expect("encode graph");
    assert!(
        !bytes
            .windows(name.len())
            .any(|window| window == name.as_bytes()),
        "encoded graph must not contain literal pseudo-UGen name {name}"
    );
}

#[test]
fn changed_lowers_to_hpz1_abs_gt() {
    let ir = build_via_rhai("changed_kr(dc_kr(0.0), 0.001);");
    let names = node_names(&ir);

    assert!(!names.contains(&"Changed"), "got {names:?}");
    assert!(names.contains(&"HPZ1"), "got {names:?}");
    assert!(
        ir.nodes
            .iter()
            .any(|node| node.name == "UnaryOpUGen" && node.special_index == 5),
        "Changed must lower through abs UnaryOpUGen"
    );
    assert!(
        ir.nodes
            .iter()
            .any(|node| node.name == "BinaryOpUGen" && node.special_index == 9),
        "Changed must lower through greater-than BinaryOpUGen"
    );
    assert_no_encoded_name(&ir, "Changed");
}

#[test]
fn lin_lin_lowers_to_mul_add_graph() {
    let ir = build_via_rhai("lin_lin_kr(dc_kr(0.5), 0.0, 1.0, 200.0, 2000.0);");
    let names = node_names(&ir);

    assert!(!names.contains(&"LinLin"), "got {names:?}");
    assert!(names.contains(&"MulAdd"), "got {names:?}");
    assert!(
        names.contains(&"BinaryOpUGen"),
        "LinLin scale/offset should be arithmetic nodes; got {names:?}"
    );
    assert_no_encoded_name(&ir, "LinLin");
}

#[test]
fn jpverb_and_greyhole_lower_to_raw_stereo_wrappers() {
    let jpverb = build_via_rhai(
        r#"
        let left = sin_osc_ar(220.0, 0.0);
        let right = sin_osc_ar(330.0, 0.0);
        j_pverb_ar([left, right]);
        "#,
    );
    let jp_names = node_names(&jpverb);
    assert!(!jp_names.contains(&"JPverb"), "got {jp_names:?}");
    assert!(jp_names.contains(&"JPverbRaw"), "got {jp_names:?}");
    let raw = jpverb
        .nodes
        .iter()
        .find(|node| node.name == "JPverbRaw")
        .expect("JPverbRaw node");
    assert_eq!(raw.num_outputs, 2);
    assert_eq!(raw.inputs.len(), 13);
    assert_no_encoded_name(&jpverb, "JPverb\0");

    let greyhole = build_via_rhai(
        r#"
        let source = sin_osc_ar(110.0, 0.0);
        greyhole_ar(source);
        "#,
    );
    let grey_names = node_names(&greyhole);
    assert!(!grey_names.contains(&"Greyhole"), "got {grey_names:?}");
    assert!(grey_names.contains(&"GreyholeRaw"), "got {grey_names:?}");
    let raw = greyhole
        .nodes
        .iter()
        .find(|node| node.name == "GreyholeRaw")
        .expect("GreyholeRaw node");
    assert_eq!(raw.num_outputs, 2);
    assert_eq!(raw.inputs.len(), 9);
    assert_no_encoded_name(&greyhole, "Greyhole\0");
}

#[test]
fn mix_splay_and_splay_az_do_not_emit_literal_pseudos() {
    let ir = build_via_rhai(
        r#"
        let a = sin_osc_ar(220.0, 0.0);
        let b = sin_osc_ar(330.0, 0.0);
        mix_ar([a, b]);
        splay_ar([a, b]);
        splay_az_ar(4, [a, b]);
        "#,
    );
    let names = node_names(&ir);

    assert!(!names.contains(&"Mix"), "got {names:?}");
    assert!(!names.contains(&"Splay"), "got {names:?}");
    assert!(!names.contains(&"SplayAz"), "got {names:?}");
    assert!(names.contains(&"BinaryOpUGen"), "got {names:?}");
    assert!(names.contains(&"Pan2"), "got {names:?}");
    assert!(names.contains(&"PanAz"), "got {names:?}");
    assert_no_encoded_name(&ir, "Mix");
    assert_no_encoded_name(&ir, "Splay");
    assert_no_encoded_name(&ir, "SplayAz");
}
