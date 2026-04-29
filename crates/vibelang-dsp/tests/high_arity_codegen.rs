//! Verifies the build.rs branch that emits `register_raw_fn` for UGens with
//! more than 20 inputs (rhai's `def_register!(A:20, …)` cap). Drives the
//! synthetic `BigArity24` fixture (`ugen_manifests/_test_arity_stub.json`)
//! through Rhai with all 24 args and checks the resulting graph node has all
//! 24 inputs wired. See `kb/synthdef-arity-limits-plan.md` §3.1, §4.1 Phase 4.

use rhai::{Dynamic, Engine};
use vibelang_dsp::{
    clear_active_builder, register_dsp_api, set_active_builder, GraphBuilderInner,
};

#[test]
fn big_arity24_register_raw_fn_full_arity_round_trip() {
    let mut engine = Engine::new();
    register_dsp_api(&mut engine);

    set_active_builder(GraphBuilderInner::new());

    // 24 distinct float literals — exercises the register_raw_fn arity.
    // Each arg is unique so we can verify it lands in the right slot.
    let args: Vec<String> = (0..24).map(|i| format!("{}.0", i + 1)).collect();
    let script = format!("big_arity24_ar({});", args.join(", "));

    let result: Result<Dynamic, _> = engine.eval(&script);
    let builder = clear_active_builder().expect("active builder vanished");

    if let Err(e) = result {
        panic!("rhai eval of `{}` failed: {}", script, e);
    }

    let big = builder
        .nodes
        .iter()
        .find(|n| n.name == "BigArity24")
        .expect("BigArity24 node not found in builder after eval");

    assert_eq!(
        big.inputs.len(),
        24,
        "BigArity24 should have all 24 inputs wired through register_raw_fn, got {}",
        big.inputs.len()
    );

    // Each input should be a Constant matching the literal we passed (i + 1).
    for (i, input) in big.inputs.iter().enumerate() {
        match input {
            vibelang_dsp::Input::Constant(v) => {
                let expected = (i + 1) as f32;
                assert!(
                    (v - expected).abs() < 1e-6,
                    "input {} expected constant {}, got {}",
                    i,
                    expected,
                    v
                );
            }
            other => panic!("input {} expected Constant, got {:?}", i, other),
        }
    }
}

/// Mid-arity (21) call also goes through register_raw_fn — explicit args 0..20
/// supplied, last 3 default-filled by the codegen template.
#[test]
fn big_arity24_register_raw_fn_partial_arity_with_defaults() {
    let mut engine = Engine::new();
    register_dsp_api(&mut engine);

    set_active_builder(GraphBuilderInner::new());

    let args: Vec<String> = (0..21).map(|i| format!("{}.0", i + 1)).collect();
    let script = format!("big_arity24_ar({});", args.join(", "));

    let result: Result<Dynamic, _> = engine.eval(&script);
    let builder = clear_active_builder().expect("active builder vanished");

    if let Err(e) = result {
        panic!("rhai eval of `{}` failed: {}", script, e);
    }

    let big = builder
        .nodes
        .iter()
        .find(|n| n.name == "BigArity24")
        .expect("BigArity24 node not found in builder after eval");

    assert_eq!(big.inputs.len(), 24, "node should still have 24 input slots");

    // First 21 are explicit; last 3 default to 0.0 from the manifest.
    for (i, input) in big.inputs.iter().enumerate() {
        let expected = if i < 21 { (i + 1) as f32 } else { 0.0 };
        match input {
            vibelang_dsp::Input::Constant(v) => assert!(
                (v - expected).abs() < 1e-6,
                "input {} expected constant {}, got {}",
                i,
                expected,
                v
            ),
            other => panic!("input {} expected Constant, got {:?}", i, other),
        }
    }
}
