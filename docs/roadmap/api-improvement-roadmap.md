# VibeLang API improvement roadmap

This roadmap is based on the current registration/dispatch inventory, not on
aspirational README examples. It separates documentation-generation work from
runtime changes; this document itself changes no behavior.

## Evidence: why unification is necessary

| Inconsistency | Current evidence | User impact |
|---|---|---|
| Fluent calls have different lifecycles | Named Voice syncs most mutations; Pattern/Melody/Sequence/Fx/Record defer; Sample mutates immediately; [`Fade.apply()`](../../crates/vibelang-rhai/src/api/sequence.rs#L562-L564) is a no-op | Authors cannot infer when a declaration exists |
| Accepted calls/fields do nothing | Pattern `set_param`; Record `immediate`, stop/cancel, pending Sample insertion; many [HTTP update fields](../../crates/vibelang-http/src/models.rs) | False success and hard-to-debug live state |
| Route prose and implementation diverge | [`RouteHandle::to`](../../crates/vibelang-rhai/src/api/route.rs#L65-L71) is additive fan-out; older project prose says replacement/deferred group fan-out | Reload/routing surprises |
| Similar units/indexing disagree | Voice MIDI channel is internal 0..15; MidiDevice/builders use 1..16; bars sometimes read signature and sometimes mean ×4 | Off-by-one and timing errors |
| Error policies are mixed | Rhai errors, clamping, warnings/fallback, panicking unwraps, and silent no-ops coexist | Tooling cannot predict or explain failures |
| Hand-maintained metadata forks | Website API cards contain fictional calls; VS Code/LSP static data omits most registrations and contains nonexistent names | Completion/docs can actively mislead |
| Demand-rate support is quarantined | All 25 `*_demand` source identities are preserved but absent from runtime, manifest-callable, LSP, and VS Code surfaces until rate and input lowering are complete | Scripts fail immediately instead of building an audio-rate graph under a demand-rate name |
| Generated catalogues lack complete boundary contracts | 1,174 registered UGen names come from 802 callable manifest classes, with 25 demand-rate records explicitly quarantined; stdlib has 890 DSP definition occurrences/887 names plus 595 intended-supported and 112 underscore import-callable functions, but no enforced export metadata | Users cannot yet predict all accepted types, coercions, and errors |
| API artifacts are not clean-tree reproducible | `--runtime-metrics` exists only in the pre-existing dirty CLI tree, not clean revision `98bed24`; handwritten WASM/Clap/editor artifacts can drift | A docs commit can advertise a contract its source revision does not ship |
| Runtime observation has no ownership/version contract | Evaluation creates isolated desired `ScriptState`; reconciliation and live runtime state occur afterward | `status(ref)` cannot truthfully promise a consistent live snapshot yet |
| Wire schemas over-promise | HTTP deserializes many ignored fields; mutations often return stale snapshots; REST and WS shapes differ without a published contract | External clients report success without effect |
| Protocol/security gaps are implicit | Open CORS/no auth/limits/origin check; fs sandbox traversal for nonexistent destinations; semantic-token legend mismatch | Unsafe exposure and broken highlighting |

Related sources: [Rhai registration root](../../crates/vibelang-rhai/src/api/mod.rs#L38-L82),
[DSP generator](../../crates/vibelang-dsp/build.rs#L432-L806),
[HTTP router](../../crates/vibelang-http/src/lib.rs#L132-L300),
[WebSocket](../../crates/vibelang-http/src/websocket.rs), and
[LSP server](../../crates/vibelang-lsp/src/server.rs).

## Concrete unified target API

### 1. One declaration lifecycle

All authoring factories should return a declaration builder owned by one
evaluation. Builder methods are pure configuration and always return the
builder. A Ref is a stable identity in desired state, not a borrowed live
runtime object. Exactly these terminals write desired state:

| Terminal | Unified meaning | Return |
|---|---|---|
| `.apply()` | Validate and upsert one declaration into the evaluation snapshot | Stable `*Ref` |
| `.start()` | `.apply()` plus desired playing state at normal quantization | Stable `*Ref` |
| `.start_now()` | `.apply()` plus explicit immediate start | Stable `*Ref` |
| `.run()` | `.apply()` plus continuous-node state (Voice only) | `VoiceRef` |
| `.remove()` / `.stop()` | Explicit desired removal/playback stop on a Ref | Same Ref or Unit by one documented rule |

Factories should not insert state. This includes Group, Voice, Sample, Buffer,
and SFZ in the versioned target surface. Avoid overloading current lookup-only
`group(path)` with declaration behavior: use explicit `group_builder(name)` and
`group_ref(path)` factories. `define_group(name, fn)` can remain versioned sugar
for `group_builder(name).body(fn).apply()`.

Snapshot references and live control should be separate names/types:

```rhai
let lead = voice("lead")
    .synth("lead_bright")
    .poly(8)
    .apply();                    // VoiceRef

pattern("line").on(lead).step("x...x...").start();
voice_ref("lead").mute();       // explicit lookup/live desired control
let observed = status(lead);    // RuntimeObservation, never a builder property
observed.reconciled_revision;
observed.running;
```

Builder properties report builder configuration only. `status(ref)` reads an
injected observation channel and returns at least `{identity,
evaluation_revision, reconciled_revision, observed_at, stale, running, error}`.
The engine must define whether a caller requests the latest available snapshot
or waits for a minimum accepted revision; absence/staleness is explicit rather
than silently falling back to desired state. No terminal may be an inert
compatibility stub.

Before implementation, the runtime/API owners must publish these state rules:

- duplicate declarations in one evaluation either merge by a documented rule
  or fail atomically; validation failure cannot partially mutate the snapshot;
- `*Ref` values are invalid across engine instances and carry language/API
  version plus stable identity, not a pointer to an audio object;
- Sample/Buffer/SFZ resources define allocation owner, reuse key, reload
  identity, replacement behavior, failed-load rollback, and release point;
- Group/body contributions define source-order ownership and removal on reload;
- reconciliation reports accepted/applied/failed revisions so observation and
  wire clients share one consistency model.

### 2. Strict, named conversion and validation

- Public numeric ranges are strict by default and return a Rhai error with code,
  function, parameter, received value, allowed values, and source position.
- Intentional coercion is named (`note_or("bad", "C4")`, `clamp_midi(...)`) or
  stated in a manifest; unsupported Dynamic input never silently targets
  `unknown` or drops a clip.
- Public MIDI channels are uniformly 1..16. A necessary zero-based value uses
  an explicit `channel_index(0..15)` API.
- Time-bearing names include units: `length_beats`, `duration_seconds`,
  `quantize_beats`. `bars` always uses the current time signature; fixed
  four-beat conversion is named `four_beat_bars` only for compatibility.
- Unknown curves, ports, parameters, synthdefs, targets, and source rates are
  validation errors. Warnings are reserved for deprecated-but-effective calls.
- Rust panics/`unwrap` at the Rhai boundary become structured Rhai errors.

### 3. Explicit routing verbs

Retain rate-specific validation and the shared modulation registry, but make
replacement/addition visible in names:

```rhai
source.output("out").to_groups([dry, send]);       // additive audio fan-out
source.output("out").replace_with_main();          // replacement
source.output("env").set_param(target, "cutoff"); // source-first
target.param("cutoff").bend_by(source, "env");    // target-first
target.input("in").replace_from(source, "out");   // single-source input
```

SET/BEND cross-verb conflicts, A2K conversion, trigger routes, fan-in limits,
and default routing must be declared in one route manifest consumed by docs and
validation. Existing short names can remain v1 aliases.

### 4. One machine-readable public manifest

Registration declarations should emit versioned entries containing:

```text
surface, type/receiver, registered name, aliases, overload signatures,
availability/cfg, parameters/types/defaults/units/ranges/coercion,
return/chaining, lifecycle/terminal, side effects, error codes,
since/deprecated/replacement, source/test anchors
```

UGen entries extend this with class/rate/input/default/output/plugin data;
stdlib entries with import path, definition kind, parameters, ports, duplicate
name, explicit export/support status, and both supported-public and
import-callable function signatures. The Markdown reference, website,
completion, LSP hover/signature help, VS Code data, and `llms.txt` consume the
same manifest.

### 5. Versioned wire contracts

- Generate OpenAPI/JSON Schema from routed DTOs and typed WebSocket schemas.
- An accepted request field must be effective; otherwise reject it as 422 with
  a stable error code. Remove unused DTOs from published schemas.
- Asynchronous mutations return 202 plus `{operation_id,accepted_revision}`;
  clients observe completion/revision via REST or WS. Synchronous reads remain
  200. Do not return a pre-reconciliation snapshot as if confirmed.
- Publish `/v1` routes and negotiate WS `protocol_version`. REST DTO and WS
  snapshot differences remain explicit schemas, not accidental reuse.
- Run old and new REST paths/WS protocol majors as a documented dual stack
  during migration. Hello advertises a supported-major range and capabilities;
  clients choose the highest overlap and fall back to the older supported major.
  Refusal happens only when no major overlaps, never merely because versions
  differ.
- Add configurable auth, origin/CORS allowlists, body/rate limits, and secure
  loopback defaults before advertising remote control.

## P0 — make the current contract truthful and safe

| ID / deliverable | Concrete outcome | Functional owner | Impact | Dependency / rollback |
|---|---|---|---|---|
| P0.1 Registration manifest v1 | Extract every current registered name/overload/property/cfg/lifecycle without changing behavior | Rhai API + tooling | Eliminates fictional/missing docs and completion entries | Snapshot quirks first; rollback keeps generated artifacts advisory |
| P0.2 Demand-rate quarantine | Completed: unregister all 25 `*_demand` identities, preserve their canonical source records, and publish explicit quarantine availability | DSP + API manifest + editors | Prevents wrong-rate graphs and misleading tooling | Lossless source records support later re-enable only after real rate, lowering, and golden tests |
| P0.3 Clean-tree artifact reproducibility | In a clean checkout, regenerate Clap help, wasm-bindgen types, core/UGen/stdlib manifests, and editor tables with zero diff | Release engineering + docs/tooling | Makes a docs/source revision self-contained | `--runtime-metrics` cannot publish before its source; rollback removes worktree-only entries |
| P0.4 Boundary/export manifests | Record every overload's accepted types/casts/clamps/fallback/error/panic and classify all 707 stdlib functions with explicit export/support metadata | Rhai/DSP + stdlib | Turns name coverage into semantic coverage | Requires parser/registration schema; rollback preserves the explicit 112-helper appendix |
| [P0.5 State ownership/version/lifetime ADR](../architecture/builder-ref-revision-resource-lifetime.md) | Accepted: pure Builder vs typed/versioned Ref, atomic apply, truthful observation revisions, explicit duplicate/contribution ownership, and generation-managed Sample/Buffer/SFZ lifetime | Runtime + Rhai API | Makes v2 implementable without false live-state claims | Architecture gate for P1 lifecycle and wire revisions; the decision itself rolls out no runtime behavior |
| P0.6 Backend capability contract | Native/browser backends report available plugins, rates, channel limits, protocol majors, and feature flags with a versioned schema | Backend + WASM | Enables truthful availability before P1 tooling | Depends on manifest identifiers; rollback labels availability unknown rather than guessing |
| P0.7 Mark or eliminate inert authoring calls | `Fade.apply`, Pattern `set_param`, Record `immediate`/stop/cancel/sample insertion either work or return explicit unsupported error | Rhai/runtime | Ends false success in scripts | Depends on P0.5 ownership decisions; retain effective v1 behavior only behind warnings |
| P0.8 Reject inert HTTP fields | Handlers implement every accepted field or respond 422; publish current schemas/statuses | HTTP API | Reliable editor/external control | Client warnings and dual-stack plan required |
| P0.9 Protocol correctness | Align semantic-token legend; make push/pull diagnostics equivalent; fix stale CLI/editor invocations | LSP + VS Code + Emacs | Correct highlighting/diagnostics/startup | Coordinated editor release; can disable faulty feature during rollback |
| P0.10 Security baseline | Fix fs sandbox creation traversal; API auth option, body/rate limits, configurable CORS/origin checks; warn on non-loopback | HTTP/security | Safe-by-default exposure | Threat model, credential/config design, reverse-proxy compatibility |
| P0.11 Lifecycle warnings | In v1, emit once-per-source-location warnings for deferred terminals, no-ops, permissive fallback, and deprecated aliases | Rhai/diagnostics | Immediate clarity without breakage | Needs source positions and warning deduplication |

P0 exits only when CI reports zero undocumented registrations, zero stale
generated artifacts from a clean checkout, 25/25 demand names either correctly
encoded or absent/quarantined, 707/707 stdlib declarations classified, and an
overload fixture for every manifest signature. Route/schema/availability drift
and fictional example calls are build failures.

P0.2 uses the quarantine branch of that gate: external scripts that previously
constructed incorrect audio-rate nodes now receive function-not-found errors.
Repository stdlib, examples, and tests had no demand-callable consumers, so
non-demand graph generation and registration remain unchanged.

## P1 — introduce the unified versioned surface

| ID / deliverable | Concrete outcome | Functional owner | Impact | Required gate |
|---|---|---|---|---|
| P1.1 Language API v2 lifecycle | Pure builders, typed/versioned `*Ref`, uniform apply/start/start_now/run/status contracts | Rhai + runtime | Predictable composition and reload | P0.1, P0.4, and approved P0.5 ADR |
| P1.2 Strict validation/error catalogue | Stable error codes and consistent rejection at every Rhai/DSP boundary | Rhai + DSP | Better diagnostics and tooling | P0.4 fixtures; compatibility warnings for permissive songs |
| P1.3 Time/channel/unit normalization | 1..16 MIDI, time-signature bars, unit-suffixed APIs | Rhai + MIDI/transport | Removes off-by-one/timing bugs | AST migration and versioned import semantics ready |
| P1.4 Routing v2 verbs | Explicit add/replace and SET/BEND names with declared rates/fan-in/out | Routing/runtime | Makes complex routing readable | P0 manifest plus reconciliation golden tests |
| P1.5 Wire API v1 | Versioned OpenAPI, typed WS messages, revisions/operation IDs, dual-stack negotiation | HTTP + editors | Trustworthy external integrations | P0.5 revision model and published transition/fallback tests |
| P1.6 Availability-aware tooling | Completion/docs hide or label target/feature/plugin-unavailable calls | LSP/docs/editors | Fewer runtime surprises | P0.1 manifest and P0.6 capability negotiation |

## P2 — remove legacy forks and finish ecosystem convergence

| ID / deliverable | Concrete outcome | Functional owner | Impact | Required gate |
|---|---|---|---|---|
| P2.1 Legacy removal | Remove v1 aliases/no-ops only after the stated time, cadence, and semver window | Release + API owners | Smaller, coherent API | Adoption telemetry, migration success metrics, rollback release retained |
| P2.2 Namespace/import cleanup | Generated import maps, collision/duplicate diagnostics, explicit stdlib exports | Stdlib + language | Scalable stdlib growth | Versioned import semantics and export manifest |
| P2.3 Rich schema-driven tooling | Shared examples, diagnostics fixes, code actions, API explorer, offline searchable indexes | Tooling/editors | Documentation and editor parity | Stable core/wire manifests |
| P2.4 Stable Rust embedding separation | Developer reference generated separately from `.vibe`/wire contracts | Crate maintainers + docs | Stops docs.rs/source confusion | Crate API versioning and docs deployment |

## Dependency graph and release gates

```text
P0.1 manifest ─┬─> P0.2 demand correctness
               ├─> P0.3 clean artifacts
               ├─> P0.4 boundary/export data ─> P1.1/P1.2
               └─> P0.6 capabilities ─────────> P1.6
P0.5 state/version/lifetime ADR ───────────────> P1.1/P1.5
language-version + versioned imports ──────────> AST migration ─> P1.3
wire schemas + dual-stack fallback ────────────> editor rollout ─> P2.1
```

| Gate | Acceptance metric | Rollback point |
|---|---|---|
| P0 contract freeze | Clean generation is deterministic; counts and every overload fixture match source; no known wrong-rate callable remains advertised as supported | Keep current v1 runtime, quarantine defective names, and publish generated artifacts as non-normative |
| P1 opt-in pilot | Representative v1/v2 golden suites match intended ScriptState, messages, routes, resource lifetime, and timing; REST/WS clients pass old/new/fallback matrices | Leave v1 default, disable v2 directive/endpoint, retain old editor protocol |
| v2 default | Migration check has no unclassified rewrite, capability fallback is observable, and release telemetry shows agreed error/regression thresholds | Flip default back to v1 without changing file meaning; keep v2 explicit |
| P2 removal | Support window elapsed, next semver-major release approved, migration adoption target met, and previous dual-stack release remains supported | Withdraw removal release or restore forwarding aliases from the last supported branch |

## Compatibility and migration strategy

1. Freeze the current behavior as language API v1 and protocol v1 fixtures,
   including documented quirks. Do not silently reinterpret old scripts.
2. Make language version an explicit `ScriptEngine` compile/evaluate input, for
   example `EvaluationOptions { language_version }`. A `// vibe-api: 2`
   directive and CLI `--language-version 2` are adapters to that input, not a
   CLI-only semantic switch. CLI run/render, HTTP `/eval`, WASM `execute`, LSP
   validation, imports, caches, and editor eval must pass the same version;
   absence means v1 during transition.
3. Generate a deprecation/alias table from the manifest. Warnings include the
   replacement and exact source span and are deduplicated per reload.
4. Define versioned import semantics before migration tooling: modules either
   declare a version or inherit the importer, cross-version imports have an
   explicit allow/error/adapter rule, and AST/module caches key on language
   version. The same module cannot silently mean different things by embedding.
5. Ship `vibe migrate --check FILE` and `vibe migrate FILE` on a Rhai-aware AST,
   not manifest string replacement. It may rewrite declared-safe method names,
   channel/unit conversions, imports, and terminals while preserving formatting;
   ambiguous lifecycle/resource cases are diagnostics requiring manual action.
6. Keep effective v1 aliases for at least six months **and** two published minor
   releases after v2 becomes default, whichever is later. Removal occurs only
   in the next semver-major release. An alias must forward to working behavior;
   no silent stubs.
7. Version REST paths and WS hello/capabilities. During the same support window,
   servers expose old and new REST paths and WS majors; editors select the
   highest mutually supported major and fall back to the old one. Refuse only
   when there is no overlap, with an actionable upgrade/downgrade message.
8. Publish a generated compatibility report per release: added, deprecated,
   removed, behavior-changed, availability-changed, and schema-changed entries.

Main migration risk is timing/audio behavior, not syntax. Golden tests must
compare desired ScriptState, reconciliation messages, route maps, and rendered
event timing for representative v1/v2 scripts.

## Publication and generation order

1. Publish this lifecycle/availability model and the generated current indexes.
2. Fix or quarantine demand-rate registrations and establish clean-checkout
   artifact generation as a release gate.
3. Emit and validate the core Rhai registration and overload-boundary manifest;
   switch Markdown, website, LSP, and VS Code core metadata to it.
4. Approve the Builder/Ref, observation revision, resource lifetime, duplicate,
   and atomic-apply architecture contract.
5. Add routing/state-sync matrices and runtime object pages from that manifest.
6. Generate DSP builder reference and UGen catalogue from the same build rules
   as [`build.rs`](../../crates/vibelang-dsp/build.rs#L432-L806).
7. Generate the stdlib definition/function catalogue with a real Rhai parser or
   explicit export metadata, preserving module paths, callable internals, and
   duplicate names.
8. Publish backend capability schemas, then make completion/reference output
   availability-aware.
9. Generate REST OpenAPI and typed WS reference from router/DTO/event types;
   publish and test the dual-stack transition.
10. Generate CLI pages from Clap help snapshots, WASM types from wasm-bindgen,
   and editor command/setting/key tables from package/Elisp declarations.
11. Thread language version through every embedding, define versioned imports,
    and ship AST-aware migration before enabling the v2 opt-in pilot.
12. Only after pilot exit metrics pass make API v2 default and begin the
    time/cadence/semver legacy-removal clock.

The checked-in [UGen](../reference/generated/ugens.md) and
[stdlib](../reference/generated/stdlib.md) indexes provide exhaustive
source-level name discovery today, including the demand quarantine and callable
underscore appendix. Until the shared manifest exists, their counts, support
classification, and source linkage should be verified in CI whenever manifests
or stdlib `.vibe` files change.
