#!/usr/bin/env python3
"""One-shot authoring aid for api/contract/http.toml (M11 v2 surface).

Computes the stable FNV-1a ids the fragment schema requires and emits the
sorted record list. The output is committed and hand-reviewed; this script
is kept for provenance and regeneration during review.
"""

import json

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
MASK = 0xFFFFFFFFFFFFFFFF


def stable_id(namespace: str, key: str) -> str:
    h = FNV_OFFSET
    for b in key.encode():
        h = ((h ^ b) * FNV_PRIME) & MASK
    return f"v1:{namespace}:{h:016x}"


def op_id(method: str, path: str) -> str:
    return stable_id("operation", f"http|{method}|{path}")


def field_id(source: str, type_name: str, field_name: str) -> str:
    return stable_id("field", f"http|{source}|{type_name}|{field_name}")


MODELS = "crates/vibelang-http/src/models.rs"
LEDGER_REGISTER_EFFECT = "v1:effect:287f24a9d934f2e1"

snap = json.load(open("api/http-api-snapshot-v1.json"))
ROUTES = [(r["method"], r["path"]) for r in snap["routes"]]
ALL_OPS = sorted({op_id(m, p) for m, p in ROUTES})
OP_NAMES = {op_id(m, p): f"{m} {p}" for m, p in ROUTES}

GET_ROUTES = [(m, p) for m, p in ROUTES if m == "GET"]
# project_v2_response: control reads carry freshness headers only, /v2/ws is
# upgrade-only, every other successful v2 GET body is wrapped in Revisioned.
CONTROL_READS = {
    "/capabilities",
    "/capabilities/details",
    "/mutation-status",
    "/receipt-events",
    "/receipts/{attempt_id}",
}
REVISIONED_OPS = sorted(
    op_id(m, p) for m, p in GET_ROUTES if p not in CONTROL_READS and p != "/ws"
)

PARAM_SET_OPS = sorted(
    op_id("PUT", p)
    for p in (
        "/groups/{id}/params/{param}",
        "/voices/{id}/params/{param}",
        "/patterns/{id}/params/{param}",
        "/effects/{id}/params/{param}",
    )
)
LOOP_CONTROL_OPS = sorted(
    op_id("POST", p)
    for p in (
        "/patterns/{id}/start",
        "/patterns/{id}/stop",
        "/melodies/{id}/start",
        "/melodies/{id}/stop",
    )
)

E = "effective"
R = "structured_rejection"


def eff(errors=(), note=None):
    return {
        "status": E,
        "effect_ids": [LEDGER_REGISTER_EFFECT],
        "error_ids": sorted(f"http.error.{c}" for c in errors),
        "observable_at": "applied",
        "note": note,
    }


def rej(errors=("unsupported_field",), note=None):
    return {
        "status": R,
        "effect_ids": [],
        "error_ids": sorted(f"http.error.{c}" for c in errors),
        "observable_at": "response_only",
        "note": note,
    }


# type -> (operation set, {field: effectiveness or None for response members})
TYPES = {
    "V2TransportUpdate": (
        [op_id("PATCH", "/transport")],
        {
            "bpm": eff(["invalid_value", "unsupported_combination"]),
            "time_signature": eff(["invalid_value", "unsupported_combination"]),
            "quantization_beats": rej(
                note="quantized transport mutation has no scheduler contract"
            ),
        },
    ),
    "V2TimeSignature": (
        [op_id("PATCH", "/transport")],
        {
            "numerator": eff(["invalid_value"]),
            "denominator": eff(["invalid_value"]),
        },
    ),
    "V2SeekRequest": (
        [op_id("POST", "/transport/seek")],
        {"beat": eff(["invalid_value"])},
    ),
    "V2GroupUpdate": (
        [op_id("PATCH", "/groups/{id}")],
        {"params": eff(["no_effect", "unsupported_combination"])},
    ),
    "V2ParamSet": (
        PARAM_SET_OPS,
        {
            "value": eff(["invalid_value"]),
            "fade_beats": eff(["invalid_value"]),
        },
    ),
    "V2VoiceCreate": (
        [op_id("POST", "/voices")],
        {
            "name": eff(["invalid_value"]),
            "synth_name": eff(["invalid_value"]),
            "polyphony": eff(["invalid_value"]),
            "gain": eff(
                ["invalid_value", "representation_conflict"],
                note="normalized into params.amp; conflicting explicit params.amp is rejected",
            ),
            "group_path": eff(["group_not_found"]),
            "params": eff(["invalid_value"]),
            "sample": rej(note="voice-source creation is not available through HTTP v2"),
            "sfz": rej(note="voice-source creation is not available through HTTP v2"),
        },
    ),
    "V2VoiceUpdate": (
        [op_id("PATCH", "/voices/{id}")],
        {
            "synth_name": rej(
                note="live synth replacement requires explicit node-recreation semantics"
            ),
            "polyphony": rej(
                note="live polyphony replacement requires explicit node-recreation semantics"
            ),
            "gain": eff(["invalid_value", "representation_conflict"]),
            "params": eff(["no_effect", "unsupported_combination"]),
        },
    ),
    "V2TriggerRequest": (
        [op_id("POST", "/voices/{id}/trigger")],
        {"params": eff(["invalid_value"])},
    ),
    "V2NoteOnRequest": (
        [op_id("POST", "/voices/{id}/note-on")],
        {
            "note": eff(["invalid_value"]),
            "velocity": eff(["invalid_value"]),
        },
    ),
    "V2NoteOffRequest": (
        [op_id("POST", "/voices/{id}/note-off")],
        {"note": eff(["invalid_value"])},
    ),
    "V2PatternCreate": (
        [op_id("POST", "/patterns")],
        {
            "name": eff(["invalid_value"]),
            "voice_name": eff(["invalid_value"]),
            "loop_beats": eff(["invalid_value"]),
            "events": eff(["invalid_value", "representation_conflict"]),
            "pattern_string": eff(["invalid_value", "representation_conflict"]),
            "params": eff(["invalid_value"]),
            "swing": eff(
                ["invalid_value", "unsupported_field"],
                note="consumed only via pattern_string (baked into event beats, stored swing zeroed); structured-rejected when supplied next to explicit events (gate F3)",
            ),
        },
    ),
    "V2PatternEvent": (
        sorted([op_id("POST", "/patterns"), op_id("PATCH", "/patterns/{id}")]),
        {
            "beat": eff(
                ["invalid_value"],
                note="effective via POST /patterns; the PATCH events carrier field is itself structured-rejected",
            ),
            "params": eff(["invalid_value"]),
        },
    ),
    "V2PatternUpdate": (
        [op_id("PATCH", "/patterns/{id}")],
        {
            "events": rej(
                note="HTTP content replacement is not yet correlated to a musical swap boundary"
            ),
            "pattern_string": rej(
                note="HTTP content replacement is not yet correlated to a musical swap boundary"
            ),
            "loop_beats": rej(
                note="HTTP content replacement is not yet correlated to a musical swap boundary"
            ),
            "params": eff(["no_effect", "unsupported_combination"]),
        },
    ),
    "V2LoopControlRequest": (
        LOOP_CONTROL_OPS,
        {
            "quantize_beats": rej(
                note="loop scheduling does not expose an HTTP quantization boundary"
            )
        },
    ),
    "V2MelodyCreate": (
        [op_id("POST", "/melodies")],
        {
            "name": eff(["invalid_value"]),
            "voice_name": eff(["invalid_value"]),
            "loop_beats": eff(["invalid_value"]),
            "events": eff(["invalid_value", "representation_conflict"]),
            "melody_string": eff(
                ["invalid_value", "parse_error", "representation_conflict"]
            ),
            "params": rej(
                note="top-level Melody parameter precedence is not defined; use events[].params"
            ),
        },
    ),
    "V2MelodyEvent": (
        sorted([op_id("POST", "/melodies"), op_id("PATCH", "/melodies/{id}")]),
        {
            "beat": eff(
                ["invalid_value"],
                note="effective via POST /melodies; the PATCH events carrier field is itself structured-rejected",
            ),
            "note": eff(["parse_error"]),
            "frequency": rej(note="note/frequency precedence is not defined"),
            "duration": eff(["invalid_value"]),
            "velocity": eff(["invalid_value"]),
            "params": eff(["invalid_value"]),
        },
    ),
    "V2MelodyUpdate": (
        [op_id("PATCH", "/melodies/{id}")],
        {
            "events": rej(
                note="HTTP Melody replacement is not yet correlated to a musical swap boundary"
            ),
            "melody_string": rej(
                note="HTTP Melody replacement is not yet correlated to a musical swap boundary"
            ),
            "lanes": rej(
                note="HTTP Melody replacement is not yet correlated to a musical swap boundary"
            ),
            "loop_beats": rej(
                note="HTTP Melody replacement is not yet correlated to a musical swap boundary"
            ),
            "params": rej(
                note="HTTP Melody replacement is not yet correlated to a musical swap boundary"
            ),
        },
    ),
    "V2SequenceClip": (
        sorted([op_id("POST", "/sequences"), op_id("PATCH", "/sequences/{id}")]),
        {
            "clip_type": eff(
                ["unsupported_field", "unsupported_value"],
                note="effective via POST /sequences; the PATCH clips carrier field is itself structured-rejected",
            ),
            "name": eff(["dependency_ambiguous", "dependency_not_found"]),
            "start_beat": eff(["invalid_value"]),
            "end_beat": eff(["invalid_value", "representation_conflict"]),
            "duration_beats": eff(["invalid_value", "representation_conflict"]),
            "once": rej(note="per-clip one-shot semantics are not implemented"),
        },
    ),
    "V2SequenceCreate": (
        [op_id("POST", "/sequences")],
        {
            "name": eff(["invalid_value"]),
            "loop_beats": eff(["invalid_value"]),
            "clips": eff(["invalid_value"]),
        },
    ),
    "V2SequenceUpdate": (
        [op_id("PATCH", "/sequences/{id}")],
        {
            "loop_beats": rej(
                note="HTTP Sequence replacement is not yet available as one atomic runtime command"
            ),
            "clips": rej(
                note="HTTP Sequence replacement is not yet available as one atomic runtime command"
            ),
        },
    ),
    "V2SequenceStartRequest": (
        [op_id("POST", "/sequences/{id}/start")],
        {"play_once": eff()},
    ),
    "V2EffectUpdate": (
        [op_id("PATCH", "/effects/{id}")],
        {"params": eff(["no_effect", "unsupported_combination"])},
    ),
    "V2SampleLoad": (
        [op_id("POST", "/samples")],
        {
            "path": eff(
                ["capability_unavailable", "invalid_value"],
                note="remote security modes reject filesystem loading with a typed 403 (gate F1)",
            ),
            "id": eff(["invalid_value"]),
        },
    ),
    "V2FadeCreate": (
        [op_id("POST", "/fades")],
        {
            "target_type": eff(["fade_target_not_found"]),
            "target_name": eff(["fade_target_not_found"]),
            "param_name": eff(["invalid_value"]),
            "start_value": eff(["invalid_value"]),
            "target_value": eff(["invalid_value"]),
            "duration_beats": eff(["invalid_value"]),
        },
    ),
    # Typed v2 response envelopes: applicability only. Response-member
    # effectiveness semantics stay recorded as compatibility debt until a
    # dedicated response-contract milestone declares them.
    "Revisioned": (
        REVISIONED_OPS,
        {
            "schema_version": None,
            "runtime_epoch": None,
            "event_sequence": None,
            "last_confirmed_revision": None,
            "data": None,
        },
    ),
    "HttpErrorEnvelope": (
        ALL_OPS,
        {
            "schema_version": None,
            "operation": None,
            "error": None,
            "receipt": None,
        },
    ),
    "HttpErrorDetail": (
        ALL_OPS,
        {
            "code": None,
            "message": None,
            "field": None,
            "reason": None,
            "supported_values": None,
        },
    ),
    "HttpCapabilities": (
        [op_id("GET", "/capabilities")],
        {
            "schema_version": None,
            "runtime_epoch": None,
            "mutation": None,
            "security": None,
        },
    ),
    "HttpSecurityCapabilities": (
        [op_id("GET", "/capabilities")],
        {
            "mode_id": None,
            "degraded": None,
            "reason_ids": None,
            "authentication_required": None,
            "origin_allowlist_required": None,
            "request_limits_enabled": None,
            "rate_limits_enabled": None,
            "audit_enabled": None,
            "eval_enabled": None,
            "privileged_detail_enabled": None,
        },
    ),
    "HttpCapabilityDetails": (
        [op_id("GET", "/capabilities/details")],
        {
            "schema_version": None,
            "runtime_epoch": None,
            "security": None,
        },
    ),
    "HttpSecurityPolicyDetails": (
        [op_id("GET", "/capabilities/details")],
        {
            "mode_id": None,
            "max_body_bytes": None,
            "rate_limit_per_minute": None,
            "max_wait_ms": None,
            "eval_enabled": None,
            "audit_enabled": None,
        },
    ),
}

# Cross-check field coverage against the snapshot.
snap_types = {t["name"]: t for t in snap["types"]}
declared_fields = 0
for type_name, (_, fields) in TYPES.items():
    snap_fields = [f["name"] for f in snap_types[type_name]["fields"]]
    assert sorted(snap_fields) == sorted(fields), (type_name, snap_fields, fields)
    declared_fields += len(fields)
assert declared_fields == 114, declared_fields

records = []

# Frozen M02 record: the ignored v1 TransportUpdate.quantization_beats field.
V1_QUANTIZATION = {
    "comment": "TransportUpdate.quantization_beats (v1) — frozen M02 ignored-field debt (PATCH /transport)",
    "target_id": field_id(MODELS, "TransportUpdate", "quantization_beats"),
    "body": """owner = "vibelang-http"
operation_id = "{op}"
consistency = "response_snapshot"

[records.effectiveness]
status = "compatibility_debt"
effect_ids = []
error_ids = []
observable_at = "response_only"

[records.effectiveness.migration]
owner = "vibelang-http"
issue = "M11 HTTP v2 effectiveness binding"
remove_by = "v2 release-ready gate"
diagnostic_id = "compat.http.ignored_quantization_beats"
""".format(op=op_id("PATCH", "/transport")),
}
records.append(V1_QUANTIZATION)

for type_name, (ops, fields) in TYPES.items():
    for f_name, semantics in fields.items():
        ops_toml = "operation_ids = [\n" + "".join(
            f'    "{o}", # {OP_NAMES[o]}\n' for o in ops
        ) + "]"
        if len(ops) == 1:
            ops_toml = f'operation_ids = ["{ops[0]}"] # {OP_NAMES[ops[0]]}'
        body = f'owner = "vibelang-http"\n{ops_toml}\n'
        comment = f"{type_name}.{f_name}"
        if semantics is not None:
            if semantics["note"]:
                comment += f" — {semantics['note']}"
            effect_ids = "".join(
                f'    "{e}",\n' for e in semantics["effect_ids"]
            )
            error_ids = "".join(f'    "{e}",\n' for e in semantics["error_ids"])
            body += (
                "\n[records.effectiveness]\n"
                f'status = "{semantics["status"]}"\n'
                f"effect_ids = [{('\n' + effect_ids) if effect_ids else ''}]\n"
                f"error_ids = [{('\n' + error_ids) if error_ids else ''}]\n"
                f'observable_at = "{semantics["observable_at"]}"\n'
            )
        else:
            comment += " — typed v2 response member (applicability only)"
        records.append(
            {
                "comment": comment,
                "target_id": field_id(MODELS, type_name, f_name),
                "body": body,
            }
        )

records.sort(key=lambda r: r["target_id"])
assert len({r["target_id"] for r in records}) == len(records)

HEADER = """fragment_schema = "https://vibelang.org/schemas/public-api-semantic-fragment/v1"
fragment_version = 1
domain = "http"

# M11 HTTP v2 operation applicability and effectiveness.
#
# The strict /v2 projection decodes its operation-scoped DTOs inside the
# middleware (validate_and_normalize_v2_request / project_v2_response), so no
# route handler signature references them and the mechanical route graph
# cannot bind them. Each record below declares, per field, the exact
# operations that bind the field's type, and for request DTO fields the
# gate-verified effectiveness semantics (effective vs structured rejection,
# with the typed problem codes the middleware can emit for that field).
# Effect id v1:effect:287f24a9d934f2e1 is the canonical mutation-ledger
# register effect (one attempt receipt + ordered transition event).
# Records are strictly sorted by target_id (schema requirement).
"""

with open("api/contract/http.toml", "w") as out:
    out.write(HEADER)
    for record in records:
        out.write(f"\n# {record['comment']}\n")
        out.write("[[records]]\n")
        out.write(f'target_id = "{record["target_id"]}"\n')
        out.write(record["body"])

print(f"wrote {len(records)} records")
