# Effective contract schema, generation ownership, and compatibility policy

| Field | Value |
|---|---|
| Status | Proposed research contract for architecture convergence |
| Assessed tree | `e5a1198a3bb478418042f2b517172f74635742b7` |
| API candidate contained by that tree | `f00c04ca1a1e79d644211eed64fc472214a75d58` |
| Current API version | `0.4.0` |
| Scope | Manifest, generators, HTTP/WebSocket, WASM, CLI, LSP, VS Code, Emacs, documentation, coverage, compatibility, and packaging |
| Implementation status | Design only; this document changes no product behavior or generated artifact |

This proposal builds on the source-backed [API surface assessment](../../api-surface-assessment.md). It chooses one machine-readable effective contract and makes every public projection a deterministic view of that contract. The contract describes what an input or operation does, when it does it, how success and failure are observed, where it is available, and which consumers expose it. A declaration that merely deserializes or registers is not sufficient evidence of effectiveness.

## Decision

VibeLang should introduce `api/public-api-manifest-v2.json` as the sole checked, generated effective contract. Its schema URI is `https://vibelang.org/schemas/public-api-manifest/v2`, and its Rust schema and validators remain owned by `crates/vibelang-api-manifest`. The current `api/public-api-manifest-v1.json` becomes a generated compatibility projection during migration; schema v1 is not extended in place.

The v2 artifact is composed from two kinds of source:

1. Mechanical declarations remain owned by the code or catalog that executes them: Rhai registrations, UGen catalogs, stdlib source, Clap declarations, Axum routes and Serde types, WASM exports, WebSocket payload types, and LSP rule/legend definitions.
2. Semantics that cannot be proved mechanically are declared in domain-owned files under `api/contract/*.toml`. These fragments may supply lifecycle, units/ranges, availability policy, effectiveness, failure/revision behavior, stability, and ownership. They may not restate a mechanical name, signature, route, serialized type, or package path.

`xtask` is the only composer and writer. It joins mechanical facts and explicit semantics by stable ID, validates total coverage, sorts and serializes deterministically, and renders all projections. Runtime code, editors, docs, and packages consume projections; none may become a second source of public truth.

This is intentionally a compiled contract rather than a hand-maintained master JSON file. It preserves the current manifest's strong extraction chain while adding explicit ownership where static inference cannot distinguish an implemented effect from an ignored field, a queue acceptance from an applied revision, or a real terminal from a success-shaped no-op.

### Normative invariants

- `schema_version` versions the JSON shape; `api_version` versions the user-facing API. They are independent.
- Schema v1 remains immutable. An unchanged v1 entry or overload retains its current `v1:entry:*` or `v1:overload:*` ID when represented in v2.
- Every public node has stability, availability, ownership, provenance, and an explicit applicability result for every behavioral facet. `unknown`, an absent facet, and an unclassified compatibility change fail the release gate.
- Every accepted input field is either effective or produces a structured rejection before success. `ignored` exists only as migration debt with an owner, issue, and removal deadline; it is forbidden for a release-ready public binding.
- Every mutation declares whether it allocates, observes, or does not use a revision. Queue acceptance is never represented as application.
- Every success state names its observable effect and consistency point. Logs, warnings, fixed sleeps, and console output are not success acknowledgements.
- Every consumer declares its inclusion policy and measurable coverage. Curated exclusion is allowed; silent drift is not.
- Generated files carry no timestamps, absolute paths, checkout-specific target paths, or unordered maps.

## Terms

| Term | Meaning |
|---|---|
| Declaration | A name, signature, route, field, method, event, flag, or package export that code accepts or exposes. |
| Effective contract | The declaration plus lifecycle, value semantics, availability, effect, failures, revision/consistency behavior, stability, owners, consumers, and evidence. |
| Mechanical source | Executing code or a canonical catalog from which a fact can be reproduced without judgment. |
| Semantic fragment | A typed, reviewed source record for a fact that cannot be safely inferred from syntax. |
| Projection | A generated subset or encoding of the v2 contract for a specific consumer. |
| Accepted field | A field that a decoder, parser, or binding permits for an operation, including optional fields. |
| Applied | The operation's declared commit/observation point has been reached, not merely parsed, evaluated, queued, or sent. |
| Consumer coverage | Included eligible contract nodes divided by eligible nodes, with every exclusion classified. |

## Current-state evidence

All counts below are from committed files at the assessed tree. They are baselines, not future compatibility promises.

| Surface | Count | Source |
|---|---:|---|
| Manifest | 3,626 entries; 8,431 overloads | `api/public-api-manifest-v1.json` `stats` |
| Effective Rhai metadata | 6,837 functions; 34 types | `PublicApiManifest.stats` |
| Core/DSP/extension Rhai | 786 entries; 875 overloads | Manifest surfaces `rhai`, `dsp_rhai`, `rhai_extension` |
| Rhai properties | 141 getters; 134 setters | Manifest kinds and lifecycle terminals |
| Public callable UGens | 1,174 entries; 5,962 overloads | Manifest surface `dsp_ugen` |
| UGen catalogs | 875 production records in 70 files, plus `_test_arity_stub.json` | `crates/vibelang-dsp/ugen_manifests` |
| Stdlib | 829 files; 890 declarations; 887 distinct definition names | Manifest `stats` |
| HTTP | 96 method/path registrations; 75 public Serde types; 297 fields | `api/http-api-snapshot-v1.json` |
| HTTP methods | 27 GET, 49 POST, 7 PATCH, 4 PUT, 9 DELETE | HTTP snapshot routes |
| HTTP feature gates | 18 MIDI routes; 4 native-recording routes | HTTP snapshot availability |
| LSP and VS Code Rhai rows | 600 each, byte-identical | Both `rhai-api.json` projections have SHA-256 `4a5a072c63af75f8e6662a8cef1c754624c60f8f4d9d8d03019656a87de07991` |
| Editor UGen bundle | 24 of 70 production categories; 478 runtime labels | Both projected UGen directories |
| Editor UGen resolution | 470 valid labels; 8 stale labels | Exact `functions` value to manifest-name comparison |
| VS Code contributions | 32 commands, 8 keybindings, 39 settings, 2 views | `vscode-extension/package.json` |
| Emacs surface | 31 user options, 56 interactive commands, 7 snippets | `emacs/vibelang-mode.el` and related modules |
| WASM callable classes | `VibelangRuntime` with 11 methods; legacy `VibelangEngine` with 7 | Generated `crates/vibelang-wasm/types/index.d.ts` |
| Generated reference | 103 CLI, 186 HTTP, 1,619 stdlib, 1,749 UGen lines | `docs/reference/generated` |

The current overload boundary scan reports 6,019 coercions, 6,119 casts, 121 clamps, 337 explicit ranges, 5,036 fallbacks, 190 structured-error paths, and 6,071 potential panic exposures. Those are useful static risk indicators, but they do not say whether a field changes state, whether a failure reaches the caller, or whether a successful mutation is current.

## Current source and generator map

### Schema and authoring manifest

| Exact path and symbol | Current ownership and behavior | Boundary that v2 must preserve or repair |
|---|---|---|
| [`crates/vibelang-api-manifest/src/lib.rs`](../../../crates/vibelang-api-manifest/src/lib.rs): `PublicApiManifest`, `ApiEntry`, `Overload`, `BoundarySemantics`, `Availability`, `Lifecycle`, `EntryDetails`, `stable_id`, `to_pretty_json` | Defines schema v1 and deterministic IDs/JSON. | Extend in a new schema version. Do not change the meaning of committed v1 JSON. |
| [`xtask/src/public_api.rs`](../../../xtask/src/public_api.rs): `generate`, `build_manifest` | Composes effective Rhai metadata, source registrations, UGens, and stdlib; writes the manifest and stdlib reference. | Retain as composer, but split discovery from semantic validation and v1 projection. |
| Same file: `SourceIndex::load`, `rhai_entries`, `rhai_type_entries` | Parses Rust registration sources and joins them to effective engine metadata. | Mechanical names/signatures/anchors remain code-owned. |
| Same file: `UgenCatalog::load`, `scan_stdlib`, `canonicalize_entries`, `validate_entries` | Loads canonical UGen JSON and `.vibe` stdlib declarations, sorts, and validates. | Preserve catalogs as mechanical sources; require semantic coverage by stable ID. |
| Same file: `classify_lifecycles` | Infers terminals from 12 spellings and whether all overloads return the receiver. | Replace heuristic truth with explicit lifecycle metadata. The heuristic may bootstrap migration only. |
| [`crates/vibelang-rhai/src/engine.rs`](../../../crates/vibelang-rhai/src/engine.rs): `ScriptEngine::public_api_metadata_json` | Constructs an engine, registers extensions, and calls Rhai metadata serialization. | Remains the effective-registration source for native/full-feature extraction. Feature matrix differences must be modeled, not flattened. |
| [`crates/vibelang-rhai/src/lib.rs`](../../../crates/vibelang-rhai/src/lib.rs): `public_api_metadata_json` | Feature-gated entry used by `xtask`. | Keep as extraction boundary, not a behavioral source. |

### UGen build-time generation

| Exact path and symbol | Current ownership and behavior | Boundary that v2 must preserve or repair |
|---|---|---|
| [`crates/vibelang-dsp/build_support.rs`](../../../crates/vibelang-dsp/build_support.rs): `UGenManifest`, `UGenInput`, `positional_arity_max`, `runtime_rate_*`, `to_snake_case` | Shared catalog shape and naming/arity rules. | This is the canonical mechanical rule set for both build output and contract extraction. |
| [`crates/vibelang-dsp/build.rs`](../../../crates/vibelang-dsp/build.rs): `load_and_validate`, `validate`, `main` | Reads catalogs, validates 875 production records, emits `OUT_DIR/generated.rs`, and generates `register_generated_ugens`. | Build output remains ephemeral. The effective contract records the generated callable names and failure/coercion semantics, not the generated Rust file. |
| [`crates/vibelang-dsp/src/lib.rs`](../../../crates/vibelang-dsp/src/lib.rs): module `ugens`, `register_dsp_api` | Includes `OUT_DIR/generated.rs` and calls `register_generated_ugens`. | Runtime registration is the execution proof. |
| [`xtask/src/public_api.rs`](../../../xtask/src/public_api.rs): module `dsp_build_support`, `GeneratedUgen`, `metadata_matches_generated_ugen` | Reuses build support rather than parsing generated Rust. | Keep one shared naming/arity implementation. Never duplicate `to_snake_case` again in a consumer validator. |

The canonical UGen catalogs currently contain optional `functions` labels that build generation ignores. Editor completion consumes those labels directly, while `validate_dynamic_editor_ugens` re-derives a different name from class and rate. V2 should remove `functions` from the canonical input or treat it only as generated output; the exact consumed label must be validated.

### Artifact composer and gates

[`xtask/src/public_artifacts.rs`](../../../xtask/src/public_artifacts.rs) `generate` currently reads the v1 manifest, runs validations, and writes eight direct artifacts:

1. `docs/reference/generated/cli-help.txt`
2. `docs/reference/generated/ugens.md`
3. `crates/vibelang-wasm/types/index.d.ts`
4. `vscode-extension/src/data/rhai-api.json`
5. `crates/vibelang-lsp/src/data/rhai-api.json`
6. `vscode-extension/src/data/stdlib.json`
7. `api/http-api-snapshot-v1.json`
8. `docs/reference/generated/http-routes.md`

It also copies the 24 curated UGen category files into both editor projection directories through `sync_ugen_projections`. Important current symbols are:

| Symbol | What it checks | Current limit |
|---|---|---|
| `validate_manifest_availability` | Known availability status and nonempty conditional/quarantine reasons | Conditions are strings; no capability identity or unavailable behavior. |
| `render_editor_rhai` | 600 manifest-derived function overload rows | Omits all 275 properties. |
| `render_wasm_types`, `wasm_exports` | Rust annotated exports to a checked declaration file | Describes signatures, not host requirements or truthful delivery. |
| `build_http_snapshot`, `extract_routes`, `extract_http_types` | Axum registrations, handler name existence, and public Serde shapes | Does not bind request/response types to routes or inspect handler effects/status/revision behavior. |
| `validate_example_imports` | Imports and a bounded fictional-call denylist in 60 example files | Does not parse public Markdown code blocks. |
| `validate_editor_consumers` | Bounded stale calls plus active source consumers | Does not validate all settings, HTTP fields, labels, diagnostics, or package paths. |
| `validate_vscode_emitter_contracts` | Active TS vs packaged JS parity and manifest-backed emitter calls | Strong gate, but limited to selected emitter paths. |
| `validate_dynamic_editor_ugens` | Re-derived class/rate name and parameter coercions | Does not inspect the `functions` string that completion inserts. |

[`scripts/public-artifacts.sh`](../../../scripts/public-artifacts.sh) is the current supported front door. It serializes Cargo, builds the `vibe` CLI, normalizes help under `COLUMNS=100`, `LC_ALL=C`, and `NO_COLOR=1`, runs `xtask`, compiles VS Code into a temporary directory in check mode, compares source/package JavaScript, verifies `package.json.main`, and type-checks WASM declarations. [`.github/workflows/ci.yml`](../../../.github/workflows/ci.yml) runs `scripts/public-artifacts.sh check` before workspace tests.

### HTTP handler versus DTO truth

The router is constructed inside [`crates/vibelang-http/src/lib.rs`](../../../crates/vibelang-http/src/lib.rs) `start_server`; there is no `build_router` symbol even though `docs/interfaces/http-and-websocket.md` labels its source link that way. DTOs are mainly in [`models.rs`](../../../crates/vibelang-http/src/models.rs), with eval, fade, and MIDI types beside their handlers.

| DTOs and handlers | Effective current truth | Required v2 record |
|---|---|---|
| `TransportUpdate`; `routes::transport::update_transport` | `bpm` and `time_signature` enqueue messages; `quantization_beats` is accepted and ignored. Returned state can precede application. | Field effectiveness, validation, operation ID, revision policy, consistency point, typed errors. |
| `ParamSet`; `set_group_param`, `set_voice_param`, `set_pattern_param`, `set_effect_param` | `value` is sent; `fade_beats` is ignored by every binding. | One field contract with four operation bindings; no success for ignored fade. |
| `VoiceCreate`; `routes::voices::create_voice` | Identity, synth, group, polyphony, and params contribute to config; `gain`, `sample`, and `sfz` are ignored. A 10 ms sleep precedes lookup. | Per-field effect and an accepted/applied receipt rather than a sleep. |
| `VoiceUpdate`; `routes::voices::update_voice` | Only `params` are applied; `synth_name`, `polyphony`, and `gain` are ignored. | Reject or implement each accepted field. |
| `PatternCreate`, `PatternUpdate`, `StartRequest`, `StopRequest`; pattern handlers | Create uses events/default params/length/swing but ignores `pattern_string`. Update applies only params. Start/stop ignore quantization. | Field-to-effect map and explicit unsupported errors. |
| `MelodyCreate`, `MelodyUpdate`, `MelodyEvent`; melody handlers | Create ignores `melody_string`, top-level params, event frequency, and event params; update ignores all five fields; start/stop ignore quantization. Invalid notes fall back to 60. | Effectiveness plus parser/fallback policy and diagnostics. |
| `SequenceCreate`, `SequenceUpdate`, `SequenceClip`; sequence handlers | Update ignores both fields. Create drops unsupported clip kinds, requires numeric clip names, defaults duration, and ignores `once`. `SequenceStartRequest.play_once` is effective. | Typed clip variants, rejection/fallback policy, and per-field effects. |
| `SampleLoad`; `routes::samples::load_sample` | Sends an unchecked path, sleeps 50 ms, then reads state. | Revisioned load receipt and typed load failure. |
| `SynthDefInfo`; synthdef read handlers | `params` are always empty and `source` is always absent because state does not track them. | Mark unavailable fields or derive them from an owning source; do not imply populated data. |
| `StopRecordingRequest`; `routes::midi::stop_recording` | `quantize` is accepted but ignored; no take is returned. | Reject/implement quantization and declare result ownership. |
| `GroupCreate`, `EffectCreate` | Public deserializable DTOs exist without matching create routes. | Either bind them to an operation or mark them non-public/internal. |

There are at least 27 distinct accepted HTTP field paths explicitly identified above as ignored in one or more current operations. The generator must produce the exact field-to-operation count rather than preserve this research count manually.

### WebSocket truth

[`crates/vibelang-http/src/websocket.rs`](../../../crates/vibelang-http/src/websocket.rs) owns `WebSocketEvent`, `SubscriptionMessage`, `WS_PROTOCOL_VERSION`, `handle_socket`, `make_hello_event`, `build_playback_snapshot`, and `run_event_broadcaster`. `WebSocketEvent.data` is an optional untyped `serde_json::Value`; payload builders create ad hoc JSON. The broadcaster polls every 50 ms and uses a 1,024-entry broadcast channel. `handle_socket` breaks on any receive error, including lag, without a gap event or resync cursor. Invalid client messages are ignored.

V2 must model each client action and event as a typed operation/event node. Every event carries protocol version, event ID, monotonic observation sequence, relevant applied revision, typed payload, and resync behavior. Poll-derived playback remains classified as telemetry rather than a mutation acknowledgement.

### WASM types and package ownership

| Exact path and symbol | Current truth | Required boundary |
|---|---|---|
| [`crates/vibelang-wasm/src/lib.rs`](../../../crates/vibelang-wasm/src/lib.rs): `ExecutionResult`, `VibelangRuntime`, `VibelangEngine` | Source of annotated Rust exports and two overlapping public classes. | Mechanical export truth plus explicit stability, host, lifecycle, effect, and failure metadata. |
| Same file: `VibelangRuntime::execute` | Builds `success: true` after evaluation, warns on synthdef/effect bridge failures and reload send failure, then returns the earlier success. | Success must include a truthful revision/result or return a typed delivery failure. |
| Same file: `VibelangRuntime::start`, `stop` | Before initialization, `start` warns and returns `Ok(())`; `stop` silently returns `Ok(())`. | Structured `not_initialized`, never success. |
| Same file: `load_synthdef_to_supersonic` | Probes `globalThis.vibelangBridge` but invokes `window.vibelangBridge`; bridge absence still returns `Ok(())`. | One declared host global and typed absence/invocation failure. |
| [`xtask/src/public_artifacts.rs`](../../../xtask/src/public_artifacts.rs): `render_wasm_types`, `wasm_exports`, `wasm_value_interfaces` | Generates the checked callable declaration from Rust AST. | Generate types from v2 bindings while checking them against Rust exports. |
| [`crates/vibelang-wasm/package.json`](../../../crates/vibelang-wasm/package.json) | Package `vibelang-wasm` v0.4.0; ships `pkg/*.wasm`, `pkg/*.js`, and `types/index.d.ts`. | Chosen canonical package owner. |
| [`landing-page/package.json`](../../../landing-page/package.json): `build:wasm`, dependency alias | Builds the crate to `landing-page/src/wasm` and imports the alias. | Generated application build output, not a source package. |
| [`landing-page/src/audio/package.json`](../../../landing-page/src/audio/package.json) and local `vibelang_wasm*` files | A second package named `vibelang-wasm` v0.1.0 with historical glue/types; not the active loader import. | Remove from public ownership or classify explicitly as archived fixture. |

The canonical distribution is the crate package. The landing page may consume its build output but must not define a competing package/version/type owner.

### LSP, VS Code, and Emacs consumers

| Consumer source | Current consumption | Gap v2 must close |
|---|---|---|
| [`crates/vibelang-lsp/src/data/mod.rs`](../../../crates/vibelang-lsp/src/data/mod.rs): `get_api_rows`, `get_api_docs`, `get_api_method_docs` | `include_str!` consumes the 600-row Rhai function projection. | Add generated property and availability/lifecycle data; expose coverage. |
| [`crates/vibelang-lsp/src/data/ugen_cache.rs`](../../../crates/vibelang-lsp/src/data/ugen_cache.rs): `EMBEDDED_MANIFESTS` | Embeds the 24 curated UGen categories. | Generate bundle index and exclusions from consumer policy. |
| [`crates/vibelang-lsp/src/features/semantic_tokens.rs`](../../../crates/vibelang-lsp/src/features/semantic_tokens.rs): `TOKEN_TYPES`, `TOKEN_MODIFIERS`, `get_legend`, token index constants | Defines a 15-type, 6-modifier legend used by the emitter. | Be the sole legend source and generate advertised capabilities from it. |
| [`crates/vibelang-lsp/src/server.rs`](../../../crates/vibelang-lsp/src/server.rs): `LanguageServer::initialize` | Independently advertises a different 17-type, 3-modifier legend. | Delete the duplicate; diffing a reorder/removal is wire-breaking. |
| Same file: `analyze_and_publish`, `LanguageServer::diagnostic` | Push adds validation and unknown synthdef/effect checks; pull returns only analysis syntax/semantic/lint diagnostics. | One generated diagnostic-rule inventory and identical rule execution by both modes. |
| [`vscode-extension/src/utils/dataLoader.ts`](../../../vscode-extension/src/utils/dataLoader.ts): `DataLoader::loadUGens`, `loadRhaiApi`, `loadStdlib` | Loads workspace-first UGen catalogs, then bundled data; loads `src/data` projections retained by `.vscodeignore`. | Package and workspace modes must consume the same contract version/digest or report mismatch. |
| [`vscode-extension/src/features/completion.ts`](../../../vscode-extension/src/features/completion.ts): `VibelangCompletionItemProvider::provideCompletionItems` | Inserts each catalog's literal `functions` values. | Validate exact labels or generate them from contract bindings. |
| [`vscode-extension/package.json`](../../../vscode-extension/package.json), [`.vscodeignore`](../../../vscode-extension/.vscodeignore) | `main` is `./out/extension.js`; TS source is excluded but `src/data/**` is retained. | Pack archive gate must prove `main`, contract data, and bundle index are present. |
| [`emacs/vibelang-lsp.el`](../../../emacs/vibelang-lsp.el): `vibelang-lsp-server-command` | Defaults to `("vibelang" "lsp")`. | Generate/default to the actual `vibe lsp` command and report contract version. |
| [`emacs/vibelang-websocket.el`](../../../emacs/vibelang-websocket.el): `vibelang--handle-hello`, `vibelang--ws-on-message`, `vibelang-ws-request-resync`, HTTP helpers | Handwritten WebSocket/HTTP client with local assumptions and polling fallback. | Generate protocol constants/types where practical; compatibility fixtures must cover Emacs parsing. |

The eight exact invalid runtime labels are `a2_k_kr`, `k2_a_ar`, `lag2_ud_ar`, `lag2_ud_kr`, `lag3_ud_ar`, `lag3_ud_kr`, `t2_a_ar`, and `t2_k_kr`. Their registered names omit the extra acronym underscore: `a2k_kr`, `k2a_ar`, `lag2ud_*`, `lag3ud_*`, `t2a_ar`, and `t2k_kr`.

### Documentation and packaging defects that the contract must catch

- `GETTING_STARTED.md` uses nonexistent `get_group`, the wrong `.solo(true)` shape, and claims sequence `.clip()` registration beyond current pattern/nested-sequence synchronization.
- `crates/vibelang-std/stdlib/effects/README.md` uses nonexistent `group.add_effect`.
- `examples/tutorials/README.md` uses `vibelang run -w`; `crates/vibelang-keys/README.md` uses nonexistent `--midi-input`.
- `emacs/README.md`, `emacs/vibelang-lsp.el`, and `emacs/vibelang-mode.el` use `vibelang lsp` rather than `vibe lsp`.
- VS Code boot and Sound Designer text use stale `vibelang`, `--api`, or `-w` forms; `extension.ts` actually launches positional shorthand plus nonexistent `--api`.
- `docs/README.md` still describes the earlier 357-name inventory and future boundary work; `docs/roadmap/api-improvement-roadmap.md` does not reflect completed manifest/gate work.
- Root README claims 20 tutorials despite 23 tutorial programs and advertises an undefined approximately 1 ms hot-reload measurement.
- The current generated-reference/example/editor gates do not structurally classify all non-archival Markdown Vibe and CLI code blocks.
- VS Code checks `package.json.main` and compiled-tree parity but does not inspect an actual packed archive. WASM checks TypeScript declarations but not the final npm tarball or the landing-page alias against the canonical package digest.

## Effective contract v2 schema

The checked artifact has this root shape. Array order is normative and sorted by stable ID; maps are serialized in lexical key order.

```json
{
  "schema": "https://vibelang.org/schemas/public-api-manifest/v2",
  "schema_version": 2,
  "api_version": "0.4.0",
  "generator": {
    "name": "vibelang-xtask-effective-contract",
    "format_version": 1
  },
  "entries": [],
  "types": [],
  "operations": [],
  "events": [],
  "capabilities": [],
  "consumers": [],
  "coverage": {},
  "stats": {}
}
```

The artifact contains no generation time or Git commit. Those make byte output depend on checkout history rather than API source. CI records the assessed commit outside the artifact; provenance inside the contract is repository-relative source/test anchors.

### Identity and references

Every node has `id`, `name`, `stability`, `availability`, `ownership`, `source_anchors`, and `test_anchors`. References use IDs, never display names.

- Existing v1 entry and overload IDs are preserved exactly for unchanged canonical keys. The `v1` prefix is the stable-ID algorithm namespace, not a claim that the containing JSON uses schema v1.
- New IDs use the same documented FNV-1a algorithm with node-specific namespaces such as `operation`, `type`, `field`, `event`, `capability`, and `consumer`.
- Canonical keys include the surface and wire identity needed to distinguish overloads or bindings. They exclude line numbers, source ordering, absolute paths, generated descriptions, and current availability.
- A rename creates a new ID and an explicit alias/replacement edge. Silently reusing an ID for a different wire or callable identity is invalid.
- `source_anchors` contain repository-relative `path`, exact `symbol`, optional `line`, and `derivation` (`compiled_metadata`, `rust_ast`, `catalog`, `stdlib_parse`, `explicit_semantics`, or `generated_projection`).
- `test_anchors` use the same shape and name the exact test symbol or fixture that proves behavior. A release-ready effective mutation needs at least one behavioral test anchor.

### Entry and overload contract

`entries` preserve the current symbol inventory and add common behavioral facets. Each overload may refine any facet whose semantics differ by signature.

| Field | Required contract |
|---|---|
| `surface`, `kind`, `registered_name`, `aliases`, `receiver`, `overloads`, `details` | Existing mechanical declaration, retained from v1. |
| `stability` | Support class and evolution policy. |
| `availability` | Capability expression and unavailable behavior, not only raw `cfg` strings. |
| `lifecycle` | Value/builder/handle/reference classification, phase, terminal effect, synchronization, repeat/cancel rules. |
| `operation_ids` | Zero or more transport-independent operations implemented by this entry. |
| `value_contract` | Unit, range, non-finite policy, coercion/parser/fallback semantics where applicable. |
| `failure` | Structured error/fallback/panic contract at this boundary. |
| `ownership`, `consumer_ids` | Implementation/contract owners and projections that expose it. |

The current `BoundarySemantics` remains as evidence but is normalized under `value_contract` and `failure`. Static AST findings are not promoted to a promised behavior without an explicit semantic source or behavioral test.

### Stability

```text
stability.level = stable | preview | experimental | deprecated |
                  unsupported_importable | internal
```

`since` is required for `stable`, `preview`, and `experimental`. `deprecated` additionally requires `deprecated_since`, `replacement_id` or a reason, and `removal_not_before`. `unsupported_importable` captures the current 112 import-callable internal stdlib functions without promising compatibility. A public consumer may include only levels allowed by its policy and must visibly label non-stable items.

### Availability and capabilities

Each node has:

```text
availability.status = available | conditional | importable | quarantined |
                      documentation_only | unavailable | removed
availability.when   = a normalized all/any/not/ref capability expression
availability.on_unavailable = hidden | structured_error | load_error |
                              completion_label_only
```

Raw Rust `cfg`, target, feature, plugin, and runtime conditions are retained as evidence, but consumers use stable capability IDs such as `target.native`, `target.wasm32`, `feature.midi`, `feature.native-recording`, `extension.fs`, and `plugin.mi-ugens`. A capability node defines its owner, detection source, dependencies/conflicts, stability, and projection rules. The same IDs feed CLI capability output, HTTP hello/health, WebSocket hello, WASM, and editors.

`conditional` without a capability expression is invalid. A completion offered when its capability is false must be visibly unavailable and must state the resulting error; silent exposure is invalid.

### Lifecycle

Every callable/type uses the shared classification below; inapplicable fields are explicitly `not_applicable`.

| Field | Values and meaning |
|---|---|
| `subject` | `value`, `builder`, `handle`, `reference`, `operation`, `type_registration`, `module_definition` |
| `phase` | `construct`, `configure`, `validate`, `register`, `enqueue`, `plan`, `stage`, `commit`, `observe`, `release`, `pure_call` |
| `terminal` | `none`, `finalize_value`, `declare`, `start`, `stop`, `cancel`, `remove`, `read`, `pure_result` |
| `effect_timing` | `none`, `evaluation_local`, `candidate_acceptance`, `runtime_queued`, `runtime_applied`, `immediate_live` |
| `synchronization` | `none`, `sync_to_candidate`, `revision_receipt`, `backend_barrier` |
| `repeat` | `pure`, `idempotent`, `replace`, `duplicate_error`, `additional_effect` |
| `cancellation` | Operation ID or `not_supported`/`not_applicable`, with the latest cancellable phase. |

Names such as `apply`, `run`, `start`, `stop`, and `cancel` are not themselves lifecycle evidence. The lifecycle theme supplies the classification for all current 194 nonterminal-chain entries and 26 named terminals; this schema owns the representation and completeness gate.

### Values, units, ranges, coercion, parsing, and collisions

`value_contract` is required for every parameter, DTO field, property, return field, setting, and event field whose value is not a purely opaque identifier.

| Field | Contract |
|---|---|
| `semantic_type` | Stable concept such as `midi.channel`, `musical.beats`, `audio.amplitude`, `transport.bpm`, not only a Rust/TypeScript type. |
| `unit` | Stable unit ID or `unitless`/`not_applicable`. |
| `range` | Optional min/max with inclusive flags and provenance; explicit `unbounded` when intentionally so. |
| `non_finite` | `reject`, `allow`, `clamp`, or `not_applicable`. |
| `coercion` | Ordered accepted source types and loss/rounding/wrap policy. |
| `parser` | `strict` or `permissive`, grammar ID, location behavior, and fallback. |
| `default` | Exact value plus whether it is wire, language, handler, or runtime default. |
| `collision` | Namespace, duplicate policy, deterministic resolution, and diagnostic ID. |

Defaults and ranges are represented once at the narrowest owning field/parameter and referenced by bindings. Equivalent MIDI, time, gain, or note values cannot silently use different representations; an intentional adapter declares its conversion and compatibility cost.

### Types, fields, and effectiveness

`types` contains records, enums, aliases, tagged unions, and error envelopes. A field record includes serialized and host names, direction (`input`, `output`, `bidirectional`), required/default/type information, value contract, stability, availability, and a list of operation bindings.

Each input binding has:

```text
effectiveness.status = effective | structured_rejection | ignored_migration |
effectiveness.effect_ids = [...]             # required for effective
effectiveness.error_ids = [...]              # required for rejection
effectiveness.observable_at = desired | applied | telemetry | response_only
effectiveness.migration = { owner, issue, remove_by } # ignored only
```

An `effect` names the state transition or external action, its target, merge/replacement behavior, and observable result. Merely reading a field, logging it, sleeping, or passing it into an object later discarded is not an effect. Output fields use an `observation` record naming their authoritative source, freshness/revision relation, and absent/unavailable behavior.

The generator fails when:

- a deserializable field has no operation binding;
- an operation binding has no effectiveness classification;
- `effective` has no observable effect/test;
- `structured_rejection` has no reachable typed error;
- a public release contains `ignored_migration`;
- two records share a serialized name but disagree on shape without distinct type IDs;
- an output is always placeholder/empty but described as available data.

### Operations, revisions, receipts, and consistency

`operations` are transport-independent behaviors. HTTP routes, Rhai terminals, CLI actions, WASM methods, and editor commands bind to an operation rather than independently defining success.

| Field | Contract |
|---|---|
| `kind` | `read`, `mutation`, `evaluation`, `subscription`, `telemetry`, `lifecycle_control` |
| `request_type_id`, `response_type_ids`, `error_type_id` | Typed request/success/error shapes. |
| `effects` | Ordered effect IDs with preconditions and commit point. |
| `idempotency` | `yes`, `no`, or `conditional` with key/scope. |
| `revision.mode` | `none`, `allocates`, or `observes`. |
| `revision.acceptance` | Whether response can be `rejected`, `accepted`, or terminal; queue/send alone can reach only `accepted`. |
| `revision.terminals` | Required terminal states and their effect/atomicity meaning. |
| `consistency` | `response_snapshot`, `desired_state`, `applied_state`, or `telemetry_sample`, plus freshness fields. |
| `security` | Trust boundary, authentication/origin/body/rate policy capability IDs. |
| `bindings` | Surface-specific wire/call identities and availability. |

The schema requires a shared receipt envelope for every `revision.mode = allocates` operation. It has the evaluation attempt or revision ID, prior/applied revision, state, phase, structured diagnostics, per-effect outcomes when partial application is supported, and observation sequence. The exact revision ID representation and final state machine come from the revision-receipt theme; no projection may invent a local substitute.

The minimum terminal vocabulary supported by the schema is `rejected`, `superseded`, `applied`, `failed`, and `partially_applied`. If the architecture decision forbids partial application, `partially_applied` is marked unavailable and any occurrence fails. This keeps the schema compatible with the decision without pre-deciding rollback implementation.

### Failures

Every boundary has a `failure` record, including reads and pure calls:

- failure stage and stable diagnostic/error IDs;
- returned transport representation, status/code, and retryability;
- whether prior desired/live state is unchanged, partially changed, or unknown;
- fallback value/policy and whether the caller is notified;
- panic/unwind exposure (`none` is required for user-controlled public input);
- delivery channel (`return`, `receipt`, `event`, `diagnostic`); logs and console output may be additional evidence only;
- cleanup/compensation and its owner when external effects began.

The existing seven boundary facets may seed this record. `unknown`, an unreported fallback, or `panic_exposure=present` at a user-controlled release boundary is a failed gate, not just a count.

### HTTP and surface bindings

An HTTP binding contains `method`, `path`, stable `operation_id`, path/query/header/body type IDs, all success statuses/types, the shared error envelope, availability, protocol version, authentication policy, idempotency/revision headers, and deprecation aliases. A handler name is an anchor, not the operation identity.

Axum route extraction remains a negative check against the binding set. The binding set must be exact in both directions: 96 current method/path registrations produce 96 bindings, and no binding lacks a route. Serde extraction similarly checks that every bound wire field exists with the declared rename/default/optionality. Effectiveness comes from explicit semantics and behavioral tests, not from AST use-count inference.

A WASM binding records class/module export, JS name, sync/async shape, required host capabilities/globals, operation ID, thrown/rejected error type, stability/deprecation, and canonical package. A CLI binding records binary, subcommand/flag identity, defaults, environment/config source, output/exit contract, and operation or capability query. Rhai bindings preserve receiver/overload identity. Editor bindings record command/completion/diagnostic/token IDs and their package locations.

### WebSocket events and diagnostics

An event node contains exact wire `type`, payload type ID, producer operation/source, protocol version, ordering (`observation_sequence`), applied revision relation, delivery semantics, loss detection, and resync operation. Untyped `data` may remain as an outer transport slot, but every registered event type must select one typed payload.

Diagnostic rules are contract nodes with stable rule ID, severity, applicability, source owner, data dependencies, default enablement, and fixes. Push and pull diagnostics both select the same rule set; their coverage report must be identical. Semantic-token types and modifiers are ordered contract lists. Emitters use generated indices and server capabilities use the same generated legend. Appending a token type is compatible; reorder/removal is wire-breaking.

### Consumers and coverage

Each consumer declares:

| Field | Example |
|---|---|
| `consumer_id`, `owner` | `editor.vscode.completion`, `editor.lsp.hover`, `editor.emacs.http` |
| `source_projection` | Generated path(s) and schema/contract digest expected at runtime |
| `eligibility` | Surfaces, kinds, stability, capabilities, and package curation policy |
| `included_ids` | Generated, never handwritten for an exhaustive consumer |
| `exclusions` | ID plus `intentional_curation`, `unsupported_host`, `deprecated`, or `not_applicable` reason and owner |
| `package` | Package name/version owner and required archive paths |

`coverage` is generated from these policies, not manually declared. It reports numerator, denominator, exclusions by reason, unresolved IDs, stale IDs, and deltas from the comparison base. Required initial rows include:

- LSP and VS Code functions: 600 of 600 eligible overload rows;
- properties: 0 of 275 current rows until the new projection lands, then 275 of 275;
- VS Code bundled UGens: 470 valid of 478 current labels, with the release target 478 of 478 and zero stale;
- curated UGen identity coverage: currently 470 of 1,174 callable identities with 704 absent; after correcting the eight labels, 478 of 1,174 with 696 intentional category exclusions unless curation also changes;
- stdlib: 890 declaration occurrences/887 distinct names, with support class visible;
- HTTP: 96 of 96 route bindings and 100% of the 297 fields classified by direction/effectiveness;
- WASM: every generated class/module export bound to stability, host, effect/failure, and package owner;
- diagnostics: identical push/pull rule-ID sets;
- documentation: all non-archival public Vibe/CLI code blocks classified and validated.

Coverage totals change only through a compatibility diff. A denominator cannot silently shrink because a projection stopped seeing a source file.

### Ownership and semantic fragments

The chosen fragment layout is:

| Fragment | Semantic owner |
|---|---|
| `api/contract/authoring.toml` | Rhai lifecycle, values, stability, and operation bindings |
| `api/contract/runtime.toml` | Runtime effects, revisions/receipts, consistency, failure/atomicity |
| `api/contract/http.toml` | Route-to-operation/type bindings and per-field effectiveness |
| `api/contract/websocket.toml` | Client actions, typed events, ordering/loss/resync |
| `api/contract/wasm.toml` | JS host, class compatibility, method operations/failures, package owner |
| `api/contract/consumers.toml` | LSP, VS Code, Emacs, documentation, and package eligibility/exclusions |

These files use a versioned fragment schema defined in `vibelang-api-manifest`. They contain only explicit semantics and references to discovered stable IDs. The composer rejects a mechanical key in a fragment, duplicate semantic ownership, missing IDs, orphaned records, and incompatible refinements. Domain fragments are preferred over one giant overlay to reduce merge contention and make review ownership visible.

The contract owner is responsible for schema meaning and cross-domain validation. The implementation owner remains responsible for behavior and its tests. The consumer owner is responsible for faithful projection/use. These may be different teams or crates and therefore are separate required fields.

## Derivation and projection ownership

```text
Rhai metadata + Rust registration AST ------+
UGen catalogs + shared build rules ---------+
stdlib source ------------------------------+
Clap/Axum/Serde/WASM/LSP source ------------+--> xtask discovery
                                                    |
api/contract/*.toml ------------------------+--> typed join + validation
                                                    |
                                      public-api-manifest-v2.json
                                                    |
             +----------------+---------------------+-------------------+
             |                |                     |                   |
       v1 manifest       transport schemas    editor/LSP data     docs/coverage
       compatibility     WASM declarations    package indexes     diff inputs
       projection
```

### Source versus projection rules

| Fact | Authoritative source | Generated projections/negative checks |
|---|---|---|
| Rhai callable/type existence and signature | Effective Rhai metadata joined to exact Rust registrations | v2 entries, v1 manifest, LSP/VS Code rows, references |
| UGen class/rates/inputs/output/plugin and callable naming | Production JSON catalogs plus `build_support.rs`; runtime registration is checked | DSP entry/overloads, editor bundle, docs; exact consumed labels resolve |
| Stdlib definition/function/import classification | `.vibe` source plus explicit support metadata | v2 entries, VS Code stdlib, generated reference |
| CLI binary/subcommands/flags/defaults | Clap declarations and normalized help | CLI reference and docs block validation |
| HTTP method/path and wire field shape | Axum router and Serde AST | v2 bindings, OpenAPI/JSON schema, HTTP reference; exact bidirectional route/type checks |
| HTTP field effect/status/revision | `api/contract/http.toml` plus handler tests | OpenAPI extensions, docs, coverage, client types |
| WebSocket payload | Typed Rust payload/action declarations | Versioned JSON Schema, client data, docs; runtime event fixture validation |
| WASM export signature | Annotated Rust source | TypeScript declarations and package index |
| WASM effect/host/stability | `api/contract/wasm.toml` plus tests | TypeScript docs, compatibility table, package metadata |
| LSP semantic legend | One Rust definition consumed by emitter and capability registration | Generated legend/coverage and protocol fixtures |
| Diagnostic rules | One typed rule catalog used by both push and pull | Rule reference and parity coverage |
| Consumer curation/package paths | `api/contract/consumers.toml` and package manifests | Data projections, coverage, archive checks |
| Public prose/code examples | Handwritten Markdown source | Parsed code-block inventory and contract/CLI validation |

Descriptions and examples may be supplied by semantic fragments, but generated reference prose is always a projection. A generated file begins with its source contract path/digest and is never edited directly.

### Generated outputs

The first implementation should produce or retain these checked outputs from v2:

- `api/public-api-manifest-v2.json` — canonical effective contract;
- `api/public-api-manifest-v1.json` — temporary compatibility projection;
- `api/http-api-snapshot-v1.json` — temporary declaration compatibility projection;
- `api/http-openapi-v1.json` — versioned effective HTTP schema;
- `api/websocket-protocol-v1.schema.json` — typed client actions/events and receipt envelopes;
- `crates/vibelang-wasm/types/index.d.ts` — canonical package declarations;
- the existing LSP/VS Code Rhai and curated UGen projections, extended with contract version/digest and exact coverage;
- generated LSP diagnostic-rule and semantic-token-legend data;
- generated CLI, HTTP, UGen, stdlib, effective-contract, coverage, and compatibility-policy references under `docs/reference/generated`;
- a machine-readable package index listing every required VS Code and WASM archive member.

OpenAPI and JSON Schema are transport projections, not alternate semantic sources. Vendor extensions may carry operation/revision/effect IDs but must reference the canonical node rather than copy prose semantics.

## Generation commands and determinism

### Current supported commands

```sh
scripts/public-artifacts.sh generate
scripts/public-artifacts.sh check
CARGO_BUILD_JOBS=1 cargo run -p xtask -- public-api generate
CARGO_BUILD_JOBS=1 cargo run -p xtask -- public-api check
```

The first two are the complete current gate. The direct `public-api` command covers only the manifest and stdlib reference.

### Proposed commands

```sh
# Compose the canonical contract and contract-only projections.
CARGO_BUILD_JOBS=1 cargo run -p xtask -- effective-contract generate
CARGO_BUILD_JOBS=1 cargo run -p xtask -- effective-contract check

# Run the complete CLI/editor/WASM/docs/package artifact workflow.
scripts/public-artifacts.sh generate
scripts/public-artifacts.sh check

# Classify every change relative to an explicit artifact, never an implicit checkout.
CARGO_BUILD_JOBS=1 cargo run -p xtask -- effective-contract diff \
  --base api/baselines/public-api-manifest-v2.json \
  --candidate api/public-api-manifest-v2.json \
  --json target/effective-contract-diff.json \
  --markdown target/effective-contract-diff.md
```

`scripts/public-artifacts.sh` continues to be the release/CI front door and invokes `effective-contract` internally. A diff base is an explicit file extracted from the comparison release/ref so the tool does not depend on network state or a mutable branch name.

### Deterministic generation requirements

1. Set `LC_ALL=C`, `NO_COLOR=1`, and fixed CLI width before any captured output.
2. Use the locked Rust and npm dependency graphs. Serialize Cargo as the current script does.
3. Normalize repository-relative paths to `/`, LF line endings, and exactly one trailing newline.
4. Sort nodes by stable ID; sort sets lexically; retain order only where order is contractual, such as parameters, effect phases, token legends, and precedence.
5. Do not emit timestamps, hostnames, absolute paths, target directories, random IDs, current process features, or unordered hash iteration.
6. Generate twice into separate temporary directories and require byte-identical trees.
7. In check mode, write only temporary files, compare every checked projection byte-for-byte, and leave the worktree unchanged.
8. Replace `validate_baseline`'s hardcoded growth-blocking counts with compatibility-diff and coverage policies. Keep minimum safety assertions only where a disappearing domain would otherwise make a denominator zero.

## Drift, negative, coverage, and package gates

The complete `check` command must fail on any of the following.

### Contract completeness

- A discovered public declaration, type, route, field, export, event, command, setting, or consumer lacks a contract node or explicit non-public classification.
- A semantic fragment references no discovered ID, duplicates another owner, or attempts to redefine a mechanical fact.
- Any applicable lifecycle, value, availability, stability, effectiveness, failure, revision, ownership, or consumer field is absent or `unknown`.
- A source/test anchor path does not exist, its symbol cannot be resolved, or a required behavioral test anchor is absent.
- A capability expression references an unknown/cyclic capability or a conditional node has no unavailable behavior.

### Behavioral truth

- An accepted request field is ignored, warning-only, or log-only; an unsupported field does not return its declared structured error.
- A success response lacks an observable effect or claims applied state before the operation's consistency point.
- A mutation that can cross a queue/runtime boundary has no revision mode/receipt.
- A public user-controlled boundary can panic/unwind or silently fallback contrary to its contract.
- An HTTP route/type binding disagrees with Axum/Serde source in either direction.
- A WASM method can resolve successfully after bridge, initialization, or reload delivery failure.
- A WebSocket event/action has untyped data, no ordering/loss policy, or no resync behavior where loss is possible.

### Consumer drift

- Any exact VS Code `functions` completion label fails to resolve. The present eight-label defect is a required negative fixture.
- LSP and VS Code function/property projections differ unexpectedly or their contract digest differs.
- Semantic-token emission indices do not decode against the advertised canonical legend.
- Push and pull diagnostics select different rule IDs, severities, or applicability.
- A setting is contributed but neither consumed nor explicitly classified as UI-only/deprecated.
- A generated consumer denominator shrinks without a compatibility diff; an excluded ID lacks a curation reason/owner.
- Workspace-first and packaged VS Code data have incompatible contract versions without a visible diagnostic.

### Documentation drift

- Any Vibe or CLI block in non-archival root docs, `docs`, crate/editor READMEs, or tutorials contains an unavailable call, wrong signature, invalid import, obsolete binary/flag, or an unclassified intentionally non-executable fragment.
- Quantitative claims tied to the contract lack a generated include/reference.
- Generated references differ from v2 or a handwritten document claims stronger effectiveness/stability than its referenced nodes.
- At minimum, fixtures reject `get_group`, `add_effect`, `vibelang run -w`, `vibelang lsp`, `--api`, and `--midi-input` in active documentation contexts.

### Packaging drift

- A VS Code packed archive lacks `package.json.main`, `out/extension.js`, required `src/data` projections, UGen bundle/index, or the declared contract version/digest; source and packaged emitter inventories differ.
- A WASM npm pack lacks the canonical JS, wasm, and `types/index.d.ts`, or its package version/type digest disagrees with the contract.
- The landing page resolves to a competing local `vibelang-wasm` package owner or historical glue is reachable as current package input.
- A generated package contains unlisted public data or a required projection is excluded by ignore/files rules.

Package gates should inspect the file list from a dry-run archive build, not merely the source tree.

## Compatibility diff policy

Every changed JSON pointer receives one or more applicable classes, a rationale, impacted consumer list, and required release action. Multi-surface changes deliberately carry multiple classes; for example, removing an HTTP operation can be both wire- and availability-breaking. The report's result is the strictest required action across all classes. An unclassified hunk fails.

| Class | Examples | Required action |
|---|---|---|
| `metadata_only` | Owner/test anchor/description correction with no promised behavior, wire data, availability, or consumer change | Review; patch-compatible. |
| `compatible_addition` | New optional response field, new operation/type, appended semantic token type, newly exposed curated completion | Minor-compatible; addition must still be fully classified and tested. A Rhai overload is compatible only if ambiguity analysis passes. |
| `compatible_relaxation` | Range widened, accepted input type added without ambiguity/loss, capability newly available, error made less likely | Minor-compatible; prove legacy fixtures unchanged. |
| `behavioral_change` | Default, fallback, error code/severity, timing, idempotency, lifecycle, consistency point, parser, or diagnostic behavior changes without changing syntax | Explicit migration note and review; breaking unless an accepted versioned opt-in or bug-fix exception is documented. |
| `source_breaking` | Callable/property removed or renamed, receiver/signature changed, required parameter added, type narrowed, WASM method signature changed | Major/opt-in version, deprecation window, replacement, and compatibility fixture. |
| `wire_breaking` | HTTP method/path/status/serialized field changed, request field becomes required, event payload/meaning changed, token legend reordered, error envelope changed | New protocol/media/URL version or negotiated capability plus migration adapter. |
| `availability_breaking` | Capability condition narrowed, native/WASM/plugin support removed, previously available node hidden | Versioned change with capability migration and consumer behavior. |
| `consumer_breaking` | Eligible completion/hover/command/diagnostic removed, package path/main/types changed, coverage denominator silently excluded | Consumer/package major or migration adapter; archive and client fixtures required. |
| `security_operational` | Authentication/origin/body/rate policy becomes stricter or insecure mode changes | Security review and explicit operator migration; may be wire/behavior breaking in addition. |
| `unclassified` | The diff engine has no rule for the changed semantic path or cannot determine impact | Always fail; add a schema-aware rule and review before merge. |

Rules are based on observable impact, not intent. Correcting the manifest to admit that a field was ignored is not `metadata_only`; it is a contract correction whose future implementation/removal still receives the appropriate behavioral/wire classification. A description change that changes a promise is likewise behavioral.

Special rules include:

- stable to deprecated is `behavioral_change`; deprecated/available to removed is source, wire, availability, and/or consumer breaking as applicable;
- an optional request field addition is compatible only when old servers reject/ignore policy is version-safe and the new field has a declared effect;
- a new error for an input that previously succeeded is behavioral or wire breaking;
- unit changes and range narrowing are source/wire breaking even if the host primitive stays numeric;
- event/token append is compatible only when consumers negotiate or safely ignore additions; reorder is wire-breaking;
- moving a package artifact while retaining a verified compatibility re-export can be `behavioral_change`; removing the old path is consumer-breaking;
- curation changes always update coverage and are never hidden as generator churn.

## Migration order

1. **Freeze the baseline.** Preserve schema v1, HTTP snapshot v1, current editor/WASM projections, counts, IDs, and known-defect negative fixtures at the assessed lineage.
2. **Land schema and composer without consumer changes.** Add v2 Rust types, fragment parsing, deterministic validation, v2 output, and v1-from-v2 byte-equivalence. Allow migration debt only through explicit `ignored_migration`/`unknown_migration` records that carry an owner and deadline and are excluded from release readiness.
3. **Join every mechanical source.** Add CLI, HTTP route/type, WASM export, WebSocket action/event, LSP rule/legend, editor contribution, documentation block, and package inventory nodes. Prove the current 3,626/8,431 declaration spine is preserved.
4. **Import the four behavioral-theme decisions.** Populate revision/receipt, lifecycle, field effectiveness, and convention/capability semantics. Remove name/signature behavioral inference. This step gates implementation of behavior, not vice versa.
5. **Make runtime and transport truth match.** Implement revisioned outcomes and effective/rejected inputs, then generate versioned HTTP/OpenAPI, WebSocket schemas, WASM results, and CLI capability/readiness projections. Keep v1 adapters until their compatibility policy permits removal.
6. **Migrate consumers.** Fix the eight UGen labels, unify semantic legend and diagnostics, add properties and capability labels, align VS Code HTTP/settings, change Emacs to `vibe lsp`, and make all consumers report a contract version/digest and coverage.
7. **Migrate docs and packages.** Validate every active code block, update stale commands/claims, choose the crate WASM package as sole owner, remove or archive competing landing-page glue, and enforce real archive contents.
8. **Turn migration warnings into release failures.** Require zero unknown/unowned/ignored public nodes, zero stale consumer IDs, full required coverage, deterministic double-generation, and a classified compatibility diff.
9. **Retire compatibility projections only after policy.** Remove v1 manifest/HTTP/WASM/editor aliases only after the stated deprecation window and client fixtures prove the supported migration path.

The order prevents generated schemas from advertising behavior before runtime outcomes are truthful, while allowing schema/gate infrastructure to land before behavior changes.

## Interaction with the other four API-unification themes

| Theme | What that theme owns | What this contract owns | Integration gate |
|---|---|---|---|
| Shared revision receipt and mutation topology | Exact attempt/revision identity, state machine, apply/rollback/partial policy, commit point, ledger, and cross-surface receipt semantics | Required operation/revision/failure fields and one projection for every CLI/Rhai/HTTP/WS/WASM binding | No revision-allocating operation lacks a shared receipt; no surface relabels accepted as applied. |
| Values/builders/handles/references and terminal verbs | Classification of each public family and the chosen terminal/composition behavior | Lifecycle enum, stable IDs, coverage, projections, and compatibility rules | All 194 current chain entries and 26 named terminals are explicit; zero signature/name-inferred lifecycle records. |
| Accepted-field and terminal effectiveness | Exact implement-versus-reject decision for every field/terminal and tests for observable effect | Field/operation effectiveness representation, zero-ignored gate, HTTP/WASM/editor/docs projections | All 297 HTTP fields and all accepted terminals are effective, observational, or structured rejection; zero success-shaped no-ops. |
| Units/ranges/parsing/collisions/availability/capabilities | Canonical convention values, parser strictness, collision resolution, capability identifiers and runtime discovery behavior | Value/capability schema, consumer expressions, diff classes, coverage, and generated capability projections | Every numeric public value is unit/range/unbounded; equivalent concepts align; all conditional nodes reference discoverable capabilities. |

The schema does not decide domain values on behalf of those studies. It decides that their results are required, typed, single-owned, diffable, and projected identically.

## Measurable implementation acceptance

| Area | Acceptance condition |
|---|---|
| Schema | Schema v2 round-trips deterministically; unknown enum values and missing applicable facets fail; v1 parsing remains supported. |
| Identity | Every unchanged v1 entry/overload retains its ID; collision and rename fixtures produce deterministic errors/edges. |
| Baseline | V2 represents all current 3,626 entries and 8,431 overloads; the v1 projection is byte-identical before intentional API changes. |
| Sources | Every discovered route/type/export/event/command/setting/package/code block is represented or explicitly non-public; zero orphan semantic records. |
| HTTP | Exactly 96 route bindings; all 75 types and 297 fields have direction, availability, stability, source, and field/operation effectiveness where applicable. |
| WebSocket | Every client action and emitted event has a typed payload, protocol version, sequence/revision relation, loss detection, and resync policy. |
| WASM | Both current classes and module exports have explicit stability/host/effect/failure/package ownership; canonical types match Rust; bridge/init/delivery failures cannot return success. |
| Editors | 600 function rows remain aligned, 275 properties are projected or explicitly excluded, all 478 bundled UGen labels resolve, and the current eight-label defect is zero. |
| LSP | One semantic legend feeds emission and advertisement; push/pull diagnostic rule-ID/severity sets are identical. |
| Coverage | Generated numerator/denominator/exclusion reports exist for every declared consumer; zero unresolved/stale IDs and zero silent denominator shrink. |
| Documentation | Every non-archival public Vibe/CLI block is classified; the listed stale calls/binaries/flags fail negative fixtures. |
| Packaging | Dry-run VS Code and WASM archive inventories exactly satisfy the generated package index; only one canonical `vibelang-wasm` package owner remains. |
| Determinism | Two clean generations are byte-identical; `check` leaves no diff; all paths are repository-relative and output has normalized newlines. |
| Compatibility | Addition, relaxation, behavior, source, wire, availability, consumer, security, and unknown fixtures produce the expected classes; real diffs contain zero unclassified hunks. |

## Measurable integration acceptance

1. A representative mutation submitted through Rhai/CLI watch, HTTP, and WASM yields the same operation ID, revision identity, terminal state, diagnostics, and applied-state observation; WebSocket reports the correlated revision/sequence.
2. Failure injection at parse, evaluation, queue delivery, planning, staging, backend synchronization, commit, and observation proves the declared failure/atomicity result on every surface.
3. Fixtures for every currently ignored HTTP field either observe the requested effect or receive the declared structured rejection; no 2xx response is followed only by a warning/log.
4. Native, WASM, MIDI-on/off, recording-on/off, extension, and plugin fixtures produce the declared capability/availability matrix and matching editor visibility.
5. A lagged WebSocket client detects a sequence gap and successfully invokes the declared resync operation to a revisioned snapshot.
6. WASM worker/window and missing-bridge fixtures match declared host support; no discovery/invocation global mismatch remains.
7. LSP semantic-token conformance decodes every emitted token against the advertised legend, and push/pull diagnostics return the same canonical rule set for one document.
8. Packaged VS Code and Emacs clients consume the same fixture contract/protocol as source-mode clients; stale or incompatible contract digests produce a visible diagnostic.
9. Supported v1 fixtures retain their promised behavior through adapters; each intentional v2 break appears in the generated diff and migration documentation.
10. Public artifact check, contract coverage, compatibility diff, docs-block validation, and archive validation all pass in one clean CI job.

## Rejected alternatives and tradeoffs

### Extend schema v1 in place

Rejected because consumers already identify `schema_version: 1` and its field meanings. Adding required behavioral fields or changing lifecycle semantics under the same version makes old and new v1 indistinguishable. V2 plus a v1 projection gives an auditable migration path.

### Treat runtime code as sufficient behavioral metadata

Rejected because syntax cannot reliably prove intent, observability, compatibility, or an ignored value. The current HTTP and WASM defects compile precisely because parsing, queueing, warning, and success are different facts. AST scans remain useful negative evidence, not the sole contract.

### Handwrite one giant effective JSON/OpenAPI document

Rejected because it duplicates thousands of registrations and types, loses effective Rhai/build-time generation, and makes code drift inevitable. Domain semantic fragments add only facts not mechanically owned elsewhere. The tradeoff is a stricter join and fragment-review discipline.

### Let each consumer generate directly from code

Rejected because it recreates today's split: editor labels, token legends, diagnostics, HTTP DTOs, WASM types, docs, and packages can each be internally reproducible yet mutually inconsistent. Consumers must project from one compiled contract and be checked against execution sources.

### Make runtime introspection the canonical artifact

Rejected because native/WASM/features/plugins produce different reachable sets, generation would require host services/targets, and packages/docs must be buildable offline. Runtime capability snapshots are projections of declared capability IDs plus detected state, not the source schema.

### Require every consumer to expose everything

Rejected because the 24-category UGen bundle is a legitimate product curation choice and some hosts cannot support native APIs. The chosen policy permits curation but requires exact eligibility, exclusions, coverage, and availability labels. Its cost is maintaining consumer policy explicitly.

### Infer compatibility solely from JSON shape

Rejected because a default, effect timing, unit, error, diagnostic, or availability change can break users without changing a primitive type. Semantic diff classes are necessary; the cost is explicit review for behavioral changes.

## Isolated open decisions

These decisions are inputs to the chosen schema, not reasons to defer it:

1. **Revision theme:** exact `RevisionId`/attempt representation, terminal state machine, whether `partially_applied` is permitted, retention, and commit/rollback semantics.
2. **Lifecycle theme:** final classification of each current builder/handle/reference family, terminal spellings, double-terminal behavior, and composition aliases.
3. **Effectiveness theme:** implement-versus-reject choice and deprecation timing for every currently ignored HTTP field, Rhai terminal, and WASM no-op.
4. **Convention/capability theme:** canonical MIDI/time/amplitude units, strict/permissive parser modes, stdlib collision resolution, capability ID catalog, and discovery payload values.
5. **Release policy:** the minimum deprecation duration/API-version rule before deleting v1 projections and the legacy `VibelangEngine`. The diff classes and required replacement edges are fixed regardless of the chosen duration.

Everything else in this proposal is explicit: schema v2 is the canonical compiled contract; domain fragments own non-inferable semantics; `xtask` owns deterministic composition; code/catalogs own mechanical declarations; the crate WASM package owns distribution; all consumer/docs/package artifacts are projections; and unclassified or ineffective public behavior fails release.

## Handoff to the story owner

The implementation can be divided internally, but one story owner should preserve the dependency order and the v1-equivalence checkpoint. The first reviewable commit should add schema/fragment types and fixture-only validators. The second should compose the current declaration graph and prove v1 byte equivalence. Behavioral metadata and projections should then land domain by domain, each with its negative fixtures and coverage delta. No domain should switch consumers before its effective behavior and compatibility classification are present in v2.
