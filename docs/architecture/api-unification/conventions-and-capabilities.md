# Conventions and runtime capabilities

- Status: proposed contract for API-unification synthesis
- Assessed baseline: `e5a1198a3bb478418042f2b517172f74635742b7`
- Candidate contained by the baseline: `f00c04ca1a1e79d644211eed64fc472214a75d58`

## Decision

Vibelang will publish one versioned vocabulary for quantities, numeric domains,
availability, capabilities, and compatibility diagnostics. The vocabulary is the
same in Rhai metadata, HTTP and WebSocket schemas, WASM bindings, generated
documentation, and editor projections.

The contract has five non-negotiable rules:

1. Every public numeric parameter declares a stable `unit_id`, `range_id`, and
   invalid-value policy. An intentionally unrestricted value declares that fact;
   omission is not a synonym for unbounded.
2. New and version-2 entry points reject non-finite, out-of-range, malformed, or
   ambiguous values. Clamping and fallback survive only behind an explicit
   compatibility profile and always emit a structured diagnostic.
3. Public MIDI channels and UMP groups are numbered 1 through 16. Zero-based
   storage remains an implementation detail exposed only by explicitly named
   `*_index` compatibility fields. Notes and raw controller values remain their
   native integer widths; high-level voice and melody velocity is normalized.
4. A capability is not “available” merely because code compiled. Availability is
   the evaluated conjunction of declaration, target, build feature, operator
   policy, runtime probe, backend semantic support, and quarantine state.
5. Capability snapshots are atomic, revisioned, deterministic, provenance-rich,
   and privacy-minimal. They report semantic support and constraints, never
   secrets or unnecessary local identifiers.

This is a design artifact. It changes no runtime behavior.

## Evidence and counting method

The source anchors and counts below refer to the assessed baseline. Counts were
computed from `api/public-api-manifest-v1.json`,
`api/http-api-snapshot-v1.json`, generated UGen JSON, and the editor bundles;
source was inspected where a manifest classification did not prove behavior.
An “entry” is one manifest identity and an “overload” is one callable signature.

The effective manifest contains 3,626 entries and 8,431 overloads. Excluding
generated UGen surfaces, it contains 786 Rhai, DSP-Rhai, and extension entries
with 875 overloads. The authoring projection contains 477 function entries / 600
overloads, 275 property entries, and 34 types. The generated DSP surface contains
1,174 entries / 5,962 overloads, derived from 875 UGen records in 70 files. The
stdlib scan finds 890 declarations in 829 files, comprising 887 distinct names.
The HTTP snapshot exposes 96 routes and 75 types.

### Availability inventory

| Manifest status | Entries | Overloads | Meaning in the current manifest |
| --- | ---: | ---: | --- |
| `available` | 1,617 | 6,353 | Declared callable, but not necessarily effectful on the active backend |
| `conditional` | 343 | 484 | Build, target, runtime, or plugin condition exists |
| `importable` | 1,593 | 1,594 | Stdlib definition discoverable through import |
| `documentation_only` | 48 | 0 | Builder form documented but not registered |
| `quarantined` | 25 | 0 | Demand-rate callable deliberately excluded |

The conditional set includes 238 MIDI-feature entries, 45 extension entries
(21 filesystem, 13 process execution, and 11 network), and 12 `mi-UGens`
entries with 156 overloads. Native/non-WASM conditions appear throughout the
recording, SFZ, backend, and MIDI surfaces. These dimensions currently collapse
into a single status even though their remediation and security implications are
different.

### Boundary inventory

| Surface | Overloads | Clamp evidence | Range evidence | Fallback evidence | Structured-error evidence | Potential-panic evidence |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| DSP Rhai | 147 | 9 | 27 | 0 | 44 | 67 |
| DSP UGen | 5,962 | 30 | 92 | 4,788 | 1 | 5,962 |
| Rhai | 683 | 82 | 113 | 96 | 103 | 2 |
| Rhai extensions | 45 | 0 | 13 | 14 | 31 | 2 |
| Stdlib | 1,594 | 0 | 92 | 138 | 11 | 38 |
| **Total** | **8,431** | **121** | **337** | **5,036** | **190** | **6,071** |

These are lexical/static evidence classes, not proofs that every path reaches a
clamp, fallback, or panic. They do prove that the public contract cannot be
recovered reliably from Rust/Rhai types alone. Across 18,786 manifest overload
parameter occurrences, none has a `unit`, `range`, or non-null `default` field.
The 5,089 generated UGen input records do have 5,048 defaults, but none has a
unit or range. All 426 explicitly numeric, non-UGen accepted-type occurrences
likewise omit units and ranges.

## Current convention inventory

### MIDI has four public numeric dialects

| Surface and source anchor | Current representation | Consequence |
| --- | --- | --- |
| `crates/vibelang-rhai/src/api/midi/device.rs`, `MidiDevice::channel`, `route_to_channel` | Setter accepts/clamps 1–16 and stores 0–15 | Human-facing convention is clear on input |
| Same file, `MidiDevice::get_channel` | Returns stored 0–15 | Setter/getter do not round-trip in one public unit |
| Same file, `MidiDevice::note_on`, `note_off`, `cc`, `program_change`, `pitch_bend` | Channel 1–16; note, velocity, and CC 0–127; pitch bend -8192–8191; all clamp | Raw MIDI values are accepted but failures are hidden |
| Same file, `MidiDevice::note_on_hires` and MIDI 2 helpers | Velocity 0–65,535; controller/per-note values 0–`u32::MAX` | Correct native widths, but field names do not encode width |
| `crates/vibelang-core/src/midi/events.rs`, `Velocity`, `ControlValue`, `GroupChannel` | Internal velocity `u16`, control `u32`, group/channel 0–15 | Suitable storage, unsuitable implicit public numbering |
| `crates/vibelang-core/src/traits/melodies.rs`, `NoteEvent` | Note 0–127, velocity `f32` documented 0–1 | High-level normalized dialect |
| `crates/vibelang-rhai/src/api/melody.rs`, `MelodyBuilder::add_note`, per-note parameters | Note clamps 0–127; velocity accepts 0–1 or interprets values above 1 as `/127` | One field heuristically accepts two units |
| `crates/vibelang-http/src/routes/midi.rs`, `SendNoteRequest`, `send_note` | `u8` channel/note/velocity, default velocity 100; no explicit ranges; channel forwarded verbatim | Wire channel is effectively zero-based core input and conflicts with Rhai |
| `crates/vibelang-http/src/models.rs`, `NoteOnRequest` | Note `u8`, velocity `f32`, default 0.8 | Voice endpoint is normalized but schema does not name the unit |
| `vscode-extension/src/views/melodyEditor.ts`, note preview request | Sends integer velocity 1–127 to the normalized voice endpoint | Editor can request gains far above unity |

`crates/vibelang-rhai/src/api/midi/routing.rs`, `KeyboardRoute::channel`, and
`crates/vibelang-core/src/midi/callbacks.rs`,
`KeyboardRouteBuilder::channel`, repeat the 1–16-to-0–15 conversion. Invalid
named note ranges preserve default bounds. `VelocityCurve` normalizes MIDI 1
values, while `ParameterCurve::from_name` returns `None` and its builder caller
silently retains the default. In `crates/vibelang-rhai/src/api/midi/midi2.rs`,
`GroupRoute` exposes group 0–15 but channel 1–16, and malformed named ranges fall
back to 0/127. The mixed group/channel convention is not justified by the wire
format because both fields are stored as four-bit indices.

Device lookup is also success-shaped: `midi_device` in
`crates/vibelang-rhai/src/api/midi/mod.rs` logs a warning and returns a sentinel
device with `u32::MAX` identity when a device is absent. That is an availability
failure and must not masquerade as a working handle.

### Beats and bars do not share one meter model

`Beat` in `crates/vibelang-core/src/types/time.rs` is a signed fixed-point
quarter-note position with 16 fractional bits, a precision of 1/65,536 beat.
`Duration` is the semantic relative wrapper. `TimeSignature::beats_per_bar`
correctly computes `numerator * 4 / denominator`.

The Rhai `bars()` helper in `crates/vibelang-rhai/src/api/helpers.rs` uses the
current contextual beats per bar, and audio `RecordingBuilder::bars` in
`crates/vibelang-rhai/src/api/recording.rs` uses the current full time signature.
Other public paths hard-code four beats:

- `calculate_loop_length_from_pattern` and the pattern time parser in
  `crates/vibelang-rhai/src/api/pattern.rs`;
- melody duration parsing in `crates/vibelang-rhai/src/api/melody.rs`;
- `SequenceBuilder::loop_bars` and `FadeBuilder::over_bars` in
  `crates/vibelang-rhai/src/api/sequence.rs`;
- MIDI recording duration in
  `crates/vibelang-rhai/src/api/midi/recording.rs` and
  `crates/vibelang-core/src/traits/recordings.rs`.

The HTTP WebSocket transport snapshot in
`crates/vibelang-http/src/websocket.rs`, `build_transport_payload`, and runtime
MIDI clock setup in `crates/vibelang-core/src/runtime.rs` use the time-signature
numerator as beats per bar and ignore the denominator. They happen to be right
for quarter-note denominators and wrong for, for example, 6/8.

`parse_time_spec` in `crates/vibelang-rhai/src/api/helpers.rs` accepts beats,
bars, milliseconds, seconds, fractions, and plain numbers, but its `bar` branch
is fixed at four beats and a fraction means a fraction of a whole note. This
mixes contextual musical duration, fixed musical duration, and wall-clock time
in one string without recording which branch was selected.

### Frequency, gain, and level are related but not interchangeable

`db` in `crates/vibelang-rhai/src/api/helpers.rs` implements
`10^(dB/20)` without finite/range checks. The same conversions appear as
`db_to_amp` and `amp_to_db` in `crates/vibelang-dsp/src/helpers.rs`; non-positive
amplitude reaches `log10` and can produce infinity or NaN. `mtof` and `ftom` in
the Rhai helpers use hertz, but `ftom` silently returns zero for non-positive
frequency.

`Voice::gain` in `crates/vibelang-rhai/src/api/voice.rs` and `SampleHandle::amp`
allow linear gain above 1.0. The LSP completion table in
`crates/vibelang-lsp/src/features/completion.rs` nevertheless describes common
`amp` as 0–1. It also hard-codes frequency as hertz and several timing units
instead of deriving them from the public contract. A normalized control ratio,
linear gain, and decibel level therefore need distinct identities.

### Parsers range from strict to deliberately lossy

The note grammar in `crates/vibelang-dsp/src/notes.rs`,
`parse_note_name_raw`, defaults a missing or unparseable octave to 4 and does not
require full consumption. Its tests explicitly accept `C4x`, `Cx`, `C300`, and
`CB4` as C4. The Rhai `note()` wrapper reports an error only when that forgiving
parser returns `None`, so trailing garbage remains accepted.

By contrast, `parse_chord` in `crates/vibelang-rhai/src/api/helpers.rs` rejects
unknown chord qualities and out-of-MIDI-range roots, and the scale helpers
return structured errors. Melody tokenization in
`crates/vibelang-rhai/src/api/melody.rs` can silently skip invalid chord notes,
return a partial result after malformed parameter syntax, default unknown chord
suffixes to major, and default root/scale resolution to C4/major.

MIDI object builders repeat the fallback pattern: `MidiDevice::note` and
`MidiDevice::pad` in `crates/vibelang-rhai/src/api/midi/device.rs` warn and use
C4 for unsupported dynamic values; curve parsers in
`crates/vibelang-rhai/src/api/midi/cc_mapping.rs` and `bend_mapping.rs` default
unknown curve names to `linear`.

### Duplicate stdlib names have order-dependent outcomes

The manifest records four duplicate definition names:

| Name | First source | Second source | Current resolution |
| --- | --- | --- | --- |
| `lfo_random` | `crates/vibelang-std/stdlib/cv/lfo/lfo_random.vibe:4` | `crates/vibelang-std/stdlib/utility/lfo.vibe:61` | Synthdef registry insertion order replaces the earlier definition |
| `lfo_saw` | `crates/vibelang-std/stdlib/cv/lfo/lfo_saw.vibe:4` | `crates/vibelang-std/stdlib/utility/lfo.vibe:44` | Same |
| `lfo_sine` | `crates/vibelang-std/stdlib/cv/lfo/lfo_sine.vibe:4` | `crates/vibelang-std/stdlib/utility/lfo.vibe:8` | Same |
| `arpeggio_up_down` | `crates/vibelang-std/stdlib/theory/arpeggios.vibe:33` | `crates/vibelang-std/stdlib/theory/bass_patterns.vibe:212` | Module-qualified functions coexist; an unqualified import relies on Rhai resolution |

`deploy_synthdef_ir` in `crates/vibelang-dsp/src/api.rs` uses
`HashMap::insert` for the global synthdef/effect registries without a collision
diagnostic. A same-spelled function and synthdef, such as `power_chord`, is not a
collision when the kind is part of the identity; two synthdefs in the same
global registry are.

### Build, target, policy, probe, and backend semantics are conflated

The CLI defaults in `crates/vibelang-cli/Cargo.toml` compile MIDI, API, LSP, and
extensions. `crates/vibelang-core/Cargo.toml` and
`crates/vibelang-rhai/Cargo.toml` default to native plus MIDI;
`crates/vibelang-http/Cargo.toml` defaults to native plus MIDI; DSP has a WASM
feature. These flags prove code inclusion only.

`ExtensionConfig` and `register_extensions` in
`crates/vibelang-rhai/src/extensions/mod.rs` add an operator-policy layer for
filesystem, execution, and network extensions. `list_available_extensions`
reports compile-time inclusion, not whether policy enabled registration.
`build_extension_config` in `crates/vibelang-cli/src/main.rs` enables compiled
extensions for local scripts unless a `--no-*` flag disables them, while HTTP
`/eval` receives no extensions unless `--api-allow-extensions` is set. The same
binary therefore has at least two extension capability scopes.

Native Rhai registers recording and SFZ APIs that are absent on WASM
(`crates/vibelang-rhai/src/api/mod.rs`). Core selects `ScsynthBackend` natively
and `WebScsynthBackend` on WASM (`crates/vibelang-core/src/lib.rs` and
`backends/mod.rs`). The duplicated `Backend` traits in
`crates/vibelang-core/src/backend.rs` include default methods that ignore
scheduling, return zero, or no-op. `WebScsynthBackend::write_buffer` in
`crates/vibelang-core/src/backends/web_scsynth.rs` returns success even though
file writing is unsupported. Method presence therefore overstates capability.

Audio recording is native-only. Its Rhai builder stores an `immediate` choice
but constructs a configuration with no start beat, and stop/cancel paths only
log. HTTP exposes four recording routes under native configuration. MIDI exposes
18 feature-gated HTTP routes. These surfaces must distinguish `declared`,
`enabled`, and `effectful`.

The 12 `mi-UGens` entries are plugin-conditional, but no runtime plugin probe was
found. `vscode-extension/src/utils/ugenAvailability.ts` filters demand-rate and
builder-only records, not installed plugins. The 25 quarantined demand-rate
identities and 48 documentation-only builder records are intentionally not
callable and must remain first-class non-available states rather than disappear
from metadata.

### HTTP has a local default, not a complete security mode

The API defaults to `127.0.0.1:1606` in
`crates/vibelang-cli/src/main.rs`; `/eval` extensions require a separate flag.
The server applies `CorsLayer` with any origin, method, and header in
`crates/vibelang-http/src/lib.rs`. No authentication, origin allowlist, body-size
limit, or rate limit is present in the assessed server. Loopback binding reduces
exposure but does not convert unrestricted CORS and effectful evaluation into an
explicit security contract.

The WebSocket protocol in `crates/vibelang-http/src/websocket.rs` has protocol
version 1 and sends a hello containing event/command lists and an initial state.
Events have type, timestamp, and data, but no capability generation, state
revision, event sequence, or resynchronization token. A lagged broadcast closes
the connection without a gap event. The hello is therefore a protocol inventory,
not runtime capability truth.

### Editor projections are incomplete and drift independently

The LSP and VS Code Rhai projections each contain 600 overload rows for 354
names: 248 free-function and 352 method rows across 32 non-null receiver names.
They omit the 275 manifest properties. The VS Code UGen bundle has 316
runtime-callable records in 24 JSON files and derives 478 unique completion
labels. Only 470 match current runtime callable names; these eight are stale:

`a2_k_kr`, `k2_a_ar`, `lag2_ud_ar`, `lag2_ud_kr`, `lag3_ud_ar`,
`lag3_ud_kr`, `t2_a_ar`, and `t2_k_kr`.

The manifest has 1,174 callable UGen identities, so 704 do not appear in that
bundle. `ugenAvailability.ts` applies static exclusions but has no runtime
capability input. The LSP hard-codes 16 common parameters rather than consuming
unit/range metadata. The semantic-token emitter legend in
`crates/vibelang-lsp/src/features/semantic_tokens.rs` has 15 token types and six
modifiers, while `crates/vibelang-lsp/src/server.rs` advertises 17 types and three
modifiers. Push diagnostics perform validation and unknown-symbol checks that the
pull diagnostic implementation omits. Capability projection cannot be considered
complete while these consumers infer support independently.

## Normative contract

The keywords MUST, MUST NOT, SHOULD, and MAY in this section are normative.

### Identifier and schema rules

- Stable IDs use lowercase dotted ASCII segments matching
  `[a-z][a-z0-9_]*`. They are semantic identities, not localized labels.
- An ID MUST NOT be repurposed. A changed meaning receives a new ID and the old
  ID becomes an alias or tombstone with a compatibility class.
- Units, ranges, invalid-value policies, capabilities, availability reasons,
  diagnostics, and provenance kinds have independent namespaces.
- All generated projections preserve these IDs even where the host language
  also provides an ergonomic name.
- JSON numbers MUST be finite. NaN and infinity are rejected before backend
  dispatch. Integer JSON fields MUST be mathematically integral and within the
  declared exact-safe range.
- Public scalar fields use unit-qualified names where context would otherwise be
  ambiguous (`duration_beats`, `frequency_hz`, `velocity_7bit`). The schema and
  effective contract still carry `unit_id` and `range_id`; values are not wrapped
  in an object on every hot-path call.
- Generic metadata or extension maps use a tagged quantity:

```json
{
  "value": 440.0,
  "unit_id": "unit.frequency.hz",
  "range_id": "range.finite.positive"
}
```

Unknown IDs are rejected in mutation input. Consumers MAY retain and display
unknown IDs in read-only snapshots for forward compatibility.

### Stable unit IDs and wire values

| Unit ID | Canonical wire value | Normative meaning |
| --- | --- | --- |
| `unit.scalar` | finite JSON number | Dimensionless value with a parameter-specific range |
| `unit.ratio.normalized` | finite JSON number | Inclusive normalized ratio from 0 through 1 |
| `unit.amplitude.linear` | finite JSON number | Linear gain, non-negative and not globally capped at unity |
| `unit.level.decibel` | finite JSON number | Signed amplitude level in dB; conversion uses `20 * log10(amplitude)` |
| `unit.frequency.hz` | finite JSON number | Cycles per second; frequency-producing inputs are positive unless a parameter explicitly permits zero |
| `unit.pitch.semitone` | finite JSON number | Equal-tempered semitone interval; fractional values are allowed |
| `unit.tempo.bpm` | finite JSON number | Quarter-note beats per minute, 1 through 999 in contract v1 |
| `unit.time.beat.quarter` | JSON number quantized to 1/65,536 | Musical position/duration measured in quarter-note beats |
| `unit.time.bar` | tagged quantity only | Contextual duration resolved using a referenced time-signature revision |
| `unit.time.second` | finite JSON number | Wall-clock/audio duration in seconds |
| `unit.time.millisecond` | finite JSON number | Wall-clock/audio duration in milliseconds |
| `unit.audio.sample_frame` | integer | Frame index/count, not individual interleaved samples |
| `unit.audio.sample_rate_hz` | finite JSON number | Sample frames per second, positive |
| `unit.audio.bus_index` | non-negative integer | Backend bus index; availability is backend-scoped |
| `unit.midi.channel` | integer | Human-facing MIDI channel 1 through 16 |
| `unit.midi.channel_index` | integer | Explicit compatibility/storage index 0 through 15 |
| `unit.midi.group` | integer | Human-facing UMP group 1 through 16 |
| `unit.midi.group_index` | integer | Explicit compatibility/storage index 0 through 15 |
| `unit.midi.note` | integer | Equal-tempered MIDI note number 0 through 127 |
| `unit.midi.velocity.normalized` | finite JSON number | High-level normalized velocity 0 through 1 |
| `unit.midi.velocity.7bit` | integer | MIDI 1 velocity 0 through 127 |
| `unit.midi.velocity.16bit` | integer | MIDI 2 velocity 0 through 65,535 |
| `unit.midi.control.7bit` | integer | Seven-bit controller value 0 through 127 |
| `unit.midi.control.14bit` | integer | Fourteen-bit controller value 0 through 16,383 |
| `unit.midi.control.32bit` | integer | Unsigned 32-bit controller/per-note value, encoded as a JSON integer |
| `unit.midi.pitch_bend.14bit_signed` | integer | Centered pitch bend -8,192 through 8,191 |

A bar quantity MUST include `time_signature` (`numerator` and `denominator`) or a
`time_signature_revision` that resolves atomically with the operation. It MUST
convert using `numerator * 4 / denominator`; fixed-four bar conversion is never
the default. Bare beats always mean quarter-note beats. A parser may offer
explicit tokens such as `2 bars@6/8`, but it must preserve the selected unit and
meter provenance until resolution.

Raw and normalized MIDI velocity MUST NOT share a field name in new schemas.
Conversions are explicit functions and diagnostics name source and target units.
The HTTP v1 MIDI channel cannot be inferred safely for values 1–15, so a v2
field/endpoint is required rather than a value heuristic.

`amplitude -> dB` requires amplitude greater than zero. `dB -> amplitude`
requires a finite dB value. Non-positive hertz is rejected by pitch conversion.
Zero-frequency oscillator or DC semantics, where valid, use a parameter-specific
range rather than weakening the unit globally.

### Stable range and invalid-value IDs

| Range ID | Domain |
| --- | --- |
| `range.finite.unbounded` | Any finite JSON number |
| `range.finite.nonnegative` | Finite number >= 0 |
| `range.finite.positive` | Finite number > 0 |
| `range.closed.0_1` | Finite number in [0, 1] |
| `range.integer.nonnegative` | Integer >= 0 within the schema's storage width |
| `range.midi.index.0_15` | Integer in [0, 15] |
| `range.midi.channel.1_16` | Integer in [1, 16] |
| `range.midi.note.0_127` | Integer in [0, 127] |
| `range.midi.u7` | Integer in [0, 127] |
| `range.midi.u14` | Integer in [0, 16,383] |
| `range.midi.u16` | Integer in [0, 65,535] |
| `range.midi.u32` | Integer in [0, 4,294,967,295] |
| `range.midi.pitch_bend14_signed` | Integer in [-8,192, 8,191] |
| `range.tempo.bpm.1_999` | Finite number in [1, 999] |
| `range.time_signature.numerator.1_32` | Integer in [1, 32] |
| `range.time_signature.denominator.power_of_two` | One of 1, 2, 4, 8, 16, or 32 |

The canonical metadata for each parameter is:

```json
{
  "unit_id": "unit.midi.channel",
  "range": {
    "range_id": "range.midi.channel.1_16",
    "minimum": 1,
    "maximum": 16,
    "minimum_inclusive": true,
    "maximum_inclusive": true,
    "finite": true
  },
  "invalid_value_policy_id": "invalid.reject"
}
```

Stable invalid-value policies are `invalid.reject`, `invalid.compat_clamp`,
`invalid.compat_fallback`, and `invalid.compat_drop`. Only `invalid.reject` is
valid in a canonical v2 signature. The other policies describe legacy behavior
and MUST carry a diagnostic. Snapping a time-signature denominator is a
compatibility coercion, not validation.

### Parser contract

Canonical parsers MUST consume the full input, use one documented grammar, and
return a structured error with code, span, expected form, and offending token.
They MUST NOT silently drop tokens or replace an invalid enum, curve, scale,
note, chord, target, or unit with a default.

Forgiving behavior remains available only as one of:

- an explicitly named API such as `parse_note_compat` or `value_or(default)`;
- a request carrying `compatibility_profile_id: "compat.vibelang.v1"`; or
- a user-authored recovery expression whose fallback is visible in source.

Every library-applied recovery emits `diagnostic.compat.fallback_applied`,
including input span, fallback value, source and target unit IDs, and a stable
replacement suggestion. Partial parses are failures unless the grammar itself
declares a list recovery mode and reports every skipped element.

### Duplicate-name contract

The stable definition identity is `(kind_id, module_id, local_name)`. Function,
synthdef, effect, type, and property kinds occupy distinct namespaces. Qualified
module functions may share a local name. An unqualified import that resolves to
more than one identity is an error with all candidates listed.

Synthdefs and effects deployed into a global runtime registry MUST reject an
existing fully qualified identity unless the submitted canonical definition hash
is identical. An identical redeployment is idempotent and reports
`diagnostic.registry.already_present`; source-order replacement is forbidden.

The canonical LFO definitions are those under `stdlib/cv/lfo/`. The utility LFO
definitions become deprecated qualified aliases/imports, then are removed after
the compatibility window. The two `arpeggio_up_down` functions retain qualified
identities; unqualified resolution requires an explicit import/alias. A name
shared across kinds remains legal only through a typed/qualified reference.

### Capability IDs

Capability IDs describe observable semantics, not source modules. The initial
registry MUST include at least:

| Capability ID | Required truth |
| --- | --- |
| `capability.audio.render.realtime` | Active backend can render and apply realtime graph mutations |
| `capability.audio.schedule.absolute_beat` | Backend honors absolute-beat scheduling rather than accepting and ignoring it |
| `capability.audio.control_bus.read` | Backend returns real control-bus values |
| `capability.audio.buffer.write_file` | Backend can persist a buffer to the requested destination |
| `capability.backend.scsynth.native` | Native scsynth backend initialized and responsive |
| `capability.backend.web_scsynth.wasm` | WASM bridge initialized and semantically responsive |
| `capability.midi.input` | MIDI input compiled, policy-enabled, initialized, and at least probeable |
| `capability.midi.output` | MIDI output compiled, policy-enabled, initialized, and at least probeable |
| `capability.midi.clock` | MIDI clock operations are supported by the active target/backend |
| `capability.midi.ump` | MIDI 2/UMP path is supported, not merely type-compiled |
| `capability.recording.audio` | Audio recording terminal operations are effectful |
| `capability.recording.midi` | MIDI recording terminal operations are effectful |
| `capability.resource.sfz` | SFZ load/play behavior is effectful on this target |
| `capability.extension.filesystem` | Extension compiled and enabled for the current evaluation scope |
| `capability.extension.process` | Process execution compiled and enabled for the current evaluation scope |
| `capability.extension.network` | Network extension compiled and enabled for the current evaluation scope |
| `capability.plugin.mi_ugens` | Required plugin family was positively probed |
| `capability.http.eval` | Eval route enabled in the named security/evaluation scope |
| `capability.api.ugen.demand_rate` | Demand-rate UGen family callable; initially unavailable due to quarantine |
| `capability.editor.rhai_projection` | Editor projection matches the effective contract revision |
| `capability.editor.ugen_projection` | Editor UGen projection matches callable identities and active availability |

New fine-grained capabilities MAY be added without redefining these IDs. A
client must check the specific semantic capability it needs; it must not infer
buffer writes from “WASM backend available,” for example.

### Availability evaluation

Each capability and callable entry has one runtime `state_id`:

- `availability.available`: all required gates passed and the declared semantics
  are effectful;
- `availability.degraded`: usable behavior exists but a declared semantic or
  quality constraint is absent; constraints and reason IDs are mandatory;
- `availability.unavailable`: a known gate failed or the API is quarantined;
- `availability.unknown`: required runtime probing has not completed or its truth
  cannot be observed.

`conditional` is declaration metadata, not a runtime state. `documentation_only`
and `quarantined` map to unavailable with a stable reason while remaining visible
to documentation and migration tools.

The evaluator processes these gates in order and retains all applicable reasons:

1. contract declaration and quarantine;
2. target support (`native`, `wasm32`, operating system);
3. compile feature;
4. operator/security policy and evaluation scope;
5. runtime dependency/plugin/device probe;
6. backend semantic probe;
7. consumer projection revision, when reporting editor capability.

Stable reason IDs include
`reason.quarantined`, `reason.documentation_only`,
`reason.target_unsupported`, `reason.compile_feature_disabled`,
`reason.operator_disabled`, `reason.security_policy_disabled`,
`reason.runtime_dependency_missing`, `reason.plugin_missing`,
`reason.probe_pending`, `reason.probe_failed`,
`reason.backend_semantics_missing`, `reason.implementation_noop`, and
`reason.editor_projection_stale`.

An entry may be present in metadata while unavailable. An absent runtime device
does not remove `capability.midi.output`; it yields an unavailable instance or an
available subsystem with zero privacy-redacted instances, depending on the
requested operation. Lookup returns a typed unavailable error, never a sentinel
handle.

### Capability snapshot v1

All runtime projections expose the semantic equivalent of this payload:

```json
{
  "schema_id": "schema.vibelang.capability_snapshot.v1",
  "contract_revision": "sha256:<effective-contract-hash>",
  "generation": 42,
  "snapshot_id": "sha256:<semantic-payload-hash>",
  "observed_at": "2026-07-15T20:30:00Z",
  "subject": {
    "runtime_id": "runtime.local",
    "target_id": "target.native.linux",
    "build_revision": "<source-revision>"
  },
  "mutation_revision": {
    "accepted": 106,
    "applied": 104
  },
  "security": {
    "mode_id": "security.http.loopback_local",
    "authenticated": false,
    "origin_policy_id": "origin.loopback_only"
  },
  "capabilities": [
    {
      "capability_id": "capability.audio.schedule.absolute_beat",
      "state_id": "availability.degraded",
      "scope_id": "scope.runtime",
      "reason_ids": ["reason.backend_semantics_missing"],
      "constraints": {"accepted_but_ignored": true},
      "provenance": [
        {"kind_id": "provenance.backend_probe", "probe_id": "probe.schedule.v1"}
      ]
    }
  ]
}
```

The sibling revision-receipt contract owns the exact mutation revision shape;
the snapshot MUST embed its accepted/applied identity without translating it
into a second revision system.

Snapshot rules:

- `generation` is monotonic within one runtime instance and increments whenever
  any semantic state, reason, constraint, policy, or relevant projection changes.
- The snapshot is assembled atomically against one runtime/configuration and
  mutation-revision view. Entries are sorted by `capability_id`; reason IDs and
  provenance records have deterministic ordering.
- `snapshot_id` is the SHA-256 of canonical JSON containing every semantic field
  except `snapshot_id` and `observed_at`. Equal semantic snapshots therefore
  have equal IDs despite observation time.
- `observed_at` is informational. Clients use `generation`, `snapshot_id`, and
  mutation revisions for synchronization, never wall-clock ordering.
- Provenance kinds are `provenance.contract`, `provenance.build`,
  `provenance.operator_policy`, `provenance.runtime_probe`,
  `provenance.backend_probe`, and `provenance.consumer_projection`. Provenance
  identifies the evidence class and stable probe/policy ID, not an implementation
  pathname.
- A failed or unrun probe produces `unknown` or `unavailable` as specified by
  its reason; it never guesses `available` from a compile flag.
- CLI JSON, HTTP `GET /capabilities`, WASM `capabilities()`, initial WebSocket
  hello, and `capabilities.changed` use this schema. WebSocket events include
  sequence, previous/new snapshot IDs, and the shared mutation revision so a
  gap triggers a fresh snapshot.
- Editor clients consume the same effective-contract revision and a selected
  runtime snapshot. Offline mode is explicitly `scope.editor.static` with
  build-derived/unknown runtime states, not fabricated availability.

### Security and privacy bounds

HTTP security mode is one of:

- `security.http.loopback_local`: loopback bind, loopback-only origins, no claim
  of remote-user isolation;
- `security.http.authenticated_remote`: non-loopback allowed with authentication,
  explicit origin allowlist, request/body limits, rate limits, and audit policy;
- `security.http.insecure_remote`: requires an explicit high-friction operator
  acknowledgement and remains visible as degraded with
  `reason.security_policy_disabled`.

A non-loopback bind MUST NOT start in the first mode. Effectful `/eval`, process,
filesystem, and network capabilities are independently scoped; authentication
does not imply they are enabled.

The default capability snapshot MAY expose build revision, target family,
aggregate device counts, sample-rate/buffer constraints, feature IDs, and
security mode. It MUST NOT expose authentication material, environment values,
filesystem roots, executed commands, network credentials, full request origins,
device names, device paths, user names, or project source. Device identities and
operator-policy details require an authenticated privileged-detail scope and
stable opaque IDs. Error text obeys the same redaction policy.

### Compatibility diagnostics

Every compatibility action produces a machine-readable diagnostic:

```json
{
  "diagnostic_id": "diagnostic.compat.midi_channel_index",
  "severity_id": "severity.warning",
  "profile_id": "compat.vibelang.v1",
  "source_unit_id": "unit.midi.channel_index",
  "target_unit_id": "unit.midi.channel",
  "input": 0,
  "effective_value": 1,
  "replacement": "channel: 1",
  "removal_contract_revision": "<declared-revision>"
}
```

The initial stable diagnostic IDs are:

- `diagnostic.compat.midi_channel_index` and
  `diagnostic.compat.midi_group_index`;
- `diagnostic.compat.velocity_raw_in_normalized_field`;
- `diagnostic.compat.value_clamped`;
- `diagnostic.compat.fallback_applied`;
- `diagnostic.compat.token_dropped`;
- `diagnostic.compat.fixed_four_bar`;
- `diagnostic.compat.parser_forgiving`;
- `diagnostic.registry.ambiguous_name`;
- `diagnostic.registry.duplicate_definition`;
- `diagnostic.capability.unavailable`, `.degraded`, and `.unknown`;
- `diagnostic.editor.projection_stale`.

Lossless integer-to-float widening within the same unit is permitted without a
diagnostic. Unit conversion, narrowing, clamping, snapping, fallback, token
dropping, and ambiguous-name selection are never implicit in the canonical
profile. Diagnostics are returned on HTTP/Rhai/WASM results and published to the
LSP/VS Code problem channel; logging alone is insufficient.

## Compatibility and migration plan

### Phase 0: describe and measure

1. Extend the effective contract schema with required `unit_id`, `range`,
   `invalid_value_policy_id`, capability requirements, and compatibility aliases.
2. Classify all 18,786 parameter occurrences as quantity-bearing or
   `not_applicable`; backfill every numeric occurrence, including explicit
   `range.finite.unbounded` where justified. Classify all 5,089 UGen inputs from
   a reviewed source table; do not infer ranges from defaults.
3. Generate baseline reports for clamp/fallback sites, missing metadata,
   collisions, capability gates, and editor coverage. Runtime behavior is
   unchanged in this phase.

### Phase 1: dual-read, canonical-write

1. Add versioned v2 DTOs/entry points. Responses, examples, completions, code
   actions, and generated docs write only canonical field names and units.
2. Preserve HTTP v1 channel numbering on v1 routes. V2 uses 1–16. Do not use a
   0/1 heuristic because values 1–15 are ambiguous.
3. Make `MidiDevice::channel` round-trip 1–16 and expose legacy storage only as
   deprecated `channel_index`; do the same for UMP groups.
4. Split ambiguous `velocity` into `velocity_normalized`, `velocity_7bit`, or
   `velocity_16bit`. Fix the melody editor preview to send normalized values.
5. Introduce strict parsers alongside explicitly named v1-compatible parsers.
   Emit diagnostics for every existing clamp, snap, fallback, or dropped token.
6. Replace fixed-four bar paths with meter-aware resolution. Preserve fixed-four
   only under `compat.vibelang.v1` and report its effective beat duration.
7. Reject new registry collisions. Convert utility LFO duplicates into aliases
   and require qualification for the arpeggio duplicate.
8. Publish capability snapshots before clients start gating UI or operations.
   Initial unknown probe states are preferable to optimistic availability.

### Phase 2: strict-by-default

Version-2 surfaces use `invalid.reject`, strict full-consumption parsing, typed
availability failures, and no success-shaped backend no-ops. Rhai scripts may
select `compat.vibelang.v1` per evaluation during a documented deprecation
window. CLI and editor code actions apply mechanical field/unit conversions and
insert qualification where unambiguous.

### Phase 3: remove legacy ambiguity

After one major-version window and telemetry/report review, remove ambiguous
fields and unnamed forgiving parsers. Retain tombstone IDs and compatibility
diff records so older artifacts receive a precise unsupported-contract error.
Removal of an ID, range narrowing, unit change, available-to-unavailable change,
or new required capability is a breaking contract diff. Adding an optional
capability, reason, or wider range is additive; changing provenance alone is
operational.

## Rejected alternatives and tradeoffs

| Alternative | Rejection and retained tradeoff |
| --- | --- |
| Keep numbers unannotated and document conventions in prose | Editors, HTTP generators, and runtime validators would continue to diverge. Required metadata is larger but mechanically testable. |
| Normalize every MIDI value to 0–1 | It loses exact/native MIDI 1 and MIDI 2 wire intent. The chosen contract keeps raw widths and uses normalized values only for high-level musical APIs. |
| Infer zero- versus one-based channel from the value | Values 1–15 are ambiguous. Versioned fields cost migration work but never silently reroute notes. |
| Clamp everything for live-coding ergonomics | It avoids interruption but hides typos and changes music nondeterministically. Explicit compat/recovery retains performance workflows with visible diagnostics. |
| Make bars always four beats | Simple, but wrong under the existing time-signature model. Contextual bars require revision provenance and are deterministic once resolved. |
| Wrap every scalar in a quantity object | Maximally explicit but too invasive for hot-path Rhai and MIDI calls. Unit-qualified field names plus contract metadata preserve ergonomics; generic maps use tagged quantities. |
| Permit last-import/last-deploy wins | Short but load-order-dependent and unreproducible. Qualified identities and collision errors make deployments stable. |
| Report compile features as capabilities | Cheap but reports ignored/no-op methods as working. The selected evaluator pays probe complexity to provide semantic truth. |
| One global available boolean | Cannot represent quarantine, operator policy, degraded backend semantics, or pending probes. The four-state model plus reasons remains compact. |
| Include device names and policy details in the default snapshot | Convenient for UI discovery but leaks local topology. Privileged detail endpoints can resolve opaque IDs after authorization. |
| Treat the current WebSocket hello as the capability snapshot | It lacks revision, sequence, provenance, backend truth, and resync semantics. Protocol inventory remains a separate field. |

## Interaction with the other four API-unification themes

1. **Shared revision and mutation receipts.** Capability snapshots embed the
   accepted/applied revision owned by the revision-receipt contract and are
   assembled atomically against it. Capability generation is not a competing
   mutation revision. A WebSocket gap or changed snapshot ID uses that contract's
   resync path.
2. **Lifecycle vocabulary.** A value, builder, handle, or reference may require a
   capability, but availability does not redefine its ownership or terminal
   semantics. Builders can be constructible while their terminal operation is
   unavailable; docs and types must show both facts.
3. **Observable effectiveness.** `availability.available` requires the terminal
   effect promised by the API. Ignored fields, log-only stop/cancel, default
   backend no-ops, and success-shaped unsupported writes are degraded or
   unavailable until the effectiveness inventory resolves them.
4. **Effective contract schema and generation.** Unit/range/capability IDs,
   compatibility aliases, and availability requirements are canonical schema
   fields, not hand-maintained editor tables. Deterministic projections and
   compatibility diffing classify unit changes, range tightening, availability
   downgrades, and collision-policy changes.

The synthesis ADR should reference, not duplicate, the detailed ownership of
revisions, lifecycle states, terminal effectiveness, and generator topology.

## Measurable implementation acceptance

The implementation story is complete only when all of these gates pass:

1. **Metadata completeness:** 18,786/18,786 manifest parameter occurrences have
   a reviewed quantity classification, as do 5,089/5,089 UGen inputs. Every
   numeric or signal-bearing occurrence has a valid unit ID, range,
   invalid-value policy, and provenance; non-quantity parameters carry
   `not_applicable`. The missing/unknown report is zero; explicit unbounded
   counts are reviewed rather than inferred.
2. **Strict boundary behavior:** a generated boundary test proves every v2
   numeric input rejects NaN/infinity, wrong integrality, and below/above-range
   values. No strict-profile path records `compat_clamp`, `compat_fallback`, or
   `compat_drop`.
3. **MIDI parity:** Rhai, HTTP v2, WASM, WebSocket events, and the editor agree on
   channels/groups 1 and 16, notes 0 and 127, normalized velocities 0/1, 7-bit
   velocities 0/127, 16-bit velocities 0/65,535, and signed bend limits. Values
   immediately outside every bound fail with the same diagnostic ID.
4. **Meter parity:** bar conversion produces 4 beats in 4/4, 3 in 3/4, and 3 in
   6/8 across pattern, melody, sequence, fade, MIDI/audio recording, transport,
   and MIDI clock. Every result records the meter or meter revision used.
5. **Audio-unit behavior:** hertz, linear amplitude, normalized ratio, and dB are
   distinct in schema and completions; zero/negative hertz and non-positive
   amplitude-to-dB fail; linear gain above one remains valid where declared.
6. **Parser behavior:** the strict note parser rejects `C4x`, `Cx`, `C300`, and
   `CB4`; invalid curve/chord/scale/target values and malformed melody tokens
   return spans and never partial silent success. The v1 compat parser preserves
   each legacy result with exactly one structured diagnostic per recovery.
7. **Collision behavior:** the three LFO duplicates resolve to one canonical
   identity plus aliases; unqualified `arpeggio_up_down` is diagnosed with two
   candidates; same-registry non-identical redeployment fails independent of
   load order; identical hashes are idempotent.
8. **Capability matrix:** golden snapshots cover native with/without MIDI,
   native with each extension policy combination, WASM, missing/present
   `mi-UGens`, quarantined demand rate, recording, and backend semantic no-ops.
   No capability becomes available solely because a compile feature is set.
9. **Snapshot determinism:** repeated unchanged probes yield the same
   `snapshot_id`; each semantic transition increments `generation` once; array
   ordering and canonical JSON are byte-stable; mutation revisions match the
   shared receipt source.
10. **HTTP security:** non-loopback startup without authenticated or explicitly
    acknowledged insecure mode fails. CORS allowlist, authentication, body/rate
    limits, and extension scopes have boundary tests. Default snapshots contain
    none of the forbidden privacy fields.
11. **Editor parity:** generated Rhai projection covers functions, methods, and
    all 275 properties; the eight stale UGen labels are zero; callable-Ugen
    coverage is reported as numerator/denominator rather than silently omitting
    704 identities; semantic-token advertised/emitted legends match; push/pull
    diagnostics share the same contract validators.

## Measurable integration acceptance

| Scenario | Required observation |
| --- | --- |
| Runtime starts, no semantic change | CLI, HTTP, WASM, WebSocket hello, and editor-selected runtime agree on contract revision and semantic snapshot ID |
| MIDI device/plugin appears or disappears | Probe provenance changes state/reason, generation increments once, and a sequenced WebSocket event points from old to new snapshot |
| Client misses WebSocket events | Sequence gap is observable and a full snapshot resynchronizes without relying on timestamps |
| Backend accepts but ignores scheduling/write/read | Requested fine-grained capability is degraded/unavailable and the terminal call cannot return unqualified success |
| Extension enabled locally but disabled for HTTP eval | Two scoped entries differ by policy provenance; no global boolean leaks permission across scopes |
| V1 request uses clamp/fallback/fixed-four behavior | Effective value is preserved, response carries a stable compat diagnostic, and editor/LSP offers a canonical replacement |
| V2 request supplies an ambiguous or invalid value | All projections reject before dispatch with equivalent code, field path, unit, range, and remedy |
| Unauthenticated/default capability request | Aggregate truth is available but device names, paths, roots, origins, credentials, and commands are absent |
| Effective contract changes unit/range/availability | Compatibility diff classifies the change and CI enforces the required version bump/migration record |

## Isolated open decisions

These decisions do not change the chosen unit, range, parser, availability, or
privacy semantics, but synthesis or implementation ownership must close them:

1. Choose the canonical JSON algorithm/profile used for `snapshot_id`, including
   exact integer handling for `u32`, and publish cross-language test vectors.
2. Assign the capability snapshot HTTP route/version and decide whether aggregate
   loopback snapshots are unauthenticated; privileged details always require
   authorization.
3. Select plugin probe mechanism, timeout, cache lifetime, and refresh trigger for
   `mi-UGens`; until a positive probe completes its state is `unknown`, not
   available.
4. Define the runtime-instance persistence boundary for `generation` and whether
   restarts also expose an opaque `instance_id`.
5. Set the exact major-version/date removal window for `compat.vibelang.v1` after
   baseline usage evidence exists.
6. Decide whether identical registry redeployment emits an informational
   diagnostic by default or only under verbose diagnostics; it remains
   idempotent either way.
7. Allocate the exact mutation-revision fields after the sibling receipt contract
   freezes its schema; capability snapshots consume that shape unchanged.
