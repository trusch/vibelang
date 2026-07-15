# VibeLang API surface assessment

Status: current-state assessment at commit `f00c04ca1a1e79d644211eed64fc472214a75d58` (2026-07-15).

This report describes the public surface that a VibeLang user, script author, editor, or integration can actually encounter. It is an assessment, not a compatibility promise. Counts come from committed source and generated inventories at the commit above; no product or generated artifacts were changed while producing it.

## Executive assessment

VibeLang has a broad, unusually inspectable public surface. Its strongest recent improvement is the checked artifact chain rooted in `api/public-api-manifest-v1.json`: Rust/Rhai registrations, generated UGen functions, stdlib definitions, HTTP declarations, WASM types, and editor projections are now countable and largely reproducible. The repository also documents behavioral gaps more candidly than most projects at this maturity. Those are important assets for stabilizing the product.

The principal weakness is that declaration coverage is ahead of behavioral contract quality. The manifest can say that an overload exists, but several important boundaries still acknowledge fallbacks or potential panics; builder terminals do not share one lifecycle model; a hot reload that reports failure can already have changed the live graph; HTTP DTOs expose fields that handlers ignore; and WASM can return success after bridge or reload delivery fails. These are not merely documentation defects. They make it difficult for callers to determine what the system accepted and what state is live.

The editor and documentation surfaces also remain split between recently derived inventories and older handwritten consumer assumptions. The LSP and VS Code Rhai function projections are manifest-backed, but property APIs are absent from those projections, bundled UGens intentionally cover only part of the engine, and eight VS Code UGen completion labels do not name registered runtime functions. Several first-run commands and code examples in prominent Markdown files are stale.

Overall maturity is **strong in discovery and inventory, mixed in consistency, and weak at transactional and failure boundaries**. The next phase should use the new manifest foundation to make execution outcomes observable and consistent before adding more surface area.

## Scope, audience, and method

The assessed public surface includes:

- the `vibe` and `vibe-keys` command lines and their configuration inputs;
- the Rhai/Vibe authoring language, including core methods, properties, DSP UGens, stdlib modules, optional extensions, and MIDI;
- runtime-visible evaluation, builder, synchronization, routing, transport, and hot-reload behavior;
- the LSP, VS Code extension, Emacs mode, generated manifests, and the application code that consumes them;
- the HTTP, WebSocket, and WASM integration boundaries;
- examples, tutorials, reference documentation, root onboarding material, and earlier audits and roadmaps.

The inventory was derived from the exact committed candidate rather than the dirty primary checkout. Primary quantitative sources were `api/public-api-manifest-v1.json`, `api/http-api-snapshot-v1.json`, `crates/vibelang-dsp/ugen_manifests`, `crates/vibelang-std/stdlib`, `docs/reference/generated`, `vscode-extension/package.json`, and the checked projections under `crates/vibelang-lsp/src/data` and `vscode-extension`. Runtime semantics were checked against the handler, registration, builder, and reconciliation implementations, especially `crates/vibelang-core/src/runtime.rs`, `crates/vibelang-rhai/src`, `crates/vibelang-http/src`, and `crates/vibelang-wasm/src/lib.rs`.

Counts distinguish an **entry** (a public name/receiver/kind identity) from an **overload** (one callable signature). A stdlib declaration can be import-callable without being supported public API. UGen function counts include generated rate variants. Conditional counts describe build or runtime availability, not simultaneous availability in every binary. Static boundary classifications describe syntax and generated closure behavior; for example, “potential panic exposure” is not a claim that every such overload always panics.

The intended audiences and support classes are:

- **Public authoring:** supported core Rhai functions, methods, properties, operator overloads, and supported stdlib declarations used by song authors.
- **Advanced public authoring:** synthdef/UGen graph construction, explicit routing, extensions, MIDI/live control, and lower-level handles. These are callable public surfaces but demand more runtime and availability knowledge.
- **Integration public:** documented CLI, HTTP/WebSocket, WASM, LSP, and editor inputs and outputs. None currently carries a repository-wide stability/version label, so “public” means user-consumable rather than semver-stable.
- **Conditional or experimental:** native-only, feature- or plugin-gated entries, 48 documentation-only UGen builder entries, and 25 quarantined demand names. Their availability status is part of the manifest and must not be inferred from presence alone.
- **Internal-only:** non-exported Rust implementation and stdlib helpers not reachable through imports. A separate set of 112 stdlib functions is import-callable internal API: technically reachable through Rhai module imports but explicitly unsupported, so it is not equivalent to internal-only code or supported public API.

No Cargo, Node, server, audio-device, or generated-artifact command was run for this assessment. That constraint avoids mutating build output and means the report relies on committed artifacts plus read-only structural checks. Existing generated-artifact gates are assessed from their implementation in `xtask/src/public_artifacts.rs` and `scripts/public-artifacts.sh`, not re-executed here.

## Quantitative inventory

| Surface | Current inventory | Evidence |
|---|---:|---|
| Canonical public manifest | 3,626 entries; 8,431 overloads | `api/public-api-manifest-v1.json` (`stats`) |
| Core Rhai plus DSP Rhai plus optional extensions | 786 entries; 875 overloads | Manifest surfaces `rhai`, `dsp_rhai`, `rhai_extension` |
| Registered Rhai types | 34 | Manifest type entries and `registered_type_declarations` |
| Core/DSP/extension functions | 477 entries; 600 overloads | 178 free-function entries/248 overloads and 299 receiver-method entries/352 overloads |
| Symbolic operator functions | 5 entries; 15 overloads | `+`, `-`, `*`, `/` for `NodeRef` graphs and mixed floats; `..` for mixed integer/float ranges |
| Rhai properties | 275 entries | 141 getters and 134 setters |
| Registration declarations | 638 | Manifest `registration_declarations` |
| Effective Rhai functions | 6,837 | Manifest `effective_rhai_functions`, including generated UGen registrations |
| Canonical UGen records | 875 production records across 70 JSON files | `crates/vibelang-dsp/ugen_manifests` |
| Public callable UGen surface | 1,174 entries; 5,962 overloads | Manifest surface `dsp_ugen` |
| UGen builders and quarantined demand names | 48 builder models; 25 quarantined names | Manifest UGen details and availability |
| Standard library | 829 `.vibe` files; 890 declarations; 887 distinct definition names | `crates/vibelang-std/stdlib` and manifest stats |
| Supported stdlib declarations | 595 public; 112 import-callable internal | Manifest stdlib support classification |
| HTTP | 96 method/path registrations; 75 serialized/deserialized type records | `api/http-api-snapshot-v1.json` |
| HTTP methods | 27 GET, 49 POST, 7 PATCH, 4 PUT, 9 DELETE | HTTP snapshot routes |
| Feature-gated HTTP routes | 18 MIDI; 4 native recording | HTTP snapshot availability |
| VS Code Rhai function projection | 600 overload rows; 354 distinct names; 33 receiver types | `vscode-extension/src/data/rhai-api.json` |
| LSP Rhai function projection | Same 600 overload rows | `crates/vibelang-lsp/src/data/rhai-api.json` |
| VS Code bundled UGens | 316 runtime-callable records in 24 category files; 478 declared completion labels | `vscode-extension/ugen_manifests` |
| Editor UGen coverage | 470 of those 478 labels match registered names; 8 are stale | Bundle-to-manifest comparison described below |
| VS Code contributions | 32 commands, 8 keybindings, 39 settings, 2 views | `vscode-extension/package.json` |
| Emacs surface | 31 user options, 56 interactive commands, 7 snippets | `emacs/vibelang-mode.el` and `emacs/vibelang-snippets` |
| Examples | 60 `.vibe` programs; 199 imports across 53 importing files | `examples` |
| Tutorial examples | 23 of the 60 programs | `examples/tutorials` |
| Generated reference | 103 CLI-help lines, 186 HTTP-reference lines, 1,619 stdlib-reference lines, 1,749 UGen-reference lines | `docs/reference/generated` |

The top-level manifest availability split is 1,617 available entries/6,353 overloads, 343 conditional entries/484 overloads, 1,593 importable stdlib entries, 48 documentation-only entries, and 25 quarantined entries. Important conditional groups are 49 native non-WASM entries, 45 extension entries, 237 MIDI entries, and 12 `mi-UGens` plugin entries with 156 overloads.

The boundary inventory classifies all 8,431 overloads: 6,019 record coercions, 6,119 casts, 121 clamps, 337 explicit ranges, 5,036 fallbacks, 190 structured-error paths, and 6,071 potential panic exposures. The generated UGen layer dominates those totals: all 5,962 UGen overloads are classified as potentially panic-exposed and 4,788 contain fallback behavior. The remaining potential panic exposure is 67 of 147 DSP-Rhai overloads, 2 of 683 core-Rhai overloads, 2 of 45 extension overloads, and 38 of 1,594 stdlib overloads. This is a useful risk map, not a runtime failure-frequency measurement.

## Surface map and ownership

The public contract is not owned by one layer:

| Consumer | Declaration source | Execution owner | User-visible projection |
|---|---|---|---|
| Vibe script | Rhai registration code and stdlib modules | `vibelang-rhai`, `vibelang-core`, `vibelang-dsp` | generated API, UGen, and stdlib references |
| CLI user | Clap declarations | `vibelang-cli` and runtime | generated CLI help plus handwritten CLI docs |
| HTTP client | Axum routes and Serde DTOs | `vibelang-http`, command channel, runtime | generated route/type reference and handwritten semantics |
| WebSocket client | event structs and polling task | `vibelang-http` | protocol notes in `docs/interfaces/http-and-websocket.md` |
| Browser/JS caller | `wasm_bindgen` exports | `vibelang-wasm` and JS bridge | generated `crates/vibelang-wasm/types/index.d.ts` plus handwritten WASM docs |
| Editor user | manifest projections, UGen JSON, editor code | LSP/VS Code/Emacs implementations | completion, diagnostics, hover, commands, snippets |

The generated artifacts provide a strong declaration spine, but behavioral ownership still crosses queues and subsystems. A request can be syntactically accepted in Rhai or HTTP, queued by one layer, partially applied by the runtime, and observed through a differently shaped WebSocket or UI state. That cross-layer path is where most high-severity findings occur.

## What is working well

### A reproducible declaration spine

`api/public-api-manifest-v1.json` is a substantial improvement over a handwritten list. It covers registrations, receivers, overload parameters, availability, lifecycle role, source provenance, and boundary metadata. `xtask/src/public_api.rs` derives core and DSP Rhai registrations, generated UGen functions, and stdlib definitions. `xtask/src/public_artifacts.rs` then checks projections, documented consumers, example imports, emitter contracts, HTTP declarations, and WASM types. `scripts/public-artifacts.sh` exposes generation and check workflows. Together these make drift visible and provide a practical basis for compatibility review.

The manifest also avoids pretending that everything found in a source file is equally supported. It distinguishes supported stdlib definitions from 112 import-callable internal functions, identifies 48 documentation-only UGen builders and 25 quarantined demand names, records feature conditions, and models by-value receivers. That classification is more useful than a raw symbol count.

### Broad, expressive authoring coverage

The authoring surface spans global time and tempo, note/chord/scale helpers, groups and voices, audio/control/trigger routing, patterns, melodies, sequences, fades, effects, samples, buffers, SFZ, recording, MIDI, and lower-level synthdef construction. The 34 registered types and 875 non-generated Rhai overloads give users both a high-level musical vocabulary and graph-level escape hatches. Fifteen overloads implement the five symbolic names: `NodeRef` graph arithmetic supports `+`, `-`, `*`, and `/` for graph/graph and both graph/float directions, while `..` provides three mixed numeric range forms; `docs/reference/dsp.md` documents the graph operators. The 1,174 callable UGen identities expose an extensive DSP vocabulary, while the 887 distinct stdlib definition names reduce the need for every script to build graphs from primitives.

Routing is especially capable. The runtime supports additive audio fan-out, main-output replacement and mute operations, single-source inputs, multi-source control fan-in, separate set and bend control routes, A2K conversion, trigger routes, and parameter modulation. Stable identifiers and reconciliation allow scripts to preserve compatible runtime entities across reloads rather than replacing the entire graph.

### Candid interface documentation

The newer interface documents are evidence-oriented. `docs/reference/README.md`, `docs/reference/runtime-model.md`, `docs/reference/runtime-objects.md`, `docs/interfaces/http-and-websocket.md`, `docs/interfaces/lsp-and-editors.md`, `docs/interfaces/cli-and-config.md`, and `docs/interfaces/wasm.md` distinguish declarations from effective behavior and explicitly identify ignored fields, deferred operations, feature gates, and current limitations. Generated references under `docs/reference/generated` anchor route, UGen, stdlib, and CLI declaration counts.

That candor makes the remaining work actionable. For example, the HTTP document does not describe ignored DTO fields as implemented features, and the runtime document explains that a failed reload is not fully transactional. This report reconciles and prioritizes those facts rather than rediscovering them as hidden behavior.

### Useful artifact and consumer checks

The checked pipeline compares LSP and VS Code Rhai data to the public manifest, checks editor source and packaged JavaScript emitter inventories for parity, validates dynamically emitted Sound Designer UGen signatures against manifest argument types and coercions, checks example imports and a fictional-call denylist, and regenerates HTTP and WASM declarations. The VS Code `.vscodeignore` deliberately retains source data required by its runtime loader while packaging compiled code. These controls materially reduce accidental API invention by tools.

### Deterministic reload staging foundations

The runtime stages sample and SFZ work away from the real-time tick, preserves queued arrival ordering, uses backend synchronization barriers, and reconciles entities by stable identity. `crates/vibelang-core/src/runtime.rs` applies changes in an explicit phase order and compares named routes and ports. Even though atomic failure behavior needs work, this is a solid base for a revisioned transaction model.

## CLI and configuration surface

### Current contract

`vibe` accepts a shorthand script file or the explicit `run`, `render`, `devices`, and `lsp` subcommands. `run` exposes 21 non-help options covering watch/API behavior, include paths, scsynth connection and boot behavior, JACK linking, devices and channels, sample rate, startup profile, optional extensions, and the filesystem sandbox. Important defaults are watch enabled, HTTP enabled, API bind `127.0.0.1`, API port `1606`, and extensions enabled when compiled. `/eval` is more restrictive: extension calls remain disabled unless `--api-allow-extensions` is set. `render` exposes format, sample rate, and bit depth.

The startup profile mechanism is a positive operational feature. `crates/vibelang-cli/src/startup_profile.rs` validates required services and links, emits WAITING/FAILED state, and can withhold script evaluation and transport until requirements are satisfied. The startup path boots or connects scsynth, loads builtins, optionally starts HTTP, evaluates the script, synchronizes synthdefs, applies the snapshot, synchronizes again, and starts transport. This ordering should become part of a machine-readable readiness contract.

The watcher recursively observes the script's parent but filters to `.vibe` files and debounces by 100 ms. A watched parse/evaluation failure is logged while the previous snapshot remains selected. Apply failures are also logged; as discussed below, the live backend may already be partially changed.

`vibe-keys` is a separate three-subcommand interface (`init`, `config-path`, `list-ports`) with global config path, keyboard-layout, MIDI client, octave, channel, and velocity options. It also exposes TOML keyboard, MIDI, and theme sections. The CLI clamps octave, channel, and velocity overrides.

### Findings

**Good — generated declaration coverage.** `docs/reference/generated/cli-help.txt` is tied to Clap declarations, and `docs/interfaces/cli-and-config.md` explains feature-sensitive behavior and defaults.

**Medium, documentation defect — first-run commands disagree with the binary.** `examples/tutorials/README.md` uses `vibelang run -w`, but the binary is `vibe` and watch is already the default; `crates/vibelang-keys/README.md` recommends the nonexistent `--midi-input`; `emacs/README.md` and `emacs/vibelang-lsp.el` default to `vibelang lsp` rather than `vibe lsp`; and VS Code welcome/boot strings contain stale `vibelang`, `-w`, or `--api` forms. These are prominent onboarding failures, not obscure archival text.

**Medium, design debt — readiness is human-readable.** Startup profiles expose meaningful states, but integrations have no stable status schema or process-ready event carrying active script revision, backend state, enabled capabilities, and HTTP address. Consumers must scrape output, poll another surface, or assume that process startup means musical readiness.

**Low, consistency debt — configuration is split.** Runtime options are command-line-only while `vibe-keys` has TOML. Feature-conditioned flags and extension allow/deny semantics require users to understand compile-time and runtime policy simultaneously. A capability dump would make the effective configuration observable without necessarily introducing another configuration format.

## Rhai/Vibe authoring surface

### Names, types, and lifecycle families

The core authoring layer has 34 public types, 477 function identities, 600 callable overloads, and 275 properties. The editor-oriented function projection divides those 600 overloads into 248 free-function and 352 receiver-method overloads. Important families include `GroupRef`, `Voice`, `PatternBuilder`, `MelodyBuilder`, `SequenceBuilder`, `FadeBuilder`, `FxBuilder`, `SampleRef`, `BufferRef`, `SfzRef`, routing builders, synthdef builders, and MIDI types.

The lifecycle metadata shows that these families do not yet form one compositional model. There are 194 nonterminal builder/handle entries and 16 named terminal entries, alongside 257 `call`/`call_result` entries and 10 call/named terminals. Properties contribute another 275 getter/setter entries, graph construction contributes 1,174 generated UGen functions, and stdlib contributes module-scoped definitions and script functions.

Groups mutate an existing runtime-facing object. A named `Voice` terminal synchronizes automatically; an anonymous voice commonly requires `.apply()`/`.run()` or resolution through another builder. Pattern and melody `.start()` calls store named objects and mark them playing. Sequence synchronization currently handles melody clips but not pattern clips or nested sequences. Fade `.apply()` is a literal no-op, while `.now()` and `.start()` share immediate behavior. Recording starts immediately rather than when the builder is copied, and `.stop()`/`.cancel()` only log state while the returned pending sample is absent. `SampleRef` mutators synchronize on every change; buffer and SFZ constructors insert immediately. DSP synthdef `.body()`/`.body_map()` are finalizing terminals.

### Findings

**High, design debt — terminal semantics are inconsistent and sometimes misleading.** Method names such as `.start()`, `.apply()`, `.run()`, `.stop()`, and `.cancel()` imply a common action model but vary among immediate mutation, snapshot registration, deferred reconcile, no-op, and logging-only behavior. This raises the cost of learning every domain and makes generic editor guidance unreliable. Evidence is distributed across builder registration modules under `crates/vibelang-rhai/src` and the synchronization paths in `crates/vibelang-core/src/runtime.rs`; the current behavior is summarized accurately in `docs/reference/runtime-objects.md`.

**High, verified behavior — some accepted composition does not become live state.** A sequence can accept different clip builder types, but only melody clips synchronize through the current runtime path. Pattern clips and nested sequences do not receive equivalent live synchronization. `GETTING_STARTED.md` describes a broader behavior, so the mismatch is both behavioral and documentary.

**Medium, correctness debt — permissive fallbacks hide malformed musical input.** Pattern bar syntax assumes four beats; `.len()` can be overridden by later step configuration; negative Euclidean integers cross unsigned conversions; melody parsing can drop or default malformed tokens; integer array notes wrap through `u8`; non-numeric entries can be dropped; and unknown chord suffixes fall back to major. These choices favor uninterrupted live coding but give scripts no consistent strict mode or diagnostics channel. The boundary inventory's 5,036 fallback classifications quantifies how pervasive this design tendency is.

**Medium, consistency defect — units and ranges vary across neighboring APIs.** Voice MIDI channels use 0–15 while other MIDI builders expose 1–16. HTTP and editor velocity representations also differ. Bars, beats, seconds, MIDI values, amplitudes, and decibels are usually plain numbers, leaving range and unit knowledge in prose or handler-specific clamps. Explicit metadata exists for only 337 ranges and 121 clamps across 8,431 overloads.

**Medium, design debt — source-order and namespace behavior leaks through the stdlib.** There are 890 declaration occurrences but 887 distinct names. `lfo_random`, `lfo_saw`, and `lfo_sine` each have duplicate synthdef declarations, and `arpeggio_up_down` has two script-function declarations. Function namespaces can disambiguate module functions, but DSP registry duplicates remain source-order-sensitive. The generated stdlib reference acknowledges this; users still need a deterministic conflict policy and diagnostics.

**Low, intentional limitation — unavailable UGen domains remain visible.** Twenty-five demand-rate names are quarantined and 48 builder models are documentation-only. Twelve `mi-UGens` records require a plugin. This is reasonably classified in the manifest, but authoring tools need to show availability at the point of completion rather than merely omitting or exposing names differently.

## Runtime, routing, and hot reload

### Current behavior

Each evaluation uses a fresh thread-local context, clears registries, and configures expression and call-depth limits of 4,096. Imports resolve from the script base, include paths, and extracted stdlib on native builds, with an in-memory alternative on WASM. A failed initial evaluation aborts startup; a failed watched evaluation leaves the selected prior script state in place.

Stable IDs use deterministic hashing with collision probing, and named entities are reconciled against the previous snapshot. Sample and SFZ loading is staged off the runtime tick, then committed in arrival order with backend synchronization. Reload applies transport and entity changes in explicit phases before routes and parameter modulation are finalized.

### Findings

**High, verified behavior — failed reload is not atomic.** `apply_reload_inner` in `crates/vibelang-core/src/runtime.rs` can perform transport changes, deletions, creations, and updates before later output, input, or parameter-route finalization returns an error. The script snapshot is withheld on failure, but backend changes already made are not rolled back. The user can therefore see an error while hearing or querying a graph that is neither the prior snapshot nor the rejected snapshot. This is the highest-leverage correctness issue because all authoring and `/eval` workflows depend on reload.

**High, observability debt — callers cannot identify the applied revision.** Evaluation success, queue acceptance, backend application, and transport readiness are distinct moments, but there is no common revision identifier or terminal result object across CLI logs, HTTP responses, WebSocket events, and WASM results. Entity-level failures are often logged and skipped while processing continues. Consumers cannot reliably distinguish fully applied, partially applied, superseded, and rejected state.

**Medium, design debt — routing rules are capable but implicit.** Audio `.to(group)` is additive, `.to_main()` and mute replace outputs, input routing is single-source, control routing can fan in, and set/bend maps can conflict. Defaults often select the first one or two audio-rate ports. These behaviors are powerful, but port cardinality, replacement versus addition, and conflict resolution should be structured contract metadata available to tools and validators.

**Medium, reliability debt — live-coding tolerance obscures entity failures.** Several application paths log an individual error and proceed. That is defensible for performance continuity, but the system lacks a consolidated reload receipt describing each entity and route outcome. A successful top-level evaluation therefore does not mean the requested musical state exists.

**Low, performance-contract debt — latency claims are not measured contracts.** Root documentation advertises approximately 1 ms hot reload, while runtime comments and multi-stage reconciliation imply a larger and workload-dependent end-to-end path. No committed benchmark definition ties the marketing number to parse-only, queueing, reconciliation, scsynth sync, or audible application.

## HTTP and WebSocket surface

### Current declaration and execution model

The HTTP snapshot records 96 routes: 27 GET, 49 POST, 7 PATCH, 4 PUT, and 9 DELETE, plus 75 type records. Eighteen routes are MIDI-gated and four are native-recording-only. The surface covers health/transport, voices, groups, patterns, melodies, sequences, fades, effects, samples, synthdefs, routing, parameters, recording, MIDI, and script evaluation.

Handlers generally enqueue runtime commands and often read state immediately afterward. Some create paths sleep for 10 or 50 ms in the hope that state has updated. The API uses permissive CORS and has no authentication, authorization, CSRF/origin policy, rate limiting, or request-body policy. The CLI's loopback default limits the default exposure, but `--api-bind` can make it network-visible.

WebSocket clients receive a protocol-version-1 hello/capability message and generic `WebSocketEvent` objects with `type`, `timestamp`, and untyped `data`. Subscriptions support exact names, prefixes, and `*`. A 20 Hz polling task publishes playback and transport events through a 1,024-entry broadcast channel. Lag terminates the sender task without a resynchronization token; invalid client frames are silently ignored.

### Findings

**High, verified contract defect — declared request fields are accepted but ignored.** `docs/interfaces/http-and-websocket.md` traces the current handlers in detail. Examples include ignored transport quantization; ignored `ParamSet.fade_beats`; ignored voice create `gain`, `sample`, and `sfz`; ignored voice update `synth`, `polyphony`, and `gain`; ignored pattern and melody source strings and start quantization; ignored sequence `once` and update fields; ignored MIDI-record quantization; and empty synthdef source/parameter results. Some top-level DTOs exist without corresponding creation routes. A 2xx response to these shapes overstates implementation.

**High, consistency defect — mutation responses can be stale.** Queue insertion and immediate state reads are not a transaction. Fixed sleeps are not correctness boundaries. A create response can race runtime application, and later entity failure may exist only in logs. This is the HTTP manifestation of the missing revision/receipt model.

**High, security risk when non-loopback — network exposure has no control plane.** Open CORS plus mutation and `/eval` endpoints is acceptable only under a clearly enforced trusted-local assumption. The CLI permits a non-loopback bind without requiring authentication or an explicit insecure acknowledgement. `/eval` correctly blocks extensions by default, but core graph, transport, and script mutation still deserve protection. This is medium under the default loopback configuration and high when publicly bound.

**Medium, protocol debt — declarations are discoverable but not versioned schemas.** There is no OpenAPI/JSON Schema contract, versioned URL strategy, idempotency/revision field, or uniform error envelope. The HTTP type inventory itself contains two `ErrorResponse` records with different source shapes. REST state and WebSocket event payloads do not share typed models.

**Medium, reliability defect — WebSocket loss is terminal and silent at the model level.** On broadcast lag, the sender ends rather than reporting a sequence gap and snapshot cursor. Untyped `data` makes compatibility dependent on prose and client assumptions. Invalid client messages should receive a structured error instead of disappearing.

**Low, intentional limitation — polling bounds event freshness.** Playback updates are sampled at 20 Hz rather than emitted from a revisioned event log. That may be sufficient for UI display, but it is not a reliable automation or synchronization protocol and should be described as telemetry.

## WASM and JavaScript surface

### Current contract

The generated `crates/vibelang-wasm/types/index.d.ts` exposes two classes. `VibelangRuntime` has eleven lifecycle, execution, query, and helper methods: async `init` and `execute`, `tick`, `start`, `stop`, `stopAll`, `getSystemSynthdefs`, `isInitialized`, `parseNote`, `dbToAmp`, and `ampToDb`. The legacy `VibelangEngine` has seven corresponding methods: synchronous `execute`, synthdef getters/clear, and the three conversion helpers. Both also expose generated disposal/free behavior. Module exports include log/version helpers and synchronous/asynchronous initialization shims, plus result, compiled-synthdef, error, bridge, and init-output types.

The browser surface intentionally excludes native SFZ/recording, hardware MIDI, and optional native extensions. The `wasm_bindgen` start hook is correctly excluded from the callable public-function inventory as module lifecycle glue.

### Findings

**High, verified contract defect — `execute` success can mean delivery failure.** `crates/vibelang-wasm/src/lib.rs` can return a successful parse/evaluation result while synthdef bridge loading or reload-command delivery has failed; those failures are sent to `console.warn`. `start` and `stop` before initialization similarly warn or no-op while returning success. JavaScript callers cannot make a correct state transition from the returned value.

**High, portability defect — bridge discovery and invocation use different globals.** Discovery probes `globalThis.vibelangBridge`, but imported calls target `window.vibelangBridge`. A worker or non-window JavaScript environment can appear bridge-capable and then silently fail or no-op. The public types do not express the required host environment.

**Medium, compatibility debt — two public classes overlap without a stated lifecycle.** `VibelangEngine` remains a legacy synchronous surface beside `VibelangRuntime`. There is no deprecation version, removal policy, or behavioral matrix explaining when results differ. Supporting both can be reasonable, but the compatibility cost should be explicit.

**Medium, consumer drift — multiple type/glue locations obscure the canonical package.** The canonical generated declaration is under `crates/vibelang-wasm/types`, while the landing page imports the `vibelang-wasm` package in `landing-page/src/audio/vibelang-loader.js` and also contains older local generated-looking glue/declarations under `landing-page/src/audio`. Those local files expose loose or historical shapes such as playback data. Even when they are not the active import, they make repository search and maintenance ambiguous.

**Good — generated callable types are checked.** The artifact code derives the annotated Rust exports and retains only the intended module-init compatibility shim. This is a strong base for making result semantics accurate rather than merely expanding the declaration file.

## LSP, VS Code, and Emacs tooling

### Manifest-backed coverage

The LSP and VS Code each carry a 600-row Rhai function inventory derived from the public manifest: 248 free-function overloads and 352 receiver-method overloads, covering 354 names and 33 receiver types. The projections intentionally contain functions, not the 275 property getter/setter entries. The VS Code stdlib projection contains 890 declaration occurrences for 887 distinct names.

The LSP advertises completion, hover, signature help, definitions, references, rename, document/workspace symbols, code actions, inlay hints, folding, document links, formatting, diagnostics, and semantic tokens. VS Code adds 32 contributed commands, 39 settings, views, playback controls, sound-design emitters, and HTTP clients. Emacs provides a substantial native mode with 31 options, 56 interactive functions, and 7 snippets.

### Findings

**Medium, verified defect — eight VS Code UGen completions call nonexistent names.** `vscode-extension/src/features/completion.ts` inserts the `functions` strings from the 24 bundled JSON files. Among 478 runtime-callable labels, 470 match manifest function entries and eight do not: `a2_k_kr`, `k2_a_ar`, `lag2_ud_ar`, `lag2_ud_kr`, `lag3_ud_ar`, `lag3_ud_kr`, `t2_a_ar`, and `t2_k_kr`. Runtime names are `a2k_kr`, `k2a_ar`, `lag2ud_ar`, `lag2ud_kr`, `lag3ud_ar`, `lag3ud_kr`, `t2a_ar`, and `t2k_kr`. `validate_dynamic_editor_ugens` in `xtask/src/public_artifacts.rs` validates a name re-derived from the UGen class and rate, not the JSON `functions` value consumed by completion, so the current gate does not catch this drift.

**Medium, protocol defect — semantic-token legend and emitted indices diverge.** `crates/vibelang-lsp/src/features/semantic_tokens.rs` defines its own token ordering and modifier bits, while the server capability advertises a different legend. A conforming client interprets numeric token types against the advertised legend, so valid server output can be colored as the wrong semantic category.

**Medium, consistency defect — push and pull diagnostics differ.** Push diagnostics include core validation and unknown-name checks that the pull-diagnostic path omits. Clients choosing different LSP diagnostic modes receive different assessments of the same document.

**Medium, coverage debt — editor data is derived only in selected domains.** Rhai function signatures are manifest-backed, but properties are absent. Parameter completion still uses a hardcoded set of 16 common parameters. Stdlib/import discovery retains regex and shallow-path assumptions. The bundled UGen set covers 24 of 70 production categories by intentional packaging policy; 704 of the 1,174 public UGen function identities are therefore outside that bundle. This can be a deliberate curated experience, but it needs an explicit completeness mode or visible coverage statement.

**Medium, consumer defect — VS Code settings and HTTP requests overpromise control.** The extension has hardcoded localhost/port and polling assumptions in paths that do not consistently honor its 39 settings. Several toggles are not forwarded to the CLI. The UI sends HTTP fields that the server ignores, inherits the stale mutation-response behavior, and uses a note velocity representation inconsistent with the REST model. Text in welcome, boot, and Sound Designer flows includes obsolete commands or flags.

**Medium, onboarding defect — Emacs starts the wrong executable.** `emacs/vibelang-lsp.el` and `emacs/README.md` default to `vibelang lsp`; the actual CLI command is `vibe lsp`. This prevents the default integration from starting even though the mode exposes substantial functionality.

**Good — source/package emitter parity is checked.** `validate_vscode_emitter_contracts` structurally compares active TypeScript emitters with packaged JavaScript and checks static calls against the public manifest. This is exactly the right control; it should be extended to completion labels and other runtime-consumed data.

## Examples and documentation

### Coverage

The repository contains 60 example programs, including 23 tutorials. Fifty-three examples contain 199 imports. Authoring-family occurrence by file is broad: voice in 54 files, group in 50, melody in 36, pattern in 19, synthdef in 17, sequence in 16, modulation in 15, sample in 12, output routing in 16, input routing in 12, parameter routing in 10, fade in 5, MIDI device use in 2, SFZ in 1, and effect definition in 1. The artifact checker validates imports and rejects a bounded list of fictional calls in all example `.vibe` files.

The four generated reference files provide useful quantitative anchors. The root and interface documentation then explain higher-level concepts. This is strong breadth, especially for musical domains that are difficult to convey through signatures alone.

### Findings

**High, onboarding documentation defect — prominent examples contain nonexistent or inaccurate API.** `GETTING_STARTED.md` uses `get_group("Drums")` and `drums.solo(true)` instead of the registered `group("Drums")` and `.solo()` shape. It describes `.start()` as immediate and claims sequence `.clip()` registration beyond current pattern/nested-sequence synchronization. `crates/vibelang-std/stdlib/effects/README.md` uses nonexistent `group.add_effect`. These examples sit close to the first-run path and should be held to the same manifest checks as editor emitters.

**Medium, setup documentation defect — flags and extension policy are stale.** `examples/tutorials/README.md`, the extension tutorial, editor READMEs, and `crates/vibelang-keys/README.md` contain obsolete executable names, `-w`, `--api`, `--midi-input`, or opt-in extension flags. The current CLI enables compiled extensions by default and offers disabling flags plus the `/eval` allow switch.

**Medium, roadmap drift — the audit narrative lags the new manifest work.** `docs/roadmap/api-improvement-roadmap.md` still presents export metadata, boundary coverage, artifact reproducibility, and editor derivation as missing or open P0 work even though the candidate includes manifest entries, all-overload boundary classifications, projections, and artifact gates. `docs/README.md` similarly refers to an earlier 357-name inventory and a future boundary matrix. The roadmap remains valuable for unresolved design choices, but its completion state no longer describes the current repository.

**Medium, documentation-policy debt — generated references and handwritten code blocks have different gates.** The repository checks generated references, editor consumers, example imports, and a fictional-call denylist, but it does not structurally validate every public Markdown Vibe/CLI code block. As a result, generated material can be current while onboarding text regresses.

**Low, accuracy debt — scale and performance statements have aged.** The root README says there are 20 hands-on tutorials although 23 tutorial `.vibe` files exist; its category counts lag the 141 effect declarations; an SFZ NOTE_OFF limitation appears stale relative to current core handling; and the approximately 1 ms hot-reload statement lacks a defined measurement. “880+” stdlib definitions remains directionally accurate against 887 distinct names.

## Risk register

Severity reflects user impact and breadth, not implementation effort. “Verified defect/behavior” is directly evident in current code or data; “design debt” is internally consistent behavior with an unsafe or confusing contract; “documentation defect” is a public claim that disagrees with implementation; “intentional limitation” is deliberately classified but still affects usability.

| ID | Severity | Type | Finding | Affected surfaces |
|---|---|---|---|---|
| R1 | High | Verified behavior | A reload can mutate live runtime state before a later phase fails; no rollback restores the prior graph. | Rhai, CLI watch, `/eval`, WASM |
| R2 | High | Contract/observability debt | No shared applied revision or receipt distinguishes evaluated, queued, partially applied, applied, and rejected work. | Runtime, CLI, HTTP, WS, WASM |
| R3 | High | Verified contract defect | HTTP accepts numerous DTO fields that handlers ignore and can return stale mutation state. | HTTP, VS Code UI |
| R4 | High | Verified contract defect | WASM reports success when bridge loading or reload delivery fails. | WASM, browser consumers |
| R5 | High when exposed | Security risk | Mutating HTTP and `/eval` endpoints have open CORS and no authentication or request controls. | HTTP, non-loopback CLI |
| R6 | High | Design debt | Builder terminal words do not imply consistent registration, synchronization, or cancellation semantics. | Rhai, tooling, docs |
| R7 | High | Documentation defect | First-run code claims nonexistent calls and unsupported sequence behavior. | Onboarding |
| R8 | Medium | Boundary risk | 6,071 overloads are statically classified as potentially panic-exposed; 5,036 have fallback behavior. | Rhai/UGen/stdlib/extensions |
| R9 | Medium | Verified tooling defect | Eight bundled VS Code UGen completion labels are not registered functions, and the artifact gate checks different derived names. | VS Code |
| R10 | Medium | Protocol defect | LSP semantic-token legend/output and push/pull diagnostics disagree. | LSP clients |
| R11 | Medium | Coverage debt | Properties, many UGen categories, and complete parameter/import knowledge are not manifest-backed in tools. | LSP, VS Code |
| R12 | Medium | Protocol debt | HTTP and WS have unversioned, differently shaped, partly untyped contracts; WS lag has no resync. | Integrations |
| R13 | Medium | Consistency debt | Units, MIDI ranges, permissive parsing, and stdlib collision policy vary by domain. | Rhai, HTTP, editors |
| R14 | Medium | Consumer/docs defect | Editor commands, settings, polling, flags, and HTTP assumptions diverge from runtime behavior. | VS Code, Emacs |
| R15 | Low | Intentional limitation | Conditional, plugin, quarantined, and editor-curated availability is not consistently visible at use sites. | Authoring/tooling |

There is no current basis for a “Critical” classification: the default HTTP bind is loopback, known defects do not by themselves establish arbitrary code execution outside explicitly exposed script evaluation, and the assessment did not exercise production deployment. R5 should be reclassified Critical for any supported untrusted-network deployment that exposes `/eval` without an external security boundary.

## Reconciliation with existing audits and roadmap

The earlier audit and roadmap correctly identified that VibeLang lacked a single export model, systematic boundary review, reproducible public artifacts, editor-consumer validation, and explicit lifecycle decisions. At the assessed commit, several of those gaps are now materially addressed:

- the public manifest exists and counts 3,626 entries/8,431 overloads;
- all overloads carry boundary classifications from one of the recognized source modes;
- CLI help, HTTP inventory, WASM declarations, UGen and stdlib references, editor Rhai data, and selected UGen projections are committed and checked;
- editor emitter source/package parity and dynamic UGen signatures are validated;
- demand UGens, builder-only models, internal stdlib functions, and conditional surfaces have explicit availability classifications;
- receiver modeling and generated property boundaries have been improved in the commits leading to this candidate.

Accordingly, roadmap items about creating the inventory and basic artifact gates should be marked complete rather than carried as active P0s. The accepted lifecycle ADR and roadmap analysis remain relevant, but the next work is adoption: unify actual terminal behavior, transaction outcomes, and consumer semantics around those decisions. The boundary matrix is no longer “future”; it is present, but its risk classes need targeted runtime tests and remediation budgets.

This report also adds current findings not fully represented in the prior material: the eight completion-label mismatches that evade the current gate; the quantitative split between manifest-backed and curated editor UGen coverage; the exact prominence of stale onboarding commands; and the need to treat revision receipts as the shared dependency for runtime, HTTP, WS, WASM, and editor correctness.

## Prioritized recommendations

These recommendations intentionally do not prescribe implementation details. Each ends in a measurable acceptance condition so it can become a bounded ticket or release criterion.

### P0 — make execution outcomes truthful

1. **Define and enforce an atomic reload outcome contract.** Make R1 and R2 one program of work: every evaluation receives a revision ID and ends as rejected, superseded, fully applied, or explicitly partially applied; a reported rejection must leave the prior observable graph and transport state intact. Acceptance: failure injection at every apply phase proves either full rollback or a documented partial-result receipt, and CLI, HTTP, WS, and WASM expose the same revision and terminal status.

2. **Remove success-shaped no-ops and ignored fields.** For every HTTP DTO field, Rhai terminal, and WASM result member, either implement its documented effect, reject it as unsupported, or remove it in a versioned change. Start with sequence clip synchronization, fade/record terminals, HTTP mutation DTOs, and WASM bridge/reload delivery. Acceptance: a generated effectiveness inventory maps 100% of accepted fields and terminal methods to an implementation or structured unsupported error; tests demonstrate that success implies the requested state is observable.

3. **Turn boundary classifications into a remediation gate.** Prioritize the 6,071 potential panic exposures and 5,036 fallbacks by reachability and consequence rather than treating the totals as equivalent. Generated UGen closure unwraps, parsing fallbacks, and extension boundaries should have representative adversarial fixtures. Acceptance: no user-controlled invalid value can unwind across a public boundary; every retained fallback is documented in manifest metadata and has a diagnostic or explicit compatibility rationale; release reports show the count trend by surface.

4. **Make non-loopback HTTP an explicit secured mode.** Define the supported trust model, origin/authentication policy, body and rate limits, and `/eval` policy. Acceptance: non-loopback startup either requires configured protection or an explicit insecure acknowledgement; automated tests cover unauthorized mutation, cross-origin requests, oversized payloads, and extension restrictions; the hello/health surface reports the active policy without exposing secrets.

### P1 — converge authoring and integration contracts

5. **Adopt one lifecycle vocabulary across builders and references.** Apply the accepted lifecycle decision to group, voice, pattern, melody, sequence, fade, FX, sample/SFZ, record, and DSP builders. Acceptance: each public type is classified as value, builder, handle, or reference; terminal verbs have one defined effect category; unsupported composition is rejected before apply; all 194 nonterminal and 26 terminal-like manifest entries carry machine-readable lifecycle metadata used by docs and editors.

6. **Publish versioned HTTP and WebSocket schemas around revisioned state.** Derive schemas from the effective handler contract, not only DTO declarations. Acceptance: all 96 routes have operation IDs, typed success/error responses, availability, mutation revision semantics, and field-effect status; WS events have typed payloads and monotonic sequence/revision values; a lagging client can detect a gap and resynchronize from a documented snapshot.

7. **Close editor correctness gaps and measure coverage.** Validate the JSON `functions` labels that VS Code actually consumes, align semantic-token legend and output, equalize push/pull diagnostics, fix executable/flag defaults, and make settings drive actual clients. Acceptance: all 478 bundled completion labels resolve to available manifest entries, the eight current mismatches are zero, semantic-token conformance tests decode every emitted token against the advertised legend, diagnostic modes return the same rule set, and a generated coverage report states which of 1,174 UGen identities and 875 core overloads/properties each editor exposes.

8. **Expose effective capabilities and availability.** Provide one read-only capability model for compile features, extensions, native/WASM differences, plugins, MIDI, recording, quarantined APIs, HTTP policy, and editor coverage. Acceptance: CLI, HTTP hello/health, WASM, and editor clients can consume or render the same versioned capability identifiers; conditional completions are labeled or suppressed from that model rather than hardcoded.

9. **Normalize units, ranges, parsing, and collisions.** Decide public conventions for MIDI channel/velocity, beats/bars/time, amplitudes/decibels, strict versus forgiving musical parsers, and duplicate stdlib definitions. Acceptance: every numeric public parameter has unit/range metadata or an explicit unbounded marker; equivalent MIDI APIs use one representation; strict parsing returns structured locations; the four current duplicate definition names have deterministic, diagnosed resolution.

### P2 — sustain the contract

10. **Make public documentation executable against the manifest.** Extend code-block checks beyond generated references and `examples`. Acceptance: every non-archival Vibe and CLI block in root, `docs`, editor READMEs, crate READMEs, and tutorials is parsed/classified; `get_group`, `add_effect`, obsolete binaries/flags, and unsupported sequence claims are rejected; historical ticket material is clearly excluded. Update roadmap completion state and quantitative claims in the same documentation release.

11. **Clarify WASM compatibility and package ownership.** State the supported host globals and lifecycle for `VibelangRuntime` versus `VibelangEngine`, and identify the single canonical JS/type distribution. Acceptance: one compatibility table covers every public method and environment; bridge absence is a typed failure; legacy removal has a version/deprecation policy; active landing-page imports resolve to the canonical generated types with no competing generated-looking tracked surface.

12. **Define performance and readiness service levels.** Separate parse, evaluate, queue, reconcile, backend sync, and audible-apply latency. Acceptance: committed benchmark definitions and hardware/environment metadata replace the undefined approximately 1 ms claim; startup and reload receipts report readiness stages; release documentation publishes percentile results for representative small and large scripts.

13. **Control surface growth.** Require new public names, fields, routes, events, settings, and generated UGen variants to declare stability, availability, lifecycle, units, failure behavior, and consumers. Acceptance: pull-request checks report the manifest/schema diff, require an explicit compatibility classification for every addition/removal/change, and prevent growth in unclassified or documentation-only entries without an owner and exit criterion.

## Suggested release gates

A near-term stabilization release should not require eliminating all 8,431-overload risks. It should require these bounded outcomes:

- zero rejected reloads that silently leave an unreported partial graph;
- zero accepted HTTP/WASM fields or operations whose only failure signal is a log or console warning;
- zero non-loopback mutation endpoints without an explicit security policy;
- zero editor completion names that fail manifest resolution and one shared LSP diagnostic rule inventory;
- zero invalid first-run commands or unregistered calls in non-archival documentation;
- a compatibility diff for the manifest, HTTP/WS schemas, WASM declarations, CLI help, and editor contributions on every release.

These gates address truthfulness and observability before demanding aesthetic uniformity. They also reuse the repository's strongest existing asset—the generated declaration spine—rather than creating a parallel audit process.

## Conclusion

VibeLang already has more public API than its apparent size suggests: thousands of generated DSP and stdlib entries sit behind a compact live-coding language, while CLI, HTTP, WebSocket, WASM, and three editor integrations expose additional ways to create and observe state. The candidate commit makes that breadth visible and substantially reproducible.

The next risk is not missing inventory; it is a declaration being mistaken for an effective contract. The highest-value work is to make evaluation and mutation transactional or explicitly partial, give every caller a revisioned receipt, reject unsupported fields and terminals, and make editor/documentation consumers derive from what they actually execute. Once those foundations are in place, lifecycle normalization, typed integration schemas, capability discovery, and sustainable compatibility policy can turn the present broad surface into a dependable one.
