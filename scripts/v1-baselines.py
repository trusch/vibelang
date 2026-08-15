#!/usr/bin/env python3
import difflib
import hashlib
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SNAPSHOT_PATH = ROOT / "api/baselines/public-artifacts-v1.json"
FAILURE_SEAM_PATH = ROOT / "api/baselines/failure-injection-seams-v1.json"
ACCEPTED_BASE = "9aff9b40db1597364279f9aacf47d436718a031e"
M04_ACCEPTED_BASE = "8c7c1fd1c70bb04e70e6f7da802ff7357acfe6a2"
M04_ACCEPTED_TREE = "a46bdf742dff00439fc19f85c1d08424867fee03"
M04_PARENT = "34d31d68c4fa821a493bd244344025d620425a16"
M04_LINEAGE = [
    "b781fe90dde75559e9b2f1767255345ffbde0af0",
    "0d62dee9fb70f5699e757a85226f818029b995ad",
    "870d2e8e11943a81130828e2e254705a9200e31d",
    "f28279429499c045b9058e5686b47382c9d5a29f",
    "09ed9a84bf4b4c250de493015c71c166406b0986",
    "f189c4ad8009c0102ca1af20562df22defeab6ae",
    "bafd94586524c67c6fc74007aa742326a81d0a94",
    M04_ACCEPTED_BASE,
]

REQUIRED_AUTHORING_FAMILIES = {
    "transport",
    "group",
    "voice",
    "pattern",
    "melody",
    "sequence",
    "fade",
    "effect",
    "output_route",
    "sample",
    "sfz",
    "buffer",
    "recording",
    "synthdef",
    "effectdef",
    "dsp_value",
    "midi",
}
REQUIRED_NEGATIVE_DEFECTS = {
    "ignored_fields",
    "invalid_ugen_labels",
    "semantic_token_mismatch",
    "push_pull_diagnostic_mismatch",
    "stale_commands",
}


def direct_test(source, anchor):
    return {"source": source, "anchor": anchor}


RELOAD_PHASE_RECEIPT_TEST = direct_test(
    "crates/vibelang-core/src/runtime.rs",
    "async fn every_reload_phase_failure_has_direct_fenced_partial_receipt_coverage()",
)


def reload_apply_seam(
    phase,
    component_index,
    component_path,
    component_action,
    source_anchor,
    current_injection,
    continuation,
    *source_assertions,
):
    return {
        "scope": "reload_apply",
        "phase": phase,
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": source_anchor,
        "source_assertions": list(source_assertions),
        "current_injection": current_injection,
        "current_outcome": (
            f"failures are structured on {component_path}; the canonical reload receipt is "
            "terminal Partial with the failed component Uncertain, rollback Uncertain, and "
            f"fenced=true; continuation={continuation}"
        ),
        "component_index": component_index,
        "component_path": component_path,
        "component_action": component_action,
        "continuation": continuation,
        "direct_tests": [RELOAD_PHASE_RECEIPT_TEST],
    }


FAILURE_SEAM_TRUTH = [
    {
        "scope": "evaluation",
        "phase": "parse",
        "source": "crates/vibelang-rhai/src/engine.rs",
        "anchor": "pub fn execute(&mut self, script: &str) -> Result<ScriptState>",
        "source_assertions": ["fn host_failure(error: &Error) -> (FailurePhase, &'static str)"],
        "current_injection": "invalid Rhai source",
        "current_outcome": "the preallocated carrier attempt is terminal Rejected as effect-free, has no revision, and preserves one attempt identity",
        "direct_tests": [
            direct_test(
                "crates/vibelang-cli/src/main.rs",
                "fn cli_parse_failure_is_effect_free_rejected_attempt()",
            ),
            direct_test(
                "crates/vibelang-rhai/src/engine.rs",
                "async fn host_parse_failure_is_effect_free_rejected_attempt()",
            ),
            direct_test(
                "crates/vibelang-http/src/routes/eval.rs",
                "async fn http_eval_parse_failure_is_effect_free_rejected_attempt()",
            ),
            direct_test(
                "crates/vibelang-wasm/src/lib.rs",
                "fn wasm_parse_failure_is_effect_free_rejected_attempt()",
            ),
        ],
    },
    {
        "scope": "evaluation",
        "phase": "evaluate",
        "source": "crates/vibelang-rhai/src/engine.rs",
        "anchor": "let result = self.engine.run(script).map_err(Error::from);",
        "source_assertions": ["record_uncertain_effect"],
        "current_injection": "runtime Rhai error after eager host work",
        "current_outcome": "the preallocated attempt is terminal Partial with candidate-local effect evidence and fenced=true; an effect-free evaluation error is Rejected",
        "direct_tests": [
            direct_test(
                "crates/vibelang-cli/src/main.rs",
                "fn cli_and_http_eager_failures_are_fenced_on_their_canonical_attempts()",
            ),
            direct_test(
                "crates/vibelang-rhai/src/engine.rs",
                "async fn host_runtime_failure_after_eager_effect_is_fenced_partial()",
            ),
            direct_test(
                "crates/vibelang-http/src/routes/eval.rs",
                "async fn http_eval_eager_failure_is_fenced_partial()",
            ),
            direct_test(
                "crates/vibelang-wasm/src/lib.rs",
                "fn wasm_eager_evaluation_failure_is_fenced_partial()",
            ),
        ],
    },
    {
        "scope": "evaluation",
        "phase": "graph_compile",
        "source": "crates/vibelang-dsp/src/api.rs",
        "anchor": "fn build(self, closure: rhai::FnPtr)",
        "source_assertions": ["self.build(closure).map_err(synthdef_error_to_eval)?"],
        "current_injection": "invalid DSP graph closure",
        "current_outcome": "a structured Rhai evaluation error reaches the carrier; the attempt is Rejected if effect-free or fenced Partial if earlier eager effects were recorded",
        "direct_tests": [
            direct_test(
                "crates/vibelang-rhai/src/engine.rs",
                "async fn host_runtime_failure_after_eager_effect_is_fenced_partial()",
            )
        ],
    },
    {
        "scope": "reload_planning",
        "phase": "validate",
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": "self.build_reload_diff(&new_state).await;",
        "source_assertions": [
            "execution.phases[6].failures = port_reconcile_failures;",
            "reload_voice_port_reconcile_failed",
        ],
        "current_injection": "diff construction or voice-port reconciliation failure",
        "current_outcome": "there is no standalone atomic validation boundary; port-reconcile failures are structured on reload/output_routes and make the reload receipt fenced Partial",
        "direct_tests": [RELOAD_PHASE_RECEIPT_TEST],
    },
    {
        "scope": "reload_planning",
        "phase": "plan",
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": "fn begin_contextual_components(",
        "source_assertions": [
            ".begin_planning(context.attempt_id(), planned, now)",
            "code: \"reload_planning_failed\"",
        ],
        "current_injection": "receipt planning transition failure",
        "current_outcome": "the canonical receipt enters Planning before Staging or Committing; a planning transition failure is terminalized as effect-free Rejected",
        "direct_tests": [
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn receipt_submission_returns_accepted_before_runtime_work_and_preserves_context()",
            )
        ],
    },
    {
        "scope": "reload_staging",
        "phase": "stage_resources",
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": "fn spawn_reload_staging(",
        "source_assertions": [
            "failure: Some(reload::StagedAssetFailure",
            "finish_lost_staged_reload(&ledger, &context, staging, cleanup)",
        ],
        "current_injection": "sample or SFZ load failure, or staged completion lost to a closed runtime queue",
        "current_outcome": "per-asset failures and lost completion are canonical terminal Partial diagnostics; loaded leftovers are deterministically reclaimed and cleanup is a receipt component",
        "direct_tests": [
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn direct_staging_load_failure_preserves_its_code_and_cleanup()",
            ),
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn lost_staged_apply_frees_every_buffer_in_deterministic_order()",
            ),
        ],
    },
    reload_apply_seam(
        "transport",
        0,
        "reload/transport",
        "apply_changes",
        "async fn phase_apply_transport_changes",
        "transport handler failure",
        "abort",
        "execution.phases[0].started = true;",
        "self.phase_apply_transport_changes(&diff).await",
    ),
    reload_apply_seam(
        "stop_deleted",
        1,
        "reload/stop_deleted",
        "stop",
        "async fn phase_stop_deleted_entities",
        "backend rejection while stopping removed entities",
        "continue",
        "execution.phases[1].failures = self.phase_stop_deleted_entities(&diff).await;",
    ),
    reload_apply_seam(
        "delete",
        2,
        "reload/delete_entities",
        "delete",
        "async fn phase_delete_entities",
        "backend or resource rejection while deleting entities",
        "continue",
        "execution.phases[2].failures = self",
        ".phase_delete_entities(&diff, group_teardown_grace)",
    ),
    reload_apply_seam(
        "open_midi",
        3,
        "reload/midi_devices",
        "open",
        "async fn phase_open_midi_devices",
        "unavailable MIDI device or MIDI connection-channel failure",
        "mixed",
        "self.phase_open_midi_devices(&new_state).await;",
        "if !continue_apply",
    ),
    reload_apply_seam(
        "create",
        4,
        "reload/create_entities",
        "create",
        "async fn phase_create_entities",
        "backend or resource failure while creating an entity",
        "continue",
        "execution.phases[4].failures = self.phase_create_entities(&diff, &new_state, staged).await;",
    ),
    reload_apply_seam(
        "update",
        5,
        "reload/update_entities",
        "update",
        "async fn phase_update_entities",
        "backend or resource failure while updating an entity",
        "continue",
        "execution.phases[5].failures = self.phase_update_entities(&diff, &new_state).await;",
    ),
    reload_apply_seam(
        "output_routes",
        6,
        "reload/output_routes",
        "finalize",
        "async fn phase_finalize_output_routes",
        "voice-port reconcile or output-route finalizer failure",
        "abort_on_finalizer_failure",
        "execution.phases[6].failures = port_reconcile_failures;",
        "self.phase_finalize_output_routes(&diff).await",
    ),
    reload_apply_seam(
        "input_routes",
        7,
        "reload/input_routes",
        "finalize",
        "async fn phase_finalize_input_routes",
        "input-route finalizer failure",
        "abort",
        "self.phase_finalize_input_routes(&input_routes).await",
        "execution.phases[7].failures.push",
    ),
    reload_apply_seam(
        "effects",
        8,
        "reload/effects",
        "apply",
        "async fn phase_apply_effects",
        "missing synthdef or backend effect failure",
        "continue",
        "execution.phases[8].failures = self.phase_apply_effects(&diff, &new_state).await;",
    ),
    reload_apply_seam(
        "groups",
        9,
        "reload/groups",
        "finalize",
        "async fn phase_finalize_groups",
        "group link or parameter finalization failure",
        "continue",
        "execution.phases[9].failures = self.phase_finalize_groups(&diff).await;",
    ),
    reload_apply_seam(
        "fades",
        10,
        "reload/fades",
        "apply",
        "async fn phase_apply_fades",
        "missing target or backend fade failure",
        "continue",
        "execution.phases[10].failures = self.phase_apply_fades(&diff, &new_state).await;",
    ),
    reload_apply_seam(
        "start_running",
        11,
        "reload/patterns",
        "reconcile_playback",
        "async fn phase_start_running_patterns",
        "pattern, melody, or sequence playback reconciliation failure",
        "continue",
        "execution.phases[11].failures = self.phase_start_running_patterns(&diff, &new_state).await;",
    ),
    reload_apply_seam(
        "trigger_running_voices",
        12,
        "reload/voices",
        "reconcile_running",
        "async fn phase_trigger_running_voices",
        "running-voice trigger or dependency failure",
        "continue",
        "execution.phases[12].failures = self.phase_trigger_running_voices(&new_state).await;",
    ),
    reload_apply_seam(
        "param_routes",
        13,
        "reload/param_routes",
        "finalize",
        "async fn phase_finalize_param_routes",
        "parameter-route finalizer failure",
        "abort",
        "self.phase_finalize_param_routes(&diff, &new_state).await",
        "execution.phases[13].failures.push",
    ),
    reload_apply_seam(
        "midi_routes",
        14,
        "reload/midi_routes",
        "apply",
        "async fn phase_apply_midi_routes",
        "device disappearance or MIDI route handler failure",
        "continue",
        "execution.phases[14].failures = self.phase_apply_midi_routes(&new_state).await;",
    ),
    {
        "scope": "reload_commit",
        "phase": "commit",
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": "async fn snapshot_script_config",
        "source_assertions": [
            "self.snapshot_script_config(new_state).await;",
            "finish_reload_receipt(&self.mutation_ledger, context, execution)",
        ],
        "current_injection": "none; commit eligibility depends on apply continuation",
        "current_outcome": "the script snapshot advances after a no-op or a completed best-effort phase sequence, including non-aborting failures; aborting transport, route, or MIDI-channel failures retain the previous snapshot, and receipt terminalization follows apply",
        "direct_tests": [
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn reload_failure_reports_exact_phase_components_and_fences()",
            )
        ],
    },
    {
        "scope": "reload_cleanup",
        "phase": "cleanup",
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": "async fn discard_staged_leftovers",
        "source_assertions": [
            "cleanup_phase.failures = cleanup;",
            "execution.phases[15].failures = cleanup;",
        ],
        "current_injection": "backend free-buffer failure for an unconsumed staged asset",
        "current_outcome": "cleanup always occupies reload/staged_assets; failure is a structured diagnostic and Uncertain component in a terminal fenced Partial receipt",
        "component_index": 15,
        "component_path": "reload/staged_assets",
        "component_action": "discard_leftovers",
        "continuation": "after_apply",
        "direct_tests": [
            RELOAD_PHASE_RECEIPT_TEST,
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn lost_staged_apply_frees_every_buffer_in_deterministic_order()",
            ),
        ],
    },
    {
        "scope": "reload_receipt",
        "phase": "receipt",
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": "fn finish_reload_receipt(",
        "source_assertions": [
            "TerminalOutcome::Partial(Partial",
            "rollback: RollbackState::Uncertain",
            "fenced: true",
        ],
        "current_injection": "any staging, apply-phase, or cleanup failure",
        "current_outcome": "all failures are caller-visible diagnostics; the first code becomes the terminal Partial code, every failed component is Uncertain, and the runtime is fenced",
        "direct_tests": [
            RELOAD_PHASE_RECEIPT_TEST,
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn reload_receipt_preserves_multiple_failures_in_one_phase()",
            ),
        ],
    },
    {
        "scope": "barrier",
        "phase": "barrier",
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": "pub async fn sync_and_wait(&self) -> Result<()>",
        "source_assertions": [
            "phase: FailurePhase::BackendBarrier",
            "Err(Error::AcknowledgementLost)",
            "Err(Error::SyncTimeout)",
        ],
        "current_injection": "backend sync failure, timeout, or acknowledgement loss",
        "current_outcome": "sync is a correlated canonical attempt; backend failure is terminal Partial and fences, while timeout or acknowledgement loss returns a distinct error and fences incomplete work",
        "direct_tests": [
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn sync_and_wait_reports_backend_failure_timeout_and_ack_loss()",
            )
        ],
    },
    {
        "scope": "fencing",
        "phase": "fence",
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": "fn mutation_is_fenced(",
        "source_assertions": [
            "Error::RuntimeFenced",
            "pub fn continue_best_effort",
            "effect_completed_after_runtime_fence",
        ],
        "current_injection": "submission after partial or unknown live state, or late completion after a newer fence",
        "current_outcome": "new mutations are rejected while fenced; late effects remain Partial; continue_best_effort requires exact acknowledgement of the active partial receipt",
        "direct_tests": [
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn staged_apply_after_newer_fence_is_truthful_partial()",
            ),
            direct_test(
                "crates/vibelang-cli/src/main.rs",
                "fn runtime_fence_requires_explicit_acknowledgement()",
            ),
        ],
    },
    {
        "scope": "admission",
        "phase": "queue_admission",
        "source": "crates/vibelang-core/src/runtime.rs",
        "anchor": "enum QueueAdmissionFailure",
        "source_assertions": ["QueueAdmissionFailure::Full", "QueueAdmissionFailure::Closed"],
        "current_injection": "full or closed runtime mutation queue",
        "current_outcome": "queue_full and queue_closed are distinct effect-free Rejected attempts; a failed admission allocates no revision, while known earlier eager effects retain a fenced Partial receipt",
        "direct_tests": [
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn queue_full_and_closed_are_distinct_and_never_allocate_failed_revision()",
            ),
            direct_test(
                "crates/vibelang-core/src/runtime.rs",
                "async fn preallocated_effectful_queue_failure_retains_partial_receipt()",
            ),
        ],
    },
    {
        "scope": "carrier",
        "phase": "cli_carrier",
        "source": "crates/vibelang-cli/src/main.rs",
        "anchor": "async fn submit_cli_reload_attempt(",
        "source_assertions": ["async fn wait_terminal", "fn require_applied"],
        "current_injection": "late Partial, sync failure, or fenced retry",
        "current_outcome": "startup and one-shot wait for terminal truth by default; Partial is nonzero and prints components/fence state; watch stops mutation submission while fenced",
        "direct_tests": [
            direct_test(
                "crates/vibelang-cli/src/main.rs",
                "async fn terminal_wait_preserves_readiness_and_late_partial_truth()",
            ),
            direct_test(
                "crates/vibelang-cli/src/main.rs",
                "fn late_partial_overrides_accepted_and_fences_retry()",
            ),
        ],
    },
    {
        "scope": "carrier",
        "phase": "rhai_carrier",
        "source": "crates/vibelang-rhai/src/engine.rs",
        "anchor": "async fn submit_host_attempt(",
        "source_assertions": ["fn host_submission_receipt(", "fn host_receipt_sink()"],
        "current_injection": "admission closes after a terminal callback or a late Partial arrives",
        "current_outcome": "candidate-local Rhai return values never replace canonical runtime truth; the canonical terminal receipt wins over admission errors and late Partial wins over Accepted",
        "direct_tests": [
            direct_test(
                "crates/vibelang-rhai/src/engine.rs",
                "fn host_carrier_keeps_late_partial_canonical()",
            ),
            direct_test(
                "crates/vibelang-rhai/src/engine.rs",
                "fn host_carrier_preserves_terminal_receipt_when_admission_channel_closes()",
            ),
        ],
    },
    {
        "scope": "carrier",
        "phase": "http_carrier",
        "source": "crates/vibelang-http/src/lib.rs",
        "anchor": "async fn project_http_mutation_response(",
        "source_assertions": ["fn canonical_http_receipt(", "fn with_receipt_status"],
        "current_injection": "pending direct mutation, evaluation-only success, or known Partial",
        "current_outcome": "pending direct mutation is 202 with canonical receipt and nested legacy result; eval success is evaluation-only; a known Partial outranks pending and never projects a legacy success response",
        "direct_tests": [
            direct_test(
                "crates/vibelang-http/src/lib.rs",
                "fn accepted_direct_mutation_is_202_with_nested_legacy_result()",
            ),
            direct_test(
                "crates/vibelang-http/src/lib.rs",
                "fn partial_receipt_outranks_pending_and_never_returns_success()",
            ),
        ],
    },
    {
        "scope": "carrier",
        "phase": "websocket_carrier",
        "source": "crates/vibelang-http/src/websocket.rs",
        "anchor": "fn telemetry(event_type: &str, data: Value, status: &RuntimeMutationStatus)",
        "source_assertions": ["event_sequence: status.event_sequence", "reset_required"],
        "current_injection": "telemetry gap, reset, or terminal Partial event",
        "current_outcome": "legacy WebSocket remains telemetry, carries epoch/sequence/receipt freshness, and never acknowledges or maps Partial to success",
        "direct_tests": [
            direct_test(
                "crates/vibelang-http/src/websocket.rs",
                "fn legacy_telemetry_carries_receipt_freshness_without_acknowledging()",
            )
        ],
    },
    {
        "scope": "carrier",
        "phase": "wasm_bridge",
        "source": "crates/vibelang-wasm/src/lib.rs",
        "anchor": "async fn load_synthdef_to_supersonic",
        "source_assertions": ["fn finish_wasm_attempt_failure(", "synthdef_bridge_failed"],
        "current_injection": "missing bridge, rejected Promise, or partial multi-asset bridge delivery",
        "current_outcome": "known initialization or bridge failure makes evaluation success false; partial delivery preserves all component evidence and yields canonical fenced Partial",
        "direct_tests": [
            direct_test(
                "crates/vibelang-wasm/src/lib.rs",
                "fn known_initialization_and_bridge_failures_are_structured_non_success()",
            ),
            direct_test(
                "crates/vibelang-wasm/src/lib.rs",
                "fn wasm_partial_bridge_load_failure_preserves_all_delivery_evidence()",
            ),
        ],
    },
    {
        "scope": "carrier",
        "phase": "wasm_dispatch",
        "source": "crates/vibelang-wasm/src/lib.rs",
        "anchor": ".submit_with_sinks(",
        "source_assertions": ["reload_dispatch_failed", "fn latest_known_receipt("],
        "current_injection": "full or closed runtime queue, or terminal callback racing admission",
        "current_outcome": "queue_full and queue_closed retain distinct canonical codes and non-success projections; a known terminal callback outranks returned admission state; success remains evaluation-only while pending",
        "direct_tests": [
            direct_test(
                "crates/vibelang-wasm/src/lib.rs",
                "fn wasm_queue_fault_receipts_preserve_distinct_carrier_projections()",
            ),
            direct_test(
                "crates/vibelang-wasm/src/lib.rs",
                "fn known_terminal_transition_outranks_returned_queue_admission()",
            ),
        ],
    },
]

REQUIRED_FAILURE_PHASES = {entry["phase"] for entry in FAILURE_SEAM_TRUTH}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def read_json(relative):
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


def build_failure_seam_catalog():
    return {
        "schema": "https://vibelang.org/schemas/v1-failure-injection-seam-inventory/1",
        "schema_version": 1,
        "accepted_base": M04_ACCEPTED_BASE,
        "provenance": {
            "assessed_commit": M04_ACCEPTED_BASE,
            "assessed_tree": M04_ACCEPTED_TREE,
            "m04_parent": M04_PARENT,
            "m04_lineage": M04_LINEAGE,
        },
        "policy": "source-backed observed M04 truth; the inventory and its tests do not alter runtime outcomes",
        "seams": [
            {"index": index, **entry}
            for index, entry in enumerate(FAILURE_SEAM_TRUTH)
        ],
    }


def render_failure_seam_catalog(catalog):
    return json.dumps(catalog, indent=2) + "\n"


def source_text(relative, source_overrides=None):
    if source_overrides and relative in source_overrides:
        return source_overrides[relative]
    return (ROOT / relative).read_text(encoding="utf-8")


def reload_phase_components(runtime_source):
    match = re.search(
        r"const RELOAD_PHASE_COMPONENTS:.*?= \[(.*?)\n\];",
        runtime_source,
        re.DOTALL,
    )
    if match is None:
        raise ValueError("apply-stage executable catalog drift: RELOAD_PHASE_COMPONENTS missing")
    return re.findall(r'\("([^"]+)",\s*"([^"]+)"\)', match.group(1))


def validate_failure_seam_catalog(catalog, source_overrides=None):
    expected = build_failure_seam_catalog()
    if catalog != expected:
        raise ValueError(
            "failure seam semantic catalog drift: committed artifact disagrees with the generated M04 truth"
        )

    indexes = [entry["index"] for entry in catalog["seams"]]
    if indexes != list(range(len(catalog["seams"]))):
        raise ValueError(f"failure seam index drift: got {indexes}")

    phases = {entry["phase"] for entry in catalog["seams"]}
    if phases != REQUIRED_FAILURE_PHASES:
        raise ValueError(
            f"failure phase catalog drift: expected {sorted(REQUIRED_FAILURE_PHASES)}, got {sorted(phases)}"
        )

    for entry in catalog["seams"]:
        source = source_text(entry["source"], source_overrides)
        for assertion in [entry["anchor"], *entry.get("source_assertions", [])]:
            if assertion not in source:
                raise ValueError(
                    f"failure seam {entry['phase']} source semantic drift in "
                    f"{entry['source']}: {assertion!r}"
                )
        if not entry["direct_tests"]:
            raise ValueError(f"failure seam {entry['phase']} has no direct executable test")
        for test in entry["direct_tests"]:
            test_source = source_text(test["source"], source_overrides)
            if test["anchor"] not in test_source:
                raise ValueError(
                    f"failure seam {entry['phase']} direct test drift in "
                    f"{test['source']}: {test['anchor']!r}"
                )

    component_entries = sorted(
        (
            entry
            for entry in catalog["seams"]
            if entry["scope"] in {"reload_apply", "reload_cleanup"}
        ),
        key=lambda entry: entry["component_index"],
    )
    component_indexes = [entry["component_index"] for entry in component_entries]
    if component_indexes != list(range(len(component_entries))):
        raise ValueError(
            f"apply-stage component index drift: expected a contiguous catalog, got {component_indexes}"
        )
    artifact_components = [
        (entry["component_path"], entry["component_action"])
        for entry in component_entries
    ]
    runtime_source = source_text("crates/vibelang-core/src/runtime.rs", source_overrides)
    executable_components = reload_phase_components(runtime_source)
    if artifact_components != executable_components:
        raise ValueError(
            "apply-stage executable catalog drift: artifact components "
            f"{artifact_components!r} != source components {executable_components!r}"
        )


def apply_artifact_drift(catalog, mutation):
    seams = catalog["seams"]
    if mutation["kind"] == "remove_phase":
        catalog["seams"] = [
            entry for entry in seams if entry["phase"] != mutation["phase"]
        ]
        return
    if mutation["kind"] == "set_field":
        entry = next(entry for entry in seams if entry["phase"] == mutation["phase"])
        entry[mutation["field"]] = mutation["value"]
        return
    raise ValueError(f"unknown artifact drift mutation: {mutation['kind']}")


def expect_semantic_drift(case, catalog, source_overrides=None):
    try:
        validate_failure_seam_catalog(catalog, source_overrides)
    except ValueError as error:
        if case["expected_error"] not in str(error):
            raise ValueError(
                f"negative drift fixture {case['id']} returned the wrong error: {error}"
            ) from error
        return
    raise ValueError(f"negative drift fixture {case['id']} was not rejected")


def test_failure_seam_drift_fixtures():
    artifact_fixture = read_json(
        "tests/fixtures/api-unification/v1/negative/failure-seam-artifact-drift.json"
    )
    for case in artifact_fixture["cases"]:
        mutated = json.loads(json.dumps(build_failure_seam_catalog()))
        apply_artifact_drift(mutated, case["mutation"])
        expect_semantic_drift(case, mutated)

    source_fixture = read_json(
        "tests/fixtures/api-unification/v1/negative/failure-seam-source-drift.json"
    )
    for case in source_fixture["cases"]:
        relative = case["path"]
        original = source_text(relative)
        if original.count(case["replace"]) != 1:
            raise ValueError(
                f"negative drift fixture {case['id']} replacement is not unique in {relative}"
            )
        source_overrides = {
            relative: original.replace(case["replace"], case["replacement"], 1)
        }
        expect_semantic_drift(case, build_failure_seam_catalog(), source_overrides)


def file_record(relative):
    data = (ROOT / relative).read_bytes()
    return {
        "path": relative,
        "bytes": len(data),
        "sha256": sha256(data),
    }


def category(paths, counts):
    records = [file_record(path) for path in sorted(paths)]
    tree = hashlib.sha256()
    for record in records:
        tree.update(record["path"].encode())
        tree.update(b"\0")
        tree.update(record["sha256"].encode())
        tree.update(b"\0")
        tree.update(str(record["bytes"]).encode())
        tree.update(b"\n")
    return {
        "counts": {
            "files": len(records),
            "bytes": sum(record["bytes"] for record in records),
            **counts,
        },
        "tree_sha256": tree.hexdigest(),
        "files": records,
    }


def manifest_counts(manifest):
    entries = manifest["entries"]
    return {
        "entries": len(entries),
        "overloads": sum(len(entry["overloads"]) for entry in entries),
        "registered_types": sum(entry["kind"] == "type" for entry in entries),
        "properties": sum(entry["kind"] in {"property_get", "property_set"} for entry in entries),
        "named_terminals": sum(
            entry.get("lifecycle", {}).get("terminal") == "named_terminal" for entry in entries
        ),
    }


def http_counts(snapshot):
    methods = {}
    for route in snapshot["routes"]:
        methods[route["method"]] = methods.get(route["method"], 0) + 1
    fields = sum(len(item.get("fields", [])) for item in snapshot["types"])
    return {
        "routes": len(snapshot["routes"]),
        "types": len(snapshot["types"]),
        "fields": fields,
        "mutating_routes": sum(route["method"] != "GET" for route in snapshot["routes"]),
        "methods": dict(sorted(methods.items())),
    }


def editor_counts():
    rhai = read_json("vscode-extension/src/data/rhai-api.json")
    lsp = read_json("crates/vibelang-lsp/src/data/rhai-api.json")
    stdlib = read_json("vscode-extension/src/data/stdlib.json")
    canonical_ugens = sorted((ROOT / "crates/vibelang-dsp/ugen_manifests").glob("*.json"))
    vscode_ugens = sorted((ROOT / "vscode-extension/ugen_manifests").glob("*.json"))
    lsp_ugens = sorted((ROOT / "crates/vibelang-lsp/src/data/ugen_manifests").glob("*.json"))
    return {
        "rhai_rows": len(rhai),
        "lsp_rhai_rows": len(lsp),
        "rhai_projections_equal": rhai == lsp,
        "stdlib_rows": len(stdlib["synthdefs"]),
        "canonical_ugen_categories": len(canonical_ugens),
        "vscode_ugen_categories": len(vscode_ugens),
        "lsp_ugen_categories": len(lsp_ugens),
    }


def wasm_counts(text):
    return {
        "interfaces": len(re.findall(r"^export interface ", text, re.MULTILINE)),
        "classes": len(re.findall(r"^export class ", text, re.MULTILINE)),
        "class_methods": len(re.findall(r"^  (?:static )?[A-Za-z][A-Za-z0-9]*\(", text, re.MULTILINE)),
        "module_functions": len(re.findall(r"^(?:export default function|export function) ", text, re.MULTILINE)),
    }


def docs_counts(paths):
    texts = [(ROOT / path).read_text(encoding="utf-8") for path in paths]
    return {
        "lines": sum(text.count("\n") for text in texts),
        "vibe_fences": sum(len(re.findall(r"^```(?:rhai|vibe)$", text, re.MULTILINE)) for text in texts),
        "shell_fences": sum(len(re.findall(r"^```(?:bash|sh|shell)$", text, re.MULTILINE)) for text in texts),
    }


def package_counts(paths):
    cargo_packages = []
    node_packages = []
    for path in paths:
        text = (ROOT / path).read_text(encoding="utf-8")
        if path.endswith("Cargo.toml"):
            match = re.search(r"^name\s*=\s*\"([^\"]+)\"", text, re.MULTILINE)
            if match:
                cargo_packages.append(match.group(1))
        elif path.endswith("package.json"):
            node_packages.append(json.loads(text).get("name", "<unnamed>"))
    return {
        "cargo_packages": sorted(cargo_packages),
        "node_packages": sorted(node_packages),
        "lockfiles": sum(path.endswith(("Cargo.lock", "package-lock.json")) for path in paths),
    }


def relative_paths(directory, pattern="*"):
    return [str(path.relative_to(ROOT)) for path in directory.glob(pattern) if path.is_file()]


def validate_catalogs():
    authoring = read_json("api/baselines/authoring-families-v1.json")
    families = {entry["family"] for entry in authoring["families"]}
    if families != REQUIRED_AUTHORING_FAMILIES:
        raise ValueError(
            f"authoring family catalog drift: expected {sorted(REQUIRED_AUTHORING_FAMILIES)}, got {sorted(families)}"
        )
    for entry in authoring["families"]:
        for key in ("script", "snapshot"):
            if not (ROOT / entry[key]).is_file():
                raise ValueError(f"missing authoring {key}: {entry[key]}")

    negative = read_json("api/baselines/known-defects-v1.json")
    defects = {entry["id"] for entry in negative["defects"]}
    if defects != REQUIRED_NEGATIVE_DEFECTS:
        raise ValueError(
            f"negative fixture catalog drift: expected {sorted(REQUIRED_NEGATIVE_DEFECTS)}, got {sorted(defects)}"
        )
    for entry in negative["defects"]:
        fixture_path = ROOT / entry["fixture"]
        if not fixture_path.is_file():
            raise ValueError(f"missing negative fixture: {entry['fixture']}")
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        for assertion in fixture.get("source_assertions", []):
            source = (ROOT / assertion["path"]).read_text(encoding="utf-8")
            if assertion["contains"] not in source:
                raise ValueError(
                    f"known defect {entry['id']} no longer matches {assertion['path']}: {assertion['contains']!r}"
                )

    validate_failure_seam_catalog(
        read_json("api/baselines/failure-injection-seams-v1.json")
    )


def build_snapshot():
    validate_catalogs()
    manifest = read_json("api/public-api-manifest-v1.json")
    http = read_json("api/http-api-snapshot-v1.json")

    editor_paths = [
        "vscode-extension/src/data/rhai-api.json",
        "crates/vibelang-lsp/src/data/rhai-api.json",
        "vscode-extension/src/data/stdlib.json",
    ]
    editor_paths += relative_paths(ROOT / "vscode-extension/ugen_manifests", "*.json")
    editor_paths += relative_paths(ROOT / "crates/vibelang-lsp/src/data/ugen_manifests", "*.json")

    doc_paths = [
        "api/README.md",
        "docs/reference/README.md",
        "docs/reference/runtime-model.md",
        "docs/reference/runtime-objects.md",
        "docs/interfaces/cli-and-config.md",
        "docs/interfaces/http-and-websocket.md",
        "docs/interfaces/lsp-and-editors.md",
        "docs/interfaces/wasm.md",
        "docs/reference/generated/stdlib.md",
        "docs/reference/generated/ugens.md",
        "docs/reference/generated/http-routes.md",
    ]

    package_paths = []
    for pattern in ("Cargo.toml", "Cargo.lock", "package.json", "package-lock.json"):
        for path in ROOT.rglob(pattern):
            relative = path.relative_to(ROOT)
            if "target" not in relative.parts and "node_modules" not in relative.parts:
                package_paths.append(str(relative))
    package_paths = sorted(set(package_paths))

    fixture_paths = relative_paths(ROOT / "api/baselines", "*.json")
    fixture_paths = [path for path in fixture_paths if path != "api/baselines/public-artifacts-v1.json"]
    fixture_paths += relative_paths(ROOT / "tests/fixtures/api-unification/v1", "**/*")

    cli_text = (ROOT / "docs/reference/generated/cli-help.txt").read_text(encoding="utf-8")
    wasm_text = (ROOT / "crates/vibelang-wasm/types/index.d.ts").read_text(encoding="utf-8")
    return {
        "schema": "https://vibelang.org/schemas/v1-public-artifact-baseline/1",
        "schema_version": 1,
        "accepted_base": ACCEPTED_BASE,
        "policy": "observed-v1-behavior-not-endorsement",
        "categories": {
            "manifest": category(
                ["api/public-api-manifest-v1.json"], manifest_counts(manifest)
            ),
            "http": category(
                ["api/http-api-snapshot-v1.json"], http_counts(http)
            ),
            "rhai_editor": category(editor_paths, editor_counts()),
            "wasm": category(
                ["crates/vibelang-wasm/types/index.d.ts"], wasm_counts(wasm_text)
            ),
            "cli": category(
                ["docs/reference/generated/cli-help.txt"],
                {
                    "help_snapshots": len(re.findall(r"^\$ vibe", cli_text, re.MULTILINE)),
                    "top_level_commands": len(
                        re.findall(r"^  (?:run|render|devices|lsp|help)\s", cli_text, re.MULTILINE)
                    ),
                },
            ),
            "docs": category(doc_paths, docs_counts(doc_paths)),
            "packages": category(package_paths, package_counts(package_paths)),
            "fixtures": category(
                fixture_paths,
                {
                    "authoring_families": len(REQUIRED_AUTHORING_FAMILIES),
                    "negative_defects": len(REQUIRED_NEGATIVE_DEFECTS),
                    "failure_phases": len(REQUIRED_FAILURE_PHASES),
                },
            ),
        },
    }


def render(snapshot):
    return json.dumps(snapshot, indent=2, sort_keys=True) + "\n"


def check(expected_text, actual_text):
    if expected_text == actual_text:
        return None
    return "".join(
        difflib.unified_diff(
            expected_text.splitlines(keepends=True),
            actual_text.splitlines(keepends=True),
            fromfile=str(SNAPSHOT_PATH.relative_to(ROOT)),
            tofile="regenerated-v1-baseline",
        )
    )


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in {"generate", "check", "test-drift"}:
        print("usage: scripts/v1-baselines.py <generate|check|test-drift>", file=sys.stderr)
        return 2

    mode = sys.argv[1]
    failure_seams = build_failure_seam_catalog()
    validate_failure_seam_catalog(failure_seams)
    failure_seam_text = render_failure_seam_catalog(failure_seams)
    if mode == "generate":
        FAILURE_SEAM_PATH.parent.mkdir(parents=True, exist_ok=True)
        FAILURE_SEAM_PATH.write_text(failure_seam_text, encoding="utf-8")
        print(f"generated {FAILURE_SEAM_PATH.relative_to(ROOT)}")
    else:
        committed_failure_seams = FAILURE_SEAM_PATH.read_text(encoding="utf-8")
        failure_seam_diff = check(committed_failure_seams, failure_seam_text)
        if failure_seam_diff is not None:
            print(failure_seam_diff, file=sys.stderr)
            print(
                "failure seam artifact drifted from executable M04 truth",
                file=sys.stderr,
            )
            return 1

    actual = render(build_snapshot())
    if mode == "generate":
        SNAPSHOT_PATH.parent.mkdir(parents=True, exist_ok=True)
        SNAPSHOT_PATH.write_text(actual, encoding="utf-8")
        print(f"generated {SNAPSHOT_PATH.relative_to(ROOT)}")
        return 0

    if mode == "check":
        expected = SNAPSHOT_PATH.read_text(encoding="utf-8")
        diff = check(expected, actual)
        if diff is not None:
            print(diff, file=sys.stderr)
            print("v1 public-artifact baseline drifted; classify the change before regenerating", file=sys.stderr)
            return 1
        print("v1 public-artifact baseline is current")
        return 0

    mutated = json.loads(actual)
    mutated["categories"]["manifest"]["files"][0]["bytes"] += 1
    if check(actual, render(mutated)) is None:
        print("drift self-test failed to detect a deterministic byte change", file=sys.stderr)
        return 1
    test_failure_seam_drift_fixtures()
    print("v1 baseline drift self-test detected a deterministic byte change")
    print("failure seam negative drift fixtures rejected artifact and source drift")
    return 0


if __name__ == "__main__":
    sys.exit(main())
