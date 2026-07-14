# Builder, Ref, revision, and resource lifetime architecture decision

| Field | Value |
|---|---|
| Status | Accepted |
| Date | 2026-07-14 |
| Decision | P0.5 state ownership/version/lifetime ADR |
| Applies to | Opt-in VibeLang language API v2 and later |
| Implementation status | Design only; no runtime behavior changes are made by this ADR |

## Context and evidence

The current authoring surface does not have one declaration lifecycle. A named
`Voice` is both a builder and a reference and writes to `ScriptState` during
most chained calls. Pattern, Melody, Sequence, Fade, and Fx builders defer some
or all writes until a terminal. Group, Sample, and SFZ handles write through to
desired state, while `BufferHandle` is a backend buffer number. Synthdef body
finalization can update global registries and deploy during evaluation.

Reload has a similar ambiguity. A successful channel send means that a desired
snapshot was queued, not that the backend applied it. Reconciliation mutates
the live backend in phases, and a later error does not undo every earlier
mutation. The runtime exposes neither a correlated revision nor a freshness
contract. Duplicate declarations and repeated group bodies use a mixture of
last-write-wins, append, merge, and rejection rules. Resource identity and
release are likewise type-specific and are not expressed as one ownership
contract.

This decision is based primarily on the completed MindHub research tickets
`research-current-builder-ref-revision-and-resource-lifetime-semantics` and
`research-design-the-registration-manifest-extraction-and-wave-implementation-seams`.
The relevant current implementation roots are:

- [`ScriptEngine`](../../crates/vibelang-rhai/src/engine.rs), Rhai ID accessors
  in [`context.rs`](../../crates/vibelang-rhai/src/context.rs), and the current
  API registration root in [`api/mod.rs`](../../crates/vibelang-rhai/src/api/mod.rs).
- Current [`Voice`](../../crates/vibelang-rhai/src/api/voice.rs),
  [`Group`](../../crates/vibelang-rhai/src/api/group.rs),
  [`Sample`](../../crates/vibelang-rhai/src/api/sample.rs),
  [`Buffer`](../../crates/vibelang-rhai/src/api/buffer.rs), and
  [`SFZ`](../../crates/vibelang-rhai/src/api/sfz.rs) authoring APIs.
- Desired-state maps and `BodyContribution` in
  [`script_state.rs`](../../crates/vibelang-core/src/reload/script_state.rs),
  reload messages in [`message.rs`](../../crates/vibelang-core/src/message.rs),
  and reconciliation in [`runtime.rs`](../../crates/vibelang-core/src/runtime.rs).
- Synthdef body finalization in
  [`vibelang-dsp/src/api.rs`](../../crates/vibelang-dsp/src/api.rs) and the
  versioned public-registration-manifest work described by the
  [API roadmap](../roadmap/api-improvement-roadmap.md).

Current tests pin fragments of those semantics rather than the v2 guarantees:
`test_voice_chained_builders`, `test_pattern_apply_not_playing`,
`test_allocate_buffer_idempotent_same_name`, and
`test_repeated_group_body_records_deterministic_content_order` cover existing
builder, reuse, and merge behavior. Runtime tests such as
`apply_reload_route_finalize_error_aborts_without_clean_complete_log` and
`sync_after_reload_defers_until_staged_reload_applies` prove snapshot/barrier
behavior, but do not prove rollback of earlier backend mutations.

These are facts about v1, not guarantees retroactively imposed on it. The rest
of this document decides the v2 contract.

## Decision summary

V2 separates declaration construction, logical identity, desired state, live
observation, and physical resource ownership:

```text
pure Builder --terminal--> immutable Candidate --accept--> Revision
                                                        |
                                                        v
                                         validate / plan / stage
                                                        |
                                          backend barrier + commit
                                                        |
                                                        v
stable typed Ref ------------------------------> applied logical entity
                                                        |
                                                        v
                                              fresh Observation

ResourceManager: logical resource binding -> physical generation -> readers
```

The normative rules are:

1. V2 factories return detached builders. Builder methods have no
   `ScriptState`, registry, backend, or deployment side effects.
2. A successful terminal validates one declaration, adds immutable candidate
   IR, and returns a stable typed Ref. A Ref is a versioned logical address,
   never a builder clone, backend node, buffer number, or owning pointer.
3. The entire evaluated candidate is validated before acceptance. A validation
   failure discards it without changing desired state, registries, resources,
   or the backend.
4. Every accepted candidate has a monotonic revision and a queryable state.
   Accepted, applied, failed, rejected, and superseded are distinct outcomes.
5. Runtime apply is validate/plan, stage an inactive generation, synchronize,
   commit once, then retire the previous generation. Before commit, the last
   applied generation remains authoritative and unmodified.
6. Duplicate declarations are errors unless the source uses an explicit
   contribution, reference, or override operation.
7. Group bodies are stable, named contributions with artifact ownership.
   Removing a contribution removes only what that contribution owns.
8. A runtime resource manager owns physical Sample, Buffer, and SFZ
   generations. Logical Refs do not allocate or free backend resources.
9. V1 remains the default during migration. V2 requires an explicit language
   major and never changes the meaning of an unversioned v1 script.

## Builders and typed Refs

### Builder contract

A v2 Builder belongs to one evaluation. It is plain configuration plus source
provenance. Cloning a Builder produces an independent value; changing or
terminating one clone cannot change another clone or candidate state.

Factories do not declare anything. A terminal such as `apply`, `start`,
`start_now`, or Voice `run` performs local declaration validation and emits
immutable candidate IR. It returns the corresponding `VoiceRef`, `GroupRef`,
`PatternRef`, `SampleRef`, `BufferRef`, `SfzRef`, or other typed Ref. Terminals
must not be compatibility no-ops. Calling a terminal twice for the same
declaration creates a duplicate and is rejected unless the second operation is
an explicit override.

Synthdef and effect body construction follows the same rule. It may compile
graph IR in memory during candidate construction, but it must not mutate a
process-global registry or deploy to the backend before the candidate is
accepted and staged.

### Ref identity

Every in-memory v2 Ref contains or is validated against all of the following:

| Component | Meaning |
|---|---|
| Language contract | Language major, public-manifest schema version, and the selected engine manifest digest |
| Engine instance | An opaque `EngineInstanceId` fixed for one `ScriptEngine` lifetime |
| Runtime epoch | An opaque epoch changed when the live backend/runtime is rebuilt or rebound |
| Logical address | Stable project namespace, canonical module path, entity kind, canonical group scope, and declaration key |

Entity kind is part of the address, so the same local spelling may be used in
different typed namespaces. Project identity is configured and stable; it is
not an absolute checkout path. Module identity is the canonical import/package
path, not evaluation order. A runtime declaration needs an explicit stable key
or an unambiguous compiler-assigned syntax key. Line numbers, evaluation
ordinals, hash-collision encounter order, and random process IDs are not
logical identity.

Direct Ref use requires an exact language contract, engine instance, and
runtime epoch match. A mismatch is a structured error; it never falls back to
name lookup. Normal accepted/applied revisions do not change Ref identity, so a
Ref remains stable across hot reload within the same engine and runtime epoch.

Refs are not portable serialized capabilities. Persisted configuration may
store a `RefAddress` containing the language major and logical address, but it
omits engine/epoch tokens and must be explicitly resolved in the target engine.
This permits diagnostics and compatibility checks without pretending that a
backend object survived a restart.

Builder properties describe builder configuration. Live or reconciled facts
come only from an observation API; Ref getters must not silently mix desired
and live state.

## Candidate validation, revisions, and atomic apply

### Candidate and revision lifecycle

One complete evaluation produces one immutable candidate. Validation covers at
least typed identity, language/manifest compatibility, duplicate declarations,
contribution ownership, dependency resolution, routes, group structure,
resource descriptors, synthdef IR, and known backend capability requirements.

Candidate construction and validation are transactional for VibeLang-managed
state. A parse, evaluation, group-body, DSP compilation, or semantic-validation
error discards all declarations from that attempt. It changes no desired/live
snapshot and allocates no backend resource. Group-body errors propagate; v2
does not preserve v1's logged-and-swallowed partial body behavior.

Only a fully evaluated and validated immutable candidate is assigned a
monotonic `RevisionId` and accepted into the runtime queue. Pre-acceptance
failures receive an `EvaluationAttemptId` and a rejected diagnostic, but do not
consume a revision or set `failed_revision`. This keeps `failed_revision`
precise: it always names a candidate the runtime accepted and later confirmed
it could not commit.

The per-revision state machine is:

```text
evaluation attempt --reject--> rejected attempt (no revision)
        |
        v
     accepted --> planning --> staging --> committing --> applied
        |            |            |             |
        +------------+------------+-------------+--> failed
        |
        +--> superseded (only before planning/staging)
```

Explicit submissions are FIFO by default. A watcher may request
`replace_pending`; it may supersede only accepted revisions whose planning has
not begun. Every skipped revision remains observable as `superseded_by`, never
as an unexplained gap or failure. Once planning begins, a revision reaches a
truthful terminal outcome before a later revision is committed.

### Apply protocol and commit point

For accepted revision `R`, the runtime must:

1. Plan against one recorded applied revision and capability snapshot. Planning
   is deterministic and has no backend effects.
2. Stage all new or changed synthdefs, resource generations, groups, nodes,
   routes, and links under an inactive generation. Record every staged
   allocation in an exact-once cleanup ledger.
3. Cross a correlated backend synchronization barrier proving that staging is
   complete. A generic queue flush is not sufficient.
4. At the requested quantization time, activate the staged generation through
   one backend-supported root/link/generation switch. Confirm that switch with
   a correlated barrier, then publish the Rust-side snapshot and `applied`
   observation for `R`.
5. Retire the old generation asynchronously under the resource release rules
   below.

The backend-visible activation is the commit point. Before it, no deletion or
in-place mutation may make the previous applied generation unusable. If
planning, staging, validation against changed capabilities, resource load, or
activation fails before a confirmed commit, the runtime cleans only the staged
generation, records `failed(R, phase, diagnostics)`, and keeps the prior
`applied_revision` authoritative.

An implementation may use a different backend primitive only if tests prove
the same observable atomicity. If a backend cannot stage and activate this way,
it must report v2 atomic apply as unavailable rather than silently use phased
v1 reconciliation. If commit acknowledgement is lost and the active generation
is uncertain, the runtime fences new work and reconciles the backend epoch. It
must not guess `applied` or `failed`.

Cleanup after a confirmed commit is not part of the commit transaction. A
retirement/free failure leaves `R` applied, reports degraded resource health,
and quarantines the resource for retry. It does not reinterpret an audible
commit as failed.

### Revision observation and freshness

The runtime keeps a bounded durable-in-process ledger for every accepted
revision and a separately queryable record for rejected attempts. Its summary
exposes at least:

| Field | Contract |
|---|---|
| `accepted_revision` | Highest revision admitted to the queue, not an apply acknowledgement |
| `applied_revision` | Revision whose commit is currently authoritative |
| `failed_revision` | Most recent accepted revision confirmed failed, with phase and diagnostics |
| `pending_revisions` | Accepted revisions not yet terminal |
| `runtime_epoch` | Epoch against which the observation and Ref were validated |
| `observation_sequence` | Monotonic publication sequence for ordering observations |
| `fresh_through_revision` | Highest revision for which this observer has a terminal or current pending record |
| `observed_at` | Runtime monotonic timestamp, plus wall-clock serialization for external clients |

The ledger, rather than the three summary maxima alone, answers whether a
specific revision applied, failed, or was superseded.

`status(ref)` is a nonblocking latest-available read. The result includes the
Ref identity, runtime epoch, applied entity generation, requested minimum
revision if any, revision summary, sequence, timestamp, diagnostics, and an
explicit `stale` reason. `status_at_least(ref, revision, timeout)` waits for an
observation fresh through that revision or returns an explicit timeout/stale
result. An observation is stale when its runtime epoch differs, its source is
disconnected, or `fresh_through_revision` is below the caller's minimum; age
limits are caller policy and must be reported rather than inferred silently.

CLI, HTTP, WebSocket, WASM, and editor clients must carry the same correlated
revision semantics. An evaluation response may say `accepted(R)`; it may say
`applied(R)` only after the exact commit acknowledgement. A generic sync or a
successful channel send cannot upgrade accepted to applied.

## Duplicate declarations

Within one v2 candidate, two declarations of the same fully qualified typed
logical address are a validation error, including byte-identical repeats. The
error identifies both source spans and owners. The rule is independent of
import or evaluation order. The same logical address appearing once in a later
candidate is normal hot-reload replacement, not a duplicate.

V2 has three explicit composition operations whose syntax may be finalized in
P1 without changing these semantics:

| Operation | Meaning |
|---|---|
| Reference/use | Depend on an existing declaration without ownership or implicit creation |
| Contribution/extend | Add independently owned group content under a stable `ContributionId` |
| Override | Add a named patch layer to an existing declaration/field and state the target owner/key |

Two implicit writers remain an error. Two override layers that target the same
field at the same precedence are also an error. Override and contribution
ordering is canonical and visible in candidate IR, never accidental
last-write-wins. Cross-kind reuse of a local name remains valid because typed
kind is part of identity; shared untyped namespaces such as aliases must remain
unique.

## Group and body contribution ownership

A Group has one structural declaration owner. Looking up a `GroupRef` neither
creates the Group nor grants ownership. Structural fields such as parent,
output, gain, and hardware routing belong to the structural declaration unless
an explicit override layer names the field.

Each body/extension has a stable, module-qualified `ContributionId`, supplied
explicitly or assigned from a stable syntax key. It records its source and owns:

- child entity declarations created in that body;
- nested group declarations it explicitly creates;
- effect-chain and routing edges it adds; and
- explicit override layers it contributes.

It does not own entities it merely references. A declaration cannot be owned
by two contributions; intentional shared entities have an independent owner
and are referenced explicitly. A second contribution declaring the same typed
address is a duplicate error.

Contributions aggregate in a documented total order: explicit order first,
then canonical fully qualified `ContributionId` as the deterministic tie
breaker. Effect order and other order-sensitive edges use that order. A source
file import reorder, unrelated edit, or line-number change therefore cannot
silently reorder content.

Each contribution is evaluated into an isolated fragment before candidate
aggregation. Any fragment error rejects the whole candidate. Removing a
contribution in the next applied revision removes its owned child declarations,
edges, and override layers only. Referenced/shared declarations and artifacts
owned by other contributions remain. Removing the last structural owner tears
down the Group after validation confirms there are no dangling contributions;
reads and saved Refs never keep it alive.

## Sample, Buffer, and SFZ resources

### Common ownership model

A per-runtime `ResourceManager` owns physical allocations and their backend
receipts. Candidates contain logical resource declarations. Refs contain
logical identity. Neither owns or directly frees a physical object.

The manager binds a logical Ref to an immutable physical `ResourceGeneration`.
A playback node pins the exact generation it started with; rebinding the Ref on
reload affects new readers, not existing ones. Staged references and committed
references are counted separately so failed staging can undo only its own
claims.

### Type-specific rules

| Resource | Logical identity and reuse | Reload/replacement contract |
|---|---|---|
| Sample | `SampleRef` uses its fully qualified typed key. A physical immutable sample generation may be reused by the strong key `(canonical source, content fingerprint, decode/channel options, loader/decoder version)`. Playback rate, gain, envelope, and routing are not sample-buffer identity. | Re-read/fingerprint the source according to the declared watch policy. Same path with changed content creates a new generation. Decode and allocate the whole replacement while inactive; on failure keep the old binding and free only newly staged work. Commit rebinds new readers; old readers retain the old generation. |
| Buffer | `BufferRef` uses its fully qualified typed key. A mutable buffer is not deduplicated across logical Refs. The same Ref reuses its physical allocation only when frames, channels, sample format, backend, and declared persistence policy remain compatible. | Compatible reload preserves contents. A shape/format change stages a new allocation. V2 requires a declared replacement policy such as `clear` or `copy_overlap`; the v1 adapter may supply its legacy policy. Copy/clear completes before commit. Failure retains the old allocation and contents. |
| SFZ | `SfzRef` uses its fully qualified typed key. An SFZ generation fingerprint covers the root SFZ content, transitive includes, every sample dependency fingerprint, parse/load options, and loader/parser version. Within an instrument, identical dependencies are deduplicated; cross-instrument content sharing is permitted only through the same immutable sample-generation key and refcounts. | Parse and validate the full dependency graph, then stage every required sample/buffer. One missing or failed dependency fails the entire new SFZ generation, releases all newly staged claims exactly once, and preserves the complete old instrument binding. Commit switches new notes to the new generation; old voices pin old dependencies until they end. |

Canonical source identity alone is never sufficient evidence that content is
unchanged. Conversely, a content fingerprint alone does not erase decode
options, loader version, mutability, or backend compatibility from the reuse
key.

### Release point

A physical generation becomes eligible for release only when all of these are
true:

1. no committed logical binding refers to it;
2. no accepted/staged plan holds a provisional claim;
3. no backend reader/node holds a generation lease; and
4. a correlated node-end/resource barrier proves the backend is quiescent.

The manager then issues free exactly once and retains the backend BufferId or
other allocation ID until free is confirmed. A failed or uncertain free
quarantines the ID and retries/reconciles; it cannot return to the allocator.
A fixed delay, including the current-style 500 ms grace, may be an additional
conservative delay but is never the ownership proof.

Shutdown stops readers, crosses a backend barrier, and drains the same ownership
ledger. Resource health and quarantined/leaked allocations are observable.

## Invariants

An implementation is conforming only if all of these remain true:

1. Before candidate acceptance, VibeLang-managed desired state, registries,
   runtime state, and resources are unchanged.
2. Every v2 logical declaration has one typed, versioned, fully qualified
   identity and at most one declaration owner in a candidate.
3. Builders are evaluation-local configuration; Refs are stable logical
   addresses; observations are timestamped live facts. No type impersonates
   another role.
4. A Ref is rejected across language contracts, engine instances, or runtime
   epochs. It never aliases a backend number or pointer.
5. Revisions are monotonic, accepted is never reported as applied, and every
   accepted revision remains queryable with a terminal or pending state.
6. Failure before commit preserves the prior applied snapshot and audible
   generation. Staged cleanup cannot delete a resource owned by that snapshot.
7. A confirmed commit publishes one new authoritative snapshot. Later cleanup
   failure is visible but cannot rewrite commit history.
8. Contribution aggregation and removal are deterministic and ownership-based,
   not evaluation-order- or line-number-based.
9. Each physical resource has one manager, every claim is balanced exactly
   once, readers pin generations, and allocation IDs are not reused before
   confirmed release.
10. V1 and v2 candidates, Builders, Refs, caches, imports, and revision ledgers
    are version-tagged and cannot cross the language-major boundary implicitly.

## V1 compatibility and v2 migration

V1 remains the default while v2 is developed and piloted. Absence of a language
version continues to mean v1 during the compatibility window. The engine takes
language version as an evaluation input; a source directive such as
`// vibe-api: 2`, CLI flag, HTTP field, WASM option, LSP setting, import
resolver, and cache key are adapters to that same input.

The v1 engine preserves its observable eager/deferred builders,
last-write/merge/append duplicate behavior, group-body behavior, IDs, and
resource rules. Internals may eventually lower v1 into candidate IR or expose
revision receipts only if v1 golden tests prove behavior and timing remain
unchanged. V2 strict duplicate, Ref, contribution, or resource policies must
not leak into an unversioned v1 script.

Modules inherit the importer language major unless they declare one. A
cross-major import is rejected by default. Any future adapter is explicit and
may translate immutable data only; live Builders and Refs never cross it.
Module/AST caches key on language contract and manifest digest.

Migration tooling may insert the v2 directive and rewrite factories/terminals
when equivalence is proven. It must stop for manual decisions when it finds
repeated declarations, order-dependent group bodies/effects, saved cloned
Voice builders, anonymous unstable identities, Buffer replacement without a
content policy, or resource/path assumptions. V2 source must never silently run
as v1 when v2 support is disabled.

The rollout sequence is revision/observation plumbing and stage/commit
internals behind compatibility gates, then v2 Builders/Refs and resource
policies, then an opt-in pilot. This ADR itself rolls out none of them.

## Rollback boundary

There are three distinct rollback boundaries:

- **Candidate/apply:** before the activation commit point, discard the staged
  generation and retain the last applied revision. After commit, changing back
  is a new accepted revision targeting an older retained candidate or newly
  evaluated source; commit history is never edited in place.
- **Resource:** old generations remain available only until their ownership
  and reader leases reach the release point. A guaranteed post-commit rollback
  window therefore requires an explicit bounded generation pin and resource
  budget; it cannot depend on already-freed assets.
- **Language rollout:** disable v2 entry points and leave v1 as default. Keep
  the last known v1 source/candidate and revision ledger separate. Never feed a
  v2 candidate or Ref to the v1 engine, and never ignore a v2 source directive
  to make a rollback appear successful.

Atomicity covers VibeLang-managed candidate state, registries, backend graph,
and managed resources. Arbitrary user-authorized external effects through
filesystem, process, network, MIDI-output, or plugin extensions are outside
the transaction and must be documented as such; the runtime cannot roll them
back by cloning `State`.

## Rejected alternatives

| Alternative | Reason rejected |
|---|---|
| Keep eager write-through builders and make `apply` cosmetic | Preserves lifecycle ambiguity and makes candidate validation non-atomic |
| Use cloned builders, names, numeric IDs, or backend pointers as Refs | Conflates configuration, logical identity, and physical lifetime; cannot validate engine/version boundaries |
| Put only a random process/engine ID in persisted identity | Breaks deterministic source and cache identity while still omitting the language contract |
| Treat accepted/enqueued or generic sync as applied | Produces false success and cannot correlate a backend failure to a submission |
| Deep-clone Rust `State` and call later assignment rollback | Cannot undo backend messages, allocations, deployments, transport, MIDI, or external effects |
| Mutate/delete the live generation and reconstruct it after failure | Makes rollback itself fallible and permits audible partial state |
| Global last-write-wins duplicates | Makes imports and evaluation order part of hidden semantics |
| Use evaluation ordinal, source line, or call-site hash alone for contribution ownership | Unrelated edits and reorderings change identity and removal behavior |
| Let group lookup implicitly create or own a Group | Makes reads affect lifetime and prevents precise last-owner teardown |
| Reuse Sample/SFZ resources by name or path alone | Misses changed content, includes, options, dependencies, and loader behavior |
| Resize/reload mutable resources in place | A partial failure can corrupt the currently applied resource |
| Free after a fixed grace period | Time is not proof that backend readers have ended |
| Silently switch existing scripts to v2 | Reinterprets timing, duplicates, identities, and resource contents without author consent |

## Test obligations before runtime rollout

No v2 runtime path may ship without focused unit tests and failure-injection
integration tests for these obligations:

### Builder, Ref, identity, and candidate purity

- zero candidate/runtime state before a terminal; independent Builder clones;
  stable qualified equality; deterministic syntax keys and collision behavior;
- rejection across language major, manifest digest, engine instance, and
  runtime epoch, plus explicit persisted-address re-resolution;
- parse, evaluation, group-body, semantic, synthdef compile, and deploy-stage
  failures produce no partial candidate, registry mutation, or backend effect;
- every entity kind has a terminal that is effective and returns its typed Ref.

### Duplicates and contributions

- a table over every entity kind for exact duplicates, conflicting duplicates,
  valid cross-kind name reuse, explicit reference, contribution, and override;
- import reorder, edits before a call site, contribution rename/removal,
  conflicting fields/effect edges, saved Refs, shared references, and
  last-structural-owner teardown;
- deterministic effect/content order and ownership isolation when one of
  several bodies is removed.

### Revision, observation, and atomicity

- monotonic accepted/applied/failed ledgers, rejected attempts without revision
  consumption, FIFO behavior, explicit pending coalescing, and superseded
  receipts;
- exact submission/barrier correlation and freshness across CLI, HTTP,
  WebSocket, WASM, and in-language status reads, including disconnect,
  timeout, runtime-epoch change, and out-of-order delivery;
- injected failure at planning, every staged allocation/create/route/group/link
  phase, both barriers, activation, and cleanup. Before commit, old live audio,
  current snapshots, routes, and bindings remain; after commit, cleanup failure
  reports degraded health without reverting applied;
- backend acknowledgement loss fences the epoch and never produces a guessed
  terminal result.

### Resource generations

- unchanged Sample reuse and same-path changed-content detection; decode-option
  and loader-version key changes; load failure retaining the old binding;
- compatible Buffer content preservation, explicit clear/copy replacement,
  resize/copy failure, mutable non-aliasing, and old-reader pinning;
- SFZ transitive include/sample fingerprinting, within-instrument deduplication,
  shared dependency refcounts, and one-of-many load failure cleaning every
  newly staged allocation while preserving the old instrument;
- exact-once release, no ID reuse after failed/uncertain free, a reader lasting
  beyond any grace timer, staged-claim rollback, backend-epoch recovery, and
  shutdown cleanup.

### Compatibility and rollback

- v1 golden scripts preserve desired `ScriptState`, IDs, group/effect order,
  messages, resource contents, and event timing with no version directive;
- v2 opt-in, versioned imports/caches, cross-major rejection, disabled-v2
  diagnostics, and migration cases requiring manual policy;
- pre-commit rollback, post-commit forward rollback with pinned resources, and
  fallback to the separately retained last known v1 revision.
