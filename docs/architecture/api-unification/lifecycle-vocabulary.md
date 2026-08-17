# Authoring lifecycle vocabulary

| Field | Value |
|---|---|
| Status | Proposed contract for API-unification synthesis |
| Assessed source | <code>f00c04ca1a1e79d644211eed64fc472214a75d58</code> |
| Assessment commit | <code>e5a1198a3bb478418042f2b517172f74635742b7</code> |
| Scope | Rhai authoring objects, routes, resources, DSP definitions, and conditional MIDI authoring |
| Product changes | None; this is a research and design artifact |

This contract specializes the accepted
[Builder, Ref, revision, and resource lifetime decision](../builder-ref-revision-resource-lifetime.md).
It does not weaken that decision's atomic candidate, typed identity, revision,
observation, contribution-ownership, or resource-generation rules. The
API-unification synthesis should resolve any later conflict in favor of those
invariants.

## Decision

VibeLang v2 will expose four author-facing object roles:

1. **Value** — detached immutable data with no logical runtime identity.
2. **Builder** — evaluation-local configuration with no desired-state,
   registry, queue, deployment, or backend effect before a terminal.
3. **Ref** — a stable typed logical address. A Ref is neither configuration nor
   a live/backend object.
4. **Observation** — a timestamped, revision-qualified report of live or
   pending facts.

<code>Handle</code> is not a fifth v2 authoring role. Existing names containing
<code>Handle</code> are classified below as builders, Refs, or observations and
will migrate accordingly. Host embeddings may still call an opaque,
process-local capability a handle, but no public Rhai handle may mix builder
configuration, logical identity, live observation, and physical ownership.

Every public callable overload will declare one or more effects from this
closed vocabulary:

| Effect | Contract |
|---|---|
| <code>construct</code> | Creates a detached Value, Builder, Ref address, or route selector; changes no candidate or live state. |
| <code>configure</code> | Returns an independently configured Builder or Value; changes no candidate or live state. |
| <code>register</code> | Validates and contributes immutable desired-state candidate IR. It does not mean applied/live. |
| <code>start</code> | Registers if necessary and requests an active/running generation with stated timing. |
| <code>stop</code> | Requests inactive/stopped state while preserving the declaration and logical identity. |
| <code>synchronize</code> | Waits for one correlated revision/receipt boundary; it does not submit a declaration. |
| <code>cancel</code> | Withdraws a pending operation or explicitly removes an edge/declaration; it never means stop-and-keep. |
| <code>observe</code> | Reads without mutation and reports revision, epoch, timestamp, and staleness for live facts. |

Effects compose. For example, <code>start</code> on a Builder has
<code>[register, start]</code>, while <code>status(ref)</code> has only
<code>[observe]</code>. The manifest must record both the effect set and the
terminal effect; a single heuristic label such as <code>named_terminal</code>
is not sufficient.

The smallest lifecycle verb set is:

| Canonical verb | Receiver | Meaning | Return inside one evaluation |
|---|---|---|---|
| <code>apply()</code> | Builder | Validate and register one declaration in the candidate, not start it | Typed Ref |
| <code>start()</code> | Startable Builder or Ref | Apply if needed and request normal, declared quantization | Typed Ref |
| <code>start_now()</code> | Startable Builder or Ref | Apply if needed and request explicit immediate start | Typed Ref |
| <code>run()</code> | VoiceBuilder or VoiceRef | Apply if needed and request continuous-node state | VoiceRef |
| <code>stop()</code> | Ref | Request inactive state; retain declaration | Same typed Ref |
| <code>remove()</code> | Ref | Remove the declaration from the candidate | Same typed Ref |
| <code>cancel()</code> | Ref for a pending/scheduled operation | Withdraw or abort without producing the normal result | Same typed Ref |
| <code>status(ref)</code> | Ref | Latest explicit observation; never a builder-property fallback | Observation |
| <code>status_at_least(ref, revision, timeout)</code> | Ref | Wait for a sufficiently fresh observation or explicit timeout/stale result | Observation |
| <code>sync(receipt, timeout)</code> | Evaluation receipt | Wait for the exact submitted revision to become terminal | Revision outcome |

The evaluation as a whole returns the shared revision receipt defined by the
mutation-outcomes theme. Builder terminals return Refs so declarations can be
composed during the same evaluation; they must not claim that the candidate is
already accepted or applied.

Routing keeps domain verbs only where the effect is unmistakable:
<code>to_groups</code>, <code>replace_with_main</code>,
<code>set_param</code>, <code>bend_by</code>,
<code>replace_from</code>, and <code>disconnect</code>. These are typed
<code>register</code> or <code>cancel</code> terminals returning a RouteRef.
Configuration such as scale and offset occurs before the terminal. There is no
post-terminal mutation of a route builder.

## Inventory method and quantitative boundary

The checked manifest at <code>api/public-api-manifest-v1.json</code> contains
3,626 entries and 8,431 overloads. The relevant core/DSP/extension portion has
34 registered types, 299 receiver-method entries with 352 overloads, 141
property getters, 134 property setters, and 178 free-function entries with 248
overloads. Thirty-two registered types have receiver methods. These counts are
from <code>stats</code> and <code>entries</code>; generated UGens and stdlib
functions remain part of the manifest but are mostly Values/calls rather than
authoring lifecycle objects.

The current lifecycle heuristic marks only 26 entries as
<code>named_terminal</code>:

| Current marked terminal | Entries |
|---|---:|
| <code>apply</code> on Voice, Pattern, Melody, Sequence, Fade, Fx, and RecordHandle | 7 |
| <code>start</code> on Pattern, Melody, Sequence, and Fade | 4 |
| <code>launch</code> on Pattern, Melody, Sequence, and Fade | 4 |
| <code>stop</code> on Pattern, Melody, and Sequence | 3 |
| SynthDef/Fx builder <code>body</code> and <code>body_map</code> | 4 |
| GroupHandle <code>body</code> | 1 |
| Voice <code>run</code> | 1 |
| Fade <code>now</code> and <code>restart</code> | 2 |
| **Total** | **26** |

That is a lower bound, not a behavioral inventory. It misses factory writes,
write-through chain methods, routing terminals, post-route mutation, direct
MIDI queueing, callback registration, and observation-looking getters that
read only an evaluation snapshot.

The repository has 60 example programs. Family occurrence counts from
<code>docs/api-surface-assessment.md</code> show the migration blast radius:
Voice appears in 54 files, Group in 50, Melody in 36, Pattern in 19, Synthdef
in 17, Sequence in 16, output routing in 16, input routing in 12, parameter
routing in 10, Sample in 12, Fade in 5, SFZ in 1, and effect definition in 1.

## Classification of all 34 registered object types

The role counts below are a behavioral classification of the assessed source,
not fields already present in the manifest:

| Current role | Types | Count |
|---|---|---:|
| Pure Value | Env, NodeRef, MidiRecordingHandle | 3 |
| Pure Builder until an explicit terminal | DSP builders, musical builders, record builder, and conditional MIDI route builders | 21 |
| Builder/handle hybrid with hidden registration | Voice, RouteHandle, MultiRouteHandle, ParamHandle, InputHandle | 5 |
| Ref/handle hybrid with mutation or command effects | GroupHandle, SampleHandle, MidiDevice | 3 |
| Reference-like handle exposing only identity data | BufferHandle, SfzHandle | 2 |
| **Total** |  | **34** |

The complete type-by-type classification is:

| Current registered type | Methods / overloads | Current reality | V2 target |
|---|---:|---|---|
| <code>vibelang_dsp::helpers::Env</code> | 0 / 0 | Pure envelope Value | Env Value |
| <code>vibelang_dsp::rhainodes::NodeRef</code> | 0 / 0 receiver methods; graph functions use it as an argument | Evaluation-local DSP graph Value despite the Ref suffix | DspNode Value; NodeRef compatibility type name |
| <code>MidiRecordingHandle</code> | 5 / 5 | Completed host-supplied result/snapshot, not a live handle | MidiRecordingSnapshot Value |
| <code>vibelang_dsp::helpers::EnvGenBuilder</code> | 4 / 8 | Pure graph Builder | EnvGenBuilder |
| <code>vibelang_dsp::helpers::EnvelopeBuilder</code> | 14 / 25 | Pure Env Builder | EnvelopeBuilder |
| <code>vibelang_dsp::api::SynthDefBuilderHandle</code> | 10 / 14 | Builder; body/body_map also register and deploy | SynthDefBuilder then SynthDefRef |
| <code>vibelang_dsp::api::FxBuilderHandle</code> | 5 / 5 | Builder; body/body_map also register and deploy | EffectDefBuilder then EffectDefRef |
| <code>Pattern</code> | 11 / 13 | Pure Builder until apply/start; stop/observe also live on the Builder type | PatternBuilder then PatternRef |
| <code>Melody</code> | 15 / 17 | Pure Builder until apply/start, except implicit sync when passed to Sequence.clip | MelodyBuilder then MelodyRef |
| <code>Sequence</code> | 8 / 13 | Pure Builder until apply/start | SequenceBuilder then SequenceRef |
| <code>Fade</code> | 18 / 20 | Pure Builder; apply is inert; start-like verbs register | FadeBuilder then FadeRef |
| <code>Fx</code> | 3 / 4 | Pure effect-instance Builder until apply | EffectBuilder then EffectRef |
| <code>RecordHandle</code> | 12 / 15 | Pure recording Builder; apply starts and returns a pending SampleHandle | RecordBuilder then RecordRef |
| <code>BendMapping</code> | 3 / 3 | Conditional MIDI route Builder | BendRouteBuilder then MidiRouteRef |
| <code>Cc32Route</code> | 4 / 5 | Deprecated compatibility Builder into the transport-transparent CC registry | Compatibility alias to CcRouteBuilder |
| <code>CcMapping</code> | 4 / 4 | Conditional transport-transparent MIDI route Builder | CcRouteBuilder then MidiRouteRef |
| <code>CcRoute</code> | 3 / 4 | Deprecated conditional MIDI route Builder | Compatibility alias to CcRouteBuilder |
| <code>GroupRoute</code> | 6 / 7 | Conditional MIDI 2 route Builder | Midi2GroupRouteBuilder then MidiRouteRef |
| <code>KeyboardRoute</code> | 9 / 10 | Conditional MIDI route Builder | KeyboardRouteBuilder then MidiRouteRef |
| <code>LooperBuilder</code> | 4 / 4 | Conditional MIDI looper Builder | LooperBuilder then MidiRouteRef |
| <code>NoteRoute</code> | 6 / 6 | Conditional MIDI route Builder | NoteRouteBuilder then MidiRouteRef |
| <code>PerNoteControllerBuilder</code> | 4 / 5 | Conditional MIDI 2 route Builder | Same Builder name then MidiRouteRef |
| <code>PerNotePitchBendBuilder</code> | 4 / 5 | Conditional MIDI 2 route Builder | Same Builder name then MidiRouteRef |
| <code>PerNotePressureBuilder</code> | 4 / 5 | Conditional MIDI 2 route Builder | Same Builder name then MidiRouteRef |
| <code>Voice</code> | 31 / 36 | Builder plus logical reference; named complete instances sync on most configuration methods | VoiceBuilder then VoiceRef |
| <code>RouteHandle</code> | 10 / 13 | Output selector, route Builder, and registered-route mutation cursor | OutputRouteBuilder then RouteRef |
| <code>MultiRouteHandle</code> | 4 / 4 | Multi-output selector plus eager route Builder | OutputRouteBuilder then one or more RouteRefs |
| <code>ParamHandle</code> | 3 / 3 | Parameter selector plus eager modulation Builder | ParamRouteBuilder then RouteRef |
| <code>InputHandle</code> | 3 / 5 | Input selector plus eager route Builder | InputRouteBuilder then RouteRef |
| <code>GroupHandle</code> | 15 / 16 | Logical path Ref with eager desired-state mutators and body contribution terminal | GroupRef; configuration moves to GroupBuilder |
| <code>SampleHandle</code> | 22 / 22 | Logical Sample Ref, write-through configuration cursor, and misleading buffer-number view | SampleBuilder then SampleRef |
| <code>MidiDevice</code> | 52 / 53 | External endpoint Ref/value plus desired-route, callback, recording, clock, and output-command terminal | MidiDeviceRef plus explicit route/command builders |
| <code>BufferHandle</code> | 2 / 2 | Logical resource name plus a physical backend buffer number | BufferRef with no physical number getter |
| <code>SfzHandle</code> | 2 / 2 | Logical instrument identity/path | SfzRef |

Property entries reinforce the split problem: Voice alone has 18 getters and 18
setters in the generated manifest, while Pattern, Melody, Sequence, and Fade
also expose builder fields and snapshot-looking playback properties on the same
type. V2 properties on Builders describe configuration only. Live facts are
available only through Observation.

## Current terminal-like effects

This section is exhaustive for the 34 registered object types and adjacent
transport functions. A “terminal-like” call is any public call that writes
ScriptState, a process registry, a deployment callback, a command queue, a
play/stop set, or claims to observe such state. Pure configuration calls are
listed when they hide one of those effects.

### Groups

Source:
<code>crates/vibelang-rhai/src/api/group.rs::{define_group,group,GroupHandle}</code>
and
<code>crates/vibelang-rhai/src/context.rs::{resolve_group_reference,get_or_create_group_id}</code>.

| Current verbs | Actual effects |
|---|---|
| <code>define_group(name, body)</code> | <code>construct + register</code>: create GroupConfig and evaluate an ordered body contribution. Closure errors are logged and swallowed. |
| <code>group(path)</code> | Intended <code>construct</code> lookup. Source has a split edge: slash-containing paths only produce a handle, while a new single-segment contextual reference passes through get_or_create_group_id and can also <code>register</code> GroupConfig/contextual claims. |
| <code>body(fn)</code> | <code>register</code>: add an ordered contribution immediately; closure errors only log. |
| <code>gain</code>, <code>mute</code>, <code>unmute</code>, <code>solo</code>, <code>set_param</code>, <code>output</code>, <code>alias</code>, <code>remove_effect</code>, <code>clear_effects</code> | <code>configure + register</code> immediately when a target config exists; several missing/invalid cases log or silently preserve state. |
| <code>name</code>, <code>parent</code>, <code>is_muted</code>, <code>is_soloed</code>, <code>effect_count</code> and matching properties | <code>observe</code> the evaluation snapshot/handle, not applied runtime state. |

### Voices

Source:
<code>crates/vibelang-rhai/src/api/voice.rs::Voice::{sync_to_state,apply,run}</code>.

| Current verbs | Actual effects |
|---|---|
| <code>voice(name)</code> | <code>construct + register</code> attempt. A bare incomplete voice does not replace an existing complete voice. |
| <code>voice()</code> | <code>construct</code> only until its structural name is resolved. |
| <code>group</code>, <code>synth</code>, <code>on</code>, <code>on_sfz</code>, <code>channel</code>, <code>cc_map</code>, <code>poly</code>, <code>mono_legato</code>, <code>gain</code>, <code>set_param</code>, two-argument <code>param</code>, <code>round_robin</code>, <code>choke</code>, <code>modulator_only</code>, <code>mute</code>, <code>unmute</code>, <code>solo</code>, <code>unsolo</code> | <code>configure + register</code> every named complete Voice. |
| <code>apply</code> | <code>register</code>: resolve anonymous identity and synchronize; missing source/unknown params warn rather than reject. |
| <code>run</code> | <code>register + start</code> continuous voice. |
| one-argument <code>param</code>, <code>output</code>, <code>outputs</code>, <code>input</code> | <code>construct</code> routing selectors; their later calls register routes. |
| getters/properties including <code>is_muted</code> and <code>is_soloed</code> | <code>observe</code> Builder/evaluation state, not applied runtime state. <code>id</code>/<code>name</code> can resolve an anonymous structural name. |

Passing a Voice to <code>Pattern.on(Voice)</code> or
<code>Melody.on(Voice)</code> also resolves and synchronizes it. That is hidden
registration in a relationship/configuration method.

### Patterns, melodies, sequences, fades, and effect instances

Sources: <code>crates/vibelang-rhai/src/api/pattern.rs::Pattern</code>,
<code>crates/vibelang-rhai/src/api/melody.rs::Melody</code>, and
<code>crates/vibelang-rhai/src/api/sequence.rs::{Sequence,Fade,Fx}</code>.

| Family | Current terminal-like verbs | Actual effects |
|---|---|---|
| Pattern | <code>apply</code>; <code>start</code>/<code>launch</code>; <code>stop</code>; <code>is_playing</code>/<code>playing</code> | Respectively <code>register</code>; <code>register + start</code>; <code>stop</code>; and <code>observe</code> evaluation snapshot. <code>set_param</code> is accepted configuration but its map is not copied into PatternConfig. |
| Melody | <code>apply</code>; <code>start</code>/<code>launch</code>; <code>stop</code>; <code>is_playing</code>/<code>playing</code> | Respectively <code>register</code>; <code>register + start</code>; <code>stop</code>; and <code>observe</code> evaluation snapshot. |
| Sequence | <code>apply</code>; <code>start</code>/<code>launch</code>; <code>stop</code>; <code>is_playing</code>/<code>playing</code> | Respectively <code>register</code>; <code>register + start</code>; <code>stop</code>; and <code>observe</code> evaluation snapshot. <code>launch</code> is deprecated. |
| Sequence relationships | <code>clip(range, Melody)</code> | <code>configure + register</code> Melody implicitly. Pattern and nested Sequence clips only <code>configure</code> the Sequence; Fade is embedded by Value. |
| Fade | <code>apply</code>; <code>start</code>/<code>launch</code>/<code>now</code>; <code>restart</code> | apply claims <code>register</code> but is a literal no-op; the next three are <code>register + start</code> with identical timing; restart is <code>register + start</code> plus force-restart. There is no stop/cancel method. |
| Fx effect instance | <code>apply</code> | <code>register</code> EffectConfig and append to an already-existing GroupConfig's effect order. An empty synth is accepted. |

### Output, input, parameter, and multi-output routing

Source: <code>crates/vibelang-rhai/src/api/route.rs</code>.

| Current receiver | Current terminal-like verbs | Actual effects |
|---|---|---|
| RouteHandle | <code>to</code>, <code>to_main</code>, <code>mute</code>, <code>to_current_group</code>, <code>to_input</code>, <code>to_param</code>, <code>to_param_audio</code>, <code>to_trigger</code> | <code>register</code> a group/main/mute, input, SET, A2K SET, or trigger route immediately. |
| RouteHandle | <code>scale</code>, <code>offset</code> | <code>configure + register</code>: mutate the already-registered modulation chosen by the preceding terminal; error when no modulation terminal ran. |
| MultiRouteHandle | <code>to</code>, <code>to_main</code>, <code>mute</code>, <code>to_current_group</code> | <code>register</code> the corresponding route for every selected port. |
| ParamHandle | <code>modulate_by</code>; then <code>scale</code>/<code>offset</code> | <code>register</code> a BEND route, then <code>configure + register</code> it after registration. |
| InputHandle | <code>from</code>, <code>from_current_group</code>, <code>disconnect</code> | <code>register</code> replacement input source or silence immediately; disconnect is semantically <code>cancel</code> but represented as a silence route. |
| RouteHandle, MultiRouteHandle, ParamHandle, InputHandle getters/properties | <code>voice_id</code>, <code>port_name</code>, <code>last_param_target</code>, <code>routes</code>, <code>target</code>, <code>target_name</code>, <code>target_synth</code>, <code>param_name</code>, <code>last_modulate_source</code> | <code>observe</code> selector/builder fields, not live route state. |

The short verbs hide materially different policies: audio
<code>to(group)</code> is additive, <code>to_main</code>/<code>mute</code> are
replacement, inputs are single-source replacement, SET and BEND use separate
maps with cross-verb conflicts, and parameter routes can fan in/out.

### Samples, SFZ, buffers, and recording

Sources:
<code>crates/vibelang-rhai/src/api/sample.rs::{sample,SampleHandle}</code>,
<code>crates/vibelang-rhai/src/api/sfz.rs::load_sfz</code>,
<code>crates/vibelang-rhai/src/api/buffer.rs::allocate_buffer</code>, and
<code>crates/vibelang-rhai/src/api/recording.rs::{RecordHandle,stop_recording,cancel_recording}</code>.

| Family | Current terminal-like verbs | Actual effects |
|---|---|---|
| Sample | <code>sample(id,path)</code> | <code>construct + register</code> SampleConfig immediately and return a handle whose buffer_id begins at zero. |
| Sample | <code>attack</code>, <code>sustain</code>, <code>release</code>, <code>amp</code>, <code>rate</code>, <code>loop_mode</code>, <code>offset</code>, <code>length</code>, <code>warp</code>, <code>speed</code>, <code>pitch</code>, <code>semitones</code>, <code>warp_to_bpm</code>, <code>window_size</code>, <code>overlaps</code>, <code>one_shot</code>, <code>gate</code>, <code>slice</code> | <code>configure + register</code>: rewrite the registered SampleConfig on every call. |
| Sample | <code>id</code>, <code>path</code>, <code>bufnum</code>/<code>buffer_id</code> methods/properties | <code>observe</code> handle fields. The number begins at zero and is not refreshed from the applied runtime binding. |
| SFZ | <code>load_sfz(id,path)</code> | <code>construct + register</code> SfzConfig immediately; existence and parse/load success are deferred. |
| SFZ | <code>id</code>, <code>path</code> methods/properties | <code>observe</code> handle fields, not parse/load state. |
| Buffer | <code>allocate_buffer(name,frames,channels)</code> | <code>construct + register</code> BufferConfig and return a deterministic reserved backend BufferId exposed as Float. Collision placement can depend on the colliding name set. |
| Buffer | <code>name</code>, <code>bufnum</code> methods/properties | <code>observe</code> handle fields; bufnum exposes physical allocation identity. |
| Record | <code>record(id)</code> | <code>construct</code> only. |
| Record | <code>apply</code> | <code>register + start</code> request and return a pending SampleHandle that is not inserted into the sample map. <code>immediate</code> configuration is dropped. |
| Record | <code>stop_recording</code>, <code>cancel_recording</code> | Claim <code>stop</code>/<code>cancel</code> but are log-only stubs; no runtime command. |
| Record | registered getters/properties | <code>observe</code> Record Builder fields, not recording runtime state. |

### Synthdef, DSP, and effect-definition objects

Source:
<code>crates/vibelang-dsp/src/api.rs::{SynthDefBuilderHandle,FxBuilderHandle,register_synthdef_api}</code>.

| Current verbs | Actual effects |
|---|---|
| <code>define_synthdef(name)</code>, <code>define_fx(name)</code> | <code>construct</code> Builders. Legacy closure overloads construct a Builder and depend on the closure to finalize it. |
| SynthDef <code>body</code>/<code>body_map</code> | <code>configure + register</code>: compile graph IR, write output/input registries, encode, deploy through the global callback, and record the body hash. |
| Fx-definition <code>body</code>/<code>body_map</code> | <code>configure + register</code>: compile graph IR, encode, deploy, and update global effect/synthdef registries. |
| Envelope/EnvGen <code>build</code>; NodeRef graph operators and functions | <code>construct</code> detached Values/graph IR only. |

The body methods are therefore deployment terminals disguised as Builder
configuration. A failed evaluation can have already changed process-global
registries or deployment state before the candidate is rejected.

### Conditional MIDI object terminals

Sources: <code>crates/vibelang-rhai/src/api/midi/device.rs</code>,
<code>crates/vibelang-rhai/src/api/midi/routing.rs</code>,
<code>crates/vibelang-rhai/src/api/midi/midi2.rs</code>,
<code>crates/vibelang-rhai/src/api/midi/cc_mapping.rs</code>,
<code>crates/vibelang-rhai/src/api/midi/bend_mapping.rs</code>,
<code>crates/vibelang-rhai/src/api/midi/looper_builder.rs</code>, and
[the MIDI reference](../../reference/midi.md).

| Area | Current terminal-like verbs | Actual effects |
|---|---|---|
| Simple device routes | <code>route_to</code>, <code>route_to_channel</code>, <code>route_cc</code>, <code>route_cc_to_group</code>, <code>open_input</code>, <code>open_output</code> | <code>register</code> desired routes/endpoints. The last four are deprecated or legacy. |
| MIDI 1 route builders | KeyboardRoute/NoteRoute <code>to</code>; CcMapping/BendMapping <code>to</code>; CcRoute <code>to_param</code>; LooperBuilder <code>to</code> | <code>register</code> ScriptState routes and input endpoints. |
| MIDI 2 route builders | GroupRoute <code>route_to</code>; PerNotePitchBendBuilder, PerNoteControllerBuilder, and PerNotePressureBuilder <code>to</code> | <code>register</code> the corresponding MIDI 2 route. |
| CC route builders | CcMapping <code>to</code> and deprecated Cc32Route <code>to</code> | <code>register</code> the same transport-transparent MIDI CC route. |
| Direct output | <code>note_on</code>, <code>note_off</code>, <code>cc</code>, <code>program_change</code>, <code>pitch_bend</code>, <code>note_on_hires</code>, <code>note_off_hires</code>, <code>cc_hires</code>, <code>pitch_bend_hires</code>, <code>send_per_note_bend</code>, <code>send_per_note_cc</code>, <code>poly_pressure_hires</code> | <code>register</code> an external-command intent for queueing; do not synchronously transmit. |
| Callbacks | <code>on_note</code>, <code>on_note_channel</code>, <code>on_cc</code>, <code>on_cc_num</code>, <code>on_clock_sync</code>, <code>on_midi</code> | <code>register</code> FnPtr/AST callbacks for later dispatch. |
| Clock/transport | <code>enable_clock</code>, <code>disable_clock</code>, <code>send_start</code>, <code>send_stop</code>, <code>send_continue</code> | <code>register</code> desired clock state or an external-command intent; send_stop is not the authoring <code>stop</code> effect. |
| Recording | <code>start_recording</code>, <code>start_recording_channel</code> | <code>register + start</code> a MIDI recording request. There is no registered Rhai stop/retrieve counterpart. |

MIDI discovery/getters and MidiRecordingHandle conversion are
<code>observe</code> or pure <code>construct</code>; they do not prove endpoint
freshness or delivery.

### Adjacent transport calls

<code>set_tempo</code>, <code>set_time_signature</code>, and
<code>set_quantization</code> register evaluation desired state.
<code>get_tempo</code>, <code>get_quantization</code>, and
<code>get_current_bar</code> observe evaluation/runtime-injected state with
type-specific freshness, not a shared live Observation. They must receive the
same effect and freshness metadata as object methods.

## V2 object and family contract

### Values, Builders, Refs, and Observations

A Builder is cloned by value. Each clone is independent. Factories and
configuration cannot call <code>context::with_state</code>, mutate an ID map,
write a process registry, invoke a deployment callback, send a message, or
allocate a physical resource. Compiling a closure into detached graph IR is
allowed, provided failure leaves all registries and candidate state untouched.

<code>apply</code> validates one declaration and contributes immutable IR to
the evaluation candidate. It returns a typed Ref usable by later declarations
in the same evaluation. Duplicate terminal calls for the same typed logical
address reject the candidate unless the source uses an explicit contribution
or override operation.

A Ref contains the language contract, engine instance, runtime epoch, and
fully qualified typed logical address required by the accepted lifetime ADR.
It contains no Builder fields, backend node ID, buffer number, pointer, or
cached live flags. Refs remain stable across accepted/applied revisions in the
same engine/runtime epoch. Cross-version, cross-engine, and cross-epoch use is
a structured error.

Observation is the only live-fact surface. It includes the Ref, runtime epoch,
requested/fresh-through/applied revision, observation sequence and timestamp,
staleness reason, lifecycle state, and diagnostics. Builder getters may report
Builder configuration, but names such as <code>playing</code>,
<code>buffer_id</code>, and <code>is_muted</code> may not silently report
desired state as though it were live.

### Groups

<code>group_builder(name)</code> constructs a GroupBuilder.
<code>group_ref(path)</code> constructs/resolves a GroupRef address without
creating state. <code>define_group(name, fn)</code> remains versioned sugar for
<code>group_builder(name).body(fn).apply()</code>.

GroupBuilder owns structural configuration. <code>body</code> adds detached,
stable, named contribution IR; it is not a terminal. GroupRef actions such as
mute/solo/remove are explicit candidate mutations and return the same Ref.
Reads use <code>status(group_ref)</code>. Group lookup never creates a group or
grants structural ownership.

Contribution identity, ordering, aggregation, error propagation, and removal
follow <code>BodyContribution</code>'s successor contract in the accepted ADR:
one structural owner, stable module-qualified contribution IDs, deterministic
ordering, isolated fragment evaluation, whole-candidate rejection, and
ownership-scoped teardown.

### Voices

<code>voice(name)</code> always constructs VoiceBuilder. Named and anonymous
builders are equally detached. <code>apply</code> returns VoiceRef;
<code>run</code> returns VoiceRef with continuous desired state. Passing a
VoiceBuilder to another object cannot synchronize it implicitly. Relationship
methods accept VoiceRef; migration tooling inserts or lifts an explicit
terminal.

Mute, solo, stop, remove, routing, and live status operate on VoiceRef.
Builder-level mute/solo are configuration flags only. Anonymous identity uses a
stable syntax key or explicit key, never an evaluation ordinal alone.

### Patterns and melodies

PatternBuilder and MelodyBuilder use <code>apply</code> for dormant
registration, <code>start</code> for normal quantization, and
<code>start_now</code> only where immediate timing is supported end to end.
<code>launch</code> is a deprecated alias of <code>start</code>, not a second
timing model.

<code>stop</code>/<code>remove</code> and <code>status</code> operate on
PatternRef/MelodyRef. <code>on</code> accepts VoiceRef and never registers a
VoiceBuilder. Pattern parameters are either implemented in PatternConfig or
rejected; v2 has no accepted inert <code>set_param</code>.

### Sequences and embedded declarations

SequenceBuilder follows the same apply/start/start_now/stop/remove/status
rules. <code>clip(range, Ref)</code> references an independently owned
PatternRef, MelodyRef, FadeRef, or SequenceRef.

For ergonomic inline composition, <code>clip(range, Builder)</code> may retain
an overload only with this fixed meaning: the Builder becomes a detached,
sequence-owned child fragment and is registered atomically when the Sequence
terminal runs. It cannot write global candidate state during <code>clip</code>.
The manifest records <code>ownership=parent</code>. Pattern, Melody, Fade, and
nested Sequence use the same rule; the current one-family implicit sync is
removed.

### Fades and effects/FX

FadeBuilder <code>apply</code> registers a dormant fade; it is never inert.
<code>start</code> uses declared normal quantization,
<code>start_now</code> means immediate, and <code>restart</code> is an explicit
new run generation on FadeRef. <code>stop</code> retains the fade declaration;
<code>cancel</code> withdraws a pending scheduled run. <code>now</code> and
<code>launch</code> are migration aliases to start_now and start respectively
only after those effects are actually distinct and effective.

The canonical noun is <code>effect</code>. <code>effect(id)</code> creates an
EffectBuilder; <code>apply</code> returns EffectRef and registers one explicit
edge in a Group contribution. <code>fx</code> remains a forwarding compatibility
alias. Empty effect-definition names or missing target groups reject the
candidate instead of returning an apparently successful builder.

### Samples, SFZ, buffers, and recording

Resource factories return Builders:

| Canonical v2 factory | Terminal | Ref |
|---|---|---|
| <code>sample(id, source)</code> | <code>apply</code> | SampleRef |
| <code>sfz(id, source)</code> | <code>apply</code> | SfzRef |
| <code>buffer(name, frames, channels)</code> | <code>apply</code> | BufferRef |
| <code>record(id)</code> | <code>apply</code> for dormant declaration; <code>start</code>/<code>start_now</code> to run | RecordRef |

Sample playback/envelope options configure SampleBuilder. They are not part of
immutable sample-buffer reuse identity unless they affect decoding. SfzBuilder
contains the logical source and parse/load options. BufferBuilder includes
shape, format, and persistence/replacement policy.

No Ref exposes a physical BufferId. DSP/Voice parameters that need a resource
accept BufferRef/SampleRef through a typed resource parameter or explicit
binding operation; lowering resolves the applied generation during planning.
This removes the current <code>BufferHandle.bufnum</code> and
<code>SampleHandle.buffer_id</code> identity leak.

RecordRef status reports pending/active/completed/cancelled/failed plus an
optional resulting SampleRef. <code>stop</code> finalizes and keeps the normal
result; <code>cancel</code> aborts and produces no normal result. Neither is a
global String-based log stub. The result SampleRef becomes usable only after
the recording outcome and resource binding are applied.

### Synthdef and DSP definitions

<code>define_synthdef</code> and canonical <code>define_effect</code> return
pure SynthDefBuilder and EffectDefBuilder. <code>body</code> and
<code>body_map</code> configure detached graph IR and return the Builder.
<code>apply</code> is the only registration terminal and returns SynthDefRef or
EffectDefRef.

Candidate validation compiles all graph IR before acceptance. Registry writes,
hash publication, backend deployment, and capability validation occur in
planning/staging, not evaluation. A failure preserves the prior registry/live
generation. The old <code>define_fx</code> spelling forwards to
<code>define_effect</code> during the compatibility window.

NodeRef is not a live Ref. V2 exposes it as the DspNode Value name in generated
docs and tooling; the NodeRef spelling remains an alias until the normal
removal gate.

### Routing

Route selection is construction/configuration. The effect occurs once, at an
explicit typed terminal:

| Current shape | Canonical v2 terminal | Effect |
|---|---|---|
| <code>source.output(p).to(group)</code> | <code>source.output(p).to_groups([group])</code> | Additive audio group edge(s) |
| repeated <code>to(group)</code> | one <code>to_groups([...])</code> | One atomic additive fan-out declaration |
| <code>to_main()</code> | <code>replace_with_main()</code> | Replace group/main/muted destination family |
| route <code>mute()</code> | <code>replace_with_silence()</code> | Replace destination family with silence |
| <code>to_current_group()</code> | <code>to_groups([current_group_ref()])</code> | Explicit additive group edge |
| <code>to_input</code> / InputHandle <code>from</code> | <code>target.input(i).replace_from(source,p)</code> | Replace single input source |
| <code>to_param</code> | <code>source.output(p).scale(x).offset(y).set_param(target,param)</code> | Register control SET route |
| <code>to_param_audio</code> | <code>...a2k().set_param(...)</code> | Explicit audio-to-control conversion plus SET |
| <code>to_trigger</code> | <code>...set_trigger(target,param)</code> | Trigger SET route |
| <code>modulate_by</code> | <code>target.param(param).scale(x).offset(y).bend_by(source,p)</code> | Register BEND route |
| InputHandle <code>disconnect</code> | RouteRef <code>disconnect()</code> | Cancel/remove the input edge; declared default/autofeed policy becomes visible |

Each terminal returns RouteRef and declares addition/replacement, rate,
fan-in/fan-out, conversion, defaults, conflict policy, and reload identity in
the manifest. A route logical address includes source Ref/port, destination
kind/Ref/port-or-param, route kind, and an explicit or deterministic edge key.

### MIDI

Conditional MIDI route types follow the same Builder-to-MidiRouteRef contract.
All <code>to</code>/<code>route_to</code>/<code>to_param</code> legacy
terminals forward to explicit canonical route terminals with generated
deprecation metadata. Callbacks become CallbackBuilder-to-CallbackRef
registrations. Recording returns a typed Ref with stop/cancel/status.

Direct MIDI output is a command, not desired-state configuration. Each send
returns or participates in the shared mutation receipt and reports queued,
delivered, rejected, failed, or unknown outcome. It is never described as
synchronized merely because a channel accepted the message. MIDI device
discovery returns MidiDeviceRef addresses validated against capability and
runtime epoch.

## Ownership and reload identity

Current IDs are FNV-derived from local names in
<code>context.rs::define_id_accessors!</code>; collisions probe within the
current evaluation, and anonymous names use reset-per-evaluation counters.
Buffer IDs use a separate reserved numeric range. These mechanisms help
reconciliation but do not constitute portable typed Ref identity.

V2 uses the accepted fully qualified address:

<code>(language contract, engine instance, runtime epoch, project namespace,
canonical module path, entity kind, canonical group scope, declaration key)</code>.

The family-specific addition is:

| Family | Logical/reload identity |
|---|---|
| Group | Typed fully qualified group key; structural owner and ContributionId are separate. |
| Voice, Pattern, Melody, Sequence, Fade, Effect | Typed declaration key plus canonical group/module scope. Configuration changes preserve Ref identity and create a new applied generation. |
| Route | Source/destination typed addresses, named ports/params, route kind, and stable edge key. Port rename/rate change is remove+add with diagnostics. |
| Synthdef/EffectDef | Typed module-qualified definition key. Encoded graph/body hash selects a generation, not logical identity. |
| Sample | SampleRef logical key; immutable physical generation reuse key includes canonical source, content fingerprint, decode/options, loader version, backend compatibility. |
| SFZ | SfzRef logical key; generation fingerprint includes root, transitive includes, all sample fingerprints, options, and parser/loader version. |
| Buffer | BufferRef logical key; mutable allocation is never shared across Refs. Compatible shape/format/persistence policy may preserve contents. |
| Record | RecordRef logical key plus observable run generation; one active run per Ref unless a later explicit concurrency contract says otherwise. |
| MIDI device/route/callback | Stable logical endpoint/route key validated against runtime epoch and advertised capability snapshot. |

A per-runtime ResourceManager owns physical Sample, Buffer, and SFZ
generations. Refs do not allocate/free. Readers pin exact generations. Reload
stages replacements, crosses a correlated barrier, commits the new binding,
and retires old generations only after logical bindings, staged claims, reader
leases, and backend quiescence are all clear. Same-path changed content is a
new generation; failure retains the old applied binding.

Absence in a later accepted candidate removes a logical declaration. Physical
release remains manager-owned. Saved Refs and observations never keep a
declaration or physical resource alive.

## Compatibility, aliases, and deprecation

V1 remains frozen and unversioned during the compatibility window. V2 is
explicit. The engine, CLI, HTTP, WASM, import resolver, LSP, editor, cache, and
migration tool must all use the same language contract. No v2 purity rule may
silently reinterpret a v1 script.

| Current v1 surface | V2 canonical surface | Migration rule |
|---|---|---|
| Voice/Pattern/Melody/Sequence/Fade/Fx <code>apply</code> | <code>apply</code> | Keep spelling; change only under explicit v2 semantics. Fade becomes effective. |
| Pattern/Melody/Sequence/Fade <code>launch</code> | <code>start</code> | Forwarding deprecated alias once equivalent behavior is implemented. |
| Fade <code>now</code> | <code>start_now</code> | Forwarding deprecated alias; never alias until immediate timing is effective. |
| Voice <code>run</code> | <code>run</code> | Keep Voice-only meaning. |
| Builder <code>stop</code>/<code>is_playing</code> | Ref <code>stop</code>/<code>status</code> | AST rewrite through saved/applied Ref; manual diagnostic when no stable Ref exists. |
| <code>fx</code> / <code>define_fx</code> | <code>effect</code> / <code>define_effect</code> | Forwarding deprecated aliases. |
| <code>GroupHandle</code> | GroupBuilder / GroupRef | <code>define_group</code> remains sugar; <code>group</code> lookup migrates to <code>group_ref</code>. |
| <code>SampleHandle</code> eager chain | SampleBuilder <code>...apply()</code> then SampleRef | AST tool appends/lifts apply; flag uses that read buffer_id. |
| <code>load_sfz</code> | <code>sfz(...).apply()</code> | Forwarding v1-only spelling plus AST rewrite. |
| <code>allocate_buffer</code> | <code>buffer(...).apply()</code> | AST rewrite; physical bufnum uses require typed resource binding migration. |
| Record <code>apply</code> | Record <code>start</code> | Semantic rewrite to preserve v1's start effect; v2 apply is dormant registration. |
| Record globals <code>stop_recording</code>/<code>cancel_recording</code> | RecordRef <code>stop</code>/<code>cancel</code> | No forwarding alias until the operation is effectful; v2 rejects unsupported stubs. |
| Synthdef/FxDef <code>body</code>/<code>body_map</code> terminal | body configuration followed by <code>apply</code> | AST tool appends apply after the completed definition chain. |
| <code>NodeRef</code> | DspNode | Type-name alias and generated-doc migration. |
| <code>on_sfz</code> | <code>on(SfzRef)</code> | Existing effective alias becomes deprecated. |
| Short routing verbs | Explicit add/replace/SET/BEND verbs | Manifest-driven AST rewrite; manual diagnostic for post-terminal scale/offset and order-dependent repeated routes. |
| Deprecated MIDI route builders/verbs | Canonical explicit route builders | Manifest-driven rewrite only when target/range/channel semantics are equivalent. |

Aliases must be executable forwarding code, carry since/deprecated/removal and
replacement metadata, emit one warning per source span, and pass the same
behavior tests as the canonical overload. A deprecated no-op is not an alias.

The existing roadmap's support gate remains: effective v1 aliases for at least
six months and two published minor releases after v2 becomes default,
whichever is later; removal only in the next semver-major release.

## Editor and documentation migration

The canonical manifest must add, per overload:

- <code>object_role</code> and <code>returns_role</code>;
- <code>effects[]</code>, <code>terminal_effect</code>, and
  <code>implicit_sync</code>;
- candidate/live/external side-effect domains;
- Ref kind and identity fields;
- parent/reference/contribution ownership;
- resource reuse/replacement/release policy identifiers;
- timing/quantization and receipt requirements;
- alias, deprecation, replacement, and safe-auto-rewrite class.

Generation must classify all 8,431 overloads, not only the 600 projected core
function overloads or the 26 heuristic terminals.

The LSP and VS Code currently consume a 600-row function projection and omit
properties. <code>crates/vibelang-lsp/src/features/completion.rs</code> already
reads lifecycle fields; it should switch to the effective effect/role schema.
Completions, hover, signature help, and diagnostics must:

- show Builder, Ref, Value, and Observation distinctly;
- show terminal effect and timing;
- diagnose unused Builders, inert/deprecated terminals, post-terminal Builder
  chaining, Builder/Ref receiver mismatches, hidden v1 eager sync, and stale
  observation assumptions;
- offer code actions for safe aliases and stop for manual ownership/resource
  decisions;
- filter conditional MIDI/native/resource calls using the capability snapshot.

The VS Code emitter boundary is
<code>vscode-extension/src/utils/sourceEmitters.ts::vibe</code> and
<code>WEBVIEW_VIBE_EMITTER_RUNTIME</code>. Pattern Editor, Melody Editor,
Sample Browser, Effect Rack, and Sound Designer must emit the selected language
major and canonical terminal. In particular, Sample Browser must append
SampleBuilder apply, and Sound Designer must emit body configuration followed
by SynthDefBuilder apply. Source and packaged-JavaScript parity checks must
continue to cover both forms.

Emacs syntax tables, snippets, templates, sidebar patterns, and docs examples
must consume generated alias/type data or be checked against it. Generated
reference pages should be organized first by role and then by effect, with a
complete terminal matrix and v1-to-v2 migration table. Handwritten Markdown
code blocks and all 60 examples join the manifest-based validation gate.

<code>vibe migrate --check</code> and <code>vibe migrate</code> must use a
Rhai-aware AST. Safe rewrites include launch/now aliases and inserting apply on
detached definitions. Manual diagnostics are required for repeated
declarations, implicit group-body ownership, saved/cloned hybrid objects,
anonymous order-based identity, record result assumptions, direct bufnum use,
post-terminal route configuration, and unsupported inert calls.

## Interaction with the other four API-unification themes

| Theme | Required interaction |
|---|---|
| Revisioned mutation outcomes and atomic reload truth | <code>register/start/stop/cancel</code> state only candidate intent. The outer evaluation receives the shared revision receipt. <code>sync</code> and Observation use the exact receipt/revision and runtime epoch; accepted/queued never upgrades to applied. |
| Eliminate success-shaped no-ops and ignored inputs | Every terminal and accepted field has an effect classification and fixture. Fade.apply, Pattern.set_param, Record.immediate/stop/cancel/result insertion, missing group/effect writes, and route post-terminal mutation must become effective or structured rejection before v2 exposure. |
| Canonical effective contract and generated projections | The manifest owns role, effect, terminal, identity, ownership, resource, timing, alias, and migration metadata. LSP, VS Code, Emacs checks, Markdown, HTTP/WASM projections, compatibility diff, and release notes consume it. Zero unclassified overloads is a release gate. |
| Shared conventions and capability discovery | Effects reference canonical units, channel/range policies, rate/conversion names, and availability/capability identifiers. A terminal may register only against a declared capability snapshot; unavailable start/route/resource/DSP/MIDI operations reject before candidate acceptance. |

The convergence owner should implement the shared revision/effective-contract
types before splitting individual authoring families. Otherwise each family
will invent incompatible Ref returns, receipts, error metadata, and capability
checks.

## Implementation sequence

1. Extend the effective-contract schema with the role/effect enums and require
   a classification for every current overload.
2. Add v1 golden fixtures that capture current ScriptState, registries,
   messages, route maps, resource bindings, warnings, no-ops, and timing for
   every family above.
3. Implement versioned Builder/Ref/Observation primitives and the shared
   evaluation receipt without changing v1 dispatch.
4. Move DSP registry/deployment and resource allocation behind candidate
   planning/staging so Builder purity is technically enforceable.
5. Implement core Group/Voice/Pattern/Melody/Sequence roles and contribution
   ownership.
6. Implement Fade/Effect/Record and Sample/SFZ/Buffer resource bindings.
7. Implement explicit route and conditional MIDI route/command terminals.
8. Generate editor/docs projections, AST migrations, aliases, and diagnostics.
9. Run cross-family failure-injection and v1/v2 integration gates before an
   opt-in pilot.

## Measurable acceptance

### Contract and static gates

- 34/34 current registered object types have a reviewed current role and v2
  target role.
- 8,431/8,431 manifest overloads have nonempty effect metadata; zero use an
  inferred fallback such as <code>call_result</code> in the release contract.
- Every Builder-returning factory/configuration overload in v2 has
  <code>implicit_sync=false</code>; static analysis finds zero calls from those
  implementations to ScriptState mutation, registry publication, queue send,
  deployment, or resource allocation boundaries.
- Every v2 terminal is effective or returns a structured unsupported/error
  outcome. Zero terminal fixtures are no-op, warning-only success, or
  log-only success.
- Every alias has canonical target, compatibility class, warning span, and
  behavioral parity fixture. Zero deprecated entries lack a replacement or
  explicit removal rationale.
- Generated LSP, VS Code, docs, compatibility diff, and migration tables match
  the canonical manifest with zero diff from a clean tree.

### Family unit gates

- For Group, Voice, Pattern, Melody, Sequence, Fade, Effect, Sample, SFZ,
  Buffer, Record, SynthDef, EffectDef, every route-builder family, and every
  conditional MIDI builder family: factory + every configuration method leaves
  candidate/runtime/registry/resource state byte-for-byte unchanged until a
  terminal.
- Builder clone tests prove independent configuration and terminal behavior.
- Every apply/start/start_now/run terminal returns the right typed Ref; every
  stop/remove/cancel receiver rejects the wrong Ref kind/engine/epoch.
- Duplicate declaration and contribution tests cover all entity kinds,
  cross-kind name reuse, source reorder, contribution removal, and deterministic
  effect/route order.
- Route tests cover additive/replacement semantics, SET/BEND conflict, A2K,
  trigger routes, fan-in/out, scale/offset ordering, named port reload, and
  disconnect/default behavior.
- Observation tests prove desired values are never returned as live facts and
  stale/disconnected/epoch-changed states are explicit.

### Integration and failure-injection gates

- One representative v1 and v2 script for every family compares evaluated
  candidate, accepted/applied revision ledger, backend messages, route maps,
  resource generations, and event timing.
- Injected failure at evaluation, graph compile, validation, staging, every
  create/update/route/effect/group phase, both barriers, commit, and cleanup.
  Before commit the prior applied graph and resource bindings remain; after
  commit cleanup failure is observable without rewriting history.
- Sample same-path content replacement, Buffer compatible preservation and
  resize policy, SFZ transitive dependency failure, recording stop/cancel/result,
  and reader generation pinning pass exact-once resource accounting.
- CLI watch, Rhai evaluation, HTTP, WebSocket, and WASM report the same
  revision terminal outcome for one submission.
- All 60 examples and all checked Markdown Vibe blocks either remain v1 golden
  fixtures or migrate to v2 with no unclassified/manual diagnostic left in the
  v2 corpus.
- Pattern Editor, Melody Editor, Sample Browser, Effect Rack, Sound Designer,
  LSP completion/hover/diagnostics, and Emacs snippets emit only
  manifest-supported canonical v2 calls when v2 is selected.

## Rejected alternatives and tradeoffs

| Alternative | Rejection/tradeoff |
|---|---|
| Keep one hybrid object type per family | Minimizes type names but preserves hidden registration, stale getters, backend ID leakage, and ambiguous clone/ownership behavior. |
| Treat Handle as a generic public role | “Handle” says nothing about purity, identity, lifetime, freshness, or ownership. Existing handles already represent three incompatible things. |
| Rename apply to declare everywhere | More semantically explicit, but creates broad syntax churn without removing ambiguity elsewhere. Keeping apply with one generated contract is the smaller compatible vocabulary. |
| Make every action use only apply | Cannot distinguish dormant registration, normal/immediate start, stop-keep, removal, cancellation, synchronization, and observation. |
| Make every domain terminal require a trailing apply | Uniform in the abstract but makes routing and stop/cancel syntax needlessly indirect. Explicit add/replace/SET/BEND/stop/cancel verbs already name their terminal effect. |
| Preserve body/body_map as deployment terminals | Keeps evaluation-time global mutation and prevents atomic candidate validation. |
| Let relationship methods auto-apply nested builders | Convenient, but hides ownership and repeats the current Melody/Pattern/Sequence asymmetry. Parent-owned inline overloads are allowed only as detached fragments committed by the parent terminal. |
| Expose BufferId/bufnum through resource Refs | Easy for DSP wiring, but couples logical identity to physical generation and makes atomic replacement/reader pinning unsafe. Typed resource binding is required. |
| Make status read Builder or desired snapshot fields | Fast and compatible, but produces false live-state claims and cannot express revision/epoch staleness. |
| Silently upgrade v1 objects to v2 | Breaks timing, duplicate, route, resource, and no-op behavior in existing songs. Versioned dual semantics are required. |
| Remove all aliases immediately | Produces unnecessary migration shock. Effective forwarding aliases with generated warnings are cheaper, but they carry implementation/test cost until the removal gate. |

## Open decisions isolated from this contract

These choices do not change the roles, effects, ownership, or terminal meanings
above and can be resolved during synthesis:

1. The exact source spelling of the language-major directive and whether Ref
   lookup factories use only <code>*_ref</code> functions or also typed module
   namespaces.
2. The concrete Rhai shape of Observation and revision outcome records
   (custom type versus immutable map), provided all required revision, epoch,
   timestamp, state, diagnostic, and staleness fields remain.
3. Whether a RecordRef permits multiple concurrent run generations. Until
   decided, the contract is one active run per Ref and a second start rejects.
4. The syntax of typed DSP resource parameters
   (<code>set_resource</code>, typed <code>set_param</code> overload, or
   declaration-time binding). Exposing a physical buffer number is not an
   option.

## Source anchors

All behavioral claims above were checked at the assessed source revision
against these exact paths and symbols:

| Concern | Source anchors |
|---|---|
| Canonical counts/lifecycle metadata | <code>api/public-api-manifest-v1.json::{stats,entries[].lifecycle,entries[].source_anchors}</code>; <code>docs/api-surface-assessment.md::{Quantitative inventory,Examples and documentation}</code> |
| Evaluation state and identity | <code>crates/vibelang-rhai/src/context.rs::{ScriptContext,define_id_accessors!,take_state,get_or_create_group_id,resolve_group_reference,resolve_auto_name}</code>; <code>crates/vibelang-rhai/src/engine.rs::ScriptEngine::{execute,execute_file,execute_file_full,execute_ast}</code> |
| Desired-state ownership | <code>crates/vibelang-core/src/reload/script_state.rs::{BodyContribution,GroupConfig,EffectConfig,ScriptState}</code> |
| Reload/apply boundary | <code>crates/vibelang-core/src/runtime.rs::Runtime::{apply_reload_with_assets,apply_reload_inner,phase_create_entities,phase_update_entities,phase_finalize_output_routes,phase_finalize_input_routes,phase_apply_effects,phase_apply_fades,phase_start_running_patterns,phase_finalize_param_routes}</code> |
| Group | <code>crates/vibelang-rhai/src/api/group.rs::{GroupHandle,define_group,group}</code> |
| Voice | <code>crates/vibelang-rhai/src/api/voice.rs::Voice::{new,new_anon,sync_to_state,apply,run}</code> |
| Pattern | <code>crates/vibelang-rhai/src/api/pattern.rs::Pattern::{sync_to_state,apply,start,launch,stop,is_playing}</code> |
| Melody | <code>crates/vibelang-rhai/src/api/melody.rs::Melody::{sync_to_state,apply,start,launch,stop,is_playing}</code> |
| Sequence/Fade/effect instance | <code>crates/vibelang-rhai/src/api/sequence.rs::{Sequence,Fade,Fx}</code> and their <code>apply/start/launch/now/restart/stop</code> implementations |
| Routing | <code>crates/vibelang-rhai/src/api/route.rs::{RouteHandle,ParamHandle,MultiRouteHandle,InputHandle}</code> |
| Sample/SFZ/Buffer/Record | <code>crates/vibelang-rhai/src/api/sample.rs::{SampleHandle,sample}</code>; <code>crates/vibelang-rhai/src/api/sfz.rs::{SfzHandle,load_sfz}</code>; <code>crates/vibelang-rhai/src/api/buffer.rs::{BufferHandle,allocate_buffer}</code>; <code>crates/vibelang-rhai/src/api/recording.rs::{RecordHandle,record,stop_recording,cancel_recording}</code> |
| Synthdef/DSP/effect definition | <code>crates/vibelang-dsp/src/api.rs::{SynthDefBuilderHandle,FxBuilderHandle,register_synthdef_api,register_synthdef_ir,register_effect_ir}</code>; <code>crates/vibelang-dsp/src/rhainodes.rs::{NodeRef,register_node_ref}</code>; <code>crates/vibelang-dsp/src/helpers.rs::{Env,EnvGenBuilder,EnvelopeBuilder}</code> |
| Conditional MIDI | <code>crates/vibelang-rhai/src/api/midi/device.rs::MidiDevice</code>; <code>crates/vibelang-rhai/src/api/midi/routing.rs::{KeyboardRoute,NoteRoute,CcRoute}</code>; <code>crates/vibelang-rhai/src/api/midi/midi2.rs::{GroupRoute,PerNotePitchBendBuilder,PerNoteControllerBuilder,PerNotePressureBuilder,Cc32Route}</code>; <code>crates/vibelang-rhai/src/api/midi/cc_mapping.rs::CcMapping</code>; <code>crates/vibelang-rhai/src/api/midi/bend_mapping.rs::BendMapping</code>; <code>crates/vibelang-rhai/src/api/midi/looper_builder.rs::LooperBuilder</code>; <code>crates/vibelang-rhai/src/api/midi/recording.rs::MidiRecordingHandle</code> |
| Existing accepted architecture | <code>docs/architecture/builder-ref-revision-resource-lifetime.md::{Builders and typed Refs,Candidate validation revisions and atomic apply,Duplicate declarations,Group and body contribution ownership,Sample Buffer and SFZ resources,V1 compatibility and v2 migration}</code> |
| Editor/documentation consumers | <code>xtask/src/public_artifacts.rs</code>; <code>crates/vibelang-lsp/src/features/completion.rs</code>; <code>vscode-extension/src/utils/sourceEmitters.ts::{vibe,WEBVIEW_VIBE_EMITTER_RUNTIME}</code>; <code>vscode-extension/src/views/patternEditor.ts</code>; <code>vscode-extension/src/views/melodyEditor.ts</code>; <code>vscode-extension/src/views/sampleBrowser.ts</code>; <code>vscode-extension/src/views/effectRack.ts</code>; <code>vscode-extension/src/views/soundDesigner.ts</code>; <code>emacs/vibelang-mode.el</code>; <code>emacs/vibelang-syntax.el</code>; <code>emacs/vibelang-templates.el</code>; <code>emacs/vibelang-sidebar.el</code>; <code>emacs/vibelang-snippets</code> |
