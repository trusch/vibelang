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
| Generated catalogues are incomplete | 1,199 registered UGen names come from 827 callable manifest classes; stdlib source has 890 DSP definition occurrences/887 names and 595 intended public imported functions, while shipped prose/web counts disagree | Users cannot reliably discover available sounds/functions |
| Wire schemas over-promise | HTTP deserializes many ignored fields; mutations often return stale snapshots; REST and WS shapes differ without a published contract | External clients report success without effect |
| Protocol/security gaps are implicit | Open CORS/no auth/limits/origin check; fs sandbox traversal for nonexistent destinations; semantic-token legend mismatch | Unsafe exposure and broken highlighting |

Related sources: [Rhai registration root](../../crates/vibelang-rhai/src/api/mod.rs#L38-L82),
[DSP generator](../../crates/vibelang-dsp/build.rs#L432-L806),
[HTTP router](../../crates/vibelang-http/src/lib.rs#L132-L300),
[WebSocket](../../crates/vibelang-http/src/websocket.rs), and
[LSP server](../../crates/vibelang-lsp/src/server.rs).

## Concrete unified target API

### 1. One declaration lifecycle

All authoring factories should return a declaration builder. Builder methods
are pure configuration and always return the builder. Exactly these terminals
write desired state:

| Terminal | Unified meaning | Return |
|---|---|---|
| `.apply()` | Validate and upsert one declaration into the evaluation snapshot | Stable `*Ref` |
| `.start()` | `.apply()` plus desired playing state at normal quantization | Stable `*Ref` |
| `.start_now()` | `.apply()` plus explicit immediate start | Stable `*Ref` |
| `.run()` | `.apply()` plus continuous-node state (Voice only) | `VoiceRef` |
| `.remove()` / `.stop()` | Explicit desired removal/playback stop on a Ref | Same Ref or Unit by one documented rule |

Factories should not insert state. This includes Group, Voice, Sample, Buffer,
and SFZ in the versioned target surface. `define_group(name, fn)` can remain
sugar for `group(name).body(fn).apply()`.

Snapshot references and live control should be separate names/types:

```rhai
let lead = voice("lead")
    .synth("lead_bright")
    .poly(8)
    .apply();                    // VoiceRef

pattern("line").on(lead).step("x...x...").start();
voice_ref("lead").mute();       // explicit lookup/live desired control
status(lead).running             // explicit runtime observation
```

Builder properties report builder configuration only. `status(ref)` reports a
versioned runtime snapshot and never reuses the same property name for both.
No terminal may be an inert compatibility stub.

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
name, and public function signatures. The Markdown reference, website,
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
- Add configurable auth, origin/CORS allowlists, body/rate limits, and secure
  loopback defaults before advertising remote control.

## P0 — make the current contract truthful and safe

| Deliverable | Concrete outcome | Impact | Risk / dependency |
|---|---|---|---|
| Registration manifest v1 | Extract every current registered name/overload/property/cfg/lifecycle without changing behavior | Eliminates fictional/missing docs and completion entries | Requires registration macro/schema design; must snapshot current quirks |
| Generated reference pipeline | Reproduce the 1,199 UGen names, 890 stdlib definitions, and 595 public imported functions; website/LSP/VS Code consume artifacts | Exhaustive discovery from one source | Rhai-aware stdlib parser or explicit metadata needed; regex is insufficient |
| Mark or eliminate inert authoring calls | `Fade.apply`, Pattern `set_param`, Record `immediate`/stop/cancel/sample insertion either work or return explicit unsupported error | Ends false success in scripts | Runtime reconciliation/recording ownership decisions |
| Reject inert HTTP fields | Handlers implement every accepted field or respond 422; publish current schemas/statuses | Reliable editor/external control | Client updates and compatibility warnings |
| Protocol correctness | Align semantic-token legend; make push/pull diagnostics equivalent; fix stale CLI/editor invocations | Correct highlighting/diagnostics/startup | Coordinated LSP/VS Code/Emacs releases |
| Security baseline | Fix fs sandbox creation traversal; API auth option, body/rate limits, configurable CORS/origin checks; warn on non-loopback | Safe-by-default exposure | Threat model, credential/config design, reverse-proxy compatibility |
| Lifecycle warnings | In v1, emit once-per-source-location warnings for deferred terminals, no-ops, permissive fallback, and deprecated aliases | Immediate clarity without breakage | Needs source positions and warning deduplication |

P0 acceptance should include CI snapshot tests that fail on an undocumented
registration, stale generated file, fictional example call, route/schema drift,
or availability mismatch.

## P1 — introduce the unified versioned surface

| Deliverable | Concrete outcome | Impact | Risk / dependency |
|---|---|---|---|
| Language API v2 lifecycle | Pure builders, typed `*Ref`, uniform apply/start/start_now/run/status contracts | Predictable composition and reload | Broad runtime/API changes; depends on P0 manifest and snapshot tests |
| Strict validation/error catalogue | Stable error codes and consistent rejection at every Rhai/DSP boundary | Better diagnostics and tooling | Compatibility with permissive songs; remove boundary unwraps |
| Time/channel/unit normalization | 1..16 MIDI, time-signature bars, unit-suffixed APIs | Removes off-by-one/timing bugs | Migration tooling required |
| Routing v2 verbs | Explicit add/replace and SET/BEND names with declared rates/fan-in/out | Makes complex routing readable | Must preserve current graph/reconciliation capabilities |
| Wire API v1 | Versioned OpenAPI, typed WS messages, revisions/operation IDs | Trustworthy external integrations | REST/WS/editor synchronized rollout |
| Availability-aware tooling | Completion/docs hide or label target/feature/plugin-unavailable calls | Fewer runtime surprises | Build feature metadata and backend capability discovery |

## P2 — remove legacy forks and finish ecosystem convergence

| Deliverable | Concrete outcome | Impact | Risk / dependency |
|---|---|---|---|
| Legacy removal | Remove v1 aliases/no-ops after the stated support window | Smaller, coherent API | Requires adoption telemetry/release discipline |
| Namespace/import cleanup | Generated import maps, collision/duplicate diagnostics, explicit stdlib exports | Scalable stdlib growth | Module-system and duplicate-name policy |
| Rich schema-driven tooling | Shared examples, diagnostics fixes, code actions, API explorer, offline searchable indexes | Documentation and editor parity | Depends on manifest stability |
| Backend capability negotiation | Runtime/browser report plugins, rates, channels, and wire features | Portable scripts and actionable fallback | Backend protocol work |
| Stable Rust embedding separation | Developer reference generated separately from `.vibe`/wire contracts | Stops docs.rs/source confusion | Crate API versioning and docs deployment |

## Compatibility and migration strategy

1. Freeze the current behavior as language API v1 and protocol v1 fixtures,
   including documented quirks. Do not silently reinterpret old scripts.
2. Add a pre-evaluation script directive such as `// vibe-api: 2` within the
   same early-header scan used by `vibe-profile`, plus CLI
   `--language-version 2`. Absence means v1 during the transition.
3. Generate a deprecation/alias table from the manifest. Warnings include the
   replacement and exact source span and are deduplicated per reload.
4. Ship `vibe migrate --check FILE` and `vibe migrate FILE` using manifest
   rewrites for renamed methods, channel conversion, time-unit names, and
   terminal insertion. Never rewrite ambiguous lifecycle cases automatically.
5. Keep effective v1 aliases for at least two minor release trains after v2 is
   default. An alias must forward to working behavior; no silent stubs.
6. Version REST paths and WS hello/capabilities. Editors negotiate and refuse a
   server whose major protocol they cannot handle.
7. Publish a generated compatibility report per release: added, deprecated,
   removed, behavior-changed, availability-changed, and schema-changed entries.

Main migration risk is timing/audio behavior, not syntax. Golden tests must
compare desired ScriptState, reconciliation messages, route maps, and rendered
event timing for representative v1/v2 scripts.

## Publication and generation order

1. Publish this lifecycle/availability model and the generated current indexes.
2. Emit and validate the core Rhai registration manifest; switch Markdown,
   website, LSP, and VS Code core metadata to it.
3. Add routing/state-sync matrices and runtime object pages from that manifest.
4. Generate DSP builder reference and UGen catalogue from the same build rules
   as [`build.rs`](../../crates/vibelang-dsp/build.rs#L432-L806).
5. Generate REST OpenAPI and typed WS reference from router/DTO/event types.
6. Generate the stdlib definition/function catalogue with a real Rhai parser or
   explicit export metadata, preserving module paths and duplicate names.
7. Generate CLI pages from Clap help snapshots, WASM types from wasm-bindgen,
   and editor command/setting/key tables from package/Elisp declarations.
8. Only then make API v2 default and begin the legacy-removal clock.

The checked-in [UGen](../reference/generated/ugens.md) and
[stdlib](../reference/generated/stdlib.md) indexes provide exhaustive discovery
today. Until the shared manifest exists, their counts and source linkage should
be verified in CI whenever manifests or stdlib `.vibe` files change.
