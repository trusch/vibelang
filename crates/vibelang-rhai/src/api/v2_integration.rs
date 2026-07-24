//! M09 closing integration suite: every M08/M09 family through the one
//! production registration root.
//!
//! These tests exercise the shared `api::install_v2_api` surface — not the
//! per-module installs — so they prove the integrated vocabulary: the full
//! route matrix with fan/conflict metadata, resource generation replacement
//! on a real `ResourceManager`, recording completion as a typed `SampleRef`,
//! strict numeric boundaries, typed device disappearance, separately keyed
//! external effects with mixed-submission rejection before revision
//! allocation, effective compatibility aliases, and the closing sweep that
//! no sentinel handle, physical ID, or log-only terminal remains in v2.

use rhai::{Engine, EvalAltResult};
use std::collections::BTreeSet;
use std::sync::OnceLock;
use std::time::SystemTime;

use crate::engine::{ScriptEngine, ScriptEvaluation, V2EngineConfig};
use crate::foundation;
use vibelang_core::candidate::{
    AuthoringDeclaration, Cancellation, Candidate, ContractDigest, DeclarationPayload,
    EngineInstanceId, EntityKind, EvaluationIdentity, LanguageContract, LifecycleAction,
    LifecycleMetadata, LogicalAddress, RouteVerb, TerminalEffect,
};
use vibelang_core::capabilities::{
    fixtures as capability_fixtures, AvailabilityGate, CapabilityCatalog, CapabilitySnapshot,
    CapabilitySubject,
};
use vibelang_core::mutation::{
    Atomicity, CandidateOrigin, CandidateSubmission, ExternalDomain, ExternalEffectDomain,
    ExternalEffectError, ExternalEffectOperation, FailurePhase, LedgerConfig, LiveState,
    MessageDomain, MutationKind, MutationLedger, MutationSource, PreAcceptancePhase, ReceiptState,
    ReceiptWindow, Rejected, RequestMaterial, RollbackState, RuntimeEpoch, RuntimeMutationStatus,
    SequencedExternalPlan, Submission, SubmissionResult, SupersessionPolicy, TerminalOutcome,
    Timestamp, MUTATION_SCHEMA_VERSION,
};
use vibelang_core::native_generation::{candidate_resource_bindings, lower_resource_components};
use vibelang_core::resource_manager::{
    PhysicalResourceId, ResourceKind, ResourceManager, SampleIdentity,
};

use super::midi::{
    resolve_midi_device_v2, MidiDeviceUnavailability, MidiOutputCommand, MidiPortDirectory,
};
use super::recording::RecordRef;

fn v2_config(seed: u8) -> V2EngineConfig {
    V2EngineConfig::new(ContractDigest::from_bytes(&[seed]), RuntimeEpoch::new())
}

fn production_engine(seed: u8) -> ScriptEngine {
    foundation::abort_evaluation();
    ScriptEngine::with_v2(v2_config(seed)).expect("v2 engine construction")
}

fn production_engine_at(seed: u8, runtime_epoch: RuntimeEpoch) -> ScriptEngine {
    foundation::abort_evaluation();
    ScriptEngine::with_v2(V2EngineConfig::new(
        ContractDigest::from_bytes(&[seed]),
        runtime_epoch,
    ))
    .expect("v2 engine construction")
}

fn evaluate(engine: &mut ScriptEngine, script: &str) -> Candidate {
    match engine.evaluate(script) {
        Ok(ScriptEvaluation::V2(candidate)) => candidate,
        Ok(ScriptEvaluation::V1(_)) => panic!("the v2 directive must select the v2 engine"),
        Err(error) => panic!("v2 evaluation failed: {error}"),
    }
}

fn evaluate_err(engine: &mut ScriptEngine, script: &str) -> String {
    match engine.evaluate(script) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("script must reject: {script}"),
    }
}

fn v2_identity(seed: &[u8]) -> EvaluationIdentity {
    EvaluationIdentity::new(
        LanguageContract::v2(ContractDigest::from_bytes(seed)),
        EngineInstanceId::new(),
        RuntimeEpoch::new(),
    )
}

/// A raw engine carrying the exact shared registration root, for tests
/// that need typed script return values instead of a finished candidate.
fn shared_root_engine() -> Engine {
    let mut engine = Engine::new();
    foundation::register(&mut engine);
    super::install_v2_api(&mut engine);
    engine
}

fn keys_of(candidate: &Candidate) -> BTreeSet<String> {
    candidate
        .declarations()
        .iter()
        .map(|declaration| declaration.address().key().as_str().to_owned())
        .collect()
}

const ROUTE_PRELUDE: &str = r#"// vibe-api: 2
define_group("leads", || {});
define_group("evens", || {});
let lead = voice("lead").synth("sine").apply();
let lfo = voice("lfo").synth("sine").apply();
let env = voice("env").synth("sine").apply();
"#;

#[test]
fn v2_integration_route_matrix_authors_exact_fan_metadata() {
    let mut engine = production_engine(41);
    let script = format!(
        "{ROUTE_PRELUDE}{}",
        r#"
output(lead, "out").to_groups([group_ref("leads"), group_ref("evens")]);
output(lead, "dry").replace_with_main();
output(lead, "spill").replace_with_silence();
output(lfo, "cv").scale(0.5).offset(0.1).set(lead, "cutoff");
output(lfo, "cv2").set_from_audio(lead, "amp");
output(env, "trig").trigger(lead, "gate");
output(env, "cv").to_input(lead, "fm");
input(lead, "sidechain").from(env, "out");
input(lead, "aux").disconnect();
param(lead, "resonance").bend_from(lfo, "cv3");
param(lead, "resonance").bend_from(env, "cv4");
"#
    );
    let candidate = evaluate(&mut engine, &script);
    let topology = candidate.route_topology();

    let fan_out = topology
        .audio
        .iter()
        .find(|(endpoint, _)| endpoint.port == "out")
        .map(|(_, summary)| summary)
        .expect("the fan-out route is in the topology");
    assert_eq!(fan_out.fan_out(), 2, "two additive group edges");
    assert!(!fan_out.to_main && !fan_out.muted);
    let main = topology
        .audio
        .iter()
        .find(|(endpoint, _)| endpoint.port == "dry")
        .map(|(_, summary)| summary)
        .expect("the main route is in the topology");
    assert!(main.to_main && main.group_destinations.is_empty());
    let muted = topology
        .audio
        .iter()
        .find(|(endpoint, _)| endpoint.port == "spill")
        .map(|(_, summary)| summary)
        .expect("the muted route is in the topology");
    assert!(muted.muted && !muted.to_main);

    let slot = |param: &str| {
        topology
            .params
            .iter()
            .find(|((_, name), _)| name == param)
            .map(|(_, slot)| slot)
            .unwrap_or_else(|| panic!("param slot {param} is in the topology"))
    };
    assert_eq!(slot("cutoff").verb, RouteVerb::Set);
    assert_eq!(slot("cutoff").fan_in(), 1);
    assert_eq!(
        slot("amp").verb,
        RouteVerb::Set,
        "A2K lands on the SET verb"
    );
    assert_eq!(slot("gate").verb, RouteVerb::Trigger);
    assert_eq!(slot("resonance").verb, RouteVerb::Bend);
    assert_eq!(slot("resonance").fan_in(), 2, "BEND fan-in stays additive");

    let sidechain = topology
        .inputs
        .iter()
        .find(|((_, port), _)| port == "sidechain")
        .map(|(_, summary)| summary)
        .expect("the wired input is in the topology");
    assert!(sidechain.source.is_some());
    let aux = topology
        .inputs
        .iter()
        .find(|((_, port), _)| port == "aux")
        .map(|(_, summary)| summary)
        .expect("the disconnect is in the topology");
    assert!(
        aux.source.is_none(),
        "an explicit disconnect is a real route, not a missing one"
    );
}

#[test]
fn v2_integration_route_verbs_are_all_effectful_terminals() {
    // B2 sweep: every route-verb spelling on the production root either
    // declares a route in the evaluated candidate or rejects with a typed
    // error — no silent non-terminal configuration path and no
    // trailing-apply requirement anywhere on the route surface.
    let verbs = [
        (
            "to_groups",
            r#"output(lead, "out").to_groups([group_ref("leads"), group_ref("evens")]);"#,
        ),
        ("to", r#"output(lead, "out").to(group_ref("leads"));"#),
        (
            "replace_with_main",
            r#"output(lead, "dry").replace_with_main();"#,
        ),
        ("to_main", r#"output(lead, "dry").to_main();"#),
        (
            "replace_with_silence",
            r#"output(lead, "spill").replace_with_silence();"#,
        ),
        ("mute", r#"output(lead, "spill").mute();"#),
        ("set", r#"output(lfo, "cv").set(lead, "cutoff");"#),
        (
            "set_from_audio",
            r#"output(lfo, "cv2").set_from_audio(lead, "amp");"#,
        ),
        ("trigger", r#"output(env, "trig").trigger(lead, "gate");"#),
        ("to_param", r#"output(lfo, "cv").to_param(lead, "cutoff");"#),
        (
            "to_param_audio",
            r#"output(lfo, "cv2").to_param_audio(lead, "amp");"#,
        ),
        (
            "to_trigger",
            r#"output(env, "trig").to_trigger(lead, "gate");"#,
        ),
        ("to_input", r#"output(env, "cv").to_input(lead, "fm");"#),
        (
            "input from voice port",
            r#"input(lead, "sidechain").from(env, "out");"#,
        ),
        (
            "input from voice default",
            r#"input(lead, "sidechain").from(env);"#,
        ),
        (
            "input from group",
            r#"input(lead, "sidechain").from(group_ref("leads"));"#,
        ),
        ("input disconnect", r#"input(lead, "aux").disconnect();"#),
        (
            "bend_from",
            r#"param(lead, "resonance").bend_from(lfo, "cv3");"#,
        ),
        (
            "modulate_by",
            r#"param(lead, "resonance").modulate_by(lfo, "cv3");"#,
        ),
    ];
    for (verb, statement) in verbs {
        let mut engine = production_engine(52);
        let candidate = evaluate(&mut engine, &format!("{ROUTE_PRELUDE}{statement}\n"));
        let routes = candidate
            .declarations()
            .iter()
            .filter(|declaration| declaration.address().kind() == EntityKind::Route)
            .count();
        assert_eq!(
            routes, 1,
            "route verb {verb} declares exactly one route with no trailing apply"
        );
    }

    // The demoted configuration surface is gone from the production root:
    // the route builder keeps no add/apply spelling to configure with.
    for statement in [
        r#"output(lead, "out").add(group_ref("leads"));"#,
        r#"output(lead, "out").apply();"#,
    ] {
        let mut engine = production_engine(53);
        let error = evaluate_err(&mut engine, &format!("{ROUTE_PRELUDE}{statement}\n"));
        assert!(
            error.contains("Function not found"),
            "non-terminal route spelling must not exist: {statement} -> {error}"
        );
    }
}

#[test]
fn v2_integration_route_conflicts_reject_the_whole_evaluation() {
    // Cross-verb: SET and BEND on the same (target, param) slot.
    let mut engine = production_engine(42);
    let error = evaluate_err(
        &mut engine,
        &format!(
            "{ROUTE_PRELUDE}{}",
            r#"
output(lfo, "cv").set(lead, "cutoff");
param(lead, "cutoff").bend_from(env, "cv2");
"#
        ),
    );
    assert!(
        error.to_lowercase().contains("route"),
        "the conflict names the route registry: {error}"
    );

    // Same-slot second SET writer.
    let mut engine = production_engine(43);
    let error = evaluate_err(
        &mut engine,
        &format!(
            "{ROUTE_PRELUDE}{}",
            r#"
output(lfo, "cv").set(lead, "cutoff");
output(env, "cv").set(lead, "cutoff");
"#
        ),
    );
    assert!(
        error.contains("cutoff"),
        "the duplicate SET writer names the slot: {error}"
    );

    // A failed evaluation leaves no residue: the same engine accepts a
    // clean script afterwards and the candidate holds only its own keys.
    let candidate = evaluate(
        &mut engine,
        "// vibe-api: 2\ndefine_group(\"fresh\", || {});",
    );
    assert_eq!(
        keys_of(&candidate),
        BTreeSet::from(["fresh".to_owned()]),
        "no declarations leak across a rejected evaluation"
    );
}

#[test]
fn v2_integration_duplicate_identity_rejects_across_families() {
    let cases = [
        ("group", "define_group(\"dup\", || {});\ndefine_group(\"dup\", || {});"),
        (
            "voice",
            "voice(\"dup\").synth(\"sine\").apply();\nvoice(\"dup\").synth(\"sine\").apply();",
        ),
        (
            "sample",
            "sample(\"dup\", \"a.wav\").apply();\nsample(\"dup\", \"a.wav\").apply();",
        ),
        (
            "buffer",
            "buffer(\"dup\").frames(8).clear().apply();\nbuffer(\"dup\").frames(8).clear().apply();",
        ),
        ("sfz", "sfz(\"dup\", \"a.sfz\").apply();\nsfz(\"dup\", \"a.sfz\").apply();"),
        (
            "record",
            "define_group(\"g\", || {});\nrecord(\"dup\").from(group_ref(\"g\")).beats(4.0).apply();\nrecord(\"dup\").from(group_ref(\"g\")).beats(4.0).apply();",
        ),
        (
            "midi device",
            "midi_device(\"dup\").port(\"P\").input().apply();\nmidi_device(\"dup\").port(\"P\").input().apply();",
        ),
    ];
    for (family, body) in cases {
        let mut engine = production_engine(44);
        let script = format!("// vibe-api: 2\n{body}\n");
        let error = evaluate_err(&mut engine, &script);
        assert!(
            error.contains("dup") || error.to_lowercase().contains("duplicate"),
            "{family}: duplicate identity must reject structurally, got: {error}"
        );
    }
}

#[test]
fn v2_integration_strict_numeric_boundaries_reject_through_the_shared_root() {
    let cases: &[(&str, &str)] = &[
        ("buffer channels 0", "buffer(\"b\").frames(8).channels(0).clear().apply();"),
        ("buffer channels 17", "buffer(\"b\").frames(8).channels(17).clear().apply();"),
        ("buffer frames 0", "buffer(\"b\").frames(0).clear().apply();"),
        (
            "sample overlaps 33",
            "sample(\"s\", \"a.wav\").overlaps(33).apply();",
        ),
        ("record beats 0", "record(\"r\").beats(0.0);"),
        ("record beats negative", "record(\"r\").beats(-4.0);"),
        ("record seconds 0", "record(\"r\").seconds(0.0);"),
        (
            "midi channel 0",
            "midi_device(\"m\").port(\"P\").input().apply();\nkeyboard_route(midi_device_ref(\"m\")).channel(0);",
        ),
        (
            "midi channel 17",
            "midi_device(\"m\").port(\"P\").input().apply();\nkeyboard_route(midi_device_ref(\"m\")).channel(17);",
        ),
        (
            "midi velocity 128",
            "midi_device(\"m\").port(\"P\").output().apply();\nmidi_out(midi_device_ref(\"m\")).key(\"k\").note_on(60, 128);",
        ),
        (
            "midi pitch_bend14 16384",
            "midi_device(\"m\").port(\"P\").output().apply();\nmidi_out(midi_device_ref(\"m\")).key(\"k\").pitch_bend14(16384);",
        ),
        (
            "route scale infinite",
            "let v = voice(\"v\").synth(\"sine\").apply();\noutput(v, \"cv\").scale(1.0/0.0);",
        ),
    ];
    for (case, body) in cases {
        let mut engine = production_engine(45);
        let script = format!("// vibe-api: 2\n{body}\n");
        assert!(
            engine.evaluate(&script).is_err(),
            "{case}: out-of-range input must reject strictly"
        );
    }
}

#[test]
fn v2_integration_record_lifecycle_completes_as_a_typed_sample_ref() {
    foundation::abort_evaluation();
    foundation::begin_evaluation(v2_identity(b"integration-record")).unwrap();
    let engine = shared_root_engine();

    engine
        .eval::<super::group::GroupRef>(r#"define_group("drums", || {})"#)
        .expect("the shared root installs the group family");
    let take = engine
        .eval::<RecordRef>(
            r#"record("take")
                .from(group_ref("drums"))
                .beats(16.0)
                .to_file("takes/one.wav")
                .channels(2)
                .start()"#,
        )
        .expect("the shared root installs the record family");
    // Stop and cancel are real keyed operations, not log-only verbs.
    engine
        .eval::<RecordRef>(r#"stop_recording("take")"#)
        .expect("the stop replacement resolves through the shared root");
    engine
        .eval::<RecordRef>(r#"cancel_recording("take")"#)
        .expect("the cancel replacement resolves through the shared root");
    let candidate = foundation::finish_evaluation().unwrap();
    assert_eq!(
        candidate.operations().len(),
        2,
        "stop and cancel are recorded operations and do not add active runs"
    );

    // The completion handle is the recording's own logical address, typed
    // as a Sample — never a sentinel or a physical buffer number.
    let sample = take.sample().expect("completion is a typed SampleRef");
    assert_eq!(sample.base().kind(), EntityKind::Sample);
    assert_eq!(
        sample.base().address().key(),
        take.base().address().key(),
        "the completion handle names the recording's logical address"
    );

    // Exactly one active run: a started declaration plus a second start
    // rejects the candidate at validation.
    foundation::begin_evaluation(v2_identity(b"integration-record-2")).unwrap();
    let engine = shared_root_engine();
    engine
        .eval::<super::group::GroupRef>(r#"define_group("drums", || {})"#)
        .unwrap();
    let take = engine
        .eval::<RecordRef>(r#"record("take").from(group_ref("drums")).beats(4.0).start()"#)
        .unwrap();
    take.clone().start().unwrap();
    let error = foundation::finish_evaluation().expect_err("two runs reject");
    assert!(
        error.to_string().contains("one active run"),
        "the rejection names the single-run rule: {error}"
    );
}

#[test]
fn v2_integration_external_effects_stay_separately_keyed_top_level() {
    foundation::abort_evaluation();
    foundation::begin_evaluation(v2_identity(b"integration-external")).unwrap();
    let engine = shared_root_engine();
    engine
        .eval::<super::midi::MidiDeviceRef>(
            r#"midi_device("mpk").port("MPK Mini").output().apply()"#,
        )
        .unwrap();
    let first = engine
        .eval::<MidiOutputCommand>(
            r#"midi_out(midi_device_ref("mpk")).channel(3).key("key-before").note_on(60, 100)"#,
        )
        .unwrap();
    let second = engine
        .eval::<MidiOutputCommand>(
            r#"midi_out(midi_device_ref("mpk")).channel(3).key("key-after").cc(7, 96)"#,
        )
        .unwrap();
    foundation::finish_evaluation().unwrap();

    let ledger = MutationLedger::new(LedgerConfig::default()).unwrap();
    let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
    let source = MutationSource::Rhai {
        engine_id: "v2.integration".into(),
    };
    let candidate_submission = |key: Option<&str>| Submission {
        kind: MutationKind::Candidate {
            origin: CandidateOrigin::RhaiHost,
        },
        source: source.clone(),
        caller_namespace: "v2.integration".into(),
        idempotency_key: key.map(str::to_string),
        require_idempotency_key: false,
        retry_epoch: Some(ledger.runtime_epoch()),
        expected_revision: None,
        atomicity: Atomicity::Required,
        supersession: SupersessionPolicy::Fifo,
        material: RequestMaterial::new(&("candidate", 1), Some(&())).unwrap(),
    };

    // A repeated key across the sequenced pair rejects before any ledger
    // work: zero attempts, zero revisions.
    let clash = SequencedExternalPlan::new(
        vec![first.submission().clone()],
        CandidateSubmission::new(candidate_submission(Some("key-before"))).unwrap(),
        Vec::new(),
    );
    assert!(matches!(
        clash,
        Err(ExternalEffectError::DuplicateIdempotencyKey { key }) if key == "key-before"
    ));
    assert_eq!(ledger.status(now).accepted_through, None);

    // Mixed submissions reject before revision allocation under both
    // atomicities.
    for (atomicity, expect_required) in
        [(Atomicity::Required, true), (Atomicity::BestEffort, false)]
    {
        let mut submission = candidate_submission(None);
        submission.atomicity = atomicity;
        let error = CandidateSubmission::with_embedded_external(
            submission,
            vec![first.submission().operation().clone()],
        )
        .expect_err("embedded external operations reject");
        match (expect_required, &error) {
            (true, ExternalEffectError::MixedRequiredAtomicSubmission { .. })
            | (false, ExternalEffectError::EmbeddedExternalEffect { .. }) => {}
            _ => panic!("unexpected mixed-submission error under {atomicity:?}: {error}"),
        }
    }
    assert_eq!(
        ledger.status(now).accepted_through,
        None,
        "mixed submissions never reach revision allocation"
    );

    // Distinct keys submit as ordered top-level attempts with exact wire
    // kinds — no runtime-created parent/child receipts.
    let plan = SequencedExternalPlan::new(
        vec![first.submission().clone()],
        CandidateSubmission::new(candidate_submission(Some("key-candidate"))).unwrap(),
        vec![second.submission().clone()],
    )
    .expect("distinct keys are accepted");
    let outcome = plan.submit(&ledger, now).expect("the plan submits");
    assert_eq!(outcome.results.len(), 3);
    assert_eq!(outcome.unsubmitted, 0);
    for result in &outcome.results {
        assert!(matches!(result, SubmissionResult::New(_)));
        assert!(matches!(
            result.receipt().request.source,
            MutationSource::Rhai { .. }
        ));
    }
    assert!(matches!(
        &outcome.results[0].receipt().request.kind,
        MutationKind::Command { domain: MessageDomain::Midi, operation }
            if operation == "external.midi.note_on"
    ));
    assert!(matches!(
        outcome.results[1].receipt().request.kind,
        MutationKind::Candidate { .. }
    ));
    assert!(matches!(
        &outcome.results[2].receipt().request.kind,
        MutationKind::Command { domain: MessageDomain::Midi, operation }
            if operation == "external.midi.cc"
    ));

    // The dedicated External wire kind addresses the domains with no music
    // domain to borrow — completing the M09 wire vocabulary.
    for (domain, expected) in [
        (ExternalEffectDomain::Filesystem, ExternalDomain::Filesystem),
        (ExternalEffectDomain::Process, ExternalDomain::Process),
        (ExternalEffectDomain::Network, ExternalDomain::Network),
    ] {
        let operation =
            ExternalEffectOperation::new(domain, "probe", &("n", 1), Some(&())).unwrap();
        assert_eq!(
            operation.wire_kind(),
            MutationKind::External {
                domain: expected,
                operation: format!("external.{domain}.probe"),
            }
        );
    }
}

#[test]
fn v2_integration_device_metadata_and_resolution_agree() {
    // The declared authoring metadata is the exact port resolution input:
    // extract it from the candidate rather than repeating the literal.
    let mut engine = production_engine(46);
    let candidate = evaluate(
        &mut engine,
        "// vibe-api: 2\nmidi_device(\"mpk\").port(\"MPK Mini\").input().apply();\n",
    );
    let declared = candidate
        .declarations()
        .iter()
        .find_map(|declaration| match declaration.payload() {
            DeclarationPayload::Authoring {
                declaration: AuthoringDeclaration::MidiDevice(device),
                ..
            } => Some(device.clone()),
            _ => None,
        })
        .expect("the device declaration is in the candidate");

    let present = MidiPortDirectory::new(vec!["MPK Mini".into()], Vec::new());
    let resolved = resolve_midi_device_v2(&declared.port, declared.direction, &present)
        .expect("the declared port resolves against a live inventory");

    // Disappearance on re-verify is the typed result, never a stale handle.
    let unplugged = MidiPortDirectory::default();
    assert!(matches!(
        resolved.verify(&unplugged),
        Err(MidiDeviceUnavailability::Disappeared { .. })
    ));
    let doubled = MidiPortDirectory::new(vec!["MPK Mini".into(), "MPK Mini".into()], Vec::new());
    assert!(matches!(
        resolve_midi_device_v2(&declared.port, declared.direction, &doubled),
        Err(MidiDeviceUnavailability::Ambiguous { count: 2, .. })
    ));
}

#[test]
fn v2_integration_resource_generations_replace_with_exact_accounting() {
    // Reload spelling: the same engine evaluates the same source twice and
    // the logical binding is stable across candidates.
    let mut engine = production_engine(47);
    let script = "// vibe-api: 2\nsample(\"kick\", \"samples/kick.wav\").one_shot().apply();\n";
    let first = evaluate(&mut engine, script);
    let second = evaluate(&mut engine, script);
    let binding_of = |candidate: &Candidate| {
        let bindings = candidate_resource_bindings(candidate).expect("typed bindings");
        assert_eq!(bindings.len(), 1);
        bindings[0].logical.clone()
    };
    let logical = binding_of(&first);
    assert_eq!(
        logical,
        binding_of(&second),
        "reload keeps the logical resource identity stable"
    );
    assert_eq!(logical.kind(), ResourceKind::Sample);
    assert!(
        !logical.address().contains("4294967295"),
        "logical bindings never carry sentinel IDs"
    );

    // Generation replacement on a real manager: stage the first content,
    // commit, pin a reader, then replace and prove exact accounting.
    let resources = ResourceManager::new();
    let identity = |fingerprint: &str| SampleIdentity {
        canonical_source: "samples/kick.wav".into(),
        content_fingerprint: fingerprint.into(),
        decode_options_digest: "decode".into(),
        loader_version: "loader".into(),
        backend: "test".into(),
    };
    let stage = resources.begin_stage().unwrap();
    let staged = resources
        .stage_sample(
            stage,
            logical.clone(),
            identity("sha256:one"),
            [PhysicalResourceId::Buffer(11)],
        )
        .unwrap();
    let generation_one = staged.generation;
    let components = lower_resource_components(
        &first,
        &std::iter::once((logical.clone(), generation_one)).collect(),
    )
    .expect("staged claims match the declared bindings exactly");
    assert_eq!(components.len(), 1);
    resources.commit_stage(stage).unwrap();
    assert_eq!(resources.generation_for(&logical), Some(generation_one));

    // Lowering rejects any claim/declaration mismatch instead of silently
    // inventing or dropping claims.
    assert!(lower_resource_components(&first, &std::collections::BTreeMap::new()).is_err());

    let lease = resources.acquire(&logical).expect("reader lease");
    let replacement_stage = resources.begin_stage().unwrap();
    let replacement = resources
        .stage_sample(
            replacement_stage,
            logical.clone(),
            identity("sha256:two"),
            [PhysicalResourceId::Buffer(12)],
        )
        .unwrap();
    let generation_two = replacement.generation;
    assert_ne!(generation_one, generation_two);
    let retirement = resources.commit_stage(replacement_stage).unwrap();
    assert_eq!(
        resources.generation_for(&logical),
        Some(generation_two),
        "the replacement generation is authoritative after commit"
    );
    assert_eq!(
        retirement.generations().collect::<Vec<_>>(),
        vec![generation_one],
        "commit retires exactly the replaced generation"
    );
    assert!(
        !resources.freeable_generations().contains(&generation_one),
        "a reader lease pins the retired generation beyond commit"
    );
    drop(lease);
    assert!(
        resources.freeable_generations().contains(&generation_one),
        "releasing the last reader frees the replaced generation exactly once"
    );
}

#[test]
fn v2_integration_compatibility_aliases_are_effective_forwardings() {
    fn canonical_bytes_of(candidate: &Candidate, key: &str) -> Vec<u8> {
        candidate
            .declarations()
            .iter()
            .find(|declaration| declaration.address().key().as_str() == key)
            .and_then(|declaration| match declaration.payload() {
                DeclarationPayload::Authoring {
                    canonical_bytes, ..
                } => Some(canonical_bytes.to_vec()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("declaration {key} has authoring bytes"))
    }

    let pairs = [
        (
            "buffer/allocate_buffer",
            "b",
            "buffer(\"b\").frames(64).channels(1).clear().apply();",
            "allocate_buffer(\"b\", 64, 1).clear().apply();",
        ),
        (
            "sfz/load_sfz",
            "k",
            "sfz(\"k\", \"kits/606.sfz\").apply();",
            "load_sfz(\"k\", \"kits/606.sfz\").apply();",
        ),
        (
            "record from/from_group",
            "r",
            "define_group(\"g\", || {});\nrecord(\"r\").from(group_ref(\"g\")).beats(4.0).apply();",
            "define_group(\"g\", || {});\nrecord(\"r\").from_group(group_ref(\"g\")).beats(4.0).apply();",
        ),
        (
            "keyboard to/route_to",
            "m",
            "let v = voice(\"v\").synth(\"sine\").apply();\nmidi_device(\"m\").port(\"P\").input().apply();\nkeyboard_route(midi_device_ref(\"m\")).channel(2).to(v);",
            "let v = voice(\"v\").synth(\"sine\").apply();\nmidi_device(\"m\").port(\"P\").input().apply();\nkeyboard_route(midi_device_ref(\"m\")).channel(2).route_to(v);",
        ),
        (
            "route to_groups/to",
            "audio.lead.out",
            "let lead = voice(\"lead\").synth(\"sine\").apply();\ndefine_group(\"leads\", || {});\noutput(lead, \"out\").to_groups([group_ref(\"leads\")]);",
            "let lead = voice(\"lead\").synth(\"sine\").apply();\ndefine_group(\"leads\", || {});\noutput(lead, \"out\").to(group_ref(\"leads\"));",
        ),
        (
            "route replace_with_main/to_main",
            "audio.lead.dry",
            "let lead = voice(\"lead\").synth(\"sine\").apply();\noutput(lead, \"dry\").replace_with_main();",
            "let lead = voice(\"lead\").synth(\"sine\").apply();\noutput(lead, \"dry\").to_main();",
        ),
        (
            "route replace_with_silence/mute",
            "audio.lead.spill",
            "let lead = voice(\"lead\").synth(\"sine\").apply();\noutput(lead, \"spill\").replace_with_silence();",
            "let lead = voice(\"lead\").synth(\"sine\").apply();\noutput(lead, \"spill\").mute();",
        ),
    ];
    for (case, key, canonical, alias) in pairs {
        let mut engine = production_engine(48);
        let canonical_candidate = evaluate(&mut engine, &format!("// vibe-api: 2\n{canonical}\n"));
        let mut engine = production_engine(48);
        let alias_candidate = evaluate(&mut engine, &format!("// vibe-api: 2\n{alias}\n"));
        assert_eq!(
            canonical_bytes_of(&canonical_candidate, key),
            canonical_bytes_of(&alias_candidate, key),
            "{case}: the alias must author byte-identical canonical payloads"
        );
    }

    // Hi-res output aliases forward to the width-qualified spellings: the
    // idempotency digest replays instead of conflicting.
    foundation::abort_evaluation();
    foundation::begin_evaluation(v2_identity(b"integration-alias")).unwrap();
    let engine = shared_root_engine();
    engine
        .eval::<super::midi::MidiDeviceRef>(r#"midi_device("m").port("P").output().apply()"#)
        .unwrap();
    let canonical = engine
        .eval::<MidiOutputCommand>(
            r#"midi_out(midi_device_ref("m")).key("hires").note_on16(60, 40000)"#,
        )
        .unwrap();
    let alias = engine
        .eval::<MidiOutputCommand>(
            r#"midi_out(midi_device_ref("m")).key("hires").note_on_hires(60, 40000)"#,
        )
        .unwrap();
    foundation::finish_evaluation().unwrap();
    let ledger = MutationLedger::new(LedgerConfig::default()).unwrap();
    let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
    let first = ledger
        .submit(
            canonical.submission().submission(ledger.runtime_epoch()),
            now,
        )
        .unwrap();
    let replay = ledger
        .submit(alias.submission().submission(ledger.runtime_epoch()), now)
        .unwrap();
    let SubmissionResult::New(original) = &first else {
        panic!("the canonical spelling is a new attempt");
    };
    let SubmissionResult::Replayed(replayed) = &replay else {
        panic!("the alias must replay the canonical attempt, not conflict");
    };
    assert_eq!(replayed.attempt_id, original.attempt_id);
    assert!(matches!(replayed.state, ReceiptState::Evaluating { .. }));
}

#[test]
fn v2_integration_lifecycle_terminals_are_distinct_and_observed() {
    let mut engine = production_engine(49);
    let candidate = evaluate(
        &mut engine,
        r#"// vibe-api: 2
define_group("g", || {});
sample("registered", "a.wav").apply();
record("armed").from(group_ref("g")).beats(4.0).start();
"#,
    );
    let effect_of = |key: &str| {
        candidate
            .declarations()
            .iter()
            .find(|declaration| declaration.address().key().as_str() == key)
            .map(|declaration| declaration.lifecycle().terminal_effect)
            .unwrap_or_else(|| panic!("declaration {key}"))
    };
    assert_eq!(effect_of("registered"), TerminalEffect::Register);
    assert_eq!(
        effect_of("armed"),
        TerminalEffect::Start,
        "start and register terminals stay lifecycle-distinct"
    );

    // Unsupported composition stays a structured failure: a record source
    // must be a group, and a wrong Ref type cannot even spell the call.
    foundation::abort_evaluation();
    foundation::begin_evaluation(v2_identity(b"integration-compose")).unwrap();
    let engine = shared_root_engine();
    engine
        .eval::<super::voice::VoiceRef>(r#"voice("v").synth("sine").apply()"#)
        .unwrap();
    let error = engine
        .eval::<RecordRef>(r#"record("r").from(voice_ref("v")).beats(4.0).apply()"#)
        .expect_err("a voice is not a recordable source");
    assert!(
        matches!(*error, EvalAltResult::ErrorFunctionNotFound(..))
            || error.to_string().to_lowercase().contains("group"),
        "unsupported composition is structured, not a silent accept: {error}"
    );
    foundation::abort_evaluation();
}

#[test]
fn v2_integration_sweep_no_sentinels_physical_ids_or_opaque_payloads() {
    let mut engine = production_engine(50);
    let candidate = evaluate(
        &mut engine,
        r#"// vibe-api: 2
define_group("band", || {});
let lead = voice("lead").synth("sine").apply();
pattern("kick").on(lead).step("x...").apply();
melody("hook").on(lead).notes("C4 E4 G4 C5").apply();
sequence("intro").loop_beats(4.0).apply();
fade("swell").on_group(group("band")).over(2.0).apply();
define_synthdef("tone").param("freq", 440.0).body(|freq| dc_ar(0.0)).apply();
define_effect("room").param("mix", 0.5).body(|input, mix| input).apply();
fx("verb");
sample("snare", "samples/snare.wav").apply();
buffer("scratch").frames(64).channels(2).clear().apply();
sfz("piano", "instruments/piano.sfz").apply();
record("take").from(group_ref("band")).beats(8.0).apply();
output(lead, "out").to_groups([group_ref("band")]);
let mpk = midi_device("mpk").port("MPK Mini").input().apply();
keyboard_route(mpk).channel(2).to(lead);
"#,
    );

    const FORBIDDEN: &[&str] = &["4294967295", "bufnum", "buffer_id", "device_id", "physical"];
    assert!(
        candidate.declarations().len() >= 14,
        "the sweep candidate covers every M08/M09 family"
    );
    for declaration in candidate.declarations() {
        match declaration.payload() {
            DeclarationPayload::Authoring {
                canonical_bytes, ..
            } => {
                let text = String::from_utf8_lossy(canonical_bytes);
                for pattern in FORBIDDEN {
                    assert!(
                        !text.contains(pattern),
                        "{}: canonical payload carries forbidden pattern {pattern}: {text}",
                        declaration.address()
                    );
                }
            }
            DeclarationPayload::Empty => {}
            DeclarationPayload::Opaque { type_id, .. } => panic!(
                "{}: v2 families author typed IR, never opaque payload {type_id}",
                declaration.address()
            ),
        }
        assert!(
            !declaration.address().key().as_str().is_empty(),
            "every declaration binds a logical address"
        );
    }
}

#[test]
fn v2_integration_sweep_no_log_only_terminals_in_v2_sources() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api");
    let source = |file: &str| {
        std::fs::read_to_string(root.join(file))
            .unwrap_or_else(|error| panic!("read {file}: {error}"))
    };

    // Families whose modules carry no logging at all.
    for file in ["sample.rs", "buffer.rs", "sfz.rs", "route.rs"] {
        let text = source(file);
        for pattern in ["log::", "tracing::", "println!", "eprintln!"] {
            assert!(
                !text.contains(pattern),
                "{file} must stay free of log terminals, found {pattern}"
            );
        }
    }

    // Classified v1 compatibility log sites are pinned by count: growing
    // the inventory means a v2 path started logging and must be reviewed.
    let recording = source("recording.rs");
    assert_eq!(
        recording.matches("log::").count(),
        2,
        "recording.rs carries exactly the two classified v1 stop/cancel log sites"
    );
    let midi = source("midi/mod.rs");
    assert_eq!(
        midi.matches("log::").count(),
        1,
        "midi/mod.rs carries exactly the one classified v1 warn site"
    );
    assert_eq!(
        midi.matches("MidiDeviceId::new(u32::MAX)").count(),
        1,
        "the v1 sentinel constructor appears exactly once, in the v1 section"
    );

    // Nothing below the detached v2 banners logs or constructs sentinels.
    for file in ["route.rs", "midi/mod.rs"] {
        let text = source(file);
        let banner = text
            .find("// Detached v2")
            .unwrap_or_else(|| panic!("{file} keeps its detached v2 banner"));
        let detached = &text[banner..];
        for pattern in [
            "log::",
            "tracing::",
            "println!",
            "eprintln!",
            "MidiDeviceId::new(u32::MAX)",
        ] {
            assert!(
                !detached.contains(pattern),
                "{file}: the detached v2 section must not contain {pattern}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// M09 landing gate (B3): submissions requiring unavailable capabilities
// yield exact typed rejection receipts before any staging or planning work.
// ---------------------------------------------------------------------------

fn capability_catalog() -> &'static CapabilityCatalog {
    static CATALOG: OnceLock<CapabilityCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../api/effective-metadata-v1.json");
        let source = std::fs::read_to_string(path).expect("accepted metadata must exist");
        capability_fixtures::catalog_from_conventions_json(&source)
            .expect("accepted metadata must validate")
    })
}

fn capability_snapshot(
    withheld: &[(&str, AvailabilityGate, &str)],
    epoch: RuntimeEpoch,
    target_id: &str,
) -> CapabilitySnapshot {
    let status = RuntimeMutationStatus {
        schema_version: MUTATION_SCHEMA_VERSION,
        runtime_epoch: epoch,
        event_sequence: None,
        accepted_through: None,
        last_confirmed_revision: None,
        last_rejected_revision: None,
        live_state: LiveState::Clean,
        pending: Vec::new(),
        receipt_window: ReceiptWindow {
            first_event_sequence: None,
            last_event_sequence: None,
            first_revision: None,
            last_revision: None,
            expires_before: None,
        },
    };
    let subject = CapabilitySubject::new(
        "runtime.local",
        target_id,
        format!("sha256:{}", "a".repeat(64)),
    )
    .expect("privacy-safe subject");
    capability_fixtures::declared_snapshot(
        capability_catalog(),
        subject,
        &status,
        withheld,
        Timestamp::parse("2026-07-23T00:00:00Z").expect("timestamp"),
    )
    .expect("deterministic snapshot")
}

#[test]
fn v2_integration_capability_rejection_yields_exact_receipts_before_staging() {
    struct Context {
        label: &'static str,
        script: &'static str,
        declared_keys: &'static [&'static str],
        required: &'static [&'static str],
        withheld: &'static [(&'static str, AvailabilityGate, &'static str)],
        binding_targets: &'static [&'static str],
        expected_dsp_ugen: Option<&'static str>,
        target_id: &'static str,
        code: &'static str,
    }
    let contexts = [
        Context {
            label: "native-only sfz on a non-native target",
            script: r#"// vibe-api: 2
sfz("piano", "instruments/piano.sfz").apply();
"#,
            declared_keys: &["piano"],
            required: &["capability.resource.sfz"],
            withheld: &[(
                "capability.resource.sfz",
                AvailabilityGate::Target,
                "reason.target_unsupported",
            )],
            binding_targets: &["v1:entry:26a908343e7f7c0d"],
            expected_dsp_ugen: None,
            target_id: "target.wasm32",
            code: "reason.target_unsupported",
        },
        Context {
            label: "native-only record on a non-native target",
            script: r#"// vibe-api: 2
define_group("band", || {});
record("take").from(group_ref("band")).beats(8.0).start();
"#,
            declared_keys: &["take"],
            required: &["capability.recording.audio"],
            withheld: &[(
                "capability.recording.audio",
                AvailabilityGate::Target,
                "reason.target_unsupported",
            )],
            binding_targets: &["v1:entry:29a64e7fe0025754", "v1:entry:66221fd87818d486"],
            expected_dsp_ugen: None,
            target_id: "target.wasm32",
            code: "reason.target_unsupported",
        },
        Context {
            label: "midi routing without the midi build feature",
            script: r#"// vibe-api: 2
let v = voice("v").synth("sine").apply();
midi_device("pads").port("MPK Mini").input().apply();
keyboard_route(midi_device_ref("pads")).channel(2).to(v);
"#,
            declared_keys: &["pads"],
            required: &["capability.midi.input"],
            withheld: &[(
                "capability.midi.input",
                AvailabilityGate::BuildFeature,
                "reason.compile_feature_disabled",
            )],
            binding_targets: &[
                "v1:entry:5653421b5f2800d5",
                "v1:entry:a95001902499186d",
                "v1:entry:ba4729487f86533a",
            ],
            expected_dsp_ugen: None,
            target_id: "target.native.linux",
            code: "reason.compile_feature_disabled",
        },
        Context {
            label: "plugin-dependent synthdef without the plugin",
            script: r#"// vibe-api: 2
define_synthdef("plugin_voice").body(|| mi_plaits_ar()).apply();
"#,
            declared_keys: &["plugin_voice"],
            required: &["capability.plugin.mi_ugens"],
            withheld: &[(
                "capability.plugin.mi_ugens",
                AvailabilityGate::RuntimeProbe,
                "reason.plugin_missing",
            )],
            binding_targets: &["v1:entry:662c4e6757067734"],
            expected_dsp_ugen: Some("MiPlaits"),
            target_id: "target.native.linux",
            code: "reason.plugin_missing",
        },
    ];

    for (index, context) in contexts.iter().enumerate() {
        let ledger = MutationLedger::new(LedgerConfig::default()).unwrap();

        // The submission comes through the one production registration root.
        let mut engine =
            production_engine_at(60 + u8::try_from(index).unwrap(), ledger.runtime_epoch());
        let candidate = evaluate(&mut engine, context.script);
        assert_eq!(
            candidate.identity().runtime_epoch(),
            ledger.runtime_epoch(),
            "{}: evaluation and admission share one runtime epoch",
            context.label
        );
        let keys = keys_of(&candidate);
        for key in context.declared_keys {
            assert!(keys.contains(*key), "{}: declares {key}", context.label);
        }
        if let Some(expected_ugen) = context.expected_dsp_ugen {
            let declaration = candidate
                .declarations()
                .iter()
                .find(|declaration| {
                    declaration.address().key().as_str() == context.declared_keys[0]
                })
                .expect("plugin-dependent synthdef declaration");
            let DeclarationPayload::Authoring {
                declaration: AuthoringDeclaration::SynthDef(definition),
                ..
            } = declaration.payload()
            else {
                panic!("{}: declaration is a SynthDef", context.label)
            };
            assert!(
                definition
                    .definition
                    .graph()
                    .nodes
                    .iter()
                    .any(|node| node.name == expected_ugen),
                "{}: candidate IR contains {expected_ugen}",
                context.label
            );
        }

        // M05 deterministic snapshot: every required capability is exactly
        // unavailable for exactly the declared catalog reason.
        let snapshot = capability_snapshot(
            context.withheld,
            candidate.identity().runtime_epoch(),
            context.target_id,
        );
        snapshot.verify_snapshot_id().expect("snapshot id verifies");
        for required in context.required {
            let status = snapshot
                .capabilities()
                .iter()
                .find(|status| status.capability_id() == *required)
                .unwrap_or_else(|| panic!("{}: status for {required}", context.label));
            assert_eq!(
                status.state_id(),
                "availability.unavailable",
                "{}: {required}",
                context.label
            );
            assert_eq!(
                status.reason_ids(),
                &[context.code.to_string()],
                "{}: {required} carries the exact reason",
                context.label
            );
        }

        // The accepted metadata binds real API entries to the same predicate
        // and the M05 availability evaluator reports them unavailable for the
        // same reason.
        let catalog = capability_catalog();
        for target_id in context.binding_targets {
            let binding = catalog
                .availability_bindings()
                .find(|binding| binding.target_id == *target_id)
                .unwrap_or_else(|| panic!("{}: exact binding {target_id}", context.label));
            assert!(
                binding
                    .predicate_capability_ids
                    .iter()
                    .any(|id| context.required.contains(&id.as_str())),
                "{}: {target_id} is bound to the rejected capability",
                context.label
            );
            let predicate = catalog
                .evaluate_availability_binding(target_id, snapshot.capabilities())
                .expect("binding evaluates");
            assert_eq!(
                predicate.state_id, "availability.unavailable",
                "{}: bound entry {}",
                context.label, predicate.target_id
            );
            assert_eq!(
                predicate.reason_ids,
                vec![context.code.to_string()],
                "{}: {target_id} carries the exact reason",
                context.label
            );
        }

        // Exact typed rejection receipt, minted at the pre-acceptance
        // capability check.
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
        let submission = Submission {
            kind: MutationKind::Candidate {
                origin: CandidateOrigin::RhaiHost,
            },
            source: MutationSource::Rhai {
                engine_id: "v2.integration.capability".into(),
            },
            caller_namespace: "v2.integration.capability".into(),
            idempotency_key: None,
            require_idempotency_key: false,
            retry_epoch: Some(ledger.runtime_epoch()),
            expected_revision: None,
            atomicity: Atomicity::Required,
            supersession: SupersessionPolicy::Fifo,
            material: RequestMaterial::new(
                &(context.label, keys.iter().cloned().collect::<Vec<String>>()),
                Some(&()),
            )
            .unwrap(),
        };
        let candidate_submission = CandidateSubmission::new(submission).unwrap();
        let SubmissionResult::New(receipt) = ledger
            .submit(candidate_submission.submission().clone(), now)
            .unwrap()
        else {
            panic!("{}: fresh submission", context.label);
        };
        assert_eq!(
            receipt.state,
            ReceiptState::Evaluating {
                phase: PreAcceptancePhase::Decode
            },
            "{}",
            context.label
        );
        assert!(receipt.revision.is_none(), "{}", context.label);
        let checking = ledger
            .transition(
                receipt.attempt_id,
                ReceiptState::Evaluating {
                    phase: PreAcceptancePhase::CapabilityCheck,
                },
                now,
            )
            .expect("the capability check is a pre-acceptance phase");
        assert_eq!(
            checking.state,
            ReceiptState::Evaluating {
                phase: PreAcceptancePhase::CapabilityCheck
            },
            "{}",
            context.label
        );
        let expected = Rejected {
            phase: FailurePhase::Capability,
            code: context.code.into(),
            message: format!(
                "submission requires unavailable {}",
                context.required.join("+")
            ),
            rollback: RollbackState::NotNeeded,
            preserved_revision: None,
        };
        let rejected = ledger
            .transition(
                receipt.attempt_id,
                ReceiptState::Terminal(TerminalOutcome::Rejected(expected.clone())),
                now,
            )
            .expect("capability rejection is a legal pre-acceptance terminal");
        assert_eq!(
            rejected.state,
            ReceiptState::Terminal(TerminalOutcome::Rejected(expected)),
            "{}: the receipt round-trips the exact typed rejection",
            context.label
        );

        // Zero residue: never accepted, no revision allocated, no planning or
        // staging reachable, and the runtime mutation truth is untouched.
        assert!(rejected.revision.is_none(), "{}", context.label);
        assert!(
            rejected.timestamps.accepted_at.is_none(),
            "{}: rejected before acceptance",
            context.label
        );
        assert!(
            rejected.timestamps.terminal_at.is_some(),
            "{}",
            context.label
        );
        let status = ledger.status(now);
        assert_eq!(status.last_confirmed_revision, None, "{}", context.label);
        assert_eq!(status.last_rejected_revision, None, "{}", context.label);
        assert!(status.pending.is_empty(), "{}", context.label);
        assert!(
            matches!(status.live_state, LiveState::Clean),
            "{}",
            context.label
        );
        assert!(
            ledger
                .begin_planning(receipt.attempt_id, Vec::new(), now)
                .is_err(),
            "{}: a capability-rejected attempt can never reach planning",
            context.label
        );

        // Control: the same required capabilities under a snapshot with no
        // withholdings have nothing to reject.
        let available = capability_snapshot(
            &[],
            candidate.identity().runtime_epoch(),
            "target.native.linux",
        );
        for required in context.required {
            let status = available
                .capabilities()
                .iter()
                .find(|status| status.capability_id() == *required)
                .expect("available status");
            assert_eq!(
                status.state_id(),
                "availability.available",
                "{}: {required} control",
                context.label
            );
        }

        let fresh = evaluate(
            &mut engine,
            "// vibe-api: 2\ndefine_group(\"fresh\", || {});",
        );
        assert_eq!(
            keys_of(&fresh),
            BTreeSet::from(["fresh".to_owned()]),
            "{}: a rejected candidate leaves no next-evaluation residue",
            context.label
        );
    }
}

// ---------------------------------------------------------------------------
// M08 landing gate (B4): Pattern/Melody/Sequence unchanged-config stop —
// re-evaluating a byte-identical declaration then stopping produces the stop
// terminal with no content swap and no identity churn.
// ---------------------------------------------------------------------------

#[test]
fn v2_integration_unchanged_config_stop_preserves_identity_and_content() {
    fn declared(candidate: &Candidate, key: &str) -> (LogicalAddress, TerminalEffect, Vec<u8>) {
        let declaration = candidate
            .declarations()
            .iter()
            .find(|declaration| declaration.address().key().as_str() == key)
            .unwrap_or_else(|| panic!("declaration {key}"));
        let bytes = match declaration.payload() {
            DeclarationPayload::Authoring {
                canonical_bytes, ..
            } => canonical_bytes.to_vec(),
            _ => panic!("{key}: authoring payload"),
        };
        (
            declaration.address().clone(),
            declaration.lifecycle().terminal_effect,
            bytes,
        )
    }

    struct Family {
        label: &'static str,
        prelude: &'static str,
        key: &'static str,
        chain: &'static str,
        changed_chain: &'static str,
    }
    let families = [
        Family {
            label: "pattern",
            prelude: "let lead = voice(\"lead\").synth(\"sine\").apply();\n",
            key: "beat",
            chain: "pattern(\"beat\").on(lead).step(\"x.x.\")",
            changed_chain: "pattern(\"beat\").on(lead).step(\"xx..\")",
        },
        Family {
            label: "melody",
            prelude: "let lead = voice(\"lead\").synth(\"sine\").apply();\n",
            key: "hook",
            chain: "melody(\"hook\").on(lead).notes(\"C4 E4 G4 C5\")",
            changed_chain: "melody(\"hook\").on(lead).notes(\"C4 E4 G4 B4\")",
        },
        Family {
            label: "sequence",
            prelude: "",
            key: "intro",
            chain: "sequence(\"intro\").loop_beats(4.0)",
            changed_chain: "sequence(\"intro\").loop_beats(8.0)",
        },
    ];

    for (index, family) in families.iter().enumerate() {
        let mut engine = production_engine(70 + u8::try_from(index).unwrap());
        let script = |action: &str, chain: &str| {
            format!(
                "// vibe-api: 2\n{}let subject = {}.apply();\n{action}(subject);\n",
                family.prelude, chain
            )
        };

        // 1. Register the declaration and start it through the typed Ref. The
        //    declaration remains dormant authoring content; playback intent
        //    is exactly one separately keyed lifecycle operation.
        let started = evaluate(&mut engine, &script("start", family.chain));
        let (address, effect, bytes) = declared(&started, family.key);
        assert_eq!(
            effect,
            TerminalEffect::Register,
            "{}: authoring is registered independently of playback intent",
            family.label
        );
        assert_eq!(
            started.operations().len(),
            1,
            "{}: exactly one start operation",
            family.label
        );
        let start = &started.operations()[0];
        assert!(
            matches!(start.action(), LifecycleAction::Start(_)),
            "{}: the operation is exactly Start",
            family.label
        );
        assert_eq!(
            start.target().address(),
            &address,
            "{}: start targets the registered identity",
            family.label
        );
        assert_eq!(
            start.lifecycle(),
            &LifecycleMetadata::reference(TerminalEffect::Start, Cancellation::BeforePlanning),
            "{}: the start receipt is exact",
            family.label
        );

        // 2. Re-evaluating the byte-identical config keeps the logical
        //    identity and the canonical content: a reload has nothing to
        //    swap and no identity churn.
        let unchanged = evaluate(&mut engine, &script("start", family.chain));
        assert_eq!(
            unchanged.identity(),
            started.identity(),
            "{}: stable typed evaluation identity across reload",
            family.label
        );
        let (address_again, effect_again, bytes_again) = declared(&unchanged, family.key);
        assert_eq!(
            address_again, address,
            "{}: stable logical identity across evaluations",
            family.label
        );
        assert_eq!(
            bytes_again, bytes,
            "{}: unchanged config authors byte-identical canonical content",
            family.label
        );
        assert_eq!(effect_again, TerminalEffect::Register, "{}", family.label);
        assert_eq!(
            keys_of(&unchanged),
            keys_of(&started),
            "{}: the declaration set is churn-free",
            family.label
        );
        assert_eq!(
            unchanged.operations(),
            started.operations(),
            "{}: the unchanged reload preserves the exact start operation",
            family.label
        );

        // 3. Stop over the same unchanged config: the declaration re-registers
        //    with byte-identical canonical content (nothing to swap) and the
        //    stop is exactly one typed lifecycle operation on the same
        //    logical identity.
        let stopped = evaluate(&mut engine, &script("stop", family.chain));
        assert_eq!(
            stopped.identity(),
            started.identity(),
            "{}: stop stays in the same typed evaluation identity",
            family.label
        );
        let (address_stop, effect_stop, bytes_stop) = declared(&stopped, family.key);
        assert_eq!(address_stop, address, "{}", family.label);
        assert_eq!(
            effect_stop,
            TerminalEffect::Register,
            "{}: the unchanged declaration re-registers without restarting",
            family.label
        );
        assert_eq!(
            bytes_stop, bytes,
            "{}: stopping an unchanged declaration swaps no content",
            family.label
        );
        assert_eq!(keys_of(&stopped), keys_of(&started), "{}", family.label);
        let operations = stopped.operations();
        assert_eq!(
            operations.len(),
            1,
            "{}: exactly one lifecycle operation",
            family.label
        );
        let stop = &operations[0];
        assert!(
            matches!(stop.action(), LifecycleAction::Stop),
            "{}: the operation is exactly Stop",
            family.label
        );
        assert_eq!(
            stop.target().address(),
            &address,
            "{}: the stop targets the unchanged identity",
            family.label
        );
        assert_eq!(
            stop.lifecycle(),
            &LifecycleMetadata::reference(TerminalEffect::Stop, Cancellation::NotCancellable),
            "{}: the stop receipt is exact",
            family.label
        );

        // 4. Negative control: a changed config authors different canonical
        //    content under the same identity — exactly the swap trigger.
        let changed = evaluate(&mut engine, &script("start", family.changed_chain));
        assert_eq!(
            changed.identity(),
            started.identity(),
            "{}: changed content does not churn typed identity",
            family.label
        );
        let (address_changed, _, bytes_changed) = declared(&changed, family.key);
        assert_eq!(
            address_changed, address,
            "{}: the changed declaration keeps its identity",
            family.label
        );
        assert_ne!(
            bytes_changed, bytes,
            "{}: changed config DOES author swapped content",
            family.label
        );
        assert_eq!(
            changed.operations(),
            started.operations(),
            "{}: the changed-config control differs only in declaration content",
            family.label
        );
    }
}
