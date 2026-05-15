use rhai::{Engine, FnPtr};
use vibelang_dsp::{encode_synthdef, GraphIR, SynthDef, UGenNode};

fn body_closure(body: &str) -> FnPtr {
    Engine::new().eval(body).expect("parse synth body closure")
}

fn build_voice_graph(name: &str, body: &str) -> GraphIR {
    SynthDef::new(name.to_string())
        .build_body_closure(body_closure(body))
        .expect("build synthdef graph")
}

fn nodes_named<'a>(ir: &'a GraphIR, name: &str) -> Vec<(usize, &'a UGenNode)> {
    ir.nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.name == name)
        .collect()
}

fn encoded_contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn binary_operator_manifest_entry_lowers_to_binary_op_ugen() {
    let ir = build_voice_graph(
        "uses_binary_op_pseudo",
        "|| ring1_ar(sin_osc_ar(110.0, 0.0), 0.5)",
    );

    assert!(
        nodes_named(&ir, "Ring1").is_empty(),
        "Ring1 is a sclang pseudo-UGen and must not be emitted literally"
    );

    let binary_ops = nodes_named(&ir, "BinaryOpUGen");
    assert_eq!(binary_ops.len(), 1);
    let (_, node) = binary_ops[0];
    assert_eq!(node.special_index, 30);
    assert_eq!(node.num_outputs, 1);
    assert_eq!(node.inputs.len(), 2);

    let bytes = encode_synthdef(&ir).expect("encode Ring1-lowered graph");
    assert!(encoded_contains(&bytes, b"BinaryOpUGen"));
    assert!(!encoded_contains(&bytes, b"Ring1"));
}

#[test]
fn unary_operator_manifest_entry_lowers_to_unary_op_ugen() {
    let ir = build_voice_graph("uses_unary_op_pseudo", "|| tanh_ar(sin_osc_ar(220.0, 0.0))");

    assert!(
        nodes_named(&ir, "Tanh").is_empty(),
        "Tanh is a UnaryOpUGen selector helper and must not be emitted literally"
    );

    let unary_ops = nodes_named(&ir, "UnaryOpUGen");
    assert_eq!(unary_ops.len(), 1);
    let (_, node) = unary_ops[0];
    assert_eq!(node.special_index, 28);
    assert_eq!(node.num_outputs, 1);
    assert_eq!(node.inputs.len(), 1);

    let bytes = encode_synthdef(&ir).expect("encode Tanh-lowered graph");
    assert!(encoded_contains(&bytes, b"UnaryOpUGen"));
    assert!(!encoded_contains(&bytes, b"Tanh"));
}
