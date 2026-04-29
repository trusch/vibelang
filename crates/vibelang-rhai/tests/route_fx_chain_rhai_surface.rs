//! Multi-output v2 Story 6b — Rhai surface tests for `RouteHandle.fx([...])`.
//!
//! Drives the API end-to-end through [`ScriptEngine::execute`]:
//!   1. Single-FX chain → 1-element fx_chain in the route record.
//!   2. Multi-FX chain (3 names) → 3-element fx_chain in declared order.
//!   3. Unknown FX name → clean Rhai error citing the offender, the closest
//!      registered FX (Levenshtein), and the full available set.
//!   4. `.fx([])` (empty array) → no fx_chain entry; route still installs.
//!   5. `.mute()` after `.fx([...])` → fx_chain dropped (muted routes have
//!      no audio path to apply FX to).

use vibelang_core::handlers::RouteDest;
use vibelang_core::types::{GroupId, VoiceId};
use vibelang_dsp::{
    register_effect_ir, register_synthdef_outputs, GraphBuilderInner, GraphIR, OutputPort,
};
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

/// Seed the FX registry with a stub IR for `name` so [`effect_exists`] /
/// [`get_all_effect_names`] return it without running `define_fx(...).body(...)`.
fn declare_fx(name: &str) {
    let ir = GraphIR::from_builder(name.to_string(), GraphBuilderInner::new());
    register_effect_ir(name.to_string(), ir);
}

#[test]
fn rhai_route_fx_single_chain_records_one_element_in_declared_order() {
    let synth = "story6b_rhai_fx_single";
    declare_synth_with_ports(synth, &["even"]);
    declare_fx("reverb_jpverb_6b_single");

    let script = format!(
        r#"
        let v = voice("vox_fx_single_6b").synth("{synth}");
        v.output("even").fx(["reverb_jpverb_6b_single"]).to(group("post_single_6b"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_fx_single_6b"));
    let g_id = GroupId::new(fnv1a_id("main/post_single_6b"));
    let key = (voice_id, "even".to_string());

    // Dest is committed.
    assert_eq!(state.routes.get(&key), Some(&RouteDest::Group(g_id)));

    // fx_chain has exactly one element, the FX synthdef name.
    let chain = state
        .route_fx_chains
        .get(&key)
        .expect("fx_chain entry must exist for the routed port");
    assert_eq!(chain.len(), 1, "expected 1-element fx_chain, got {}", chain.len());
    assert_eq!(chain[0], "reverb_jpverb_6b_single");
}

#[test]
fn rhai_route_fx_multi_chain_preserves_declared_order() {
    // 3-FX chain: chain order in the record matches the script's array order.
    let synth = "story6b_rhai_fx_multi";
    declare_synth_with_ports(synth, &["sine"]);
    declare_fx("a_6b_multi");
    declare_fx("b_6b_multi");
    declare_fx("c_6b_multi");

    let script = format!(
        r#"
        let v = voice("vox_fx_multi_6b").synth("{synth}");
        v.output("sine")
            .fx(["a_6b_multi", "b_6b_multi", "c_6b_multi"])
            .to(group("post_multi_6b"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_fx_multi_6b"));
    let key = (voice_id, "sine".to_string());

    let chain = state
        .route_fx_chains
        .get(&key)
        .expect("fx_chain entry must exist for the routed port");
    assert_eq!(chain.len(), 3, "expected 3-element fx_chain, got {}", chain.len());
    assert_eq!(
        chain.as_slice(),
        &["a_6b_multi", "b_6b_multi", "c_6b_multi"],
        "chain must be in declaration order"
    );
}

#[test]
fn rhai_route_fx_unknown_name_errors_cites_offender_and_available() {
    // Unknown FX name in the chain aborts the script with an error that
    // names the offending entry and lists the registered FX. When a close
    // match exists the message includes a "did you mean" hint.
    let synth = "story6b_rhai_fx_unknown";
    declare_synth_with_ports(synth, &["even"]);
    // Register a candidate with a small edit distance so the
    // closest-match suggestion fires deterministically.
    declare_fx("reverb_jpverb_6b_typo");

    let script = format!(
        r#"
        let v = voice("vox_fx_unk_6b").synth("{synth}");
        v.output("even").fx(["reverb_jpverv_6b_typo"]).to(group("any_unk_6b"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let err = engine
        .execute(&script)
        .expect_err("unknown FX name must abort script with a Rhai error");
    let msg = err.to_string();

    // The bad name is named, the available set is listed, and the closest
    // registered FX shows up as a suggestion.
    assert!(msg.contains("reverb_jpverv_6b_typo"), "msg = {}", msg);
    assert!(msg.contains("not a registered FX synthdef"), "msg = {}", msg);
    assert!(msg.contains("'reverb_jpverb_6b_typo'"), "msg = {}", msg);
}

#[test]
fn rhai_route_fx_empty_chain_records_no_fx_entry_but_route_installs() {
    // .fx([]) is a valid (no-op) chain — the route still installs but
    // route_fx_chains has no entry for the key.
    let synth = "story6b_rhai_fx_empty";
    declare_synth_with_ports(synth, &["sine"]);

    let script = format!(
        r#"
        let v = voice("vox_fx_empty_6b").synth("{synth}");
        v.output("sine").fx([]).to(group("post_empty_6b"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_fx_empty_6b"));
    let g_id = GroupId::new(fnv1a_id("main/post_empty_6b"));
    let key = (voice_id, "sine".to_string());

    assert_eq!(state.routes.get(&key), Some(&RouteDest::Group(g_id)));
    assert!(
        state.route_fx_chains.get(&key).is_none(),
        "empty fx_chain must not record an entry"
    );
}

#[test]
fn rhai_route_fx_then_to_main_records_chain() {
    // .fx([...]).to_main() also propagates the chain — terminal verb other
    // than .to is wired the same way.
    let synth = "story6b_rhai_fx_to_main";
    declare_synth_with_ports(synth, &["sine"]);
    declare_fx("delay_6b_to_main");

    let script = format!(
        r#"
        let v = voice("vox_fx_to_main_6b").synth("{synth}");
        v.output("sine").fx(["delay_6b_to_main"]).to_main();
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_fx_to_main_6b"));
    let key = (voice_id, "sine".to_string());

    assert_eq!(state.routes.get(&key), Some(&RouteDest::Main));
    let chain = state
        .route_fx_chains
        .get(&key)
        .expect("fx_chain must persist for .to_main()");
    assert_eq!(chain.as_slice(), &["delay_6b_to_main"]);
}

#[test]
fn rhai_route_fx_then_mute_drops_chain() {
    // .mute() drops any pending fx_chain — muted routes have no audio path
    // for FX to run on, so silently retaining the chain would leak it on a
    // later un-mute.
    let synth = "story6b_rhai_fx_mute";
    declare_synth_with_ports(synth, &["sine"]);
    declare_fx("phase_6b_mute");

    let script = format!(
        r#"
        let v = voice("vox_fx_mute_6b").synth("{synth}");
        v.output("sine").fx(["phase_6b_mute"]).mute();
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let state = engine.execute(&script).expect("script must succeed");

    let voice_id = VoiceId::new(fnv1a_id("vox_fx_mute_6b"));
    let key = (voice_id, "sine".to_string());

    assert_eq!(state.routes.get(&key), Some(&RouteDest::Muted));
    assert!(
        state.route_fx_chains.get(&key).is_none(),
        ".mute() must not retain any fx_chain"
    );
}

#[test]
fn rhai_route_fx_unknown_with_no_registered_fx_lists_none() {
    // Edge case: the registry has no FX at all. The error must still mention
    // the bad name and not crash on the empty candidate list.
    let synth = "story6b_rhai_fx_unknown_empty_registry";
    declare_synth_with_ports(synth, &["sine"]);
    // NOTE: deliberately no declare_fx() calls — but the registry is global
    // across tests, so other tests' FX may still be visible. We only check
    // that the offender shows up + the message names "not a registered FX".

    let script = format!(
        r#"
        let v = voice("vox_fx_unk_empty_6b").synth("{synth}");
        v.output("sine").fx(["totally_unknown_fx_zzz"]).to(group("any_unk_empty_6b"));
        "#,
        synth = synth,
    );

    let mut engine = ScriptEngine::new();
    let err = engine
        .execute(&script)
        .expect_err("unknown FX name must abort script with a Rhai error");
    let msg = err.to_string();

    assert!(msg.contains("totally_unknown_fx_zzz"), "msg = {}", msg);
    assert!(msg.contains("not a registered FX synthdef"), "msg = {}", msg);
}
