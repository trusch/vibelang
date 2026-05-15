use rhai::{Engine, FnPtr};
use vibelang_dsp::{encode_synthdef, GraphIR, Input, SynthDef, UGenNode};

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

#[test]
fn local_buf_graph_inserts_matching_max_local_bufs() {
    let ir = build_voice_graph(
        "uses_local_buf",
        "|| {
            let left = local_buf_ir(1.0, 128.0);
            let right = local_buf_ir(2.0, 256.0);
            sin_osc_ar(440.0, 0.0)
        }",
    );

    let max_local_bufs = nodes_named(&ir, "MaxLocalBufs");
    assert_eq!(
        max_local_bufs.len(),
        1,
        "LocalBuf users should emit exactly one MaxLocalBufs declaration"
    );

    let (max_index, max_node) = max_local_bufs[0];
    assert_eq!(max_node.rate, vibelang_dsp::Rate::Scalar);
    assert_eq!(max_node.num_outputs, 1);
    assert_eq!(max_node.inputs.len(), 1);
    match max_node.inputs[0] {
        Input::Constant(n) => assert_eq!(n, 2.0),
        ref other => panic!("MaxLocalBufs input should be Constant(2), got {other:?}"),
    }

    let local_bufs = nodes_named(&ir, "LocalBuf");
    assert_eq!(local_bufs.len(), 2);
    assert!(
        local_bufs.iter().all(|(index, _)| max_index < *index),
        "MaxLocalBufs must be emitted before LocalBuf nodes"
    );

    let bytes = encode_synthdef(&ir).expect("encode LocalBuf graph");
    assert!(
        bytes
            .windows(b"MaxLocalBufs".len())
            .any(|w| w == b"MaxLocalBufs"),
        "encoded graph should contain MaxLocalBufs"
    );
}

#[test]
fn non_local_buf_graph_does_not_insert_max_local_bufs() {
    let ir = build_voice_graph("no_local_buf", "|| sin_osc_ar(440.0, 0.0)");

    assert!(
        nodes_named(&ir, "LocalBuf").is_empty(),
        "test fixture should not contain LocalBuf"
    );
    assert!(
        nodes_named(&ir, "MaxLocalBufs").is_empty(),
        "non-LocalBuf graphs should not gain MaxLocalBufs"
    );

    let bytes = encode_synthdef(&ir).expect("encode non-LocalBuf graph");
    assert!(
        !bytes
            .windows(b"MaxLocalBufs".len())
            .any(|w| w == b"MaxLocalBufs"),
        "encoded graph should not contain MaxLocalBufs"
    );
}
