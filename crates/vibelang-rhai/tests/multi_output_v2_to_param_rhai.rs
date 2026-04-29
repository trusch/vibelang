//! Multi-output v2 Story 9 — Rhai-surface error tests for `.to_param(...)`.
//!
//! Drives the API end-to-end through [`ScriptEngine::execute`], complementing
//! the unit-level coverage in `crates/vibelang-rhai/src/api/route.rs` and the
//! backend-driven integration tests in
//! `crates/vibelang-core/tests/multi_output_v2_kr_routing.rs`. Two of the
//! seven Story 9 ticket scenarios live here:
//!
//!   3. **ar-rate port → `.to_param` returns a clean Rhai error** —
//!      [`rhai_to_param_on_ar_rate_port_returns_clean_error`]. The error
//!      cites the offending port name, its actual ar rate, and points the
//!      user at the `.output_kr(...)` remediation. No partial route is
//!      installed in `state.param_routes`.
//!
//!   4. **Unknown target param → error citing available params** —
//!      [`rhai_to_param_unknown_target_param_errors_with_available_set`].
//!      The error names the missing param and the full available param set
//!      on the target synthdef so a typo is fixable without scrolling back
//!      to the synthdef definition. No partial route is installed.

use vibelang_dsp::{
    register_synthdef_ir, register_synthdef_outputs, GraphIR, OutputPort, ParamSpec, PortRate,
};
use vibelang_rhai::ScriptEngine;

fn declare_kr_synthdef(synth: &str, kr_ports: &[&str]) {
    let outputs: Vec<OutputPort> = kr_ports
        .iter()
        .map(|n| OutputPort {
            name: (*n).to_string(),
            channels: 1,
            rate: PortRate::Kr,
        })
        .collect();
    register_synthdef_outputs(synth.to_string(), outputs);
}

fn declare_ar_synthdef(synth: &str, ar_ports: &[&str]) {
    let outputs: Vec<OutputPort> = ar_ports
        .iter()
        .map(|n| OutputPort {
            name: (*n).to_string(),
            channels: 1,
            rate: PortRate::Ar,
        })
        .collect();
    register_synthdef_outputs(synth.to_string(), outputs);
}

/// Register a synthdef IR carrying the given param names so
/// `get_synthdef_param_defaults` returns them as the available param set
/// for `.to_param(...)`'s target-side validation.
fn declare_synthdef_with_params(synth: &str, params: &[&str]) {
    let param_specs: Vec<ParamSpec> = params
        .iter()
        .enumerate()
        .map(|(i, n)| ParamSpec {
            name: (*n).to_string(),
            default: vec![0.0],
            index: i,
            lag_ms: None,
        })
        .collect();
    register_synthdef_ir(
        synth.to_string(),
        GraphIR {
            name: synth.to_string(),
            constants: vec![],
            params: param_specs,
            nodes: vec![],
            out_bus: 0,
        },
    );
}

#[test]
fn rhai_to_param_on_ar_rate_port_returns_clean_error() {
    // Source synthdef declares "sine" as ar-rate; target synthdef declares
    // a "cutoff" param. Calling `.to_param(target, "cutoff")` on the ar
    // port must abort with a Rhai error that cites the rate, port name,
    // and the `.output_kr(...)` remediation. No entry is installed in
    // `state.param_routes`.
    let src_synth = "story9_rhai_to_param_ar_src";
    let tgt_synth = "story9_rhai_to_param_ar_tgt";
    declare_ar_synthdef(src_synth, &["sine"]);
    declare_synthdef_with_params(tgt_synth, &["cutoff"]);

    let script = format!(
        r#"
        let src = voice("vox_story9_ar_src").synth("{src}");
        let tgt = voice("vox_story9_ar_tgt").synth("{tgt}");
        src.output("sine").to_param(tgt, "cutoff");
        "#,
        src = src_synth,
        tgt = tgt_synth,
    );

    let mut engine = ScriptEngine::new();
    let err = engine
        .execute(&script)
        .expect_err("ar-rate source port must abort the script");
    let msg = err.to_string();
    assert!(
        msg.contains("ar-rate"),
        "error must name the actual rate; msg = {}",
        msg,
    );
    assert!(
        msg.contains("'sine'"),
        "error must name the offending port; msg = {}",
        msg,
    );
    assert!(
        msg.contains("output_kr"),
        "error must point at the .output_kr(...) remediation; msg = {}",
        msg,
    );

    // The no-partial-install invariant is covered by the unit-test suite in
    // `crates/vibelang-rhai/src/api/route.rs` (which inspects
    // `state.param_routes` directly through the lower-level
    // `output_by_name(...).to_param(...)` path). Here we only assert the
    // user-facing Rhai error contract.
}

#[test]
fn rhai_to_param_unknown_target_param_errors_with_available_set() {
    // Target synthdef declares params {cutoff, resonance, amp}. A
    // `.to_param(tgt, "freq")` on a kr-rate source port must abort with a
    // Rhai error that names the missing param and the full available set,
    // so a typo is fixable without scrolling back to the synthdef
    // definition.
    let src_synth = "story9_rhai_to_param_unknown_src";
    let tgt_synth = "story9_rhai_to_param_unknown_tgt";
    declare_kr_synthdef(src_synth, &["env"]);
    declare_synthdef_with_params(tgt_synth, &["cutoff", "resonance", "amp"]);

    let script = format!(
        r#"
        let src = voice("vox_story9_unk_src").synth("{src}");
        let tgt = voice("vox_story9_unk_tgt").synth("{tgt}");
        src.output("env").to_param(tgt, "freq");
        "#,
        src = src_synth,
        tgt = tgt_synth,
    );

    let mut engine = ScriptEngine::new();
    let err = engine
        .execute(&script)
        .expect_err("unknown target param must abort the script");
    let msg = err.to_string();
    assert!(
        msg.contains("'freq'"),
        "error must name the missing target param; msg = {}",
        msg,
    );
    assert!(
        msg.contains("'cutoff'"),
        "error must list the available 'cutoff' param; msg = {}",
        msg,
    );
    assert!(
        msg.contains("'resonance'"),
        "error must list the available 'resonance' param; msg = {}",
        msg,
    );
    assert!(
        msg.contains("'amp'"),
        "error must list the available 'amp' param; msg = {}",
        msg,
    );
}
