//! Verify `sound_in_*` helpers emit a real server-side graph.
//!
//! `SoundIn` is sclang-only — the SC server has no plugin by that name.
//! These helpers must expand to `In.ar(NumOutputBuses.ir + bus, n)` so the
//! synthdef adapts to whatever `-o` value scsynth was started with.
//! A regression here was the cause of the 2026-05-03 studio-setup outage:
//! a hand-rolled synthdef hardcoded `+ 2.0` and silently broke when the
//! user grew their interface from 2 outputs to 10.

use rhai::{Dynamic, Engine};
use vibelang_dsp::{
    clear_active_builder, encode_synthdef, register_dsp_api, set_active_builder,
    GraphBuilderInner, GraphIR,
};

fn build_via_rhai(script: &str) -> GraphIR {
    let mut engine = Engine::new();
    register_dsp_api(&mut engine);
    set_active_builder(GraphBuilderInner::new());
    let _: Dynamic = engine
        .eval(script)
        .unwrap_or_else(|e| panic!("rhai eval failed for {script:?}: {e}"));
    let builder = clear_active_builder().expect("active builder vanished");
    GraphIR::from_builder("rt_sound_in".to_string(), builder)
}

fn ugen_names(ir: &GraphIR) -> Vec<&str> {
    ir.nodes.iter().map(|n| n.name.as_str()).collect()
}

#[test]
fn sound_in_channel_emits_in_plus_num_output_buses_no_soundin() {
    let ir = build_via_rhai("sound_in_channel(0);");
    let names = ugen_names(&ir);

    assert!(
        !names.contains(&"SoundIn"),
        "graph must not emit SoundIn (sclang-only); got {names:?}"
    );
    assert!(
        names.contains(&"NumOutputBuses"),
        "graph must include NumOutputBuses for input-bus offset; got {names:?}"
    );
    assert!(
        names.contains(&"In"),
        "graph must read inputs via In; got {names:?}"
    );

    // Bytes round-trip: NumOutputBuses appears verbatim, SoundIn does not.
    let bytes = encode_synthdef(&ir).expect("encode succeeds");
    assert!(
        bytes
            .windows("NumOutputBuses".len())
            .any(|w| w == b"NumOutputBuses"),
        "encoded bytes should contain NumOutputBuses"
    );
    assert!(
        !bytes.windows("SoundIn".len()).any(|w| w == b"SoundIn"),
        "encoded bytes must not contain SoundIn"
    );
}

#[test]
fn sound_in_channel_node_overload_does_not_emit_soundin() {
    // The NodeRef overload is what real synthdefs hit when `channel` is a
    // synth parameter (the bug-causing path in the 2026-05-03 outage). Drive
    // it with a kr node via dc_kr — same dispatch as a Control output.
    let ir = build_via_rhai(
        r#"
        let n = dc_kr(0.0);
        sound_in_channel(n)
        "#,
    );
    let names = ugen_names(&ir);
    assert!(
        !names.contains(&"SoundIn"),
        "NodeRef overload must not emit SoundIn; got {names:?}"
    );
    assert!(
        names.contains(&"NumOutputBuses"),
        "NodeRef overload must include NumOutputBuses; got {names:?}"
    );
    assert!(
        names.contains(&"BinaryOpUGen"),
        "NodeRef overload must add NumOutputBuses + channel; got {names:?}"
    );
}

#[test]
fn sound_in_multichannel_uses_num_output_buses_offset() {
    let ir = build_via_rhai("sound_in(2);");
    let names = ugen_names(&ir);

    assert!(
        !names.contains(&"SoundIn"),
        "sound_in must not emit SoundIn; got {names:?}"
    );
    assert!(
        names.contains(&"NumOutputBuses"),
        "sound_in must include NumOutputBuses; got {names:?}"
    );

    // For the multi-channel form we read directly from NumOutputBuses (no
    // BinaryOp offset), since the bus is the start of the input range.
    let in_node = ir
        .nodes
        .iter()
        .find(|n| n.name == "In")
        .expect("In node present");
    assert_eq!(in_node.num_outputs, 2, "In must have 2 output channels");
}

#[test]
fn sound_in_ar_alias_matches_sound_in_channel() {
    // `sound_in_ar(0)` is the manifest-style spelling and must produce the
    // same expansion as `sound_in_channel(0)`.
    let a = build_via_rhai("sound_in_ar(0);");
    let b = build_via_rhai("sound_in_channel(0);");
    assert_eq!(
        ugen_names(&a),
        ugen_names(&b),
        "sound_in_ar must alias sound_in_channel"
    );
}
