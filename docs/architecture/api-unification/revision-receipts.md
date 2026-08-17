# Shared revision receipts

Status: proposed contract, ready for implementation planning
Assessed tree: `e5a1198a3bb478418042f2b517172f74635742b7`
Candidate tree: `f00c04ca1a1e79d644211eed64fc472214a75d58`

## Decision

Every accepted mutation, regardless of ingress, enters one ordered runtime ledger.
The ledger assigns a `RevisionId` only after parsing, evaluation, validation, and
idempotency checks have accepted the request. Every attempt also has an
`AttemptId`, including attempts rejected before admission.

A receipt has exactly four terminal outcomes:

- `rejected`: no requested effect remains, or rollback restored the last
  confirmed state;
- `superseded`: the attempt or accepted request was deliberately replaced or
  cancelled before its commit boundary, and no requested effect remains;
- `applied`: all effects promised by the request's declared atomicity and
  capability contract crossed their effective boundary;
- `partial`: some effect remains, may remain, or cannot be proved either applied
  or rolled back.

`accepted`, `planning`, `staging`, and `committing` are non-terminal states. Queue
admission, a successful channel send, dispatch to a JavaScript bridge, a backend
socket write, or advancing an in-memory snapshot is never itself `applied`.

This contract refines
[builder-ref-revision-resource-lifetime.md](../builder-ref-revision-resource-lifetime.md):
its public terminal `failed` state becomes `rejected` when the previous confirmed
state was restored, and `partial` otherwise. Its internal phase model and
inactive-generation commit remain the intended atomic implementation.

## Scope and definitions

The contract covers:

- whole-script candidates from CLI startup, CLI watch, HTTP `/eval`, and WASM;
- direct HTTP transport, graph, note, MIDI, recording, and resource commands;
- runtime mutations produced by Rhai terminal operations;
- internal messages needed to stage, reconcile, synchronize, and activate those
  mutations;
- receipt and status projection to CLI, HTTP, WebSocket, Rhai, and WASM.

The contract does not claim that a person heard audio. `Applied` means the
managed runtime and backend crossed the strongest observable activation barrier
advertised by the selected capability. A receipt records that observation and,
where relevant, the musical or backend time at which the change became
effective.

Terms used below:

| Term | Meaning |
|---|---|
| attempt | One submitted input before or after acceptance; identified by `AttemptId`. |
| revision | One accepted ordered mutation; identified by `RevisionId` within a `RuntimeEpoch`. |
| confirmed state | The latest revision whose promised effects are known to be active, or the state restored after a confirmed rollback. |
| effective boundary | The point after which a mutation's promised effect is live: commit swap, backend acknowledgment, or a musical quantization boundary. |
| partial | A terminal condition with applied, failed, or uncertain components; it is not shorthand for an ordinary error. |
| projection | A transport-specific view of the same canonical receipt, never an independent success model. |

## Quantitative inventory

The inventory is frozen to the assessed tree. Generated counts come from
`api/public-api-manifest-v1.json`, `docs/api-surface-assessment.md`, and
`api/http-api-snapshot-v1.json`; message counts were enumerated from
`crates/vibelang-core/src/message.rs`.

| Surface | Assessed inventory | Mutation relevance |
|---|---:|---|
| Canonical manifest | 3,626 entries / 8,431 overloads | Shows the scale of projections that must not invent independent lifecycle semantics. |
| Core, DSP, and extension Rhai surfaces | 786 entries / 875 overloads | 16 builder/handle terminals, 10 call terminals, 134 property setters, 194 non-terminal chains, and 257 calls/call-results. |
| Public types, functions, properties | 34 types; 477 functions / 600 overloads; 275 properties | Builder and handle identities need one receipt-bearing terminal model. |
| Registration declarations / effective Rhai functions | 638 / 6,837 | Hand-maintained per-function receipt behavior is not viable. |
| HTTP routes | 96 total: 27 GET, 69 non-GET | All 69 non-GET routes are mutation ingress and currently lack a shared revision. |
| HTTP mutation families | effects 3, eval 1, fades 7, groups 6, melodies 5, MIDI 16, patterns 6, recordings 2, samples 2, sequences 7, transport 4, voices 10 | The family total is 69. |
| Runtime message envelopes | 15 domains / 74 variants in a native all-features build | Includes infrastructure/internal variants such as `Sync`, `Reload::ApplyStaged`, recording completions, and MIDI reconciliation. |
| WebSocket state sampling | 20 Hz, broadcast capacity 1,024 | Events have no revision or sequence; receiver lag ends the sender. |
| Runtime command queue | capacity 1,024 | `send` only proves admission; `try_send` conflates full and closed. |

The 69 HTTP mutation routes are:

- effects (3): `DELETE /effects/{id}`, `PATCH /effects/{id}`,
  `PUT /effects/{id}/params/{param}`;
- eval (1): `POST /eval`;
- fades (7): `DELETE /fades`, `POST /fades`, and
  `POST /fades/{effect/{id},group/{path},melody/{name},pattern/{name},voice/{name}}`;
- groups (6): `PATCH /groups/{id}`, `PUT /groups/{id}/params/{param}`, and
  `POST /groups/{id}/{mute,solo,unmute,unsolo}`;
- melodies (5): `POST /melodies`, `DELETE /melodies/{id}`,
  `PATCH /melodies/{id}`, and `POST /melodies/{id}/{start,stop}`;
- MIDI (16): `POST /midi/{cc,close,input/open,note/off,note/on,output/open}`,
  `POST /midi/clock/{disable,enable}`,
  `POST /midi/record/{start,stop}`,
  `POST /midi/route/keyboard`, `DELETE /midi/route/{index}`,
  `DELETE /midi/routes`, and
  `POST /midi/transport/{continue,start,stop}`;
- patterns (6): `POST /patterns`, `DELETE /patterns/{id}`,
  `PATCH /patterns/{id}`, `PUT /patterns/{id}/params/{param}`, and
  `POST /patterns/{id}/{start,stop}`;
- recordings (2): `POST /recordings/{id}/{cancel,stop}`;
- samples (2): `POST /samples` and `DELETE /samples/{id}`;
- sequences (7): `POST /sequences`, `DELETE /sequences/{id}`,
  `PATCH /sequences/{id}`, and
  `POST /sequences/{id}/{pause,resume,start,stop}`;
- transport (4): `PATCH /transport` and
  `POST /transport/{seek,start,stop}`;
- voices (10): `POST /voices`, `DELETE /voices/{id}`,
  `PATCH /voices/{id}`, `PUT /voices/{id}/params/{param}`, and
  `POST /voices/{id}/{mute,note-off,note-on,stop,trigger,unmute}`.

These spellings come from `api/http-api-snapshot-v1.json` and are also described
in `docs/interfaces/http-and-websocket.md` and
`docs/reference/generated/http-routes.md`. Braces containing comma-separated
names above are documentation shorthand for the individually counted routes.
WebSocket has no runtime mutation command today: client frames only subscribe
or unsubscribe. It is nevertheless an egress surface and must project receipts.

## Current mutation topology

### End-to-end flow

```text
CLI file/watch --------\
HTTP /eval -------------+--> Rhai compile/evaluate --> ScriptState
WASM execute -----------/            |                     |
                                    | eager DSP globals    |
direct HTTP commands ---------------+---------------------+--> RuntimeHandle
Rhai handle terminals ------------------------------------/       |
                                                               mpsc(1024)
                                                                  |
                                                              Runtime::tick
                                                                  |
                 planning --> asset staging --> ordered reconcile/apply phases
                                                                  |
                 backend socket/JS bridge --> sync/ack --> quantized live swap
                                                                  |
                                  CLI / HTTP / WS / WASM observations
```

There is no common attempt identifier, revision, terminal acknowledgment, or
component ledger across this path today.

### Ingress and egress map

| Surface and exact source | Current ingress | Current success point | Current egress and gap |
|---|---|---|---|
| CLI startup, `crates/vibelang-cli/src/main.rs::run_simple_mode` | `execute_script` evaluates builtins and the selected script, sends `Reload::Apply`, calls `sync_and_wait`, then sends transport start. | Channel admission and a sync notifier; native sync failure still notifies. | Logs only. Initial asset/backend work can fail after evaluation, and there is no revision to query. |
| CLI watch, `run_simple_mode` watch task | Notify events are filtered to `.vibe`, debounced for 100 ms, and evaluated by `execute_script`. MIDI AST/callbacks are replaced before reload admission. | The log `Reload successful` follows `RuntimeHandle::send`, not apply. | Parse/eval errors are logged. Earlier DSP deployment or MIDI callback changes can precede a later failure. A queue of size 16 plus debounce coalesces files without receipt supersession. |
| HTTP eval bridge, `run_simple_mode` and `evaluate_code` | `POST /eval` passes source over a standard channel to a CLI-owned evaluation task, then sends `Reload::Apply`. | `success: true` means evaluation and runtime channel admission. | HTTP 400 covers evaluation/send failure; post-admission apply failure is only a runtime log. |
| Rhai engine, `crates/vibelang-rhai/src/engine.rs::ScriptEngine::{execute,execute_file,execute_file_full}` and `context::take_state` | A fresh engine compiles/runs code against process-global registries and builds `ScriptState`. | Returning `ScriptState` means evaluation completed, not runtime application. | An evaluation error is returned after cleanup, but DSP/global side effects may already have occurred. |
| DSP terminals, `crates/vibelang-dsp/src/api.rs::{set_deploy_callback,deploy_synthdef_ir,deploy_fx_ir,SynthDefBuilderHandle::body,SynthDefBuilderHandle::body_map,FxBuilderHandle::body,FxBuilderHandle::body_map}` | Builder terminals register IR, encode graphs, invoke the deployment callback, and record a deployed hash. | Callback success. CLI callback uses `RuntimeHandle::try_send`; WASM installs a no-op callback and later performs bridge loading. | IR is registered before encoding/deploy; hash is recorded after queue admission, before backend confirmation. A later script error or backend failure can leave a hash that suppresses retry. |
| Rhai state builders and handles, `crates/vibelang-rhai/src/api/{global,group,voice,pattern,melody,sequence,sample,buffer,sfz,recording,route}.rs` and `api/midi/*.rs` | Property setters and terminals mutate the evaluation context or synchronize handle state. | Usually local builder completion, state insertion, or runtime queue admission. | No uniform receipt type; terminal vocabulary and eager/live behavior vary by family. |
| Direct HTTP commands, `crates/vibelang-http/src/routes/*.rs` via `lib.rs::AppState::send` | 69 non-GET routes map DTOs to `Message` variants. | `RuntimeHandle::send` admission. Some handlers sleep 10 or 50 ms and then read state. | Response DTOs vary. Sleeps do not establish apply. Several accepted fields are ignored or become success-shaped no-ops, as catalogued in `docs/interfaces/http-and-websocket.md`. |
| HTTP direct state vs script reload, `crates/vibelang-core/src/runtime.rs::Runtime::build_reload_diff` and `reload::calculate_diff` | Direct commands mutate live state while reload compares old script snapshot to new script snapshot. | Per-command handler return, generally invisible to caller. | Direct tweaks may be intentionally preserved, but there is no base revision, conflict marker, or unified order explaining the result. |
| WebSocket, `crates/vibelang-http/src/websocket.rs::{WebSocketEvent,handle_socket,state_broadcaster}` | Client input only changes connection-local subscriptions. | A subscribe/unsubscribe response. | Generic `type/timestamp/data` events have no epoch, revision, sequence, resume token, or receipt. Invalid frames are ignored; lag closes the send loop. |
| WASM runtime execute, `crates/vibelang-wasm/src/lib.rs::VibelangRuntime::{new,init,execute,tick}` | Constructors clear process-global registries; `init` loads builtins; `execute` clears registries again, evaluates script, manually sends encoded synthdefs/effects to the JS bridge, then queues reload. | `ExecutionResult.success` is constructed from evaluation; bridge/reload failures are warnings and do not change it. | Reload is not applied until the embedder calls `tick`. Multiple successful executes may only be queued. `load_synthdef_to_supersonic` also returns success when the reflected bridge is missing. |
| WASM runtime transport, `VibelangRuntime::{start,stop,stop_all}` | Sends transport messages when runtime exists. | Queue admission; `start` before initialization warns and returns success, `stop` returns success. | No receipt or later failure channel. |
| WASM legacy compiler, `VibelangEngine::{new,execute,clear_synthdefs}` | `new`/`clear_synthdefs` clear global DSP registries; `execute` evaluates into `last_state` and compiled registries without a runtime. | `ExecutionResult.success` means parse/evaluation completed. | It is compile-only, but global registry mutation and clearing are unversioned. V2 replaces this with a side-effect-free `CompiledCandidate`; the legacy adapter does not claim a runtime revision. |
| WASM backend, `crates/vibelang-core/src/backends/web_scsynth.rs::WebScsynthBackend` | Awaits a JavaScript Promise returned by `js_send_osc` or `js_load_synthdef`. | Promise resolution; backend `sync` uses the trait's no-op default. | Promise resolution proves bridge acceptance, not AudioWorklet execution or audible activation. |

Concrete HTTP false-positive cases are not merely timing races.
`routes/melodies.rs::update_melody` and
`routes/sequences.rs::update_sequence` warn about unsupported fields and return
the current object without sending a mutation. The assessed interface inventory
also traces ignored group `quantization_beats`, voice `gain/sample/sfz`,
pattern `pattern_string`, sequence-clip `once`, and MIDI recording `quantize`
fields to `crates/vibelang-http/src/routes/{groups,voices,patterns,sequences,midi}.rs`.
These become pre-admission `rejected` diagnostics in V2, not `applied` no-ops.

### Core queue, reconcile, and apply

`crates/vibelang-core/src/runtime.rs::RuntimeHandle::{send,try_send}` writes to
the bounded runtime channel. `Runtime::tick` drains messages and logs
`handle_message` errors without a route back to the submitter.

Native whole-script reload uses
`pending_reloads: VecDeque<ScriptState>` and
`Runtime::advance_reload_queue`. Asset staging returns through the internal
`Reload::ApplyStaged` message. The queue is FIFO: there is no request identity,
deduplication, cancellation, or deliberate supersession. WASM skips the native
off-thread staging path and applies when ticked.

`Runtime::apply_reload_inner` performs the following observable phases:

| Order | Phase and source symbols | Failure behavior now | Partial/stale risk |
|---:|---|---|---|
| 1 | Diff/planning, `Runtime::build_reload_diff`, `reload::calculate_diff`, and `Runtime::apply_voice_port_reconciles` | Voice bus reconciliation errors are logged and skipped while planning continues. | Planning can mutate live allocations before a commit exists. |
| 2 | Asset staging, `Runtime::{reload_staging_plan,spawn_reload_staging,advance_reload_queue}` | Sample loads run concurrently, SFZ loads sequentially; failed assets are omitted and application continues. | A reload can apply with a subset of requested assets, with no component report. |
| 3 | Transport, `Runtime::phase_apply_transport_changes` | Errors propagate. Tempo can change before a later time-signature failure. | Old snapshot remains even though transport may have changed. |
| 4 | Stop/delete old resources | Many backend results are discarded or only logged. | Runtime state can forget a node whose backend free failed. |
| 5 | Create samples, groups, voices, patterns, melodies, sequences | Most per-object errors are logged and application continues. | Requested graph may be only partly present. |
| 6 | Update parameters and contents | Many handler results are ignored. Removed parameters can disappear from state while the backend retains the last value. | State/backend divergence is not enumerated. |
| 7 | Output and input route finalization | Errors propagate after earlier phases. | A late route error leaves earlier changes live while script snapshot remains old. |
| 8 | Effects and chain ordering | Most errors are logged. Chain order state can update before backend node movement fails. | State claims an order that backend may not have. |
| 9 | Group finalization and sync | Results, including the 10 ms sync attempt, are ignored. | Dependent nodes may be activated without a confirmed barrier. |
| 10 | Fades and restarts | A failed fade start can still be inserted in state; start failures are generally logged/ignored. | Configuration can claim an operation that never began. |
| 11 | Parameter routes | Errors propagate near the end. | Nearly all preceding phases may remain despite failure. |
| 12 | MIDI routes | Failures are predominantly logged and application continues. | External device/routing state can diverge. |
| 13 | Snapshot advance | Reached only after the preceding method returns success. | On a propagated error, the old script snapshot coexists with any leaked live/backend changes. |

`Runtime::advance_reload_queue` logs an apply failure and moves to the next
candidate. No submitter learns which phases completed. Existing test
`runtime::tests::apply_reload_route_finalize_error_aborts_without_clean_complete_log`
proves the missing clean-complete log on a late failure; it does not implement
rollback.

### Backend synchronization and audible/live activation

The backend observation strength varies:

| Boundary | Exact implementation | What it proves | What it does not prove |
|---|---|---|---|
| Runtime queue | `RuntimeHandle::send` | The message entered the Tokio channel. | It was handled or applied. |
| UDP/socket write | `ScsynthBackend` commands using `send_msg` | The OS accepted a packet write. | scsynth executed the command. |
| Synthdef reply | `crates/vibelang-core/src/backends/scsynth.rs` `/done` and correlated `/fail` handling | The correlated synthdef load succeeded or failed. | Other graph commands succeeded. |
| scsynth sync | `Backend::sync` / `Runtime::sync_with_retry` | `/synced` establishes server processing order through the barrier. | A human heard audio, or previously scheduled audio can be recalled. |
| Native `sync_and_wait` | `RuntimeHandle::sync_and_wait` and `Message::Sync` handling | The runtime processed the sync message. | Backend sync succeeded: native code warns but always notifies. |
| JS bridge Promise | `WebScsynthBackend` methods through `js_send_osc` and `js_load_synthdef` | The bridge Promise resolved. | The worklet rendered the change. |
| Quantized content swap | `PatternState::{queue_content_update,apply_pending_swap}`, `MelodyState::{queue_content_update,apply_pending_swap}` | The new content became active at a scheduling boundary. | Events already sent inside the 50 ms lookahead were cancelled. |

Patterns and melodies store only one `pending_content`. A newer update replaces
the older pending value without identifying it as superseded. For playing
objects, application is delayed until a quantized boundary enters the scheduling
window. Events already dispatched under the old content can remain audible for
the lookahead tail. Therefore a receipt for such a change stays non-terminal
until the swap and reports `effective_at` plus `audible_tail_until` when known.

## Current failure, stale-state, and retry semantics

### Failure timing

Failures can appear at five materially different times:

1. decode/parse/evaluate, before runtime admission;
2. eager evaluation side effects, before a later evaluation failure;
3. channel or bridge admission;
4. planning, staging, or one of the ordered live/backend phases;
5. backend acknowledgment or deferred musical activation.

Current projections collapse these times. A CLI log, HTTP 2xx, or WASM
`success` can precede phases 4 and 5. Conversely, errors logged in phases 4 and
5 cannot be correlated to the originating surface.

### Stale observations

- HTTP mutation handlers can read state after fixed sleeps, so a response may be
  pre-apply or from another mutation.
- REST snapshots, WebSocket samples, and CLI logs lack a shared
  `RuntimeEpoch/RevisionId` watermark.
- WebSocket wall-clock timestamps do not order events reliably and there is no
  gap/resume protocol.
- An apply failure can leave the script snapshot stale relative to runtime state
  and backend state.
- Direct commands have no expected/base revision, so concurrent writes are
  silently last-observed rather than explicitly reconciled.
- A DSP deployed hash can be newer than confirmed backend contents.

### Supersession, cancellation, and idempotency

There is no API-level idempotency key, cancellation endpoint, or base-revision
precondition. Reload candidates are FIFO. CLI debounce merely drops/coalesces
file-system notifications. A queued pattern/melody content value can overwrite
an earlier one, but the displaced request has no identity or terminal outcome.
A retry of a direct HTTP command creates another command; a retry of a note or
recording command can be observably unsafe.

### Rollback feasibility

Pure candidate construction, validation, asset staging, and inactive-generation
planning are rollback-safe because they have not touched the live graph. A
single generation pointer/root swap is rollback-capable if the old generation is
retained until activation is confirmed.

The current ordered in-place update is not generally rollback-safe:

- freed nodes, MIDI output, file/recording operations, note events, and already
  scheduled audio are external effects;
- ordinary scsynth commands do not all have correlated acknowledgments;
- parameter and routing updates overwrite state needed to reconstruct the old
  graph;
- process-global DSP registries and deployed hashes are outside the runtime
  snapshot;
- the JS bridge has no strong sync barrier;
- compensation can itself fail.

The contract therefore never turns a failed compensation into `rejected`.
Unless restoration is confirmed, the outcome is `partial` and the runtime is
fenced from further atomic mutations until reconciled, reset, or explicitly
continued under best-effort policy.

## Canonical receipt contract

The following is normative, language-neutral IDL written in Rust-like notation.
Serialized names are the snake-case names shown here.

```rust
type AttemptId = String;       // globally unique, opaque
type RuntimeEpoch = String;    // changes on runtime reset/reconstruction
type RevisionId = u64;         // strictly increasing within one epoch
type EventSequence = u64;      // strictly increasing within one epoch
type Digest = String;          // algorithm-prefixed canonical request digest

struct MutationReceipt {
    schema_version: u16,
    attempt_id: AttemptId,
    runtime_epoch: RuntimeEpoch,
    revision: Option<RevisionId>,
    event_sequence: EventSequence,
    request: RequestIdentity,
    state: ReceiptState,
    previous_confirmed_revision: Option<RevisionId>,
    timestamps: ReceiptTimestamps,
    diagnostics: Vec<Diagnostic>,
}

struct RequestIdentity {
    kind: MutationKind,
    source: MutationSource,
    digest: Digest,
    idempotency_key: Option<String>,
    expected_revision: Option<RevisionId>,
    atomicity: Atomicity,
    supersession: SupersessionPolicy,
}

enum ReceiptState {
    Evaluating { phase: PreAcceptancePhase },
    Accepted { queue_position: Option<u32> },
    Planning,
    Staging { completed: u32, total: u32 },
    Committing { phase: CommitPhase },
    Terminal(TerminalOutcome),
}

enum TerminalOutcome {
    Rejected(Rejected),
    Superseded(Superseded),
    Applied(Applied),
    Partial(Partial),
}

enum PreAcceptancePhase {
    Decode,
    Parse,
    Evaluate,
    Validate,
    IdempotencyCheck,
    ExpectedRevisionCheck,
    CapabilityCheck,
    Admission,
}

enum CommitPhase {
    Reconcile,
    Activate,
    BackendBarrier,
    MusicalBoundary,
    ExternalEffects,
    Rollback,
}

enum MutationSource {
    Cli { mode: CliMode, source: Option<String> },
    Http { method: String, path: String, request_id: String },
    Rhai { engine_id: String },
    Wasm { instance_id: String },
    Internal { parent_revision: RevisionId },
}

enum CliMode {
    Startup,
    Watch,
    EvalServer,
}

struct ReceiptTimestamps {
    submitted_at: Timestamp,
    accepted_at: Option<Timestamp>,
    last_transition_at: Timestamp,
    terminal_at: Option<Timestamp>,
}

struct Diagnostic {
    severity: DiagnosticSeverity,
    code: String,
    message: String,
    component_path: Option<String>,
    source_span: Option<SourceSpan>,
}

enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

struct SourceSpan {
    source: String,
    line: u32,
    column: u32,
    end_line: u32,
    end_column: u32,
}
```

### Identity and ordering

- `AttemptId` exists before parsing and is returned for every submission.
- `RevisionId` is allocated exactly once, after canonical evaluation/validation,
  idempotency, expected-revision, and capability checks accept the mutation.
- Rejected pre-admission attempts have `revision: null`. Rejected post-admission
  revisions retain their revision; holes are permanent and observable.
- `RuntimeEpoch` changes whenever the runtime or receipt ledger is reconstructed
  without continuity. A revision is only meaningful with its epoch.
- `EventSequence` increments for every canonical receipt transition and status
  event. It never resets within an epoch and is the WebSocket resume/dedup key.
- IDs and counters are integers/opaque strings on the wire, not wall-clock
  timestamps. Timestamps are diagnostic only.

Monotonicity invariants:

1. An attempt's source, digest, idempotency identity, epoch, and allocated
   revision never change.
2. Receipt state never regresses, staging progress never decreases, and one
   terminal event is emitted exactly once in the canonical ledger.
3. Accepted revisions are planned and cross their activation boundary in
   `RevisionId` order. A higher revision cannot become `applied` until every
   lower accepted revision is terminal.
4. `accepted_through`, `last_confirmed_revision`, and `event_sequence` never
   decrease within an epoch. Resetting continuity creates a new epoch rather
   than lowering a counter.
5. Projection delivery may duplicate or reorder packets, but consumers restore
   canonical order by `EventSequence` and never overwrite a newer receipt with
   an older one.

### Request policy

`MutationKind` distinguishes at least:

```rust
enum MutationKind {
    Candidate { origin: CandidateOrigin },
    Command { domain: MessageDomain, operation: String },
    Compensation { for_revision: RevisionId },
}

enum CandidateOrigin {
    ScriptFile,
    WatchReload,
    HttpEval,
    RhaiHost,
    WasmRuntime,
    WasmCompiler,
}

enum MessageDomain {
    Transport,
    SynthDef,
    Sample,
    Sfz,
    Recording,
    Group,
    Voice,
    Pattern,
    Melody,
    Sequence,
    Effect,
    Fade,
    Reload,
    Sync,
    Midi,
}

enum Atomicity {
    Required,
    BestEffort,
}

enum SupersessionPolicy {
    Fifo,
    ReplacePending { key: String },
}
```

`Required` is the V2 default for candidate/graph mutations. If selected
capabilities cannot provide a rollback-safe inactive-generation commit, the
request is rejected during capability validation, before a revision is
committed. `BestEffort` is explicit and is the compatibility mode for legacy
in-place operations or inherently external effects.

`ReplacePending` may supersede only a revision that has not crossed into
`planning`. The key is a declared domain, such as one CLI watch source or one
pattern's pending content. FIFO is the default for commands whose order is
musically meaningful.

### Terminal payloads

```rust
struct Rejected {
    phase: FailurePhase,
    code: String,
    message: String,
    rollback: RollbackState, // NotNeeded or Confirmed only
}

struct Superseded {
    reason: SupersessionReason,
    by_revision: Option<RevisionId>,
}

enum SupersessionReason {
    Replaced,
    Cancelled,
}

struct Applied {
    effective_at: EffectiveAt,
    confirmations: Vec<Confirmation>,
    components: Vec<ComponentOutcome>,
    audible_tail_until: Option<EffectiveAt>,
}

struct Partial {
    phase: FailurePhase,
    code: String,
    components: Vec<ComponentOutcome>,
    rollback: RollbackState,
    fenced: bool,
    last_confirmed_revision: Option<RevisionId>,
}

enum FailurePhase {
    Decode,
    Parse,
    Evaluate,
    Validate,
    Idempotency,
    ExpectedRevision,
    Capability,
    Admission,
    Planning,
    Staging,
    Reconcile,
    Activate,
    BackendBarrier,
    MusicalBoundary,
    ExternalEffect,
    Rollback,
}

struct ComponentOutcome {
    path: String,
    action: String,
    state: ComponentState,
    effective_at: Option<EffectiveAt>,
    confirmation: Option<Confirmation>,
    diagnostic: Option<Diagnostic>,
}

enum ComponentState {
    Applied,
    Failed,
    Uncertain,
    NotStarted,
}

struct EffectiveAt {
    observed_at: Timestamp,
    musical_beat: Option<String>,
    backend_time_seconds: Option<f64>,
}

enum RollbackState {
    NotNeeded,
    Confirmed,
    Failed,
    Unavailable,
    Uncertain,
}

enum Confirmation {
    RuntimeCommit,
    BackendBarrier { backend: String, token: String },
    MusicalBoundary { beat: String, backend_time: Option<f64> },
    ExternalAcknowledgment { system: String, token: String },
}

type Timestamp = String; // RFC 3339 UTC on JSON transports
```

`Rejected.rollback` cannot contain `Failed`, `Unavailable`, or `Uncertain`.
Those cases are `Partial`. `Applied.components` are all successful and include
the promised scope, not merely the subset that happened to work.

Each `ComponentOutcome` identifies a stable component path such as
`voice/bass`, `route/main-output`, `sample/kick`, `midi-output/device-id`, or
`transport/tempo`; its planned action, observation strength, and effective
boundary are explicit. Partial receipts must enumerate every planned component
exactly once as `Applied`, `Failed`, `Uncertain`, or `NotStarted`. At least one
component in `Partial` is `Applied` or `Uncertain`; if nothing remains or may
remain, the terminal outcome is `Rejected` instead.

### State transition rules

Allowed transitions are:

```text
evaluating -> rejected
evaluating -> superseded
evaluating -> partial       legacy eager-effect compatibility only
evaluating -> accepted
accepted  -> rejected
accepted  -> superseded
accepted  -> partial        only when effect/dispatch uncertainty is discovered
accepted  -> planning
planning  -> rejected
planning  -> partial
planning  -> staging
planning  -> committing
staging   -> rejected
staging   -> partial
staging   -> committing
committing -> rejected    only after confirmed restoration
committing -> applied
committing -> partial
```

No terminal state transitions to another terminal state. Corrections create a
new compensation revision and link to the original. Duplicate transport
delivery replays the latest canonical receipt; it does not add a transition.
V2 evaluation and planning are side-effect-free, so their `partial` edges exist
only for honest legacy projection, crash recovery, or an implementation defect.
A pre-admission partial attempt has no revision but still fences the runtime.

`Superseded` is deliberately not available after planning starts. A cancellation
request after that boundary is rejected with `too_late_to_cancel` and may propose
a new compensating mutation. If a racing cancellation stops only part of an
operation, the original revision is `partial`, never `superseded`.

### Idempotency

The idempotency namespace is
`(runtime_epoch, authenticated_caller_namespace, idempotency_key)`.

- Same key and same canonical digest returns the same `AttemptId`, revision, and
  latest receipt without resubmission.
- Same key and different digest is a pre-admission `rejected` attempt with code
  `idempotency_conflict` and no new revision.
- Keys remain valid for at least the published receipt-ledger retention window.
- Clients must be told when a key has expired; the server must not silently
  reinterpret an expired retry as the original operation.
- No key means each submission is a new attempt. Non-idempotent operations such
  as note-on, record-start, and external MIDI output should require a key in V2
  remote APIs.

`expected_revision` is checked against the latest confirmed revision at
acceptance and recorded as the planning base. Because an earlier pending
revision may terminate first, it is checked again immediately before planning.
A mismatch at acceptance is a pre-admission `rejected` attempt; a mismatch on
recheck is a post-admission `rejected` revision, both with
`revision_conflict` and no requested effect. V2 replacement, delete, and
whole-candidate APIs require the field unless the caller explicitly selects
unconditional best effort. Ordered event-like commands may omit it.

### Cancellation

Cancellation is itself an idempotent control request addressed to `AttemptId` or
`RevisionId`. It does not enter the musical revision sequence unless it must
produce a compensating mutation.

- evaluating/accepted: stop work; terminal `superseded { reason: cancelled }`;
- planning or later: return `too_late_to_cancel` without changing the original;
- terminal: return the existing terminal receipt;
- unknown/expired: return a typed `receipt_not_found` rejection.

### Runtime status

The canonical summary is:

```rust
struct RuntimeMutationStatus {
    schema_version: u16,
    runtime_epoch: RuntimeEpoch,
    event_sequence: EventSequence,
    accepted_through: Option<RevisionId>,
    last_confirmed_revision: Option<RevisionId>,
    live_state: LiveState,
    pending: Vec<PendingRevision>,
    receipt_window: ReceiptWindow,
}

enum LiveState {
    Clean,
    Partial { revision: RevisionId, fenced: bool },
    Unknown { since_revision: RevisionId, fenced: bool },
}

struct PendingRevision {
    attempt_id: AttemptId,
    revision: RevisionId,
    state: ReceiptState,
    expected_revision: Option<RevisionId>,
}

struct ReceiptWindow {
    first_event_sequence: EventSequence,
    last_event_sequence: EventSequence,
    first_revision: Option<RevisionId>,
    last_revision: Option<RevisionId>,
    expires_before: Option<Timestamp>,
}
```

`accepted_through` is the largest allocated revision; it is not a success
watermark. `last_confirmed_revision` advances only to an `applied` revision.
Rejected and superseded revisions leave it unchanged, including when rollback
confirmed restoration of that previous revision; a partial revision never
advances it. An applied compensation is a new revision and can advance it.
Every state/snapshot projection carries the epoch, event sequence, and
`last_confirmed_revision` that make it fresh.

### Event contract

`ReceiptEvent` carries the full current receipt, not a transport-specific delta:

```rust
struct ReceiptEvent {
    schema_version: u16,
    runtime_epoch: RuntimeEpoch,
    event_sequence: EventSequence,
    previous_event_sequence: Option<EventSequence>,
    receipt: MutationReceipt,
}
```

Delivery is at least once. Consumers deduplicate by epoch/sequence. On a gap,
they query status and receipts from the retained ledger, then resume after a
sequence. If the requested sequence predates retention, the server sends a typed
`reset_required` event with the current status. A WebSocket lag never silently
continues and never masquerades as a clean stream.

## Projection rules

The canonical contract is authoritative; projections may omit fields only when
they can be recovered through an included receipt URL/handle.

### CLI

- Startup and watch allocate an attempt and print its short ID immediately.
- Watch uses `ReplacePending { key: canonical_file_path }` only before planning;
  replaced attempts visibly end as `superseded`.
- `Reload successful` is replaced by terminal output such as
  `revision 42 applied at beat 128` or a component summary for `partial`.
- Parse/evaluation rejection shows an attempt ID and no revision.
- `--wait applied` is the interactive default; `--wait accepted` is an explicit
  low-latency option and prints that the operation is still pending.

### Rhai

- Builders stay pure candidate data until a named terminal operation submits
  them.
- Whole-script evaluation produces one candidate attempt; DSP terminals add
  synthdefs/effects to that candidate instead of mutating process-global
  deployment registries.
- Runtime handles expose `receipt()`/`status()` and terminal operations return a
  typed `RevisionRef` rather than success-shaped unit values.
- Script evaluation diagnostics and runtime receipt diagnostics use the same
  code/path schema.
- Extension operations with unavoidable file, process, network, or device
  effects declare an external component and best-effort atomicity; they cannot
  be hidden inside an atomic candidate.

### HTTP

- V2 mutation requests return `202 Accepted` with the canonical current receipt,
  `Location: /v2/receipts/{attempt_id}`, and epoch/revision headers when
  allocated.
- `GET /v2/receipts/{attempt_id}` returns the latest receipt.
- `DELETE /v2/receipts/{attempt_id}` requests cancellation under the rules above.
- `GET /v2/mutation-status` returns `RuntimeMutationStatus`.
- `Prefer: wait=terminal` may wait up to a declared timeout. Timeout returns 202
  and the still-pending receipt, not success.
- Every one of the 69 non-GET route operations maps to `MutationKind::Command`
  or `Candidate`. Fixed 10/50 ms sleeps are removed.
- `POST /eval` returns parse/evaluation rejection or the same accepted/terminal
  receipt as file and WASM execution.
- State GET responses include epoch, `last_confirmed_revision`, and
  `event_sequence`.

### WebSocket

- V2 remains receipt/status egress, not a second mutation command protocol.
- Clients subscribe to `receipt.*` and `status` with `after_event_sequence`.
- Every event is a `ReceiptEvent` or typed gap/reset event.
- Legacy playback events remain available but gain the same epoch, sequence, and
  freshness watermark in V2.
- Malformed commands receive typed protocol errors. Broadcast lag triggers
  gap/reset recovery rather than ending without explanation.

### WASM

- `execute_v2`, `start_v2`, `stop_v2`, and `stop_all_v2` return an attempt handle
  whose current value is the canonical receipt.
- An embedder can await terminal status, subscribe to `ReceiptEvent`, or poll.
- The handle remains pending until `tick` drives application. It does not return
  `applied` merely because evaluation or bridge dispatch succeeded.
- Missing runtime, missing bridge, Promise rejection, and bridge acknowledgment
  timeout are typed receipt failures.
- The web backend advertises its actual confirmation capability. Without a
  worklet/backend acknowledgment it cannot accept `Atomicity::Required` and uses
  explicit best effort.
- Legacy `VibelangEngine::execute` becomes a compile-only
  `CompiledCandidate` result with an evaluation attempt ID but no runtime
  revision. `clearSynthdefs` is deprecated with the process-global registries.

### Internal runtime and backend

- All `Message` domains carry a `MutationContext` with attempt/revision,
  component path, idempotency identity, and reply sink.
- Internal `Reload::ApplyStaged` and `Sync` preserve that context; they never
  allocate another public revision.
- `Runtime::tick` records a transition before logging diagnostics. No
  receipt-bearing error is log-only.
- `sync_and_wait` becomes a real result-bearing barrier. The native implementation
  propagates backend sync failure instead of notifying success unconditionally.
- The inactive-generation commit owns synthdef hashes, assets, graph state,
  MIDI plans, and script snapshot. A hash becomes deployed only after the
  backend acknowledgment required by the selected capability.

## Atomicity and partial-apply policy

The implementation target for `Atomicity::Required` is:

1. parse/evaluate into a side-effect-free candidate;
2. validate names, references, capabilities, resources, and base revision;
3. stage synthdefs, samples, SFZs, and inactive graph objects under the revision;
4. obtain required backend acknowledgments without exposing the generation;
5. activate one generation root/pointer at the effective boundary;
6. cross the backend or musical barrier;
7. publish `applied` and retire the old generation only after confirmation.

Failure before step 5 is `rejected`. Failure after step 5 is:

- `rejected` only if swapping back and the confirmation barrier prove restoration;
- otherwise `partial` with exact components and `fenced: true`.

Operations that cannot fit this transaction—already-emitted MIDI/note events,
recording/file I/O, external process/network calls, or backends without an
activation barrier—must be isolated as declared best-effort components. A
mixed request is rejected under `Atomicity::Required` unless its external
effects can be deferred until after atomic activation and their failure policy
is explicitly accepted. Under best effort, any mixed success is terminal
`partial`.

## Compatibility and migration

### Phase 0: contract and correlation

- Add canonical IDs, receipt storage, transition validation, and
  `MutationContext` without changing existing route paths.
- Adapt all current ingress to allocate attempts and all message handling to
  publish receipts.
- Retain legacy logs and DTOs, but attach receipt IDs/URLs. Never infer
  `applied` from an old success flag.
- Add capability discovery fields for atomic commit, backend barrier, musical
  boundary, cancellation window, and ledger retention.

### Phase 1: honest legacy best effort

- Project current in-place reconcile as `Atomicity::BestEffort`.
- Convert every logged/ignored phase result into a component result.
- Any leaked or uncertain effect becomes `partial`.
- Remove HTTP sleeps, queue-admission success language, native sync false
  positives, and WASM bridge warnings that still return success.
- Preserve legacy response shapes for one compatibility window; add
  `deprecated_success_semantics: queue_admitted` or equivalent metadata and a
  receipt link.

### Phase 2: V2 projections

- Introduce the HTTP, WebSocket, CLI, Rhai, and WASM projections above from one
  generated schema.
- Require idempotency keys and expected revisions where specified.
- Add resume/gap recovery and a bounded queryable ledger.
- Deprecate `ExecutionResult.success` and heterogeneous terminal unit/bool
  returns in favor of `RevisionRef`/`MutationReceipt`.

### Phase 3: atomic candidate apply

- Implement side-effect-free candidates and inactive revision generations as
  decided in `builder-ref-revision-resource-lifetime.md`.
- Negotiate `Atomicity::Required` only on capable backends.
- Move DSP deployment registries and hashes into staged generation ownership.
- Hold quantized revisions through their real effective boundary and disclose
  unavoidable old-event tails.

No migration step may translate a known `partial` outcome into a legacy success.
Legacy clients may receive less detail, but not a stronger claim.

## Rejected alternatives

| Alternative | Why rejected | Tradeoff retained by the chosen contract |
|---|---|---|
| Keep per-surface success types and add better wording | The same mutation can enter through five surfaces and fail after any of them returns. Wording cannot correlate later failure or stale state. | Projections may remain ergonomic, but derive from one schema. |
| Treat queue admission as applied | It ignores runtime tick, staging, reconcile, backend, and quantized activation. | `accepted` is a first-class low-latency state. |
| Use one timestamp as revision | Wall clocks can collide, regress, and do not define runtime order. | Diagnostic timestamps remain alongside epoch-scoped counters. |
| Allocate a revision before parse | It fills the ordered runtime ledger with inputs that never became mutations and complicates expected-revision semantics. | `AttemptId` gives pre-accept failures stable identity. |
| Use `failed` for every non-success | It hides whether effects leaked and whether retry/rollback is safe. | `rejected` and `partial` encode the safety distinction; error codes retain detail. |
| Add `cancelled` as a fifth terminal state | Cancellation before planning has the same no-effect displacement invariant as replacement. | `superseded.reason` distinguishes cancelled from replaced. |
| Allow supersession at any phase | Stopping in-place staging/commit can itself create partial state. | Supersession is cheap and deterministic only before planning; later work needs compensation. |
| Promise exactly-once delivery | Network/worker restarts make it impractical and unnecessary. | At-least-once events plus stable idempotency and monotonic sequence are testable. |
| Declare applied when the script snapshot advances | Backend and quantized musical state can lag or fail. | `effective_at` and confirmation strength state the actual boundary. |
| Claim acoustic audibility | No microphone/listener observation exists and scheduled tails cannot always be recalled. | Backend/musical confirmation plus `audible_tail_until` is honest and useful. |
| Roll back by replaying the old script | External effects and unacknowledged backend commands are not invertible, and replay can fail. | Retained inactive generations enable bounded confirmed rollback where supported. |
| Put mutations on WebSocket as well as HTTP | It creates another ingress command/ack protocol without a demonstrated need. | WebSocket is the resumable live receipt/status projection; HTTP/Rhai/WASM remain ingress. |

## Failure-injection seams

The assessed tree already has useful but incomplete seams:

| Boundary | Existing seam/evidence | Required extension |
|---|---|---|
| Rhai compile/evaluate | `ScriptEngine` returns compile/eval errors; engine tests exercise execution. | Assert no candidate registry, deploy hash, queue message, or revision leaks on every evaluation failure point. |
| DSP deploy callback | `set_deploy_callback` is injectable; `deploy_synthdef_ir` orders registry/hash work visibly. | Inject encode failure, callback rejection, queue full, backend reject, and ack loss; verify hash advances only at confirmed deployment. |
| Runtime admission | Queue capacity is fixed at 1,024 and `try_send` exposes full/closed failure. | Deterministically fill and close the queue; require distinct codes and stable idempotent retry. |
| Reload staging | `Runtime::{reload_staging_plan,spawn_reload_staging}` isolates sample/SFZ staging through `SamplesHandler::stage_load` and `SfzHandler::stage_load`. | Add a deterministic staging fault plan, then fail each asset before/after peers finish; assert rejected for required atomicity or exact partial components for best effort. |
| Backend create/set routing | `crates/vibelang-core/src/handlers/routes.rs` test backend exposes `fail_next_create` and `fail_next_set`. | Generalize a phase-indexed backend fault plan to every create, update, move, map, free, and sync call. |
| Late apply error | `apply_reload_route_finalize_error_aborts_without_clean_complete_log`. | Assert terminal partial today; later assert rejected with confirmed generation rollback. |
| Synthdef backend | `ScsynthBackend` tests inject correlated `/fail` for synthdef load. | Inject ordinary command failure, missing `/done`, sync timeout, late reply, duplicate reply, and reply after cancellation. |
| Pattern/melody boundary | `PatternState` and `MelodyState` expose pending/apply methods. | Submit two replace-pending revisions around the lookahead window; prove exact supersession, terminal timing, and tail reporting. |
| WebSocket lag | Broadcast capacity and receiver error path are explicit. | Force lag; assert gap event, ledger catch-up, duplicate delivery dedupe, retention reset, and no silent clean continuation. |
| WASM bridge | Bridge lookup and Promise await are isolated in `vibelang-wasm` and `WebScsynthBackend`. | Test missing bridge, rejected/never-resolving Promise, no tick, delayed tick, duplicate execute, runtime reset, and event ordering. |
| Rollback | No complete cross-phase rollback harness exists. | Retain old generation, fail every commit/barrier step, then compare runtime/backend component inventory and confirmation watermark. |

## Measurable acceptance criteria

### Contract and unit acceptance

1. One canonical schema serializes losslessly in Rust, HTTP JSON, WebSocket
   events, Rhai values, and WASM/TypeScript bindings.
2. Transition-table tests reject every edge not listed above and every terminal
   rewrite.
3. Property tests show `RevisionId` and `EventSequence` are strictly monotonic
   within an epoch under concurrent submission.
4. Pre-admission failures always have an attempt and no revision; accepted
   failures retain their revision.
5. A `rejected` receipt cannot encode uncertain/failed rollback or any remaining
   component; schema construction makes that state invalid.
6. Every planned component appears exactly once in a terminal component
   partition.
7. Same idempotency key/digest returns byte-equivalent identity and no new
   message; a changed digest is `idempotency_conflict`.
8. Cancellation races cover every transition boundary and never produce both
   `superseded` and an applied effect.

### Runtime/backend integration acceptance

1. A fault is injected before and after every phase in the apply table.
2. Required-atomic faults before activation terminate `rejected` with the last
   confirmed state unchanged.
3. Faults after activation terminate `rejected` only when a backend barrier
   confirms restoration; all other cases terminate `partial` and fence.
4. Queue full, queue closed, runtime handler failure, staging failure, backend
   rejection, sync timeout, and acknowledgment loss are distinguishable codes.
5. `sync_and_wait` fails when backend sync fails; no caller receives an
   unconditional notifier success.
6. The DSP deployed-hash registry never gets ahead of confirmed backend
   deployment.
7. For each of the 74 message variants, tests declare it internal,
   receipt-bearing mutation, or receipt-linked completion; no variant is
   unclassified.
8. For all 69 HTTP mutations, a conformance test verifies one receipt,
   idempotency behavior, expected-revision behavior where required, and removal
   of timing sleeps.
9. CLI file, CLI watch, HTTP `/eval`, and WASM execution of the same candidate
   produce the same request digest and terminal semantics.
10. Quantized pattern/melody receipts cannot become `applied` before the swap
    boundary and report any unavoidable lookahead tail.
11. WebSocket loss/reconnect tests recover every retained transition by
    sequence or receive explicit `reset_required`.
12. WASM cannot report `applied` until runtime tick and the advertised bridge
    confirmation boundary have occurred.

### Projection acceptance

1. No user-visible string or boolean names queue/bridge admission
   `successful`/`applied`.
2. Every terminal CLI, HTTP, WS, Rhai, and WASM result round-trips to the same
   canonical receipt.
3. Every state response declares epoch and confirmed-revision freshness.
4. Unknown schema versions and unknown enum values fail visibly and preserve
   raw diagnostics; they do not downgrade to success.
5. Generated docs enumerate the four terminal outcomes, transition graph,
   capability matrix, and each surface's wait/cancel/idempotency behavior.

## Interaction with the other four API-unification themes

| Theme | Required interaction |
|---|---|
| Lifecycle vocabulary: builders, handles, refs, and terminal verbs | Builders remain pure; terminal verbs return `RevisionRef`/receipt handles. The four receipt outcomes are the lifecycle vocabulary for every handle and ref. |
| Eliminate success-shaped no-ops and ignored inputs | Ignored/unsupported fields are rejected before revision allocation. Unsupported backend capabilities cannot return `applied`. Explicit best effort reports component failures as `partial`. |
| Canonical effective contract and generated projections | `MutationReceipt`, status, transitions, diagnostics, and capabilities belong in the canonical manifest. HTTP schemas, Rhai docs, TypeScript/WASM bindings, CLI help, and WS documentation are generated projections. |
| Shared conventions and capability discovery | IDs, naming, error codes, wait policies, expected revision, idempotency, cancellation windows, atomicity, backend confirmation, and retention use one conventions table and are discoverable at runtime. |

This receipt contract also depends on the revision-owned resource lifetime
decision: a receipt can make strong atomic claims only if graph resources,
deployed synthdefs, assets, and snapshots are owned by a staged revision rather
than process-global eager registries.

## Open decisions

These choices are intentionally isolated; they do not change the four terminal
outcomes or ordering invariants.

1. **Ledger durability and retention.** Choose in-memory versus persisted
   receipts, minimum time/count window, and restart recovery. The chosen values
   must be exposed as capabilities.
2. **Identifier encoding.** Choose UUIDv7/ULID or another opaque attempt/epoch
   format and the canonical request-digest algorithm. Ordering continues to come
   from counters, not identifier timestamps.
3. **Authentication namespace.** Define the caller identity used to scope HTTP
   and future remote idempotency keys.
4. **Backend activation primitive.** Decide the concrete scsynth generation
   group/root swap and the Web Audio worklet acknowledgment needed to advertise
   required atomicity.
5. **External-effect policy.** Decide which MIDI, recording, filesystem,
   process, and network operations may be deferred post-commit and which must be
   split into separate best-effort revisions.
6. **Quantized tail representation.** Choose exact beat/rational and backend
   clock encoding for `effective_at` and `audible_tail_until`.
7. **WASM progress driver.** Decide whether the host must keep calling `tick` or
   the V2 runtime owns an internal scheduler. Receipt semantics are unchanged.
8. **HTTP versioning window.** Choose `/v2` path versus negotiated media type and
   the removal date for legacy queue-admission success fields.

## Source index

All paths and symbols below are from the assessed tree.

| Concern | Exact source paths and symbols |
|---|---|
| CLI startup/watch/eval | `crates/vibelang-cli/src/main.rs::{main,run_simple_mode,execute_script,evaluate_code}` |
| Rhai parse/evaluate/state extraction | `crates/vibelang-rhai/src/engine.rs::ScriptEngine::{new,execute,execute_file,execute_file_full,execute_ast}`; `crates/vibelang-rhai/src/context.rs::take_state` |
| Rhai mutation families | `crates/vibelang-rhai/src/api/{global,group,voice,pattern,melody,sequence,sample,buffer,sfz,recording,route}.rs`; `crates/vibelang-rhai/src/api/midi/*.rs` |
| DSP eager registration/deploy | `crates/vibelang-dsp/src/api.rs::{SYNTHDEF_HASH_REGISTRY,set_deploy_callback,deploy_synthdef_ir,deploy_fx_ir,SynthDefBuilderHandle::body,SynthDefBuilderHandle::body_map,FxBuilderHandle::body,FxBuilderHandle::body_map}` |
| Message inventory | `crates/vibelang-core/src/message.rs::{Message,TransportMessage,SynthDefMessage,SampleMessage,SfzMessage,RecordingMessage,GroupMessage,VoiceMessage,PatternMessage,MelodyMessage,SequenceMessage,EffectMessage,FadeMessage,ReloadMessage,MidiMessage,SyncMessage}` |
| Queue and dispatch | `crates/vibelang-core/src/runtime.rs::{Runtime::tick,Runtime::handle_message,RuntimeHandle::send,RuntimeHandle::try_send,RuntimeHandle::sync_and_wait}` |
| Reload staging/apply | `crates/vibelang-core/src/runtime.rs::{Runtime::advance_reload_queue,Runtime::reload_staging_plan,Runtime::spawn_reload_staging,Runtime::apply_reload_with_assets,Runtime::apply_reload_inner,Runtime::build_reload_diff,Runtime::apply_voice_port_reconciles,Runtime::phase_apply_transport_changes,Runtime::sync_with_retry}`; `crates/vibelang-core/src/reload/mod.rs::{calculate_diff,ChangeQuant}` |
| Quantized swaps | `crates/vibelang-core/src/state.rs::{PatternState::queue_content_update,PatternState::apply_pending_swap,MelodyState::queue_content_update,MelodyState::apply_pending_swap}`; `crates/vibelang-core/src/handlers/{patterns,melodies}.rs` |
| Backend acknowledgments | `crates/vibelang-core/src/backends/scsynth.rs` `ScsynthBackend` `/done`, `/fail`, and `/synced` handling; `crates/vibelang-core/src/backends/web_scsynth.rs::{WebScsynthBackend,js_send_osc,js_load_synthdef}` |
| HTTP ingress | `crates/vibelang-http/src/lib.rs::{AppState::send,start_server}`; `crates/vibelang-http/src/routes/*.rs`; `api/http-api-snapshot-v1.json` |
| WebSocket egress | `crates/vibelang-http/src/websocket.rs::{WebSocketEvent,handle_socket,state_broadcaster,is_subscribed}` |
| WASM ingress/projection | `crates/vibelang-wasm/src/lib.rs::{ExecutionResult,VibelangRuntime::new,VibelangRuntime::init,VibelangRuntime::execute,VibelangRuntime::tick,VibelangRuntime::start,VibelangRuntime::stop,VibelangRuntime::stop_all,VibelangEngine::new,VibelangEngine::execute,VibelangEngine::clear_synthdefs,load_synthdef_to_supersonic}` |
| Existing public inventories | `api/public-api-manifest-v1.json`; `api/http-api-snapshot-v1.json`; `docs/api-surface-assessment.md`; `docs/interfaces/http-and-websocket.md`; `docs/interfaces/wasm.md`; `docs/reference/generated/http-routes.md` |
| Prior atomic revision decision | `docs/architecture/builder-ref-revision-resource-lifetime.md` |
