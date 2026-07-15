# API-unification implementation dependency and landing map

| Field | Value |
|---|---|
| Status | Normative companion to [the API-unification ADR](adr-effective-api-contract.md) |
| Assessed root | `e5a1198a3bb478418042f2b517172f74635742b7` |
| Scope | Future implementation tasks, dependency order, merge ownership, and measurable landing gates |
| Product changes in this document | None |

## Landing principles

Each task lands as a reviewable conventional commit or a small ordered commit
series on the accepted lineage. Every implementation task includes its unit or
integration tests. A task does not edit generated outputs unless it is the
named projection owner for that landing boundary. Shared high-contention paths
are serialized even when downstream domain work can proceed in parallel.

The graph deliberately separates four milestones:

1. the contract can describe current declarations without changing behavior;
2. current v1 behavior reports honest best-effort receipts;
3. v2 behavior becomes effective or structurally rejected;
4. consumers switch only after the relevant behavior and compatibility adapter
   are proven.

## Dependency graph

```text
M00 baseline freeze
  |
M01 schema + fragment validators
  |
M02 declaration composer + v1 equivalence
  |
M03 core IDs + receipt ledger + MutationContext
  |
M04 honest v1 best-effort instrumentation
  +----------------------+
  |                      |
M05 conventions +        M06 pure Candidate +
capability snapshot      Builder/Ref/Observation base
  |                      |
  +----------+-----------+
             |
M07 native generation activation + ResourceManager
             |
M08 core authoring families
             |
M09 routes, resources, recording, MIDI, external effects
             |
       +-----+------------------+
       |                        |
M10 HTTP v2 + WebSocket         M11 WASM v2
       +------------+-----------+
                    |
M12 editors + migration + docs + packages
                    |
M13 final integration and default-readiness gate
```

M05 and M06 may be developed in parallel after M04, but their shared contract
and projection refresh lands through one integration owner. M10 and M11 may be
developed in parallel after M09. All other arrows are hard landing dependencies.

## Task-by-task landing map

### M00 — freeze the v1 behavior and artifact baseline

**Depends on:** accepted architecture commit.

**Owned paths:** existing fixture/test directories, explicit baseline files
under `api/baselines/`, and no behavior source except minimal test seams.

**Deliverables:**

- byte snapshots and counts for the current v1 manifest, HTTP snapshot, Rhai
  and editor projections, WASM declarations, CLI help, docs references, and
  package inventories;
- golden v1 scripts for every authoring family, including current eager writes,
  no-ops, warnings, route order, resource IDs, and timing;
- negative fixtures for all currently known ignored fields, stale success,
  eight invalid UGen labels, semantic-token mismatch, push/pull diagnostic
  mismatch, stale commands, and WASM bridge false success;
- phase-indexed failure-injection seam inventory without changing outcomes.

**Landing gate:** generation/check of the existing toolchain is clean and the
baseline files are reproducible from the assessed lineage. Golden fixtures pin
behavior rather than endorsing it. No current defect is silently corrected in
this task.

**Merge boundary:** lands alone before schema or behavior work. Later tasks may
update a baseline only with a classified compatibility record.

### M01 — add schema-v2 and semantic-fragment types

**Depends on:** M00.

**Owned paths:** `crates/vibelang-api-manifest/**`, schema fixtures, and
fixture-only `api/contract/*.toml` examples.

**Deliverables:**

- schema-v2 records for identity, ownership, stability, lifecycle roles/effects,
  values, effectiveness, operations, receipts, failures, capabilities,
  consumers, coverage, and compatibility classes;
- versioned parsers for the six semantic fragments;
- stable-ID preservation and rename/alias rules;
- RFC 8785/SHA-256 and decimal-counter cross-language vectors;
- validation of missing facets, unknown enums, duplicate owners, orphan IDs,
  mechanical-fact restatement, and unclassified diff paths.

**Landing gate:** v2 schema round-trips deterministically; v1 parsing remains
unchanged; fixture mutations produce the expected error and compatibility
classes. No runtime or consumer changes.

**Merge boundary:** exclusive ownership of the manifest crate. M02 starts only
after this commit is reviewed.

### M02 — compose the declaration graph and prove v1 equivalence

**Depends on:** M01.

**Owned paths:** `xtask/src/public_api.rs`, `xtask/src/public_artifacts.rs`, new
`xtask` contract/diff modules, `api/contract/*.toml`, and generated artifacts as
the sole projection owner for this boundary.

**Deliverables:**

- discovery nodes for Rhai/UGen/stdlib, CLI, Axum/Serde, WebSocket, WASM, LSP,
  VS Code/Emacs, Markdown blocks, and package manifests;
- typed join of mechanical facts and semantic fragments;
- canonical v2 output, coverage, compatibility diff, package index, and
  deterministic double-generation;
- v1 manifest and HTTP snapshot projections from v2;
- migration-debt records for every current ignored/log-only/stale/dead binding,
  with owners and exit gates.

**Landing gate:** all 3,626 entries and 8,431 overloads are represented;
unchanged IDs match; v1 manifest and current projections are byte-identical;
all 96 routes, 75 types, and 297 fields are discovered in both directions;
generation twice is byte-identical and check mode leaves the tree clean.

**Merge boundary:** exclusive ownership of `xtask` and generated outputs. No
domain task refreshes projections independently after this point.

### M03 — add core identity, receipts, ledger, and transition validation

**Depends on:** M02.

**Owned paths:** new focused modules under `crates/vibelang-core`, plus the
minimal exports needed by adapters. Avoid broad runtime behavior edits here.

**Deliverables:**

- UUIDv7 attempt/epoch IDs, checked revision/event newtypes, digests, canonical
  receipt/status/component types, and wire adapters;
- transition-table validator, monotonic ledger, retention window, idempotency,
  expected-revision, cancellation, and gap-query primitives;
- `MutationContext` and reply/event sinks that internal messages can carry;
- one canonical error/diagnostic envelope and redaction hooks.

**Landing gate:** property tests cover monotonic concurrent allocation,
transition edges, terminal immutability, component partitioning, rejected versus
partial invariants, idempotency conflicts, expected-revision races,
cancellation races, retention/reset, and lossless JSON/Rhai/WASM-friendly wire
round trips.

**Merge boundary:** exclusive ownership of shared core contract types. No
transport defines a parallel receipt.

### M04 — instrument v1 as honest best effort

**Depends on:** M03.

**Owned paths:** `crates/vibelang-core/src/message.rs`, runtime dispatch/reload
paths, existing handler domains, then CLI/Rhai ingress adapters in an ordered
series. Generated output remains untouched.

**Deliverables:**

- `MutationContext` propagation through all 74 current message variants,
  internal staging/completion messages, sync, and logs;
- attempt allocation at CLI startup/watch/eval, Rhai host submission, current
  HTTP mutation ingress, and WASM execution;
- exact component results for every v1 in-place phase and handler failure;
- real result-bearing `sync_and_wait` and backend failure propagation;
- current queue admission exposed as accepted, never applied;
- any leaked/uncertain v1 effect reported as partial, with fencing when state is
  unknown.

**Landing gate:** every message variant is classified internal,
receipt-bearing, or receipt-linked; queue full/closed, handler failure, staging
failure, route failure, backend rejection, sync timeout, and acknowledgement
loss have distinct codes. All v1 golden behavior remains unchanged apart from
the additive truthful receipt/diagnostic projection.

**Merge boundary:** serialized commits in this order: message/context, runtime
ledger, domain handlers, CLI/Rhai adapters. Review after each boundary because
`runtime.rs` and `message.rs` are high-contention files.

### M05 — populate conventions and semantic capability snapshots

**Depends on:** M04.

**Owned paths:** manifest semantic fragments and focused core capability/value
modules; no authoring-family behavior change.

**Deliverables:**

- stable unit/range/parser/collision/diagnostic/capability registries;
- quantity applicability for 18,786 parameter occurrences and 5,089 UGen
  inputs, including explicit unbounded and not-applicable records;
- privacy-minimal deterministic capability snapshots embedding the canonical
  receipt watermark;
- target/build/policy/probe/backend/consumer availability evaluator;
- native/WASM/MIDI/recording/extensions/plugin/security golden matrices;
- plugin probe caching/timeout/reconnect behavior.

**Landing gate:** no numeric occurrence lacks metadata; snapshot IDs are
byte-stable; semantic changes increment generation once; no capability is
available from a compile flag alone; default snapshots contain no forbidden
privacy fields; v1 compat recovery produces one structured diagnostic.

**Merge boundary:** may develop alongside M06, but semantic fragment and
generated projection changes wait for the shared post-M06 projection commit.

### M06 — introduce pure Candidate, Builder/Ref/Observation primitives

**Depends on:** M04.

**Owned paths:** candidate/logical-identity primitives in core and base
authoring infrastructure in `vibelang-rhai`; family-specific APIs remain for
M08/M09.

**Deliverables:**

- immutable Candidate IR, fully qualified typed logical addresses, language
  contract/engine/epoch validation, contribution ownership, and deterministic
  syntax keys;
- detached Builder base, typed Ref base, immutable Observation and RevisionRef;
- `// vibe-api: 2` evaluation selection across import/cache boundaries;
- duplicate/reference/contribution/override validation;
- side-effect-free DSP definition IR and staged registry/hash ownership seams.

**Landing gate:** base factories/configuration make zero ScriptState, registry,
queue, deployment, allocation, or backend changes; clone independence and
cross-contract/engine/epoch rejection pass; parse/evaluation/body/DSP-compile
failure leaves no Candidate or global residue; v1 goldens remain unchanged.

**Merge boundary:** may develop alongside M05. Shared Rhai registration roots
are updated only once at the end of this task, then a single M05+M06 projection
refresh is owned by the integration maintainer.

### M07 — implement inactive native generations and resource ownership

**Depends on:** M05 and M06.

**Owned paths:** core runtime/reload/state/backends/resource modules. No
transport or editor changes.

**Deliverables:**

- deterministic plan against one confirmed revision/capability snapshot;
- inactive graph generation staging, correlated native backend acknowledgements,
  one generation-root/link activation, and confirmed restoration;
- per-runtime ResourceManager for Sample, Buffer, and SFZ generations, reader
  leases, staged/committed claims, exact-once free, and quarantine;
- quantized boundary and audible-tail reporting;
- capability probes that advertise required atomicity only after proof.

**Landing gate:** failure before/after every planning, stage, create, update,
route, effect, activation, barrier, commit, and cleanup boundary yields exactly
the declared rejected/partial/applied state. Old audio/graph/resources remain
authoritative before commit. Cleanup failure cannot rewrite an applied commit.
Sample content replacement, Buffer shape policy, SFZ transitive failure, reader
pinning, and uncertain free pass exact accounting.

**Merge boundary:** exclusive ownership of `runtime.rs`, reload/state, backend
activation, and resource-manager roots. Lands before family migrations.

### M08 — migrate core authoring families

**Depends on:** M07.

**Owned paths:** Rhai Group, Voice, Pattern, Melody, Sequence, Fade, Effect,
SynthDef, and EffectDef family modules plus focused core lowering/tests.

**Deliverables:**

- pure factories/configuration and typed terminals/Refs for each family;
- one structural Group owner and stable named body contributions;
- explicit lifecycle effects, dormant/start/immediate/stop/remove/cancel
  behavior, and live Observation only through status;
- parent-owned inline sequence fragments with cycle/dependency validation;
- strict v2 parsers and tagged content unions;
- effective forwarding aliases and AST migration classifications.

**Landing gate:** family table tests prove purity, clone independence, duplicate
rules, contribution ordering/removal, typed terminal returns, no desired/live
getter confusion, Pattern/Melody/Sequence unchanged-config stop, distinct Fade
timing, full sequence dependency materialization, and structured failure rather
than logs/no-ops.

**Merge boundary:** family modules may be implemented in separate worktrees
after shared primitives freeze. They land one family at a time. Only the final
family integration commit edits shared registration/module lists and generated
contract fragments.

### M09 — migrate routes, resources, recording, MIDI, and external effects

**Depends on:** M08.

**Owned paths:** Rhai route/sample/SFZ/buffer/recording/MIDI/extension modules and
their focused core handlers. Generated outputs remain integration-owned.

**Deliverables:**

- explicit route add/replace/SET/BEND/trigger/A2K/disconnect terminals with
  stable RouteRefs and full fan-in/out/conflict metadata;
- Sample/SFZ/Buffer builders and typed resource binding without physical IDs;
- RecordRef start/stop/cancel/status and real SampleRef completion, one active
  run, and scheduling semantics;
- MIDI 1–16 public channels/groups, width-qualified values, typed unavailable
  device results, route/callback Refs, and receipt-bearing output commands;
- filesystem/process/network/MIDI/file effects split into best-effort revisions.

**Landing gate:** route matrix, generation replacement, record completion,
strict numeric/parser boundaries, device disappearance, external-effect
failure, and capability rejection all yield exact receipts. No sentinel handle,
physical buffer leak, accepted unsupported field, or log-only terminal remains
in v2.

**Merge boundary:** route/resource/record/MIDI subdomains may develop in
parallel after M08, but shared registration and core message changes land in a
single ordered integration series before transports start.

### M10 — publish HTTP v2 and typed WebSocket projections

**Depends on:** M09.

**Owned paths:** `vibelang-http`, HTTP/WS semantic fragments, schemas, client
fixtures, and this boundary's generated transport projections.

**Deliverables:**

- `/v2` request/response bindings for all current operations using strict
  operation-scoped DTOs and one error envelope;
- canonical `202` receipts, receipt/status/cancel/capability routes,
  idempotency/expected-revision behavior, bounded wait, and revisioned GETs;
- implement-or-reject decisions from the effectiveness inventory, including
  content unions, fades, gain, group names, sequence dependencies, and rejected
  quantization/source fields;
- typed receipt/status/telemetry WS events, sequence gaps, ledger catch-up, and
  reset-required snapshots;
- loopback/authenticated/insecure security modes, CORS/origin/body/rate/audit
  boundaries, and privileged capability detail.

**Landing gate:** exactly 96 current bindings are accounted for; all 69 current
non-GET operations map to a canonical operation/receipt; all 126 reachable
request fields and every mutation output member are effective or structured
rejection; no fixed sleeps; receiver lag recovers or resets visibly; security
and privacy boundary tests pass.

**Merge boundary:** HTTP routes and models land before generated OpenAPI/JSON
Schema/client types. One projection refresh closes the task.

### M11 — publish the WASM v2 runtime contract

**Depends on:** M09.

**Owned paths:** `vibelang-wasm`, web backend bridge seams, WASM semantic
fragment, TypeScript/package fixtures, and no HTTP code.

**Deliverables:**

- receipt-bearing `execute_v2`, transport methods, capability/status access,
  event subscription/polling, and explicit host-driven tick progress;
- one `globalThis.vibelangBridge` host contract for windows and workers with
  typed load/activation acknowledgements and timeout/rejection paths;
- compile-only legacy engine classification and deprecation adapter;
- canonical crate package ownership and landing-page consumption of its digest.

**Landing gate:** missing initialization/bridge, Promise rejection/timeout,
reload-send failure, no tick, delayed tick, duplicate execution, runtime reset,
backend acknowledgement loss, and worker/window fixtures return the declared
receipt/capability truth. No success-shaped warning remains. Package types match
Rust exports.

**Merge boundary:** may develop alongside M10. Generated TypeScript and archive
index update only after source behavior and package fixtures pass.

### M12 — switch editors, migration tooling, docs, and packages

**Depends on:** M10 and M11.

**Owned paths:** LSP, VS Code, Emacs, migration CLI, docs/examples, package
manifests, consumer semantic fragment, and the final consumer projection refresh.

**Deliverables:**

- functions, methods, all 275 properties, lifecycle/effect/availability data,
  exact UGen labels, and coverage/digest in editor projections;
- one semantic-token legend and one diagnostic-rule catalog used by push/pull;
- VS Code requests/settings/UI waiting on receipts and capability gating;
- Emacs `vibe lsp` and typed HTTP/WS compatibility fixtures;
- Rhai-aware `vibe migrate --check`/`migrate`, safe rewrites, and explicit manual
  diagnostics;
- active Markdown block validation, corrected generated quantitative claims,
  canonical WASM package ownership, and dry-run archive gates.

**Landing gate:** 600 function rows remain aligned, 275/275 properties are
covered, 478/478 bundled labels resolve, curated UGen coverage reports its true
numerator/denominator, legends decode, push/pull rule sets match, no handwritten
request key is outside the schema, all public blocks classify, and source/packed
clients consume the same digest.

**Merge boundary:** consumer source changes land before the sole final generated
projection/docs refresh. Package archive outputs are never committed unless
already part of the declared checked artifact set.

### M13 — final integration and default-readiness gate

**Depends on:** M12.

**Owned paths:** integration/failure-injection fixtures, release policy records,
compatibility baselines, and no new feature behavior.

**Deliverables:**

- one cross-surface Candidate matrix for CLI/Rhai/HTTP/WASM/WS/state reads;
- every failure phase on native and WASM, required and best effort, quantized
  and immediate, plus reconnect/reset and cleanup;
- native/WASM/MIDI/recording/extensions/plugin/security capability matrix;
- deterministic double generation, classified real compatibility diff, docs
  validation, and VS Code/WASM dry-run archives in one clean CI job;
- evidence for whether v2 may become default; this task does not force the
  default if a gate is red.

**Landing gate:** all ADR acceptance bullets are green; v2 has zero unknown,
unowned, unclassified, compatibility-debt, stale consumer, no-op, silent
fallback, or unreported partial bindings. Supported v1 goldens remain green.
The worktree is clean after the complete gate.

**Merge boundary:** lands alone. Any behavior fix discovered here returns to
its owning task/domain and reruns affected downstream gates; the integration
commit itself contains fixtures, policy evidence, and generated convergence
only.

## Shared-path serialization table

| High-contention boundary | Exclusive landing owner/order |
|---|---|
| `crates/vibelang-api-manifest/**` | M01, then schema-aware fixes through a designated contract owner |
| `xtask/src/public_api.rs`, `xtask/src/public_artifacts.rs`, contract/diff modules | M02, then one projection owner at each M05+M06, M08, M09, M10/M11, and M12 boundary |
| `api/contract/*.toml` | Domain tasks edit only their owned fragment; projection owner resolves cross-fragment changes |
| `crates/vibelang-core/src/message.rs` | M03 exports, M04 context propagation, M09 external-domain additions; serialized |
| `crates/vibelang-core/src/runtime.rs`, reload/state roots | M04 instrumentation, M07 transaction, M08/M09 focused lowering; serialized reviews |
| Rhai registration roots | M06 base, M08 family integration, M09 route/resource/MIDI integration; one owner per boundary |
| Generated artifacts | Never edited by domain workers; one clean generation commit per named boundary |
| Package manifests/indexes | M11 canonical WASM source, M12 consumer/package convergence; serialized |

## Final evidence bundle

The implementation program's final handoff must record:

- exact base and accepted head commits;
- contract digest, capability snapshot fixture IDs, and compatibility-diff
  result;
- counts for entries, overloads, parameters, UGen inputs, routes/types/fields,
  terminal/effectiveness records, editor coverage, docs blocks, and archives;
- native and WASM failure-injection matrix results;
- v1 compatibility results and remaining deprecation dates/releases;
- complete commands, stdout/stderr, exit codes, and clean `git status` for the
  artifact, test, docs, diff, and package gates.

No task may infer readiness from a subset of this bundle, and no consumer may
ship against a contract digest that did not pass the matching behavior gates.
