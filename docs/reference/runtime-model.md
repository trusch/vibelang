# Execution, identity, hot reload, and state lifecycle

## One evaluation produces one desired snapshot

Before each execution, VibeLang clears its thread-local script state and the
user synthdef/effect registries, evaluates the script, and submits the resulting
snapshot to runtime reconciliation. A failed initial evaluation aborts startup;
a failed watched reload is logged and the current runtime continues. Runtime
messages and HTTP mutations are queued, so “stored” or a successful response is
not a transactional acknowledgement from the audio server.

Source: [`ScriptEngine`](../../crates/vibelang-rhai/src/engine.rs),
[`ScriptState`](../../crates/vibelang-core/src/reload/script_state.rs), and
[`api::register_api`](../../crates/vibelang-rhai/src/api/mod.rs#L38-L82).

## Identity and context

- Named objects use their author-visible name plus current group context to
  obtain stable IDs across reloads.
- `define_group(name, fn)` enters the group while evaluating its body, so child
  voices, patterns, effects, and groups resolve relative to that context.
- `group(path)` resolves aliases and relative paths but creates only a handle;
  it does not insert a missing group.
- Anonymous Voice, Pattern, Melody, and Fade builders derive structural names
  when synchronized. Anonymous Voice stays deferred until another API resolves
  it or `apply()` / `run()` is called.
- Source-order contributions to a group body and effect application order are
  significant.

Alias conflicts are Rhai errors. Errors inside `define_group` and `group.body`
closures are logged and swallowed by those wrappers rather than propagated to
the caller.

## Exact lifecycle matrix

| Type or call family | Builder changes | Snapshot synchronization | Live/state notes |
|---|---|---|---|
| Transport setters | None | Setter call | Reconciled from global state; getters read evaluation state |
| `define_group` / existing `GroupHandle` mutators | Chain immediately | Each effective call | `group(path)` alone inserts nothing; a missing target can make mutators no-ops |
| Named `Voice` | Chain immediately | After essentially every source/config mutation once complete | Incomplete declarations do not replace an earlier valid voice |
| Anonymous `Voice` | Deferred | `apply()`, `run()`, or resolution by another API | `run()` also marks continuous running |
| Route, input, and parameter terminals | Handle configuration plus terminal | Terminal call | Stored immediately; rate/port/target conflicts error |
| Pattern | Pure builder | `apply()` or `start()` | `start()` also marks playing; `stop()` only marks stopped |
| Melody | Pure builder | `apply()`, `start()`, and when clipped into a Sequence | `start()` also marks playing |
| Sequence | Pure builder | `apply()` or `start()` | Melody clips are synchronized; Pattern and nested Sequence builders are not |
| Fade | Pure builder | `start()`, `launch()`, `now()`, `restart()`, or as a Sequence clip | `apply()` is a literal no-op; `now()` currently stores the same state as `start()` |
| Fx | Pure builder | `apply()` | Appends to effect order only if its group snapshot already exists |
| Sample | Inserted by `sample()` | Constructor and every handle mutator | Buffer number begins at 0 until runtime assignment |
| Buffer | None | `allocate_buffer()` | Same shape survives reload; changed shape reallocates |
| SFZ | None | `load_sfz()` | Native only; parse/existence validation occurs later |
| Record | Pure builder | `apply()` | `immediate()` is not copied; stop/cancel globals only log; returned pending sample is not inserted |
| MIDI routes/builders/callbacks | Builder until terminal | Terminal call | Direct output and transport are queued; not synchronously sent |
| DSP graph builders | In-memory graph construction | `body()` / `body_map()` finalizes a synthdef or effect | Graph conversion paths may panic on invalid Dynamic values |

Source implementations: [Group](../../crates/vibelang-rhai/src/api/group.rs),
[Voice](../../crates/vibelang-rhai/src/api/voice.rs),
[routing](../../crates/vibelang-rhai/src/api/route.rs),
[Pattern](../../crates/vibelang-rhai/src/api/pattern.rs),
[Melody](../../crates/vibelang-rhai/src/api/melody.rs),
[Sequence/Fade/Fx](../../crates/vibelang-rhai/src/api/sequence.rs),
[Sample](../../crates/vibelang-rhai/src/api/sample.rs), and
[Record](../../crates/vibelang-rhai/src/api/recording.rs).

## Defaults shared by the authoring model

- Current group is the default group for Voice, Pattern, Fx, and Record.
- Voice defaults: polyphony 4, gain 1, trigger gate, round robin 0, not
  mono-legato, not muted/soloed/modulator-only.
- Pattern and Melody default to four beats. Melody gate is 0.5. Swing is 0.
- Sequence defaults to 16 beats.
- Fade defaults to target group `""`, parameter `amp`, from 0, to 1, duration
  four beats, linear curve.
- Fx defaults to no synth, no parameters, and the current group.

“Bar” conversion is not uniformly time-signature-aware. Global `bars(x)` reads
the current signature, but `Sequence.loop_bars`, `Fade.over_bars`, and several
notation paths hard-code four beats per bar. These inconsistencies are tracked
in the [roadmap](../roadmap/api-improvement-roadmap.md).

## Error and permissiveness model

VibeLang currently mixes four failure styles:

1. Rhai errors for invalid roots/scales, routing ports/rates/conflicts, builder
   port validation, alias conflicts, and deploy failures.
2. Clamping for many musical and MIDI ranges.
3. Warnings plus fallback for missing voices, unknown fade curves, malformed
   melody tokens, and incomplete voices.
4. Silent no-op or ignored input for accepted but unimplemented fields.

Each reference entry calls out the current choice. Do not assume a method that
returns its receiver succeeded; Group output validation, for example, logs and
returns the unchanged handle.

## Hot-reload relationships

Runtime reconciliation diffs stable IDs. Named output ports are matched by
name: renaming or changing a port rate removes the old port and drops dependent
routes with a warning. Unchanged allocated buffers are preserved; changed
frames/channels trigger reallocation. A synthdef body with identical encoded
bytes is not redeployed. Pattern and Melody replacement/start timing is subject
to runtime quantization even though authoring calls only update desired state.

For a single implicit stereo output, the legacy port is named `out`. Explicit
`.output(...)` calls replace that implicit port set.
