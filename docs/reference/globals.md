# Globals, transport, helpers, and assertions

All functions on this page are global in the default `.vibe` engine unless an
availability note says otherwise. Source registrations are
[`helpers::register`](../../crates/vibelang-rhai/src/api/helpers.rs#L13-L76),
[`global::register`](../../crates/vibelang-rhai/src/api/global.rs#L10-L30), and
[`assert::register`](../../crates/vibelang-rhai/src/api/assert.rs#L49-L105).

## Transport declaration

| Exact signature | Return | Snapshot effect and validation |
|---|---|---|
| `set_tempo(bpm: Float)`; `set_tempo(bpm: Int)` | Unit | Sets desired BPM, clamped to 1..999 |
| `get_tempo()` | Float | Reads the current evaluation snapshot |
| `set_time_signature(numerator: Int, denominator: Int)` | Unit | Numerator clamps 1..32. Denominator clamps 1..32 and snaps by buckets to 1, 2, 4, 8, 16, or 32 |
| `set_quantization(beats: Float)`; `set_quantization(beats: Int)` | Unit | Stores `max(beats, 0)`. Explicit 0 means immediate; never calling it preserves runtime next-bar behavior |
| `get_quantization()` | Float | Returns 0 both when unset and when explicitly immediate |
| `get_current_bar()` | Int | Always 1 during script evaluation; it is not a live transport query |

## Music values

| Exact signature | Return | Semantics |
|---|---|---|
| `db(decibels: Float)`; `db(decibels: Int)` | Float | `10^(decibels / 20)` linear amplitude |
| `note(name: String)` | Int | MIDI note 0..127 or Rhai error |
| `chord(name: String)`; `chord(name: String, octave: Int)` | Array of Int | MIDI notes, default octave 4; invalid root or quality errors; members clip to MIDI bounds |
| `scale(root: String, type: String)`; `scale(root: String, type: String, octave: Int)` | Array of Int | One octave of MIDI notes, default octave 4; invalid root/type errors |
| `scale_degree(root: String, type: String, degree: Int)` | Int | Degree from the named scale; invalid root/type errors |
| `bars(count: Float)`; `bars(count: Int)` | Float | Beats using the current time-signature numerator |

`note` accepts A–G case-insensitively, stacked ASCII or Unicode sharps/flats,
and an optional signed octave. Missing or unparseable octave text falls back to
4; the final MIDI value must be 0..127. Examples: `C4`, `d#3`, `Bb5`, `F♯2`.

Chord qualities are: major (`""`, `maj`, `major`, `M`); minor (`m`, `min`,
`minor`, `-`); `7`, `maj7`/`M7`, `m7`/`min7`/`-7`, `dim7`/`°7`,
`m7b5`/`ø7`/`ø`; `sus2`, `sus4`/`sus`, `7sus4`; `aug`/`+`, `aug7`/`+7`;
`dim`/`°`; `add9`, `add11`, `add13`; `9`, `maj9`, `m9`/`min9`, `11`, `13`;
`6`, `m6`/`min6`, and `5`.

Scale types and aliases are `major`/`ionian`,
`minor`/`natural_minor`/`aeolian`, `harmonic_minor`, `melodic_minor`, `dorian`,
`phrygian`, `lydian`, `mixolydian`, `locrian`,
`pentatonic`/`major_pentatonic`, `minor_pentatonic`, `blues`, `chromatic`, and
`whole_tone`.

```rhai
let a4 = note("A4");                 // 69
let c_minor = chord("Cm7", 3);       // MIDI notes
let dorian = scale("D", "dorian", 3);
let half_bar = bars(0.5);
```

## Mixed numeric ranges

VibeLang adds `Int .. Float`, `Float .. Float`, and `Float .. Int`. Each casts
the floating endpoint toward zero and returns an integer half-open Range. Rhai’s
normal integer range remains available. This exists primarily for expressions
such as `0..bars(8)`.

## Arrays

Each function returns a new Array; it does not mutate the input value.

| Exact signature | Result and edge cases |
|---|---|
| `zip(a: Array, b: Array)` | Array of two-element arrays, truncated to the shorter input |
| `shuffle(array: Array)` | Fisher–Yates order using the global VibeLang RNG |
| `rotate(array: Array, positions: Int)` | Positive rotates right, negative left; empty stays empty |
| `reverse(array: Array)` | Reversed array |
| `flatten(array: Array)` | Recursively flattens nested arrays |
| `repeat(array: Array, count: Int)` | Concatenates copies; negative count is zero |
| `take(array: Array, count: Int)` | First `count`; negative is zero |
| `skip(array: Array, count: Int)` | Omits first `count`; negative is zero |

The registered names are exactly `zip`, `shuffle`, `rotate`, `reverse`,
`flatten`, `repeat`, `take`, and `skip`; names such as `array_zip` are not
authoring aliases.

## Random values

| Exact signature | Return | Edge behavior |
|---|---|---|
| `random()` | Float | Approximately `[0, 1]` from a global xorshift64 state |
| `random_range(min: Float, max: Float)` | Float | `[min, max)` formula; reversed/equal/NaN ranges are not validated |
| `random_range(min: Int, max: Int)` | Int | Inclusive; returns `min` when `min >= max` |
| `random_int(max: Int)` | Int | `[0, max)`; returns 0 for `max <= 0` |
| `random_choice(array: Array)` | Dynamic | Unit for an empty array |
| `random_seed(seed: Int)` | Unit | Replaces global RNG state |

Native execution initially seeds from system time. WASM uses a deterministic
constant until `random_seed` is called.

## Numeric helpers

| Exact signature | Return | Notes |
|---|---|---|
| `clamp(value: Float, min: Float, max: Float)`; all-Int overload | Same numeric type | Requires `min <= max`; Rust clamp may panic otherwise |
| `lerp(a: Float, b: Float, t: Float)` | Float | Does not clamp `t` |
| `map_range(value, in_min, in_max, out_min, out_max: Float)` | Float | Equal input endpoints are not guarded |
| `smoothstep(edge0: Float, edge1: Float, x: Float)` | Float | Clamps normalized position; equal edges are not guarded |
| `wrap(value: Float, max: Float)` | Float | Range `[0,max)`; `max <= 0` returns 0 |
| `quantize(value: Float, step: Float)` | Float | Nearest multiple; `step <= 0` returns input |
| `mtof(note: Float)` | Float | MIDI note to Hz at A4=440 |
| `ftom(freq: Float)` | Float | Hz to MIDI; nonpositive input returns 0 |
| `to_int(value: Float)` | Int | Rust cast toward zero, with Rust saturation rules for extremes |
| `to_float(value: Int)` | Float | Numeric cast |
| `to_string(value: Int)`; `to_string(value: Float)` | String | Rust numeric display formatting |
| `timestamp()` | Float | Native only; Unix seconds |
| `timestamp_ms()` | Int | Native only; Unix milliseconds |

The frequency aliases are `mtof` and `ftom`; `midi_to_freq` and `freq_to_midi`
are not registered core names.

## Script tests and exit

Every assertion takes an explicit message. A failed assertion records a failure
and continues unless `test_fail_fast(true)` was set; in fail-fast mode it raises
a Rhai error. Test output uses `[TEST:name]`, `[PASS]`, `[FAIL]`, and
`[DONE] passed/total` records.

| Exact signatures | Purpose |
|---|---|
| `test_start(name: String)`; `test_end() -> Bool`; `test_fail_fast(enabled: Bool)` | Suite lifecycle |
| `exit()`; `exit(code: Int)`; `test_end_and_exit()` | Store an i32 exit code and raise an intentional Rhai Return |
| `assert(condition: Bool, message: String)`; `assert_true(...)`; `assert_false(...)` | Boolean assertions; `assert` aliases `assert_true` |
| `assert_eq(a, b, message)` | Exact overloads for Int, Float, String, Bool, and same-type Dynamic values |
| `assert_ne(a, b, message)` | Exact overloads for Int, Float, String |
| `assert_gt`, `assert_lt`, `assert_gte`, `assert_lte` | `(a: Int,b: Int,message)` or all-Float |
| `assert_approx(a: Float,b: Float,message)` | Default epsilon 0.0001 |
| `assert_approx(a: Float,b: Float,epsilon: Float,message)` | Explicit epsilon |
| `assert_len(array: Array, expected: Int, message)` | Array length |
| `assert_contains(array: Array, value: Int, message)`; String value overload | Array membership of exact supported type |
| `assert_empty(array, message)`; `assert_not_empty(array, message)` | Array emptiness |
| `assert_in_range(value, min, max, message)` | Inclusive all-Int or all-Float overload |
| `assert_starts_with(string,prefix,message)`; `assert_ends_with`; `assert_contains_str` | String predicates |

`exit(code)` casts the Rhai Int to i32. The execution layer recognizes the
raised Return as an intentional exit rather than an ordinary script failure.
