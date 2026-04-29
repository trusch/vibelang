//! Multi-output Story 14b — Rhai surface tests for `voice.output(...)`.
//!
//! Drives the API end-to-end through [`ScriptEngine::execute`]:
//!   1. Index out-of-range → clean Rhai error.
//!   2. Unknown port name → error message cites the declared port set.
//!   3. Two `.to(group)` calls on the same port → replace semantics
//!      (only one `(voice, port)` entry survives; the second `.to()` wins).

use vibelang_core::handlers::RouteDest;
use vibelang_core::types::{GroupId, VoiceId};
use vibelang_dsp::{register_synthdef_outputs, OutputPort};
use vibelang_rhai::ScriptEngine;

/// Mirror of `vibelang_rhai::context::hash_name_to_id` — entity IDs are
/// derived deterministically from names so the test can predict the
/// `VoiceId` / `GroupId` that ends up in `state.routes` without holding
/// a live script context.
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

fn declare_synth_with_ports(synth: &str, port_names: &[&str]) {
    let ports: Vec<OutputPort> = port_names
        .iter()
        .map(|n| OutputPort {
            name: (*n).to_string(),
            channels: 1,
            rate: vibelang_dsp::PortRate::Ar,
        })
        .collect();
    register_synthdef_outputs(synth.to_string(), ports);
}

#[test]
fn rhai_output_idx_out_of_range_errors_with_port_list() {
    // 2 declared ports — index 99 is well past the end.
    let synth = "story14b_rhai_oor_idx";
    declare_synth_with_ports(synth, &["sine", "even"]);

    let script = format!(
        r#"
        let v = voice("vox_oor14b").synth("{synth}");
        v.output(99).to(group("fx_oor14b"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let err = engine
        .execute(&script)
        .expect_err("idx 99 must abort script with a Rhai error");
    let msg = err.to_string();

    // Error mentions the offending index and the declared port count/names.
    assert!(msg.contains("99"), "msg = {}", msg);
    assert!(msg.contains("out of range"), "msg = {}", msg);
    assert!(msg.contains("'sine'"), "msg = {}", msg);
    assert!(msg.contains("'even'"), "msg = {}", msg);
}

#[test]
fn rhai_output_unknown_name_errors_cites_declared_port_set() {
    let synth = "story14b_rhai_unknown_name";
    declare_synth_with_ports(synth, &["sine", "even", "odd"]);

    let script = format!(
        r#"
        let v = voice("vox_unk14b").synth("{synth}");
        v.output("xyz").to(group("fx_unk14b"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let err = engine
        .execute(&script)
        .expect_err("unknown port name must abort script with a Rhai error");
    let msg = err.to_string();

    // Error names the bad input AND every declared port so the user can
    // fix the typo without re-reading the synthdef.
    assert!(msg.contains("xyz"), "msg = {}", msg);
    assert!(msg.contains("'sine'"), "msg = {}", msg);
    assert!(msg.contains("'even'"), "msg = {}", msg);
    assert!(msg.contains("'odd'"), "msg = {}", msg);
}

#[test]
fn rhai_outputs_name_list_to_group_fans_out() {
    // v.outputs(["sine", "odd"]).to(group("g")) → both "sine" and "odd"
    // route to g; the unlisted "even" port has no route entry.
    let synth = "ergo_rhai_outputs_names_to_group";
    declare_synth_with_ports(synth, &["sine", "even", "odd"]);

    let script = format!(
        r#"
        let v = voice("vox_outs_names_rhai").synth("{synth}");
        v.outputs(["sine", "odd"]).to(group("leads_outs_names"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_outs_names_rhai"));
    let g_id = GroupId::new(fnv1a_id("main/leads_outs_names"));

    assert_eq!(
        state.routes.get(&(voice_id, "sine".to_string())),
        Some(&RouteDest::Group(g_id))
    );
    assert_eq!(
        state.routes.get(&(voice_id, "odd".to_string())),
        Some(&RouteDest::Group(g_id))
    );
    assert!(state.routes.get(&(voice_id, "even".to_string())).is_none());
}

#[test]
fn rhai_outputs_idx_list_to_main_fans_out() {
    let synth = "ergo_rhai_outputs_idx_to_main";
    declare_synth_with_ports(synth, &["sine", "even", "odd"]);

    let script = format!(
        r#"
        let v = voice("vox_outs_idx_rhai").synth("{synth}");
        v.outputs([1, 2]).to_main();
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_outs_idx_rhai"));
    assert_eq!(
        state.routes.get(&(voice_id, "even".to_string())),
        Some(&RouteDest::Main)
    );
    assert_eq!(
        state.routes.get(&(voice_id, "odd".to_string())),
        Some(&RouteDest::Main)
    );
    assert!(state.routes.get(&(voice_id, "sine".to_string())).is_none());
}

#[test]
fn rhai_outputs_mixed_types_mute_fans_out() {
    let synth = "ergo_rhai_outputs_mixed_mute";
    declare_synth_with_ports(synth, &["sine", "even", "odd"]);

    let script = format!(
        r#"
        let v = voice("vox_outs_mixed_rhai").synth("{synth}");
        v.outputs(["sine", 2]).mute();
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_outs_mixed_rhai"));
    assert_eq!(
        state.routes.get(&(voice_id, "sine".to_string())),
        Some(&RouteDest::Muted)
    );
    assert_eq!(
        state.routes.get(&(voice_id, "odd".to_string())),
        Some(&RouteDest::Muted)
    );
}

#[test]
fn rhai_outputs_unknown_name_errors_cites_offender_and_ports() {
    let synth = "ergo_rhai_outputs_unknown_name";
    declare_synth_with_ports(synth, &["sine", "even", "odd"]);

    let script = format!(
        r#"
        let v = voice("vox_outs_unk_rhai").synth("{synth}");
        v.outputs(["sine", "triangle"]).to(group("any"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let err = engine
        .execute(&script)
        .expect_err("unknown name in list must abort with a Rhai error");
    let msg = err.to_string();
    assert!(msg.contains("triangle"), "msg = {}", msg);
    assert!(msg.contains("'sine'"), "msg = {}", msg);
    assert!(msg.contains("'even'"), "msg = {}", msg);
    assert!(msg.contains("'odd'"), "msg = {}", msg);
}

#[test]
fn rhai_outputs_empty_list_clean_error() {
    let synth = "ergo_rhai_outputs_empty";
    declare_synth_with_ports(synth, &["sine", "even"]);

    let script = format!(
        r#"
        let v = voice("vox_outs_empty_rhai").synth("{synth}");
        v.outputs([]).to(group("any"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let err = engine
        .execute(&script)
        .expect_err("empty list must abort with a Rhai error");
    let msg = err.to_string();
    assert!(
        msg.contains("outputs() requires at least one port name or index"),
        "msg = {}",
        msg
    );
}

#[test]
fn rhai_outputs_replace_semantics_under_fanout() {
    // Two consecutive outputs(["a"]).to(...) calls — the second wins; only
    // one (voice, port) entry survives.
    let synth = "ergo_rhai_outputs_replace";
    declare_synth_with_ports(synth, &["a", "b"]);

    let script = format!(
        r#"
        let v = voice("vox_outs_replace_rhai").synth("{synth}");
        v.outputs(["a"]).to(group("outs_replace_g1"));
        v.outputs(["a"]).to(group("outs_replace_g2"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_outs_replace_rhai"));
    let g2_id = GroupId::new(fnv1a_id("main/outs_replace_g2"));

    let count = state
        .routes
        .keys()
        .filter(|(vid, port)| *vid == voice_id && port == "a")
        .count();
    assert_eq!(count, 1, "expected exactly one route entry, got {}", count);

    assert_eq!(
        state.routes.get(&(voice_id, "a".to_string())),
        Some(&RouteDest::Group(g2_id))
    );
}

#[test]
fn rhai_output_re_route_replaces_prior_dest() {
    // Single declared port — re-routing the same `(voice, port)` must
    // overwrite the previous destination, not accumulate (additive
    // fan-out is deferred to a v2 story).
    let synth = "story14b_rhai_replaces";
    declare_synth_with_ports(synth, &["even"]);

    let script = format!(
        r#"
        let v = voice("vox_replace14b").synth("{synth}");
        v.output("even").to(group("replace14b_g1"));
        v.output("even").to(group("replace14b_g2"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_replace14b"));
    let port_key = (voice_id, "even".to_string());

    // Exactly one entry for this `(voice, port)` after both `.to()` calls.
    let count = state
        .routes
        .keys()
        .filter(|(vid, port)| *vid == voice_id && port == "even")
        .count();
    assert_eq!(
        count, 1,
        "expected exactly one route entry for (voice, port), got {}",
        count
    );

    // The surviving destination is g2 — last `.to()` wins.
    let g2_id = GroupId::new(fnv1a_id("main/replace14b_g2"));
    let dest = state
        .routes
        .get(&port_key)
        .expect("route entry must exist for the routed port");
    assert_eq!(
        dest,
        &RouteDest::Group(g2_id),
        "expected RouteDest::Group(g2) — replace semantics, not g1"
    );
}
