# Runtime objects and routing

This page lists every registered non-DSP core type, constructor, method,
overload, alias, and property. “Chain” means the receiver type is returned.
Review [state lifecycle](runtime-model.md) before relying on fluent syntax.

## GroupHandle

Availability: all targets. Source:
[`group.rs`](../../crates/vibelang-rhai/src/api/group.rs#L430-L477).

### Construction and lookup

| Exact signature | Return | Effect |
|---|---|---|
| `define_group(name: String, body: FnPtr)` | GroupHandle | Creates/resolves a child of the current group, inserts defaults, evaluates body in that context |
| `group(path: String)` | GroupHandle | Resolves relative path/alias only; does not create missing group state |

Closure errors from `define_group` are logged but do not fail the call.

### Members

| Exact signature / property | Return | State and validation |
|---|---|---|
| `name()`; `.name` | String | Last path component |
| `parent()` | String | Parent path, or empty |
| `gain(value: Float)` | Chain | Stores group parameter `amp` |
| `mute()`; `unmute()` | Chain | Snapshot flags |
| `solo(enabled: Bool)` | Chain | Snapshot flag; there is no zero-argument overload |
| `is_muted()`; `.muted` | Bool | Current script snapshot |
| `is_soloed()`; `.soloed` | Bool | Current script snapshot |
| `set_param(name: String, value: Float)` | Chain | Replaces named group parameter |
| `effect_count()` | Int | Length of current effect order |
| `body(body: FnPtr)` | GroupHandle | Evaluates an additional ordered body contribution; closure errors only log |
| `alias(name: String)` | GroupHandle or error | Adds alias; duplicate/conflicting aliases error |
| `output(bus: Int)` | Chain | Mono hardware bus 0..15 |
| `output(buses: Array<Int>)` | Chain | One mono bus, or two consecutive stereo buses, all 0..15 |
| `remove_effect(id: String)` | Chain | Removes from group order and global effect map |
| `clear_effects()` | Chain | Removes every ordered effect and its global map entry |

Invalid output arrays, nonconsecutive stereo pairs, or out-of-range buses log
and return the unchanged handle rather than raising. Calling mutators through a
`group(path)` handle before that group exists can be a no-op. There is no
registered `get_group`, `effect`, or `add_effect`; use `fx(id)...apply()` while
the intended group is current.

```rhai
let drums = define_group("drums", || {
    fx("room").synth("reverb").param("mix", 0.2).apply();
});
drums.gain(db(-3)).output([0, 1]);
```

## Voice

Availability: core on all targets; SFZ overloads are native-only; MIDI
overloads require `midi`. Source:
[`voice.rs`](../../crates/vibelang-rhai/src/api/voice.rs#L856-L929).

### Construction, getters, and properties

| Exact signature / property | Return | Notes |
|---|---|---|
| `voice(name: String)`; `voice()` | Voice | Named or structural anonymous builder |
| `id()` | String | Stable public identity string |
| `name()`; `.name` | String | Resolved builder name |
| `synth_name()`; `.synth_name` | String | Empty when unset |
| `get_gain()`; `.gain` | Float | Builder gain |
| `polyphony()`; `.polyphony` | Int | Builder polyphony |
| `group_path()`; `.group_path` | String | Resolved group path |
| `is_muted()`; `.muted` | Bool | Snapshot flag |
| `is_soloed()`; `.soloed` | Bool | Snapshot flag |

### Builder, action, and route-entry methods

| Exact signature | Return | Semantics |
|---|---|---|
| `group(path: String)` | Voice or error | Resolves group alias/path and syncs named complete voice |
| `synth(name: String)` | Chain | Selects synthdef |
| `on(source: String)` | Chain | Selects source by name |
| `on(sample: SampleHandle)` | Chain | Selects `sample_voice` or `warp_voice`, copying sample settings |
| `on(sfz: SfzHandle)`; `on_sfz(sfz)` | Chain | Native only; alias uses `sfz_voice_stereo` |
| `on(device: MidiDevice)` | Chain | MIDI feature; binds device |
| `channel(channel: Int)` | Chain | MIDI feature legacy internal 0..15 convention |
| `cc_map(param: String, cc: Int)` | Chain | MIDI feature; CC clamps 0..127 |
| `poly(count: Int)` | Chain | Clamps 1..255 |
| `mono_legato(enabled: Bool)` | Chain | Sets mono-legato behavior |
| `gain(value: Float)` | Chain | Injects `amp` unless an explicit `amp` param exists |
| `set_param(name: String, value: Float)`; `param(name,value)` | Chain | Exact aliases; replaces parameter |
| `param(name: String)` | ParamHandle | Target-first modulation entry |
| `round_robin(count: Int)` | Chain | Minimum 0 |
| `choke(group: String)` | Chain | Assigns choke group |
| `modulator_only()` | Chain | Suppresses normal audio output use |
| `mute()`; `unmute()`; `solo()`; `unsolo()` | Chain | Snapshot flags |
| `apply()` | Voice | Gives anonymous builders a stable structural name and synchronizes |
| `run()` | Voice | Synchronizes and marks continuous running |
| `output(name: String)`; `output(index: Int)` | RouteHandle or error | Named or zero-based output port |
| `outputs(entries: Array<String or Int>)` | MultiRouteHandle or error | Nonempty plural selection |
| `input(name: String)` | InputHandle | Named input target; target existence is validated at a terminal call |

Named voices synchronize after essentially every mutation once a source is
complete. An incomplete declaration is not inserted and does not overwrite an
earlier valid version. Unknown parameters and a missing source on `apply()` warn
rather than error. Unknown output names/indices error with the available list;
a synthdef without explicit output metadata has the legacy stereo `out` port.

## Output, input, and parameter routing

Availability: all targets; actual plugin/backend support may vary. Source:
[`route.rs` registrations](../../crates/vibelang-rhai/src/api/route.rs#L1152-L1191).

### RouteHandle

Created by `voice.output(...)`. All terminal methods store routes immediately
and return the RouteHandle unless an error is stated.

| Exact signature | Rate requirement | Effect |
|---|---|---|
| `to(group: GroupHandle)` | Audio (`ar`) | Adds a group destination; duplicate suppressed |
| `to_main()` | Audio | Replaces group/main/muted destination set with main |
| `mute()` | Any | Replaces group/main/muted destination set with muted |
| `to_current_group()` | Audio | Adds resolved current group or errors without context |
| `to_input(target: Voice, input: String)` | Audio | Replaces the target input source |
| `to_param(target: Voice, param: String)`; Fx target overload | Control (`kr`) | Installs source-first SET modulation |
| `to_param_audio(target: Voice, param: String)`; Fx overload | Audio | Installs SET with runtime A2K conversion |
| `to_trigger(target: Voice, param: String)`; Fx overload | Trigger (`tr`) | Installs trigger SET route |
| `scale(value: Float)` | Existing non-trigger modulation | Last value wins; errors before a modulation terminal |
| `offset(value: Float)` | Existing non-trigger modulation | Last value wins; errors before a modulation terminal |

Contrary to older project prose, current audio `.to(group)` routing is additive
fan-out. The first group route clears main/muted; further distinct groups are
added. `to_main()` and `mute()` replace that destination family.

### MultiRouteHandle

Created by `voice.outputs([...])`. It has `to(group)`, `to_main()`, `mute()`,
and `to_current_group()`, applying the same rules to every selected port. It
does not expose input or parameter terminals.

### ParamHandle

Created by `voice.param(name)` or `fx.param(name)`.

| Exact signature | Return | Semantics |
|---|---|---|
| `modulate_by(source: Voice, port: String)` | ParamHandle or error | Target-first BEND route; source must be `kr` |
| `scale(value: Float)`; `offset(value: Float)` | ParamHandle or error | Changes most recently installed modulation; defaults are 1 and 0 |

Source-first SET and target-first BEND write separate maps for one shared route
registry. Mixing both verbs for the same target/parameter is rejected. Multiple
sources may fan into one target parameter, and one source may fan out to
multiple targets. Targets must be registered scalar parameters. Trigger routes
cannot be scaled or offset.

### InputHandle

Created by `target.input(name)`.

| Exact signature | Return | Effect |
|---|---|---|
| `from(source: Voice)` | InputHandle | Uses legacy/default `out`; this path lacks the explicit port-rate validation of the overload below |
| `from(source: Voice, port: String)` | InputHandle or error | Audio-rate named source port |
| `from(source: GroupHandle)` | InputHandle | Group mix as source |
| `from_current_group()` | InputHandle or error | Current group mix |
| `disconnect()` | InputHandle | Replaces source with silence |

Input targets are single-source replacement. Errors identify actual rates,
available ports/parameters, and route conflicts.

### Default output routing

Without explicit routes, a one-port synthdef routes that port to its group; two
ports route both; more than two route the first two and leave the rest silent.
Legacy synthdefs have one stereo `out`. Port identity across reload is by name;
renames and rate changes drop dependent routes with a warning.

```rhai
let env = voice("env").synth("cv_env").modulator_only();
let lead = voice("lead").synth("lead_bright");

env.output("out").to_param(lead, "cutoff").scale(1200.0).offset(200.0);
lead.output("out").to(group("main/leads"));
lead.output("out").to(group("main/record_send")); // additive fan-out
```

## Pattern

Availability: all targets. Source:
[`pattern.rs`](../../crates/vibelang-rhai/src/api/pattern.rs#L400-L438).

| Exact signature / property | Return | Semantics |
|---|---|---|
| `pattern(name: String)`; `pattern()` | Pattern | Named/anonymous pure builder; defaults length 4, swing 0, current group |
| `on(voice: String)`; `on(voice: Voice)` | Chain | Target voice; missing name warns at sync |
| `step(text: String)` | Chain | Parses step notation |
| `euclid(hits: Int, steps: Int)`; `euclid(hits,steps,rotation: Int)` | Chain | Generates step string |
| `len(beats: Float)` | Chain | Loop length |
| `swing(amount: Float)` | Chain | Clamps 0..1 and delays odd events |
| `set_param(name: String, value: Float)` | Chain, current no-op | Accepted builder map is not read during synchronization |
| `apply()` | Pattern or error | Stores snapshot |
| `start()`; `launch()` | Pattern or error | Stores and marks playing; `launch` is alias |
| `stop()` | Unit | Marks stopped |
| `is_playing()`; `.playing` | Bool | Script snapshot |
| `.name` | String | Resolved name; no `name()` method is registered |

`|` separates bars; each nonempty bar is hard-coded to four beats and its
non-whitespace characters divide that bar. `x` velocity is 0.7, `X` is 1,
`o`/`O` is 0.3, digits 1..9 map `digit/9`, and `.`, `_`, `0`, `-` are rests.
Invalid tokens error with position. Euclidean zero steps yields empty, hits at
least steps yields all hits, and rotation normalizes. Negative Rhai counts cast
to `usize` without validation.

## Melody

Availability: all targets. Source:
[`melody.rs`](../../crates/vibelang-rhai/src/api/melody.rs#L1051-L1087).

| Exact signature / property | Return | Semantics |
|---|---|---|
| `melody(name: String)`; `melody()` | Melody | Defaults length 4, gate 0.5, transpose 0, swing 0 |
| `on(voice: String)`; `on(voice: Voice)` | Chain | Target voice |
| `root(note: String)` | Chain | Root used by scale-degree notation |
| `scale(name: String)` | Melody or error | Validates scale name |
| `notes(text: String)`; `notes(values: Array)` | Chain | Parses notation or note values |
| `add_note(beat: Float, note: Int, velocity: Float, duration: Float)` | Chain | MIDI note and velocity clamp |
| `add_chord(beat: Float, notes: Array, velocity: Float, duration: Float)` | Chain | Ignores non-Int entries and empty chords |
| `len(beats: Float)`; `gate(value: Float)`; `swing(value: Float)` | Chain | Gate/swing clamp 0..1 |
| `transpose(semitones: Int)` | Chain | Resulting notes clamp 0..127 |
| `apply()` | Melody | Stores snapshot |
| `start()`; `launch()` | Melody | Stores and marks playing; `launch` alias |
| `stop()` | Unit | Marks stopped |
| `is_playing()`; `.playing` | Bool | Script snapshot |
| `.name` | String | Property only |

Text notation uses four-beat `|` bars and supports absolute notes, scale degrees
1..7, apostrophe/comma octave shifts, `-` ties, `.`/`_` rests, bracket chords,
suffix chords, and per-note option brackets. Invalid scale names error, but many
malformed tokens/notes are silently ignored; an invalid degree root falls back
to C4 and an unknown chord suffix to a major triad.

## Sequence

Availability: all targets. Source:
[`sequence.rs`](../../crates/vibelang-rhai/src/api/sequence.rs#L767-L806).

| Exact signature / property | Return | Semantics |
|---|---|---|
| `sequence(name: String)` | Sequence | Defaults loop length 16 beats; no anonymous overload |
| `loop_bars(value: Float)`; `loop_bars(value: Int)` | Chain | Hard-coded `value * 4` beats |
| `loop_beats(value: Float)` | Chain | Direct loop length |
| `clip(range: Range, source: Pattern or Melody or Fade or Sequence)` | Chain | Adds typed clip; Dynamic dispatch recognizes integer Range plus the listed source types |
| `apply()` | Sequence | Stores snapshot |
| `start()`; `launch()` | Unit | Stores and marks playing; `launch` deprecated alias |
| `stop()` | Unit | Marks stopped |
| `is_playing()`; `.playing` | Bool | Script snapshot |
| `.name` | String | Property only |

The floating-range overloads and Dynamic integer-range path both describe
half-open beat spans. Unsupported Dynamic values silently add nothing. Melody
clips synchronize their builder; Pattern and nested Sequence clips do not.

```rhai
sequence("song")
    .loop_bars(8)
    .clip(0..16, pattern("a").on(kick).step("x..."))
    .clip(16..32, melody("b").on(lead).notes("C4 E4 G4 C5"))
    .start();
```

## Fade

Availability: all targets. Source:
[`Fade`](../../crates/vibelang-rhai/src/api/sequence.rs#L298-L654).

| Exact signature | Return | Semantics |
|---|---|---|
| `fade(name: String)`; `fade()` | Fade | Defaults target group `""`, param `amp`, 0 to 1 over 4 beats, linear |
| `on_group(name: String)` | Chain | Group target |
| `on_voice(name: String)`; `on_voice(voice: Voice)` | Chain | Voice target |
| `on_effect(name: String)`; `on_effect(effect: Fx)` | Chain | Effect target |
| `on(target: Dynamic)` | Chain | Voice, Fx, or String; String means group; unsupported value warns |
| `param(name: String)`; `from(value: Float)`; `to(value: Float)` | Chain | Builder values |
| `over(beats: Float)` | Chain | Minimum 0.0625 beats |
| `over_bars(bars: Int)` | Chain | Hard-coded bars × 4, then same minimum |
| `curve(name: String)` | Chain | Linear/easing curve; unknown silently becomes linear |
| `exp(exponent: Float)` | Chain | Exponential curve; exponent unvalidated |
| `spline(points: Array)` | Chain | Flat `[time,value,...]`; invalid/unpaired entries ignored |
| `point(time: Float, value: Float)` | Chain | Appends spline point; unvalidated |
| `apply()` | Fade, **no-op** | Does not write state |
| `start()`; `launch()`; `now()` | Fade | Writes fade and playing marker; all three currently represent the same immediacy |
| `restart()` | Fade | Same plus force-restart |

There is no registered Fade stop/cancel method. Anonymous names are derived
from target and parameter at synchronization.

## Fx

Availability: all targets. Source:
[`Fx`](../../crates/vibelang-rhai/src/api/sequence.rs#L656-L745).

| Exact signature | Return | Semantics |
|---|---|---|
| `fx(id: String)` | Fx | Pure builder in current group; empty synth and parameter map |
| `synth(name: String)` | Chain | Selects effect synthdef |
| `param(name: String, value: Float)` | Chain | Builder parameter |
| `param(name: String)` | ParamHandle | Effect-target modulation entry |
| `apply()` | Fx | Stores effect and appends ID to existing group order |

Only `apply()` synchronizes. It accepts an empty synth name. If the current
group configuration does not yet exist, its effect order cannot be appended.
There are no Fx getters/properties or registered Group convenience methods.

## SampleHandle

Availability: all targets, with native/WASM path-resolution differences.
Source: [`sample.rs`](../../crates/vibelang-rhai/src/api/sample.rs#L337-L397).

`sample(id: String, path: String) -> SampleHandle` inserts immediately. On
native builds, a relative script-adjacent path is used only if it exists;
otherwise the original path is retained. WASM retains the raw path.

| Exact member | Return | Semantics/default |
|---|---|---|
| `id()` / `.id`; `path()` / `.path` | String | Handle fields |
| `bufnum()` / `.bufnum`; `buffer_id()` / `.buffer_id` | Int | Exact aliases; initially 0 until runtime assignment |
| `attack(seconds: Float)`; `release(seconds: Float)` | Chain | Defaults 0.001 / 0.01; no duration validation |
| `sustain(level: Float)` | Chain | Default 1; clamps 0..1 |
| `amp(value: Float)`; `rate(value: Float)` | Chain | Defaults 1; rate unvalidated |
| `loop_mode(enabled: Bool)` | Chain | Default false |
| `offset(seconds: Float)`; `length(seconds: Float)` | Chain | Default offset 0/no length; unvalidated |
| `warp(enabled: Bool)` | Chain | Default false |
| `speed(value: Float)`; `pitch(value: Float)`; `semitones(value: Float)`; `warp_to_bpm(bpm: Float)` | Chain | Enable warp; speed/pitch defaults 1 |
| `window_size(seconds: Float)` | Chain | Default 0.1; clamps 0.01..1 |
| `overlaps(count: Float)` | Chain | Default 8; clamps 1..32 |
| `one_shot()`; `gate()` | Chain | Select trigger mode; default gate |
| `slice(start_seconds: Float, end_seconds: Float)` | Chain | Sets slice without ordering/range validation |

Every mutator rewrites the inserted sample snapshot.

## BufferHandle

Availability: all targets. Source:
[`buffer.rs`](../../crates/vibelang-rhai/src/api/buffer.rs#L134-L144).

| Exact signature / property | Return | Semantics |
|---|---|---|
| `allocate_buffer(name: String, frames: Int, channels: Int)` | BufferHandle | Inserts immediately; frames minimum 1, channels clamp 1..16 |
| `name()`; `.name` | String | Allocation name |
| `bufnum()`; `.bufnum` | Float | Deliberately Float for synth parameters |

IDs are deterministic FNV-derived values in 2048..4095 with linear collision
probing; exhaustion panics. Same name/shape is stable across reload, while a
changed shape reallocates.

## SfzHandle

Native only. Source:
[`sfz.rs`](../../crates/vibelang-rhai/src/api/sfz.rs#L89-L104).

`load_sfz(id: String, path: String) -> SfzHandle` inserts immediately. Methods
and properties are `id()` / `.id` and `path()` / `.path`. Relative-path fallback
matches Sample. Author-time code does not verify file existence or parse the
instrument. Bind it with `voice(...).on(handle)` or the `on_sfz` alias.

## RecordHandle

Native only. Source:
[`recording.rs`](../../crates/vibelang-rhai/src/api/recording.rs#L256-L294).

| Exact signature / property | Return | Semantics |
|---|---|---|
| `record(id: String)` | RecordHandle | Defaults current group, no length, count-in 0, metronome false, no path, non-immediate, 2 channels |
| `id()` / `.id`; `group_path()` / `.group_path` | String | Builder fields |
| `bars(value: Float or Int)`; `beats(value: Float or Int)`; `seconds(value: Float or Int)` | Chain | Mutually replace length mode; values unvalidated; bars reads current signature at apply |
| `from_group(path: String)` | Chain | Resolves group at apply |
| `count_in(bars: Float)`; `metronome(enabled: Bool)` | Chain | Values unvalidated |
| `to_file(path: String)` | Chain | Relative paths become script-relative |
| `immediate()` | Chain, current no-op | Builder flag is not copied into core config |
| `channels(count: Int)` | Chain | Clamps 1..2 |
| `apply()` | SampleHandle | Stores recording request and returns pending handle; returned sample is not inserted in sample map |
| `stop_recording(id: String)`; `cancel_recording(id: String)` | Unit, current stubs | Only log intent; no runtime command is dispatched |

Treat the three explicit gaps as current limitations, not future promises.
