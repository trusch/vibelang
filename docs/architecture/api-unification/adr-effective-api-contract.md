# ADR: one effective API contract and one mutation truth

| Field | Decision |
|---|---|
| Status | Frozen candidate; acceptance is attached to the exact reviewed commit in the architecture-review ticket |
| Decision scope | API-unification v2 architecture and implementation order |
| Assessed root | `e5a1198a3bb478418042f2b517172f74635742b7` |
| Product behavior changed here | None |
| Supersedes | Conflicting vocabulary and open choices in the five studies listed below; it does not replace their source evidence |

## Context

The public declaration spine is broad and reproducible, but a declaration is
not yet a reliable promise of behavior. The assessed tree contains 3,626 public
manifest entries and 8,431 overloads, 96 HTTP routes, overlapping native and
WASM surfaces, and several editor and documentation projections. Current
failures can occur after an ingress has already returned success, builders mix
configuration with mutation, accepted fields can be ignored, capability claims
can be stronger than backend behavior, and consumers infer conventions
independently.

This ADR synthesizes these reviewed studies:

- [Shared revision receipts](revision-receipts.md)
- [Authoring lifecycle vocabulary](lifecycle-vocabulary.md)
- [API effectiveness inventory](effectiveness-inventory.md)
- [Conventions and runtime capabilities](conventions-and-capabilities.md)
- [Effective contract schema](effective-contract-schema.md)

It also preserves the invariants in the accepted
[Builder, Ref, revision, and resource lifetime ADR](../builder-ref-revision-resource-lifetime.md).
Where a study used different words or left a choice open, the reconciliation
tables below are normative.

## Decision summary

VibeLang v2 has one compiled, versioned effective contract and one runtime
mutation truth:

```text
executing declarations + semantic fragments
                    |
                    v
        public-api-manifest-v2.json
          /        |        |       \
     language   transports  tools   docs/packages
                    |
mutation ingress --> canonical receipt ledger --> revisioned observations
                    |
          plan / stage / activate / confirm
```

The static contract states what an operation promises, the conditions under
which it is available, and how it is projected. The runtime receipt ledger
states what happened to one attempt or accepted revision. A projection may
change representation, but it may not create a different success model.

The following rules are non-negotiable:

1. `api/public-api-manifest-v2.json` is the sole checked effective contract.
2. `xtask` is the sole composer and writer of the contract and its generated
   projections.
3. Runtime mutations have one canonical attempt/revision/receipt state machine
   owned by `vibelang-core`.
4. V2 authoring roles are Value, Builder, Ref, and Observation. `Handle` is a
   legacy spelling, not a v2 role.
5. Every operation-scoped input and output member has an effectiveness record.
   Release-ready v2 has zero unclassified, ignored, log-only, stale-success, or
   dead public bindings.
6. Every quantity declares a unit, range or explicit unbounded marker, and an
   invalid-value policy. V2 rejects invalid or ambiguous values.
7. Availability is evaluated from semantic capability truth, not inferred from
   a compiled method or feature flag.
8. V1 remains behaviorally frozen behind an explicit compatibility profile.
   V2 is opt-in until the release gate permits it to become the default.

## Ownership and dependency direction

No new shared crate is introduced solely for this program. Ownership follows
behavior, which avoids making the manifest crate a runtime dependency hub.

| Concern | Authoritative owner | Allowed consumers | Prohibited ownership |
|---|---|---|---|
| Schema-v2 records, stable semantic IDs, fragment schema, compatibility-diff classes, deterministic JSON | `crates/vibelang-api-manifest` | `xtask`, validators, tooling | Runtime state machines and transport behavior |
| Mechanical declarations | The executing crate or canonical catalog: Rhai registrations, UGen manifests/build rules, stdlib source, Clap, Axum/Serde, WASM exports, typed WS payloads, LSP rule/legend definitions | `xtask` discovery | Repetition in semantic fragments |
| Non-inferable semantics | Domain-owned `api/contract/*.toml` fragments validated by the manifest crate | `xtask` join | Names, signatures, routes, field shapes, or package paths already discoverable mechanically |
| Composition, validation, generation, projection, compatibility diff | `xtask` | CI and release tooling | Editors, runtime crates, or docs writing competing manifests |
| `AttemptId`, `RuntimeEpoch`, `RevisionId`, `EventSequence`, receipt/state transitions, mutation ledger, `MutationContext`, component outcomes | `vibelang-core` | CLI, Rhai, HTTP, WebSocket, WASM | Per-transport receipt enums or success booleans |
| Runtime capability snapshot and probe results | `vibelang-core`, using capability IDs defined by the effective contract | CLI, HTTP/WS, WASM, editors | Compile-time feature lists presented as runtime truth |
| Builder/Ref/Observation host types and candidate lowering | `vibelang-rhai`, with shared logical identity and receipt primitives from `vibelang-core` | Scripts and migration tooling | Backend IDs or physical resource ownership in Refs |
| Backend activation, acknowledgements, resource generations, live observations | `vibelang-core` backend/runtime/resource owners | Receipt ledger and capability evaluator | HTTP, WASM, or Rhai adapters guessing apply state |
| Wire bindings | `vibelang-http`, `vibelang-wasm`, and `vibelang-cli` | Their clients | Canonical domain semantics duplicated in DTOs |
| LSP, VS Code, Emacs, documentation, package indexes | Generated projections plus explicit consumer policy | User-facing consumers | Hand-maintained exhaustive symbol or capability lists |

Dependency direction is `executing source -> discovery -> compiled contract ->
projection`, while runtime behavior is `ingress -> vibelang-core ledger ->
observation -> projection`. The static manifest never imports runtime state, and
runtime crates do not depend on `xtask`.

## Static contract authority

### Source model

The v2 contract is compiled from two source classes:

1. Mechanical facts are discovered from the code or catalog that executes or
   exposes them.
2. Semantic facts that cannot be proven mechanically are supplied in the six
   domain fragments `authoring.toml`, `runtime.toml`, `http.toml`,
   `websocket.toml`, `wasm.toml`, and `consumers.toml` under `api/contract/`.

Fragments reference discovered stable IDs. A fragment that repeats a name,
signature, path, serialized field, or package location fails. Duplicate
semantic owners, orphan records, missing IDs, and incompatible refinements also
fail.

The canonical artifact is `api/public-api-manifest-v2.json`, using schema URI
`https://vibelang.org/schemas/public-api-manifest/v2`. Schema v1 is immutable.
During migration, `api/public-api-manifest-v1.json` and
`api/http-api-snapshot-v1.json` are generated compatibility projections. The
first composer checkpoint must reproduce the existing v1 artifacts byte for
byte.

The checked artifact contains no timestamp, Git commit, hostname, absolute
path, target directory, random value, or runtime probe result. It is sorted by
stable ID and uses repository-relative source and test anchors. `xtask` writes
all generated artifacts; check mode writes only temporary output, compares it
byte for byte, and leaves the worktree clean.

### Stable identity and canonical encodings

- Existing unchanged v1 entry and overload IDs retain their current stable IDs.
- New contract nodes use the existing documented FNV-1a stable-ID algorithm
  with node-specific namespaces. A rename creates a new ID plus an alias or
  replacement edge; an ID is never repurposed.
- Runtime `AttemptId` and `RuntimeEpoch` are UUIDv7 opaque values. Their time
  component is not used for ordering.
- Runtime `RevisionId` and `EventSequence` are `u64` newtypes internally and
  lowercase decimal strings on JSON/JavaScript wires, preventing loss above
  JavaScript's exact integer range. Rhai and Rust may expose checked integer
  wrappers, not floating-point counters.
- Contract and capability semantic hashes use SHA-256 over RFC 8785 JSON
  Canonicalization Scheme bytes. Cross-language fixtures pin the bytes and
  digest.
- Every attempt has a `submission_digest` over its normalized ingress payload.
  An accepted candidate or command also has an `operation_digest` over the
  canonical, source-independent operation or Candidate IR. Idempotency and
  cross-surface equivalence use `operation_digest`; parse failures retain only
  `submission_digest`.
- Beat positions in receipts use signed fixed-point integer ticks at
  1/65,536 quarter-note beat resolution, serialized as decimal strings.
  Backend seconds remain a finite number and never replace the musical value.

## Canonical mutation contract

### Identity and admission

Every submission receives an `AttemptId` before decode or parse. A
`RevisionId` is allocated exactly once, after decode, parse/evaluation,
candidate validation, capability checks, idempotency, and expected-revision
checks succeed. Pre-admission rejection has no revision. An accepted revision
keeps its number even if it later rejects, is superseded, or becomes partial;
holes are observable.

The canonical ordering key is `(RuntimeEpoch, RevisionId)`. Receipt transitions
and status events are ordered by `(RuntimeEpoch, EventSequence)`. Wall-clock
timestamps are diagnostic metadata only.

The in-process ledger is not persisted in the first v2 implementation. Restart
or unrecoverable backend reconstruction creates a new epoch. The runtime retains
the newest 10,000 transition events and every event younger than 15 minutes; an
event is evictable only after both protections no longer apply. The current
window and expiration boundary are exposed as capability/status fields.

### States and terminal outcomes

The canonical non-terminal states are:

```text
evaluating -> accepted -> planning -> staging -> committing
```

`accepted`, `planning`, `staging`, and `committing` are all pending states.
`Pending` is a projection label, never a fifth terminal outcome and never
success.

There are exactly four terminal outcomes:

| Outcome | Invariant |
|---|---|
| `rejected` | No requested effect remains, or a correlated rollback/restoration barrier confirmed the previous state. |
| `superseded` | The attempt/revision was replaced or cancelled before planning and no requested effect remains. |
| `applied` | Every effect promised by the selected atomicity and capability contract crossed its declared effective boundary. |
| `partial` | An effect remains, may remain, or cannot be proven applied or restored. Every planned component is partitioned as applied, failed, uncertain, or not started. |

`partial` is terminal, never a successful subset. Unknown activation or rollback
fences the runtime. Best-effort operations may produce an unfenced partial only
when all uncertain effects are absent and the remaining failure cannot corrupt
later ordering; the component ledger still remains terminal and visible.

The term `failed` from the earlier lifetime ADR and the schema study is not a v2
wire outcome. A post-admission failure maps to `rejected` only when no effect
remains or restoration was confirmed; otherwise it maps to `partial`.
`partially_applied` normalizes to `partial`. Status may expose
`last_rejected_revision` as a summary, but that is not another terminal state.

No terminal can transition to another terminal. Corrections are new revisions.
`superseded` is allowed only before planning. A cancellation after planning is
rejected as `too_late_to_cancel`; compensation, if requested, is a new revision.
Accepted revisions plan and cross activation in revision order. A higher
revision cannot become applied until every lower accepted revision is terminal.

### Atomicity and effective boundaries

`Atomicity::Required` is the default for v2 Candidate and managed graph
mutations. It is admitted only when the selected runtime/backend capabilities
prove inactive-generation staging, correlated activation, and the required
barrier. The protocol is:

1. create and validate a side-effect-free Candidate;
2. plan against one confirmed revision and capability snapshot;
3. stage synthdefs, resource generations, graph objects, routes, and ownership
   under the new revision without changing the active generation;
4. obtain the declared backend acknowledgements;
5. activate through one generation-root/link switch at the requested boundary;
6. cross the correlated backend or musical barrier;
7. publish `applied` and retire the old generation asynchronously.

Failure before activation rejects and preserves the previous generation.
Failure after activation rejects only after confirmed restoration; otherwise it
is partial and fenced. Cleanup failure after a confirmed commit leaves the
revision applied, reports degraded resource health, and quarantines the resource
for retry.

`Atomicity::BestEffort` is explicit. V1 in-place reconciliation is projected as
best effort while it is instrumented. External MIDI output, recording/file I/O,
process execution, and network effects are separate best-effort revisions; they
cannot be hidden inside a required-atomic Candidate. Mixed requests are split
before admission or rejected.

Queue admission, channel send, socket write, JS dispatch, snapshot assignment,
and fixed sleep are not effective boundaries. Native `applied` requires a
correlated generation activation plus backend barrier. Quantized pattern or
melody changes stay pending until the musical swap and report an unavoidable
lookahead tail. WASM `applied` requires runtime progress plus the advertised
bridge/worklet acknowledgement; otherwise only explicit best effort is
available.

V2 evaluation cannot synchronously execute a filesystem, process, network,
MIDI-output, recording/file-write, or other external side effect inside a
required-atomic Candidate. The author must submit that work as a separate
best-effort operation and pass its immutable result into a later Candidate, or
receive a capability/atomicity rejection. V1 evaluation preserves current
extension behavior and reports any leaked pre-admission effect as an attempt
with terminal partial outcome.

### Idempotency, concurrency, and observation

The idempotency namespace is `(runtime_epoch, caller_namespace,
idempotency_key)`. Remote authenticated requests use the stable authenticated
subject. Loopback-local requests use one runtime-local namespace. Insecure
remote mode may mutate only with a server-issued opaque session namespace; it
never derives identity from IP address, origin text, or a secret exposed in
capabilities.

Same namespace/key/digest returns the existing attempt and latest receipt.
Same key with a different digest rejects as `idempotency_conflict`. V2 remote
note, MIDI output, record-start, and other non-idempotent external operations
require a key. Replace/delete/whole-candidate operations also require
`expected_revision` unless explicit unconditional best effort was selected.
The expected revision is checked at admission and again immediately before
planning. A later mismatch rejects the allocated revision with no requested
effect.

Every live projection carries `runtime_epoch`, `event_sequence`, and
`last_confirmed_revision`. WebSocket receipt delivery is at least once and uses
the full current receipt. A sequence gap invokes ledger catch-up; an expired gap
returns `reset_required` and a fresh status/snapshot. Wall-clock timestamps and
20 Hz telemetry never acknowledge mutations.

## Authoring roles and terminal effects

V2 exposes exactly four authoring roles:

| Role | Meaning |
|---|---|
| Value | Detached immutable data without runtime identity. |
| Builder | Evaluation-local configuration with no desired-state, registry, queue, deployment, resource, or backend effect before a terminal. |
| Ref | Stable typed logical address qualified by language contract, engine instance, runtime epoch, project/module scope, kind, and declaration key. |
| Observation | Revision-, epoch-, sequence-, and timestamp-qualified live or pending facts with explicit staleness. |

The schema may classify current v1 types as `legacy_handle`, but no v2 binding
may expose Handle as a role. Refs never contain builder fields, backend IDs,
buffer numbers, pointers, cached live flags, or resource ownership. Physical
Sample, Buffer, and SFZ generations are owned by a per-runtime ResourceManager,
and readers pin the generation they use.

Every callable declares one or more effects from this closed vocabulary:
`construct`, `configure`, `register`, `start`, `stop`, `synchronize`, `cancel`,
and `observe`. The canonical terminal spellings are `apply`, `start`,
`start_now`, Voice-only `run`, Ref `stop`, `remove`, `cancel`, `status`,
`status_at_least`, and receipt `sync`. Routing uses explicit add/replace/SET/BEND
verbs and returns `RouteRef`.

| Terminal shape | Effect categories | Return during evaluation |
|---|---|---|
| `Builder.apply()` | `register` | Matching typed Ref |
| `Builder-or-Ref.start()` | `register` when needed, then `start` | Matching typed Ref |
| `Builder-or-Ref.start_now()` | `register` when needed, then immediate `start` | Matching typed Ref |
| `VoiceBuilder-or-Ref.run()` | `register` when needed, then continuous `start` | `VoiceRef` |
| `Ref.stop()` | `stop`; declaration remains | Same typed Ref |
| `Ref.remove()` | `cancel` with declaration-removal mode | Same typed Ref |
| `Ref.cancel()` | `cancel` of a pending/scheduled operation | Same typed Ref |
| `status(ref)` | `observe` | Observation |
| `status_at_least(ref, revision, timeout)` | `synchronize`, then `observe` | Observation or explicit timeout/stale Observation |
| `sync(receipt, timeout)` | `synchronize` | Canonical terminal revision outcome |
| Route add/replace/SET/BEND terminal | `register` | `RouteRef` |
| `RouteRef.disconnect()` | `cancel` with edge-removal mode | Same `RouteRef` |

Builder configuration and relationship methods remain pure. Inline sequence
builders are parent-owned detached fragments and materialize atomically at the
Sequence terminal. Pattern, Melody, Fade, and nested Sequence follow the same
rule; cycles are always rejected with a dependency path. RecordRef permits one
active run. DSP resource parameters use `set_resource(param, ref)`; physical
buffer-number access is not part of v2.

`RecordBuilder.apply()` registers a dormant declaration; `start()` and
`start_now()` create the one active run. The v1 `RecordHandle.apply()` adapter
continues to start recording and migrates mechanically to v2 `start()`. The v2
WASM surface omits `stopAll`: transport stop is `stop_v2`, and a future graph
reset must be a separately named, receipt-bearing operation. Legacy `stopAll`
is deprecated rather than preserved under a misleading name.

The language-major source spelling is `// vibe-api: 2`. CLI, HTTP, WASM, LSP,
imports, and caches adapt to the same evaluation input. Ref lookup factories use
explicit `*_ref` spellings such as `group_ref`; lookup never creates or owns a
declaration. Rhai Observation and receipt records are immutable registered
types, not mutable maps.

## Effectiveness authority and zero-unclassified rule

Runtime outcomes and static effectiveness are separate dimensions. The
canonical per-operation binding classification is:

| Static class | Meaning |
|---|---|
| `effective` | Dispatch and authoritative observation/test anchors prove the promised effect. |
| `structured_rejection` | The operation rejects before dispatch with a typed field/operation error and supported alternative. |
| `compatibility_debt` | A frozen v1 binding is ignored, log-only, stale, or dead; it has an owner, diagnostic, migration issue, and removal gate. Forbidden in release-ready v2. |

The research terms `implemented`, `unsupported`, `ignored`, `log-only`,
`stale`, and `dead` are audit inputs mapped to these three classes. They are not
additional v2 states. The contract compiler discovers effectiveness by
`(surface, operation, input path or output member)`, not by Rust field alone,
because shared fields can behave differently by operation.

The operation-scoped implement-versus-reject choices in
[the effectiveness inventory](effectiveness-inventory.md) are normative unless
this ADR explicitly overrides them. The overrides are: v2 Fade `apply` becomes
effective dormant registration rather than removal; Fade normal/immediate
starts must be behaviorally distinct before aliases become available; v2 Record
`apply` is dormant while the v1 adapter preserves its start effect; and inline
Sequence dependencies use parent-owned detached fragments rather than hidden
global synchronization. HTTP field choices, strict unsupported rejections, and
removal of dead/placeholder v2 members otherwise stand as inventoried.

Every accepted v2 input is effective or structurally rejected. Every effective
record has declaration, dispatch, observation, and behavioral test anchors. No
response may claim a fresher consistency point than its receipt. Release-ready
v2 forbids ignored fields, warning-only or log-only terminals, fixed-sleep
acknowledgements, placeholder outputs described as available, and success after
bridge or backend failure.

The zero-unclassified discovery join covers at least:

- all 8,431 current manifest overloads and all 34 registered types;
- all 18,786 overload parameter occurrences and 5,089 UGen inputs;
- all request leaves bound per HTTP operation, all non-GET response members,
  and memberless success responses;
- all 96 HTTP method/path bindings, 75 types, and 297 fields;
- all WebSocket actions, advertised events, envelope fields, and typed payload
  paths;
- every public WASM class/module export and result member;
- all 26 currently marked Rhai terminals plus explicitly audited terminal-like
  free functions;
- concrete VS Code request keys, settings, commands, and emitter output;
- every non-archival public Vibe/CLI Markdown block and package member.

Adding one discovered item without a complete record must produce exactly one
unclassified failure. Denominators cannot shrink because discovery stopped
seeing a source.

## Quantities, parsers, collisions, and capabilities

The conventions in [Conventions and runtime capabilities](conventions-and-capabilities.md)
are accepted as normative with these fixed choices:

- Every public quantity has `unit_id`, `range_id` or explicit unbounded marker,
  invalid-value policy, and provenance. V2 uses `invalid.reject`; compatibility
  clamp/fallback/drop is confined to `compat.vibelang.v1` and emits one
  structured diagnostic per recovery.
- Public MIDI channels and UMP groups are 1 through 16. Explicit legacy storage
  fields use `*_index` and 0 through 15. High-level voice/melody velocity is
  normalized 0 through 1; raw MIDI fields are named `velocity_7bit` or
  `velocity_16bit`.
- Beats are quarter-note beats. Bars carry a time signature or its revision and
  use `numerator * 4 / denominator`. Fixed-four behavior exists only in v1
  compatibility diagnostics.
- Linear amplitude, normalized ratio, decibels, hertz, seconds, milliseconds,
  and sample frames are distinct units.
- Strict parsers consume the full input and return spans. V2 never defaults an
  invalid note, chord, curve, scale, target, enum, or token. Pattern content is
  a tagged union of events or pattern text. Melody content is a tagged union of
  events, text, or lanes. Supplying more than one representation rejects as
  ambiguous.
- Definition identity is `(kind_id, module_id, local_name)`. Non-identical
  duplicate deployment rejects independent of load order; identical hashes are
  idempotent and report only under verbose diagnostics. The `cv/lfo` definitions
  are canonical, and ambiguous unqualified `arpeggio_up_down` rejects with both
  candidates.

Capability IDs describe observable semantics. Runtime state is `available`,
`degraded`, `unavailable`, or `unknown`; `conditional` remains declaration
metadata. Evaluation joins declaration/quarantine, target, build feature,
operator/security policy, runtime probe, backend semantic probe, and consumer
projection revision in that order and retains all reason IDs.

The initial catalog in the conventions study is required, plus explicit
receipt capabilities for ledger retention, expected-revision support,
idempotency, cancellation window, backend barrier, musical boundary, and atomic
generation activation. A method compiled with a default no-op backend method is
degraded or unavailable, never available.

`mi-UGens` uses `probe.plugin.mi_ugens.v1`: a backend query with a one-second
timeout, cached only within the runtime epoch and refreshed on explicit request
or backend reconnect. Before a positive result, state is `unknown`; a negative
or timed-out probe is unavailable with its reason.

The aggregate capability route is `GET /v2/capabilities`. Loopback-local access
may read the privacy-minimal aggregate without authentication. Remote access
requires the active security policy. Privileged device and policy detail is a
separate authenticated `GET /v2/capabilities/details` binding.

## Target and transport availability

| Surface | Initial v2 contract |
|---|---|
| Native core/scsynth | Required atomicity is unavailable until an inactive generation root/link switch and correlated scsynth barrier pass probes and failure injection. Queue/socket availability alone is insufficient. |
| WASM/Web Audio | The host continues to drive explicit `tick` in v2. Receipts remain pending without progress. Required atomicity is unavailable until the bridge/worklet returns a correlated activation acknowledgement; best effort is explicit meanwhile. Discovery and invocation use one declared `globalThis.vibelangBridge` contract, including workers. |
| Audio recording and SFZ | Native/semantic-capability gated. Absent on WASM unless a future version adds an effectful implementation. |
| MIDI and UMP | Feature, target, policy, device/probe, and backend-semantic gated. Discovery returns typed unavailable results, never sentinel devices. |
| Extensions | Filesystem, process, and network are separately scoped per evaluation. Local script permission never leaks to HTTP `/eval`. External effects are separate best-effort revisions. |
| HTTP mutation | V2 ingress is under `/v2`. Mutation responses are `202` with the canonical receipt and `Location`, unless a requested bounded wait returns a terminal receipt. No fixed sleeps. |
| WebSocket | Receipt/status/telemetry egress with sequence-gap recovery; it is not another mutation ingress protocol. |
| CLI/Rhai | Both expose the same attempt/revision outcome. `--wait applied` is the interactive default; accepted-only waiting is explicit. |

WASM's legacy `VibelangEngine` is compile-only during migration and receives no
runtime revision. The canonical distribution is
`crates/vibelang-wasm/package.json`; landing-page build output is a consumer,
not a second package owner.

## Security and privacy boundary

HTTP has three explicit modes:

1. `security.http.loopback_local`: loopback bind and loopback-only origins;
2. `security.http.authenticated_remote`: authentication, explicit origin
   allowlist, body/request limits, rate limits, audit policy, and separately
   scoped eval/extensions;
3. `security.http.insecure_remote`: high-friction operator acknowledgement,
   degraded capability state, limits still enabled, and no claim of remote-user
   isolation.

A non-loopback bind fails in loopback-local mode. Authentication does not enable
`/eval` or filesystem/process/network extensions. Capability and status payloads
do not expose credentials, environment values, source, filesystem roots, full
origins, commands, device names/paths, usernames, or secret-bearing error text.
Privileged detail uses stable opaque device IDs. Source paths/spans in remote
diagnostics are project-relative or redacted according to scope.

Request/receipt digests exclude secrets and authentication material. Audit logs
may reference attempt/revision IDs and redacted operation IDs, but they are not
the receipt ledger and cannot substitute for caller-visible failure.

## Versioning, compatibility, and deprecation budget

V1 remains frozen and unversioned during migration; no new public feature is
added only to v1. V2 uses the language directive above, `/v2` HTTP routes,
versioned WebSocket schemas/events, v2 WASM methods, and a shared contract digest
in every consumer. Unknown schema/contract versions fail visibly rather than
downgrading to success.

The compatibility budget is:

- compatibility debt may exist only in the generated v1 ledger and may never
  increase without an explicitly approved security or data-loss exception;
- release-ready v2 has zero compatibility-debt bindings;
- an effective v1 alias is retained for at least six months and two published
  minor releases after v2 becomes the default, whichever is later;
- removal occurs only in the next semver-major release after that minimum gate;
- every alias has a canonical target, since/deprecated/removal metadata,
  warning span, behavior fixture, and migration class;
- a deprecated no-op is not an alias. Known no-ops are rejected in v2 from its
  first release while v1 preserves behavior with a diagnostic.

This budget resolves the studies' one-release, one-major-window, and
six-month/two-minor proposals in favor of the strictest combined rule. It
applies to `launch`, `Fade.now`, v1 terminal shapes, physical buffer-number
access, ignored HTTP fields, and `VibelangEngine`. Removal timing is based on
the default-v2 milestone, not the date this ADR lands.

Every contract diff is classified as metadata-only, compatible addition,
compatible relaxation, behavioral, source-breaking, wire-breaking,
availability-breaking, consumer-breaking, or security-operational. One change
may carry multiple classes; the strictest release action wins. An unclassified
hunk fails.

## Migration and landing rules

The detailed task graph is in
[Implementation dependency and landing map](implementation-dependency-graph.md).
The immutable checkpoints are:

1. freeze v1 artifacts, counts, IDs, golden behavior, and known-defect negative
   fixtures;
2. land schema/fragment types and fixture-only validators;
3. compose the declaration graph and prove v1 byte equivalence;
4. land receipt IDs, transition validation, ledger, and MutationContext without
   changing success semantics;
5. instrument current v1 behavior as honest best effort so every leaked or
   uncertain effect is partial rather than success;
6. land conventions/capability metadata and privacy-minimal snapshots;
7. make Candidate construction pure and add Builder/Ref/Observation primitives;
8. implement native inactive-generation/resource transaction support and only
   then advertise required atomicity;
9. migrate authoring families and transport bindings domain by domain;
10. switch consumers only after their operations are effective or rejected and
    generated projections are current;
11. validate docs/packages/migrations and pass the final cross-surface matrix;
12. make v2 default only after every release gate is green.

Generated artifacts have one landing owner per integration wave. Parallel
domain tasks modify semantic source and behavior/tests, not shared generated
output. Shared `xtask`, manifest-schema, `vibelang-core` runtime/message/state,
and Rhai registration-root changes are serialized at the boundaries named in
the landing map.

## Negative and drift gates

The complete artifact gate fails on any of these conditions:

- any discovered public item or applicable facet is unknown, missing,
  unowned, orphaned, or unclassified;
- a v2 input is ignored, warning-only, log-only, or dispatches despite its
  declared structured rejection;
- a mutation crosses a queue/runtime boundary without a canonical receipt;
- a rejected revision leaves an unreported effect, or an uncertain effect is
  not partial and fenced;
- a user-controlled v2 boundary can panic/unwind or silently clamp, fallback,
  drop, wrap, or partially parse;
- a route/type/export/event/command/setting/package binding disagrees with its
  executing source in either direction;
- a WASM method resolves success after missing initialization, bridge,
  dispatch, backend acknowledgement, or apply failure;
- a WebSocket event is untyped, unordered, cannot expose loss, or cannot
  resynchronize;
- a capability becomes available solely from build inclusion or method
  presence;
- any exact editor completion label fails to resolve; the current eight stale
  UGen labels remain negative fixtures until fixed;
- semantic-token emission differs from the advertised legend, or push/pull
  diagnostic rule sets differ;
- a consumer denominator shrinks, exclusion lacks reason/owner, or source and
  packaged contract digests differ;
- an active Markdown example contains an unavailable call, wrong signature,
  obsolete command/flag, or unclassified non-executable content;
- VS Code or WASM dry-run archives differ from the package index, or a second
  canonical WASM package owner appears;
- two clean generations differ, check mode dirties the tree, or a compatibility
  diff contains an unclassified hunk.

## Measurable acceptance

### This architecture story

This story is complete only when:

1. the isolated lineage starts at the assessed root and contains exactly the
   five reviewed Markdown studies plus this ADR and its dependency map;
2. the ADR resolves shared type/crate ownership, static and runtime authority,
   outcomes, lifecycle/effectiveness vocabularies, conventions, availability,
   security, compatibility, migration, merge boundaries, and acceptance;
3. a separate read-only reviewer inspects the five studies, this ADR, and the
   map at one exact commit and records source-backed blocker/non-blocker
   findings;
4. every blocker is corrected and a fresh independent review accepts the new
   exact commit; no semantic edit occurs after the accepted review;
5. scoped Markdown/path/diff checks prove no product-code or generated artifact
   change, the isolated worktree is clean, the dirty primary fingerprint is
   unchanged, and no Cargo or Node command ran.

### Contract and implementation wave

- V2 represents all 3,626 entries and 8,431 overloads; unchanged v1 IDs and the
  initial v1 projection are byte-identical.
- All 18,786 parameter occurrences and 5,089 UGen inputs have reviewed quantity
  applicability; zero numeric values omit unit/range/unbounded and invalid-value
  policy.
- All 96 HTTP routes, 75 types, and 297 fields are exact; every non-GET binding
  returns the shared receipt and no mutation handler sleeps as acknowledgement.
- All current terminals and terminal-like operations are effective or
  structured rejection. Builder purity tests find zero pre-terminal state,
  registry, queue, deployment, or resource effects.
- Transition/property tests prove monotonic revisions/sequences, valid edges,
  idempotency, expected revisions, cancellation races, component partitioning,
  and no terminal rewrite.
- Phase-indexed failure injection before and after every apply boundary proves
  prior-state preservation, confirmed rollback, partial/fencing, and cleanup
  health exactly as declared.
- Native and WASM capability matrices never advertise a stronger confirmation
  boundary than their backend probes prove.
- V2 boundary tests reject non-finite/out-of-range/ambiguous input consistently;
  v1 compatibility preserves legacy results with structured diagnostics.
- All 478 bundled UGen labels resolve, all 275 properties are projected or
  explicitly covered, semantic legends match, and push/pull diagnostics select
  the same rules.
- Every public active code block and required archive member is classified;
  generated coverage and compatibility reports have zero unresolved or stale
  IDs.

### Final cross-surface integration

One canonical Candidate submitted through CLI watch, Rhai host, HTTP `/v2`, and
WASM must produce the same canonical operation ID, operation digest, revision
semantics, diagnostics, and terminal outcome. WebSocket must report the
correlated event sequence, and revisioned state reads must agree. The matrix
repeats with failures at decode, parse, evaluation, admission, planning,
staging, activation, backend barrier, musical boundary, observation, and
cleanup on native and WASM. No surface may upgrade accepted to applied, hide
partial state, or claim a capability absent from the selected snapshot.

## Reconciled contradictions

| Topic | Conflicting study language | Frozen resolution |
|---|---|---|
| Mutation outcomes | Effectiveness used Applied/Pending/Rejected; earlier ADR/schema used failed/partially-applied; receipts used four terminals | Pending is non-terminal. Terminals are rejected, superseded, applied, partial. Failed maps to rejected only with no remaining effect/confirmed rollback; otherwise partial. |
| Static effectiveness | Inventory classes differed from schema `effective`/`structured_rejection`/`ignored_migration` | Canonical classes are effective, structured_rejection, compatibility_debt. Audit labels map into them; v2 forbids debt. |
| Lifecycle role | Schema allowed generic Handle; lifecycle study rejected it | `legacy_handle` may describe v1 only. V2 roles are Value, Builder, Ref, Observation. |
| `Fade.apply` | Effectiveness proposed removal and immediate start; lifecycle proposed effective dormant registration | V2 `FadeBuilder.apply` registers a dormant fade. `start` uses declared normal timing and `start_now` is immediate. Until those are behaviorally distinct, unsupported variants reject; aliases are not activated early. |
| `Record.apply` | Current/effectiveness behavior starts a recording; lifecycle vocabulary reserves apply for dormant registration | V1 apply keeps its start behavior. V2 apply is dormant and v2 start/start_now run; migration rewrites the v1 terminal. |
| Sequence dependencies | Effectiveness proposed implicit materialization; lifecycle required no hidden global registration | Inline builders are detached parent-owned fragments materialized by the parent terminal. Ref overloads reference independently applied declarations. Cycles reject. |
| WASM `stopAll` | Effectiveness required verified all-node behavior or absence; existing method only stops transport | Omit it from v2. Use `stop_v2` for transport; any future graph reset gets a distinct receipt-bearing operation. |
| Deprecation window | One release, one major window, and six months/two minors appeared | Strict combined budget: six months and two minor releases after v2 default, whichever later, then only a semver-major removal. |
| Schema v2 terminal set | Schema left failed/partially_applied optional | Schema projection uses the four frozen receipt terminals; old names are migration aliases only. |
| Revision wire numbers | Runtime proposals used `u64`; JS projections require exact values | `u64` internally, decimal string on JSON/JS wires. |
| Capability snapshot revision shape | Convention study sketched accepted/applied fields | Snapshot embeds the canonical epoch/event/accepted-through/last-confirmed shape by reference; no second revision system. |
| HTTP versioning | Path, media type, or header was open | `/v2` paths, with explicit schema/contract digest headers; no content-negotiation-only version. |
| WASM progress | Host tick or internal scheduler was open | Host-driven explicit tick in initial v2; lack of progress remains pending and is advertised. |
| External effects | Could be deferred inside mixed Candidate or split | Split into separate best-effort revisions; required-atomic Candidate never hides them. |

## Rejected alternatives

| Alternative | Reason rejected |
|---|---|
| Treat declaration, deserialization, queue admission, socket write, or bridge dispatch as success | None proves the promised live effect or correlates later failure. |
| Give each surface its own result type/state machine | It recreates contradictory success and stale observations for the same mutation. |
| Put runtime receipt behavior in the manifest crate | Static schema ownership would become a runtime dependency hub and blur behavior ownership. |
| Hand-maintain a giant JSON/OpenAPI master | It duplicates executing declarations and guarantees source drift. |
| Infer all semantics from source AST | Intent, effectiveness, consistency, compatibility, and backend truth cannot be proved from syntax alone. |
| Preserve hybrid Handles in v2 | They conflate configuration, identity, observation, and physical lifetime. |
| Let v2 silently clamp/fallback for live-coding ergonomics | It hides errors and makes tools/transport behavior nondeterministic; explicit v1 compatibility preserves recovery visibly. |
| Report compile features as capabilities | Compiled code can be policy-disabled, unprobed, no-op, or semantically unsupported. |
| Roll back by replaying or deep-cloning current state | It cannot undo external/backend effects and makes restoration itself unobservable and fallible. |
| Permit partial success as applied | Callers cannot reason about retry, compensation, or authoritative state. Partial is a distinct terminal with component truth. |
| Switch consumers before behavior lands | Generated surfaces would advertise promised behavior that runtime paths do not yet provide. |
| Remove v1 aliases immediately | It creates avoidable migration shock and discards the value of golden compatibility fixtures. |

## Closed implementation choices and user decisions

All architecture-significant choices raised by the five studies are closed in
this ADR or assigned to a measurable implementation task. Exact Rust module
layout, internal data structures, probe transport messages, and UI presentation
remain implementation details constrained by the contract and tests.

There is no unresolved user decision required to start the implementation wave.
Changing a frozen choice above requires a new ADR or an explicitly classified
compatibility change; it is not ordinary implementation discretion.
