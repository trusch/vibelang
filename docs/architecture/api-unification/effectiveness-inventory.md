# API effectiveness inventory and contract

Status: assessed design at `e5a1198a3bb478418042f2b517172f74635742b7`

This inventory answers one question for every accepted field or terminal covered by
the API-unification assessment: after acceptance, is there an observable effect
that matches the contract? It covers HTTP request DTO fields, HTTP mutation
response members, WebSocket input and output payloads, the WASM surface, named
Rhai terminals and terminal-like recording functions, and the matching VS Code
request emitters. It does not propose that enqueueing, logging, or returning an
unchanged snapshot counts as success.

The assessed commit contains the 3,626-entry/8,431-overload API manifest and the
96-route HTTP snapshot described by `docs/api-surface-assessment.md`. All source
anchors below refer to that commit.

## Chosen contract

An accepted input has exactly one of these outcomes:

1. **Applied:** the requested effect reaches the authoritative state, and the
   response identifies the applied revision (or a later observable revision).
2. **Pending:** an asynchronous effect is durably accepted, and the response
   supplies an operation ID plus a terminal observation channel. Pending is not
   success.
3. **Rejected:** validation fails before dispatch with a structured error naming
   the operation, field, reason, and supported alternative.

No public operation may accept a field, drop it, only log it, or return a
pre-application snapshot as a successful mutation result. A terminal must either
produce its named lifecycle transition or return a structured evaluation error.

The effectiveness ledger is keyed by `(surface, operation, input JSON pointer or
member)`, not only by Rust type and field. `SendNoteRequest.velocity`, for example,
is effective for note-on and ignored for note-off; `PatternEvent` is effective in
create and log-only when nested under update. Type-only accounting would conceal
both defects.

### Classification used in this assessment

| Class | Meaning |
| --- | --- |
| `implemented` | The value reaches the intended desired/live state, or the returned member truthfully describes the documented scope. |
| `ignored` | The value is accepted but has no intended effect and no diagnostic. |
| `log-only` | The value or a delivery failure produces a log/warning but no caller-visible failure. |
| `stale` | Some related work occurs, but the named semantic or returned value is not correlated with the applied state. |
| `unsupported` | The operation rejects the value structurally and tells the caller what is supported. This is a valid terminal outcome. |
| `dead` | A public-looking declaration has no reachable operation or downstream consumer. |

There are almost no current `unsupported` classifications: serde can reject
malformed shapes, but recognized fields that cannot be honored are normally
accepted. The remediation deliberately converts many ignored/log-only cases into
versioned, structured `unsupported` responses.

## Quantitative inventory

| Surface | Assessed inventory | Effectiveness denominator |
| --- | ---: | --- |
| HTTP | 96 routes: 27 GET, 49 POST, 7 PATCH, 4 PUT, 9 DELETE; 75 schema types | 42 deserializable structs with 139 declared fields. 126 fields are reachable from request routes; 13 are dead declarations. There are 69 non-GET operations whose responses require mutation semantics. |
| HTTP responses | 33 serializable structs with 177 declared fields | Every member returned from a non-GET route, plus memberless status responses. Read-only DTO members are outside this ticket except where the same DTO is returned by mutation. |
| WebSocket | one 3-member envelope, one 2-member input message, 7 advertised event names | 55 distinct playback-snapshot paths (7 root collections and 48 nested members), 6 specialized-event members, and hello/capability members. |
| WASM | 11 `VibelangRuntime` methods and 7 legacy `VibelangEngine` methods, excluding constructors/destructors; module init/log/version exports | 7 `ExecutionResult` members, 2 `CompiledSynthdef` members, and declared bridge/error result members. |
| Rhai | Lifecycle metadata has 212 `call_result`, 194 `non_terminal_chain`, and 26 `named_terminal` entries in `api/public-api-manifest-v1.json` | All 26 named terminals, plus the terminal-like recording stop/cancel functions and recording `immediate` flag. |
| VS Code | canonical request types plus concrete `RuntimeManager`/editor emitters | Every field actually emitted for the operations above, and extra client fields that the server cannot accept or honor. |

The 13 dead HTTP declarations are `SourceLocation.{file,line,column}` (response
metadata deriving `Deserialize` without an inbound route),
`GroupCreate.{name,parent_path,params}`, `EffectCreate.{id,synthdef_name,
group_path,params,position}`, and `ClockOutputRequest.{device_id,enabled}`. Their
declarations are at `crates/vibelang-http/src/models.rs:13`, `:84`, `:428` and
`crates/vibelang-http/src/routes/midi.rs:96`. No router operation consumes them.

## HTTP request field ledger

The declaration root is `crates/vibelang-http/src/models.rs:13-542`, with
route-local DTOs in `routes/eval.rs:12-25`, `routes/fades.rs:31-99,301-330,
441-447`, and `routes/midi.rs:31-98,572-578`. The rows below exhaust the 139
declared struct fields; fields sharing the same dispatch and decision are grouped.

### Transport, groups, voices, and evaluation

| Operation and accepted fields | Current class | Declaration to observation | Decision |
| --- | --- | --- | --- |
| `PATCH /transport`: `bpm` | `implemented` input, `stale` receipt | `TransportUpdate` at `models.rs:48`; validated and sent as `TransportMessage::SetTempo` by `routes/transport.rs:58-79`; later visible in runtime/WS transport state. The immediate DTO is not correlated with application. | Keep; return the shared applied-revision receipt and state at that revision. |
| `PATCH /transport`: `time_signature.numerator`, `.denominator` | `implemented` input, `stale` receipt | `models.rs:42,48`; sent by `routes/transport.rs:81-99`; later visible as WS `time_sig`. | Keep; validate supported ranges and return the shared receipt. |
| `PATCH /transport`: `quantization_beats` | `ignored` | Declared at `models.rs:51`; `update_transport` never reads it. `transport_to_api` reports a constant `1.0` at `routes/transport.rs:13-48`. | Reject as `unsupported_field` in contract v2 until quantized transport mutation exists. Remove the response constant. |
| `POST /transport/seek`: `beat` | `implemented` input, `stale` memberless receipt | `SeekRequest` at `models.rs:55`; dispatched by `routes/transport.rs:140-168`; later observable as current beat. | Keep with applied revision/current beat. |
| `GroupCreate`: `name`, `parent_path`, `params` | `dead` | `models.rs:84-90`; the router exposes list/get/update/delete/actions but no create handler. | Remove from generated contract and VS Code types. Add a route only with a separate reviewed creation design. |
| `PATCH /groups/:id`: `params` and map values | `implemented` input, `stale` entity receipt | `GroupUpdate` at `models.rs:93`; each pair becomes `GroupMessage::SetParam` in `routes/groups.rs:133-166`; the returned `Group` is fetched without an applied acknowledgment. | Keep; make the set atomic and revisioned. |
| Group/voice/pattern/effect `PUT .../params/:param`: `value` | `implemented` input, memberless `stale` receipt | Shared `ParamSet` at `models.rs:99`; handlers dispatch `SetParam`, e.g. groups `routes/groups.rs:284-313`, voices `routes/voices.rs:282-310`, patterns `routes/patterns.rs:258-286`, effects `routes/effects.rs:169-210`. | Keep and return the shared receipt. |
| The same operations: `fade_beats` | `ignored` | Deserialized by all four handlers above and never read. VS Code exposes it. | Implement by dispatching a fade when present; direct set remains the `None` behavior. |
| `POST /voices`: `name`, `synth_name`/`synthdef`, `polyphony`, `group_path`/`group_id`, `params` | `implemented` input, `stale` entity receipt | `VoiceCreate` at `models.rs:138-149`; converted to `VoiceConfig` and sent by `routes/voices.rs:371-457`. The handler sleeps 10 ms and reads state rather than awaiting the command. `group_path` is parsed as a numeric ID despite its path/name spelling. | Keep; resolve canonical group identifiers and acknowledge application. |
| `POST /voices`: `gain`, `sample`, `sfz` | `ignored` | Declared at `models.rs:142,148-149`; the create handler does not read them. | Map `gain` to canonical `amp`. Reject `sample`/`sfz` in v2 until voice-source creation is implemented. |
| `PATCH /voices/:id`: `params` | `implemented` input, `stale` entity receipt | `VoiceUpdate` at `models.rs:153`; pairs dispatch `VoiceMessage::SetParam` in `routes/voices.rs:130-162`; response is an immediate state read. | Keep, atomically revisioned. |
| `PATCH /voices/:id`: `synth_name`, `polyphony`, `gain` | `ignored` | Recognized by serde at `models.rs:154-156`; never read by `update_voice`. | Implement `gain` as `amp`; reject synth/polyphony replacement in v2 pending explicit node-recreation semantics. |
| `POST /voices/:id/trigger`: `params` | `implemented` input, memberless `stale` receipt | `TriggerRequest` at `models.rs:162`; forwarded in `routes/voices.rs:164-199`. | Keep with applied/pending operation receipt. |
| `POST /voices/:id/note-on`: `note`, `velocity` | `implemented`, unit contract `stale` | `NoteOnRequest` at `models.rs:168`; both dispatched at `routes/voices.rs:221-250`. Server default is `0.8`, while VS Code defaults and emits values such as `100`. | Keep after the conventions theme selects and validates one velocity unit/range. |
| `POST /voices/:id/note-off`: `note` | `implemented`, memberless `stale` receipt | `NoteOffRequest` at `models.rs:179`; dispatched by the adjacent note-off handler. | Keep with receipt. |
| `POST /eval`: `code` | `implemented` evaluation, effect receipt `stale` | `EvalRequest` at `routes/eval.rs:12`; sent through `EvalJob`/oneshot at `routes/eval.rs:28-103`. `EvalResponse` reports evaluation, not completion of queued audio mutations. | Keep evaluation result and add the shared mutation receipt/revision set produced by the evaluation. |

### Patterns and melodies

| Operation and accepted fields | Current class | Declaration to observation | Decision |
| --- | --- | --- | --- |
| `POST /patterns`: `name`, `voice_name`, `loop_beats`, `events`, `swing`, `params` | `implemented` input, `stale` entity receipt | `PatternCreate`/`PatternEvent` at `models.rs:228-265`; converted to core events/config in `routes/patterns.rs:289-407`. Event `beat` and optional `params` both reach the config. | Keep, validate ranges, and return the revisioned entity. |
| `POST /patterns`: `pattern_string` | `ignored` | Accepted at `models.rs:258`; never read by `create_pattern`. | Implement using the canonical pattern parser. Reject simultaneous conflicting `events` and `pattern_string`. |
| `PATCH /patterns/:id`: `params` | `implemented` input, `stale` entity receipt | `PatternUpdate` at `models.rs:235`; pairs dispatch `PatternMessage::SetParam` in `routes/patterns.rs:164-208`. | Keep with one atomic revision. |
| `PATCH /patterns/:id`: `events` and nested `beat`/`params`, `loop_beats` | `log-only` | Handler detects `events`/`loop_beats`, logs that reload is required, then returns current state at `routes/patterns.rs:164-208`. Nested event fields are therefore log-only in this operation. | Implement content replacement and loop-length change as a revisioned update. |
| `PATCH /patterns/:id`: `pattern_string` | `ignored` | Deserialized at `models.rs:237`; not even included in the warning condition. The Pattern editor sends it. | Implement alongside event replacement with explicit mutual-exclusion validation. |
| Pattern `POST .../start` and `/stop`: `quantize_beats` | `ignored` | `StartRequest`/`StopRequest` at `models.rs:270-277`; handlers bind `_req` and send immediate start/stop at `routes/patterns.rs:210-255`. | Reject in v2 until scheduler support exists; omission means immediate. |
| `POST /melodies`: `name`, `voice_name`, `loop_beats`, `events`; event `beat`, `note`, `duration`, `velocity` | `implemented` input, `stale` entity receipt | `MelodyCreate`/`MelodyEvent` at `models.rs:302-331`; converted in `routes/melodies.rs:234-362`. Invalid note text silently becomes MIDI 60 at `:242-251`. | Keep, but reject invalid notes and use the canonical velocity/duration contract. |
| `POST /melodies`: event `frequency`, event `params`, top-level `melody_string`, top-level `params` | `ignored` | All deserialize in `models.rs:305-313,327-331`; no conversion consumes them. | Implement `melody_string` and per-event params. Reject `frequency` and top-level params in v2 until precedence/meaning is specified. |
| `PATCH /melodies/:id`: `events` and nested members, `loop_beats` | `log-only` | `MelodyUpdate` at `models.rs:335`; handler only warns for these two fields and returns current state at `routes/melodies.rs:164-184`. | Implement atomic content/length replacement. |
| `PATCH /melodies/:id`: `melody_string`, `lanes`, `params` | `ignored` | Accepted at `models.rs:337-341`; never read. The Melody editor sends `lanes` and `loop_beats`. | Implement canonical `lanes`/string editing, rejecting conflicting representations; reject `params` until its scope is chosen. |
| Melody `POST .../start` and `/stop`: `quantize_beats` | `ignored` | Shared request structs; handlers bind `_req` and immediately dispatch at `routes/melodies.rs:186-231`. | Same versioned rejection as patterns. |

### Sequences, effects, samples, fades, and MIDI

| Operation and accepted fields | Current class | Declaration to observation | Decision |
| --- | --- | --- | --- |
| `POST /sequences`: `name`, `loop_beats`, `clips`; clip `type`, `name`, `start_beat`, `end_beat`, `duration_beats` | `implemented` for recognized pattern/melody/sequence IDs; otherwise `ignored` | `SequenceCreate`/`SequenceClip` at `models.rs:362-387`; converted in `routes/sequences.rs:274-366`. `fade` and unknown types are dropped; names are parsed as numeric IDs. | Keep recognized types, resolve canonical names/IDs, support fade, and reject unknown type/target or ambiguous end/duration. |
| `POST /sequences`: clip `once` | `ignored` | Accepted at `models.rs:372`; never consumed in clip conversion. | Reject in v2 unless per-clip one-shot semantics are implemented. |
| `PATCH /sequences/:id`: `loop_beats`, `clips` and all six nested clip fields | `log-only` | `SequenceUpdate` at `models.rs:392`; handler only warns and returns current state at `routes/sequences.rs:151-170`. | Implement atomic config replacement with dependency validation. |
| `POST /sequences/:id/start`: `play_once` | `implemented`, memberless `stale` receipt | `SequenceStartRequest` at `models.rs:398`; inverted into core `looping` at `routes/sequences.rs:173-203`. | Keep with applied revision. |
| `EffectCreate`: `id`, `synthdef_name`, `group_path`, `params`, `position` | `dead` | `models.rs:428-434`; no create route consumes it. | Remove from generated contract; add only with an explicit effect lifecycle operation. |
| `PATCH /effects/:id`: `params` | `implemented`, `stale` entity receipt | `EffectUpdate` at `models.rs:438`; dispatch in `routes/effects.rs:86-131`. | Keep with atomic revision. |
| `POST /samples/load`: `path`, `id` | `implemented`, `stale` entity receipt | `SampleLoad` at `models.rs:468`; queued then followed by a fixed 50 ms sleep/read in `routes/samples.rs:88-170`. | Return `pending` with operation ID, then terminal loaded/failed event and revision. |
| `POST /fades`: `target_type`, `target_name`, `param_name`, `start_value`, `target_value`, `duration_beats` | `implemented`, `stale` entity receipt | `FadeCreate` at `models.rs:536`; converted and sent in `routes/fades.rs:516-581`. | Keep, validate target/parameter/duration, return applied revision/fade ID. |
| Five specialized fade routes: `param`, `to`, `duration_beats`, `from`, `curve`; curve payload `exp` or `spline` | `implemented` for valid targets/curves; unknown curve-name semantic `stale` | DTOs and `CurveSpec` at `routes/fades.rs:31-99,301-330`; dispatched by handlers at `:147-436`. `parse_curve_name` at `:102-131` silently maps unknown names to linear. | Keep all fields; reject unknown curve names and invalid spline/exponent values. |
| `DELETE /fades`: `target_type`, `target_name`, `param` | `implemented`, memberless `stale` receipt | `CancelFadeRequest` at `routes/fades.rs:441`; converted and sent at `:450-513`. | Keep with applied revision and cancellation count/ID. |
| MIDI open/close/clock/transport operations: `OpenDeviceRequest.device_id` | `implemented`, memberless `stale` receipt | `routes/midi.rs:31`; forwarded by the relevant handlers. | Keep with applied/device-state receipt. |
| MIDI note-on: `device_id`, `channel`, `note`, `velocity` | `implemented`; optional velocity has default behavior | `SendNoteRequest` at `routes/midi.rs:37`; consumed by `send_note_on` at `:256-286`. | Keep with validated MIDI ranges and pending/applied receipt. |
| MIDI note-off: `device_id`, `channel`, `note` | `implemented`; `velocity` is `ignored` | Same DTO; `send_note_off` at `routes/midi.rs:288-317` does not read velocity. | Split note-on/off DTOs and remove/reject velocity on note-off. |
| MIDI CC: `device_id`, `channel`, `cc`, `value` | `implemented` | `SendCcRequest` at `routes/midi.rs:46`; consumed at `:319-349`. | Keep with receipt and range validation. |
| MIDI start recording: `device_id`, `channel` | `implemented` | `StartRecordingRequest` at `routes/midi.rs:55`; consumed at `:355-386`. | Keep with recording operation ID/status. |
| MIDI stop recording: `device_id` | `implemented` | `StopRecordingRequest` at `routes/midi.rs:62`; consumed at `:388-419`. | Keep with terminal recording result. |
| MIDI stop recording: `quantize` | `ignored` | Explicit `#[allow(dead_code)]` at `routes/midi.rs:64-66`; never forwarded. | Reject in v2 until implemented. |
| `ClockOutputRequest.device_id`, `.enabled` | `dead` | Future-only declaration at `routes/midi.rs:94-99`; no handler. | Remove from public generated contract. |
| Add keyboard route: `device_id`, `voice_id`, `channel`, `note_min`, `note_max`, `transpose` | `implemented` input, `stale` message receipt | `AddKeyboardRouteRequest` at `routes/midi.rs:572`; sent at `:606-649`. Response is a formatted echo, not applied state. | Keep with route ID/revision; validate range ordering. |

## HTTP mutation response ledger

All 69 non-GET operations must be treated as mutations for response-shape
generation. Most handlers call `AppState::send` and return immediately. Queue
acceptance is not command application, so every current status-only success and
entity response is a `stale` mutation receipt even when individual fields happen
to be accurate.

| Response shape/member set | Current effectiveness | Source anchor and remediation |
| --- | --- | --- |
| Memberless `200`/`201`/`204` from transport controls, deletes, mute/solo/start/stop/note/trigger/fade cancellation/MIDI controls | `stale` carrier | Handlers return after `send`; representative anchors are `routes/transport.rs:101-168`, `routes/patterns.rs:210-286`, and `routes/midi.rs:256-419`. Replace with `{operation_id, outcome, accepted_revision, applied_revision, resource}`. |
| `TransportState.{bpm,time_signature,running,current_beat,loop_beats,loop_beat,server_time_ms}` | real snapshot members but `stale` relative to mutation | Conversion at `routes/transport.rs:13-48`; add revision and wait/observe semantics. |
| `TransportState.quantization_beats` | `stale` constant | Hard-coded `1.0` in the same conversion. Remove from v2 until backed by state. |
| `Group.{name,path,parent_path,children,node_id,audio_bus,link_synth_node_id,muted,soloed,params}` | state-derived but mutation `stale` | `group_to_api`, `routes/groups.rs:53-89`. Return at applied revision. |
| `Group.{synth_node_ids,source_location}` | `stale` placeholder | Both are always `None` in `group_to_api`. Remove from v2 or implement. |
| `Voice.{name,synth_name,polyphony,gain,group_path,group_name,muted,params,active_notes,running,running_node_id}` | state-derived but mutation `stale`; `group_name` is a raw ID string | `voice_to_api`, `routes/voices.rs:57-86`; fix identifier mapping and revision correlation. |
| `Voice.{output_bus,soloed,sfz_instrument,vst_instrument,sustained_notes,source_location}` | `stale` placeholder | Fixed `None`/`false` or not backed by create input in `voice_to_api`. Remove or implement. |
| `Pattern.{name,voice_name,group_path,loop_beats,events,status.state}` | state-derived but mutation `stale` | `pattern_to_api`, `routes/patterns.rs:62-120`. |
| `Pattern.{params,status.start_beat,status.stop_beat,is_looping,source_location,step_pattern}` | `stale` placeholder/approximation | The converter emits `None`, `true`, or only playing/stopped without scheduled beats. Implement or omit in v2. |
| `Melody.{name,voice_name,group_path,loop_beats,events,status.state}` | state-derived but mutation `stale` | `melody_to_api`, `routes/melodies.rs:62-120`. Event frequency/params are not reconstructed. |
| `Melody.{params,status.start_beat,status.stop_beat,is_looping,source_location,notes_patterns}` | `stale` placeholder/approximation | Fixed `None`/`true` or incomplete state in the same converter. Implement or omit. |
| `Sequence.{name,loop_beats,clips,play_once,active}` | state-derived but mutation `stale` | `sequence_to_api`, `routes/sequences.rs:59-107`; add applied revision. |
| `Sequence.source_location` and returned clip `once` for non-fade clips | `stale` placeholder | Conversion returns absent values. Remove or implement. |
| `Effect.{id,synthdef_name,group_path,node_id,params}` | state-derived but mutation `stale` | `effect_to_api`, `routes/effects.rs:29-42`. |
| `Effect.{bus_in,bus_out,position,vst_plugin,source_location}` | `stale` placeholder | Always `None` in the converter. Remove or implement. |
| `Sample.{id,path,buffer_id,num_channels,num_frames,sample_rate,synthdef_name}` | eventual state, fixed-delay `stale` receipt | `sample_to_api` and load handler at `routes/samples.rs:30-44,88-170`. Use pending/terminal operation. |
| `Sample.slices` | `stale` placeholder | Always `None` in `sample_to_api`. Omit until implemented. |
| `ActiveFade.{id,name,target_type,target_name,param_name,start_value,target_value,current_value,duration_beats,start_beat,progress}` | state-derived but mutation `stale` | Fade handlers return without a cross-surface revision. Return authoritative fade ID/revision. |
| `EvalResponse.{success,result,error}` | `implemented` for Rhai evaluation, `stale` for resulting audio effects | `routes/eval.rs:19-25,41-103`. Add effect receipts; do not reinterpret evaluation success as applied success. |
| `AddKeyboardRouteResponse.message` | `stale` echo | `routes/midi.rs:583-585,606-649`; replace with route ID/revision. `RouteInfoDto.keyboard_route_count` is separately hard-coded to zero at `:587-605`. |
| Error members `{error,message}` (general) and `{error}` (MIDI) | `implemented` but inconsistent | `models.rs:627-655` and `routes/midi.rs:110-114`. Use one generated error envelope with `operation`, `field`, `reason`, and supported values. |

## WebSocket payload ledger

The WebSocket implementation is entirely hand-built in
`crates/vibelang-http/src/websocket.rs`. It polls runtime state every 50 ms
(`run_event_broadcaster`, `:530-638`) and has neither a state revision nor event
sequence number. It is useful telemetry, but cannot prove a mutation applied.

| Payload/member | Current class and effect | Decision |
| --- | --- | --- |
| Inbound `SubscriptionMessage.action`, `.events` | `implemented` for `subscribe`/`unsubscribe` at `:113-157`; unknown action, malformed JSON, and non-text input are `ignored`. Starting from wildcard then subscribing does not narrow the set. | Generate a client-command union; reply with structured accepted/rejected subscription state. Define replace/add/remove semantics. |
| Envelope `type`, `timestamp`, `data` | `implemented` emission at `WebSocketEvent`, `:23-29`; `timestamp` is server wall time, not ordering. | Add monotonic `event_sequence` and `state_revision`; keep timestamp as metadata. |
| `hello.data.protocol_version`, `server`; capability `events`, `commands`, `wildcard_subscriptions`, `initial_snapshot_event` | `implemented` static capability announcement at `make_hello_event`, `:182-205`. Seven event names are advertised. | Generate from the canonical contract/capability registry rather than a literal list. |
| Snapshot roots `transport`, `groups`, `voices`, `patterns`, `melodies`, `sequences`, `fades` | `implemented` 20 Hz state projection at `build_playback_snapshot`, `:214-232`; overall observation is `stale` for mutation correlation. | Add source revision and full/delta marker. |
| Transport `playing`, `beat`, `bar`, `beat_in_bar`, `bpm`, `time_sig` | `implemented` at `build_transport_payload`, `:234-247`. | Keep; use canonical time-signature/beat types. |
| Group `name`, `parent`, `muted`, `soloed`, `meter_peak`, `voices`, `patterns`, `melodies`, `params` | `implemented` state projection at `build_groups_payload`, `:250-310`. | Keep and generate schema. |
| Voice `name`, `synth`, `muted`, `soloed`, `group`, `active_nodes` | `implemented` at `build_voices_payload`, `:313-329`. | Keep and generate schema. |
| Pattern and melody `name`, `playing`, `loop_position`, `loop_length` | `implemented` at `build_patterns_payload`/`build_melodies_payload`, `:332-364`. | Keep, using the shared lifecycle vocabulary. |
| Sequence `name`, `playing`, `paused`, `position`, `length`, `looping`, `active_clips` | `implemented` at `build_sequences_payload`, `:366-388`. | Keep; align lifecycle names. |
| Active clip `type`, `name`, `clip_index`, `progress`, `nested_clips` | Pattern/melody/sequence values are `implemented`; fade `progress` is `stale`, hard-coded to `1.0` at `:414-420`. Missing referenced state can omit a clip. | Use a generated tagged union; compute fade progress and emit explicit missing dependency/cycle diagnostics. |
| Fade `target_type`, `target_name`, `param`, `start_value`, `current_value`, `target_value`, `progress` | `implemented` at `build_fades_payload`, `:466-528`. | Keep and generate from the effective contract. |
| `transport.beat`: `beat`, `bar`, `beat_in_bar`; started/stopped: `beat`; bpm: `bpm` | `implemented` polled events at `:589-629`. | Keep with event sequence and state revision. |
| Broadcast lag/receiver error | `dead` connection after error | Any `rx.recv()` error breaks the sender task at `:74-85`; there is no gap event/resync. | Send `gap` with last sequence and require snapshot resync, or close with a documented retry code. |

The 55 snapshot paths are 7 root collections plus 48 distinct nested members:
transport 6, group 9, voice 6, pattern 4, melody 4, sequence 7, active clip 5,
and fade 7. Specialized events add 6 members (3 beat + 1 started + 1 stopped
+ 1 bpm).

## WASM method and result ledger

Declarations and behavior are in `crates/vibelang-wasm/src/lib.rs`; the public
TypeScript projection is `crates/vibelang-wasm/types/index.d.ts`.

| Method/member | Current class and trace | Decision |
| --- | --- | --- |
| `VibelangRuntime.init()` | `implemented`; initializes runtime and rejects on initialization error at `lib.rs:127-165`. | Keep; publish typed error codes. |
| `execute(script)` | Rhai evaluation is `implemented`; bridge delivery/reload is `log-only`; returned success is `stale` | At `lib.rs:171-255`, synthdef/effect bridge failures and reload-send failure only call `console.warn`. Before initialization, a script can parse successfully without application. | Return/reject with evaluation plus operation receipt. Bridge absence/rejection, reload rejection, or terminal apply failure must prevent `success: true`. |
| `tick()` | `implemented` only when initialized; before init it is `ignored` at `:262-266`. | Reject `not_initialized`. |
| `start()` | `implemented` when initialized; before init it logs and resolves, so `log-only` at `:270-286`. | Reject `not_initialized` and return revisioned transport outcome. |
| `stop()` | `implemented` when initialized; before init it silently resolves, so `ignored` at `:290-299`. | Reject `not_initialized`. |
| `stopAll()` | `stale` naming | It only calls `stop()` at `:303-311`; it does not free all active nodes/resources. | Implement the documented all-node stop or replace with the lifecycle-standard transport stop name. |
| `getSystemSynthdefs()` | `implemented`, but serialization fallback is `stale` | `:317-334` can return `null` on serializer failure while TypeScript promises an array. | Return a typed result/rejection; never silently change shape. |
| `isInitialized()` | `implemented` state query at `:336-340`. | Keep. |
| `parseNote()`, `dbToAmp()`, `ampToDb()` | `implemented` pure utilities at `:344-358`. | Keep, with generated units/ranges. |
| `VibelangEngine.execute()` | `implemented` only as legacy parse/evaluation state; no live runtime application at `:397-455`. | Deprecate immediately; remove in the next major after callers migrate to `VibelangRuntime`. Its name must not coexist as an apparently equivalent engine. |
| Legacy `getSynthdefs()`, `getSystemSynthdefs()`, `clearSynthdefs()`, `parseNote()`, `dbToAmp()`, `ampToDb()` | locally `implemented` at `:457-485`, but form an overlapping stale surface | Deprecate/remove with `VibelangEngine`; retain pure utilities as module exports if needed. |
| Module init/default init, `version()` | `implemented` loader/version behavior | Keep generated declarations. |
| Module `log(message)` | deliberately `log-only` and truthful by name | Keep; it is not a mutation terminal. |
| `ExecutionResult.success` | `stale` | Constructed before bridge/reload delivery completes. | Replace with `evaluation_outcome` and the shared applied/pending/rejected receipt. |
| `ExecutionResult.error` | `stale`/incomplete | Captures evaluation errors, not delivery or application errors. | Make failures phase-tagged: parse/evaluate/bridge/dispatch/apply. |
| `ExecutionResult.groups`, `.voices`, `.patterns`, `.melodies`, `.tempo` | `implemented` evaluation snapshot, `stale` relative to live runtime | Populated from evaluated `ScriptState` at `:171-255`. | Name as `desired_state_summary` and pair with applied revision. |
| `CompiledSynthdef.name`, `.data` | `implemented` bridge input at `:85-90`. | Keep with typed bridge acknowledgment. |
| `VibelangBridge.loadSynthdef(...) -> Promise<unknown>` result | `ignored` | `call_bridge_load_synthdef`, `:368-389`, probes `globalThis` but calls `window.vibelangBridge`, ignores resolved data, and treats a missing bridge as success. | Resolve one global consistently; require a typed `{name,loaded}` result and propagate rejection. |
| Declared `VibelangError.{message,name,stack}` | `dead` public result shape | Present in `types/index.d.ts`, but no method returns it. | Remove or make it the generated structured error type. |

## Rhai terminal ledger

`api/public-api-manifest-v1.json` classifies exactly 26 entries as
`named_terminal`.
The following table accounts for all 26. “Implemented” here means the terminal
updates desired state; the shared revision-receipt work must still connect an
evaluation to live application.

| Named terminal(s) | Count | Current class and source | Decision |
| --- | ---: | --- | --- |
| `SynthDefBuilderHandle.body`, `SynthDefBuilderHandle.body_map`; `FxBuilderHandle.body`, `FxBuilderHandle.body_map` | 4 | `implemented`, returning `Result`, at `crates/vibelang-dsp/src/api.rs:467-548` (registered at `:718-727`). | Keep; surface deploy/compile failures structurally. |
| `GroupHandle.body` | 1 | closure execution is implemented, but closure failure is `log-only`; it logs and returns the handle at `crates/vibelang-rhai/src/api/group.rs:279-306`. | Return `Result` and fail evaluation. |
| `Voice.apply` | 1 | valid state sync is `implemented`; missing source only warns and can leave no inserted voice at `api/voice.rs:615-689`. | Return `Result`; reject missing dependencies. |
| `Voice.run` | 1 | `implemented` desired running state at `api/voice.rs:834-837`. | Keep and revision. |
| `Pattern.apply`, `Melody.apply`, `Sequence.apply` | 3 | `implemented` desired-state sync at `api/pattern.rs:154-218`, `api/melody.rs:436-547`, `api/sequence.rs:169-251`. | Keep; dependency failure must be structured. |
| `Pattern.start`, `Melody.start`, `Sequence.start` | 3 | `implemented` desired playing state at `pattern.rs:221-237`, `melody.rs:550-571`, `sequence.rs:254-265`. | Keep under the shared lifecycle vocabulary. |
| `Pattern.launch`, `Melody.launch`, `Sequence.launch` | 3 | `stale` synonym: each is only an alias for `start`, despite launch/quantization implications (`pattern.rs:239-242`, `melody.rs:573-576`, `sequence.rs:267-270`). | Deprecate and remove, or define a real scheduled launch in the lifecycle story. Chosen default is removal after one deprecation release. |
| `Pattern.stop`, `Melody.stop`, `Sequence.stop` | 3 | desired-state removal is implemented, but live stop is `stale` for otherwise unchanged entities. Reload diff `phase_start_running_patterns` at `crates/vibelang-core/src/runtime.rs:3218-3419` only stops removed running IDs in conditions that can leave an unchanged live entity playing. | Add explicit desired stop operations to the diff and integration-test the unchanged-config case. |
| `Fade.apply` | 1 | `ignored`: the body literally returns `self` at `api/sequence.rs:562-566`. | Remove in v2; `start` is the chosen terminal. |
| `Fade.start`, `Fade.now` | 2 | both are `implemented` desired-state insertion but semantically `stale`: their bodies are equivalent at `api/sequence.rs:600-609,639-648`. | Keep one immediate `start`; reserve scheduled/quantized behavior for an explicitly parameterized terminal. Deprecate `now`. |
| `Fade.launch` | 1 | `stale` alias for `start` at `api/sequence.rs:614-616`. | Deprecate/remove with other `launch` aliases. |
| `Fade.restart` | 1 | `implemented`; sets force-restart at `api/sequence.rs:623-633`, consumed by reload at `crates/vibelang-core/src/reload/mod.rs:619-634` and runtime fade handling at `runtime.rs:3140-3185`. Runtime target failures can still be log-only. | Keep, but propagate apply failure through the revision outcome. |
| `Fx.apply` | 1 | valid desired-state insertion is `implemented`; missing/failed runtime synth creation is `log-only` at `api/sequence.rs:722-742` and `core/runtime.rs:2866-2907`. | Return structured dependency/apply errors. |
| `RecordHandle.apply` | 1 | request path is effectively `ignored`, result is `stale` | At `api/recording.rs:178-231` it writes `ScriptState.recordings` and fabricates a pending `SampleHandle`. Reload has no consumer of `new_state.recordings`; direct recording messages are handled only at `core/runtime.rs:808-817`. The returned sample is absent from the script sample registry (`api/sample.rs:46-54`). | Implement a reload diff that emits a recording operation and resolves a real terminal sample, or reject the terminal. Chosen direction is implementation because recording is documented and core handlers exist. |

Three additional recording semantics are accepted outside the 26 named-terminal
records:

- `RecordHandle.immediate()` sets `start_immediately` at
  `api/recording.rs:160-164`, but `apply` does not copy it into the core config at
  `:209-219`: `ignored`. Implement it or remove it from the builder.
- `stop_recording(id)` and `cancel_recording(id)` only log at
  `api/recording.rs:242-254`: `log-only`. They must dispatch by operation ID and
  return terminal status.
- Core recording completion itself is real: direct messages reach
  `crates/vibelang-core/src/handlers/recordings.rs:215-237` and insert a sample.
  The missing link is the Rhai/reload request and its receipt, not audio capture.

### Sequence dependency defect

Rhai accepts Pattern, Melody, Fade, and nested Sequence clips (`ClipInfo` at
`api/sequence.rs:56-62`), and core playback handles all four variants at
`crates/vibelang-core/src/handlers/sequences.rs:273-393`. Acceptance is not
symmetrical, however:

| Builder path | Dependency materialization | Class |
| --- | --- | --- |
| `clip_melody` and dynamic Melody | Calls `melody.sync_to_state()` at `api/sequence.rs:104-113,133-165` | `implemented` |
| `clip_fade` and dynamic Fade | Embeds fade config in the sequence at `:116-122,133-165` | `implemented` |
| `clip_pattern` and dynamic Pattern | Stores only the derived ID at `:94-101,133-165`; no pattern sync | `ignored` dependency |
| `clip_sequence` and dynamic Sequence | Stores only the derived ID at `:125-131,133-165`; no nested sequence sync | `ignored` dependency |

The chosen contract is that accepting a typed clip atomically materializes its
dependency. Pattern and nested Sequence builders must sync before the parent is
inserted; nested traversal must detect cycles and return a structured error with
the dependency path. Merely rejecting these two clip types would contradict the
core model and current documentation, so it is rejected as the primary design.

## Matching VS Code request ledger

The client types are hand-written in `vscode-extension/src/api/types.ts`; request
dispatch is `vscode-extension/src/api/runtimeManager.ts`. These rows cover the
fields actually emitted by the extension plus declared drift that implies a
supported request.

| Client request/field | Server effect | Decision |
| --- | --- | --- |
| `updateTransport`: `bpm`, `time_signature` | `implemented`; response stale | Keep and consume receipt. |
| `updateTransport.quantization_beats` | `ignored` | Stop emitting; surface the v2 unsupported error until implemented. |
| Group/voice/pattern/effect param `value` | `implemented`; response stale | Keep. |
| The same param calls: `fade_beats` | `ignored` | Keep only after server fade implementation; gate by capability meanwhile. |
| `createVoice`: `name`, `synth_name`, `polyphony`, `params` | implemented | Keep. |
| `createVoice.gain` | ignored; `sampleBrowser.ts:502-508` and other preview paths send it | Map to canonical amp. |
| `createVoice.group_path` | nominally consumed, but preview sends `"main"` while the server parses a numeric ID | `unsupported` in practice/stale identifier contract. Resolve canonical names. |
| Type-only `VoiceCreate.sample`, `.sfz` | ignored if sent | Remove from client v2 until supported. |
| Pattern editor `pattern_string`, `loop_beats` | ignored/log-only; emitted at `views/patternEditor.ts:679-701` | Server must implement before the editor reports saved. |
| `PatternUpdate.events`, `.params` | log-only/implemented respectively | Keep after atomic replacement support; reject unsupported combinations. |
| Pattern start/stop `quantize_beats` | ignored; emitted by `runtimeManager.ts:522-531` | Do not emit until capability says scheduled lifecycle is supported. |
| Melody editor `lanes`, `loop_beats` | ignored/log-only; emitted at `views/melodyEditor.ts:737-752` | Implement before reporting saved. |
| Melody `events`, `melody_string`, `params` | log-only/ignored/ignored | Align with one generated update union. |
| Melody start `quantize_beats` | ignored | Stop emitting until supported. |
| Sequence update `loop_beats`, `clips` and nested clip fields | log-only | Consume v2 implementation or report unsupported; never treat current entity echo as save success. |
| Sequence start `play_once` | implemented; response stale | Keep with receipt. |
| Voice note preview `note`, `velocity` | dispatched, but velocity units are stale; editors/sample browser send MIDI-style values such as 100 at `melodyEditor.ts:563-655` and `sampleBrowser.ts:519` | Generate one unit/range and convert at the boundary. |
| Sample load `path`, `id` | implemented with fixed-delay stale response | Show pending until terminal operation event. |
| Eval `code` | evaluation implemented, live effect uncorrelated | Consume evaluation plus applied revision. |
| Client-only `PatternCreate.group_path`; `MelodyCreate.group_path`, `.lanes` | server DTO does not accept these create fields | Remove from generated client request or add explicit server semantics; chosen v2 default is removal. |
| Client `GroupCreate` and `EffectCreate` shapes | dead server operations | Remove until routes exist. |
| `RuntimeManager` group mute/solo and pattern start/stop return `Promise<Entity|null>` | `stale` TypeScript result: server sends memberless status | Generate result types from routes; return the shared receipt, not an entity. |
| `RuntimeManager.delete()` boolean | `stale`: `fetch()` maps 404 to `null`, and `delete()` returns `true` when no exception is thrown (`runtimeManager.ts:250-253,320-326`) | Return a generated deleted/not-found/rejected result. |

## Remediation choices and rejected alternatives

### Chosen changes

1. Introduce the shared mutation receipt from the mutation-outcome theme for HTTP,
   eval, WASM, and WS: `operation_id`, `outcome`, `accepted_revision`, optional
   `applied_revision`, resource identity, and structured errors.
2. Make the HTTP v2 request decoder deny unknown fields and consult generated
   per-operation effectiveness metadata. Recognized but unavailable fields return
   `422 unsupported_field`; they are never silently dropped.
3. Implement the fields on which shipped editors already depend: param fades,
   pattern content/length/string updates, melody lanes/content/length updates,
   sequence config replacement, voice gain, name-based group resolution, and
   typed sequence dependency materialization.
4. Version-reject quantization fields, voice source/replacement fields, MIDI
   recording quantize, per-clip `once`, and ambiguous representations until their
   semantics exist.
5. Remove dead DTOs and placeholder response members from v2 projections.
6. Make Rhai/WASM delivery failures caller-visible and use one lifecycle vocabulary
   for apply/start/stop. Implement recording dispatch and real sample resolution.

### Rejected alternatives

| Alternative | Why rejected |
| --- | --- |
| Treat queue-send success as mutation success | It cannot distinguish handler rejection, missing dependency, device/bridge failure, or a later revision overwriting the request. |
| Keep accepting no-op fields but document them | A client cannot distinguish a typo from an intentionally ignored field, and current editors already report false success. |
| Inventory only Rust DTO fields | Shared DTOs have operation-dependent behavior; it misses note-off velocity and update-nested event no-ops. |
| Remove all currently ineffective fields | Pattern/melody editor persistence, param fades, sequences, and recording are intentional product paths with downstream machinery. Removing them would discard useful supported concepts. |
| Implement every accepted field immediately | Several fields lack a decided lifecycle, unit, or conflict rule. Structured v2 rejection is safer and measurable until those decisions land. |
| Use WebSocket polling as the mutation acknowledgment | Polling has no revision/sequence and can skip intermediate state or disconnect on lag. |

## Compatibility and migration

1. **Inventory/shadow release:** ship the generated ledger and diagnostics in CI.
   On v1 requests, emit a deprecation warning and telemetry identifier for any
   ignored/log-only/stale field while preserving behavior for one announced
   release window.
2. **Client-first migration:** regenerate VS Code and WASM declarations; update
   editors to wait for applied/pending outcomes; stop sending fields selected for
   rejection. Add capability checks for optional features.
3. **Contract v2:** select one explicit version mechanism (open decision below).
   V2 uses deny-unknown-fields and structured `unsupported_field`; entity-returning
   mutations return receipts plus revisioned projections. V1 remains frozen and
   explicitly labeled legacy during the window.
4. **Rhai/WASM deprecation:** warn once for `launch`, `Fade.apply`, `Fade.now`, and
   `VibelangEngine`; make their replacements available in the same release; remove
   them in the next major contract version.
5. **Removal:** after the measured window, delete v1-only ignored fields, dead
   Group/Effect create DTOs, placeholders, and the legacy engine. Compatibility
   reports from the canonical-schema theme must classify each removal as breaking.

## Generated zero-unclassified gate

The canonical effective-contract source must emit an effectiveness record with:

```text
effectiveness_id
surface
operation
input_path_or_member
declaration_anchor
dispatch_anchor
observation_anchor
classification
chosen_action
availability_or_capability
introduced_version
deprecated_version
test_id
```

Generation fails if any discovered item lacks exactly one record. Discovery must
join, not merely concatenate:

- all serde-deserializable request leaf fields reachable from each router method
  and normalized path;
- all response members reachable from non-GET operations and all memberless
  success statuses;
- WS input union members, event envelope members, advertised event names, and
  every `json!` payload path;
- every `wasm_bindgen` public method and every member of its generated result;
- all manifest `named_terminal` overloads plus an explicit allowlist of audited
  terminal-like free functions;
- generated VS Code request members and concrete request-object keys at emitters.

Required CI assertions:

1. `unclassified_count == 0` on every surface.
2. `implemented` records have a declaration, dispatch, observation, and test
   anchor.
3. `unsupported` records have a test for structured rejection and no dispatch.
4. `ignored`, `log-only`, `stale`, and `dead` are forbidden in the current v2
   effective contract; they are permitted only in a versioned legacy ledger with
   a removal version.
5. The generated HTTP/WS/TypeScript/WASM projections contain no field omitted by
   the ledger and no ledger field omitted by the projections.
6. An operation-context fixture proves reused fields can have different outcomes.
7. Compatibility diff fails a release when an implemented field becomes rejected,
   changes units/ranges, or loses its observation path without an allowed breaking
   version.

## Measurable implementation and integration acceptance

- HTTP: all 126 route-reachable declared fields have operation-scoped records;
  all 13 dead declarations are absent from v2; all 69 non-GET routes return an
  applied/pending/rejected receipt; no mutation handler uses a fixed sleep as an
  acknowledgment.
- Pattern/Melody editors: after saving changed strings/lanes/lengths, the response
  revision is observed on WS and a GET at that revision contains the exact edit.
- Sequences: a parent containing an otherwise-unapplied Pattern and nested
  Sequence materializes both; a cyclic nested sequence is rejected with the
  dependency path; all four clip variants appear in authoritative state.
- Rhai lifecycle: start then stop of an unchanged Pattern, Melody, and Sequence
  stops live playback; deprecated aliases produce diagnostics; no terminal
  failure is log-only.
- Recording: `record(...).apply()` reaches a core recording operation; immediate
  mode changes scheduling; stop/cancel return terminal statuses; completed sample
  identity resolves to a sample present in authoritative/script state.
- Fades: `Fade.apply` is unavailable in v2; the chosen `start` behavior is unique;
  unknown curves reject; target failure returns rejected rather than a log.
- WASM: missing bridge, bridge rejection, reload-send rejection, and apply failure
  each prevent a success result; uninitialized tick/start/stop reject with typed
  codes; `stopAll` has either verified all-node behavior or is absent.
- WS: every event has monotonic sequence and state revision; induced receiver lag
  produces a documented gap/resync outcome rather than silent sender death.
- VS Code: no hand-written request field is outside the generated v2 schema; save
  UI does not report success before applied revision; delete distinguishes deleted
  from not-found.
- CI: a fixture adding one serde field, WS payload member, WASM result member, Rhai
  terminal, or VS Code request key without an effectiveness record fails with
  `unclassified_count == 1`; adding its complete record returns the count to zero.

## Interaction with the other four API-unification themes

| Theme | Required interaction |
| --- | --- |
| Revisioned mutation outcomes/shared revision receipt | Supplies the applied/pending/rejected carrier and the authoritative observation used by every `implemented` record. This inventory supplies the fields and failure phases that receipt must cover. |
| Authoring lifecycle vocabulary | Decides the canonical apply/start/stop/schedule terms and deprecation mapping for `launch`, `now`, `run`, and recording terminals. Effectiveness tests verify that each retained term is behaviorally distinct. |
| Canonical effective contract/generated projections | Owns the machine-readable ledger and generation of HTTP, WS, TypeScript, and WASM shapes. This inventory provides the initial classifications, operation keys, anchors, and zero-unclassified rule. |
| Units, ranges, parsing, availability, and capabilities | Decides velocity units, beat/duration ranges, identifier parsing, curve names, native/MIDI availability, and capability gating. An unavailable capability must yield structured `unsupported`, never ignored success. |

## Open decisions (intentionally isolated)

1. Choose the wire version mechanism: `/v2`, media type, or required contract
   header. The effectiveness rule is independent of that choice.
2. Choose the exact mutation atomicity model for multiple fields/params and how a
   partial hardware failure is represented in the shared receipt.
3. Choose the canonical velocity unit and conversion boundary; the conventions
   theme owns this decision.
4. Choose precedence/conflict rules for `events` versus `pattern_string`, and for
   melody `events` versus `melody_string`/`lanes`. V2 must reject ambiguity until
   decided.
5. Decide whether nested Sequence values may be cyclic by reference. The chosen
   implementation requires cycle detection either way; the open point is whether
   a cycle is always invalid or allowed with explicit bounded semantics.
6. Set the exact v1 deprecation duration and release number for removal of legacy
   WASM/Rhai names and ignored HTTP fields.
