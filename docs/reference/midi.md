# MIDI authoring API

Availability: native builds compiled with the `midi` feature and an appropriate
host MIDI backend. It is not a browser/WASM hardware API. Registration source:
[`midi::register`](../../crates/vibelang-rhai/src/api/midi/mod.rs#L270-L485).

## Discovery and MidiDevice

| Exact signature / property | Return | Semantics |
|---|---|---|
| `list_midi_devices()` | Array of MidiDevice | Merges same-name midir input/output entries and adds PipeWire MIDI 2 inputs |
| `midi_device(query: String)`; `midi_device(index: Int)` | MidiDevice | Numeric string/index searches input index; otherwise case-insensitive input-first, output, then PipeWire substring lookup |
| `id()` / `.id` | Int | Internal device ID; list IDs and raw input indices are not guaranteed interchangeable |
| `name()` / `.name` | String | Resolved name |
| `has_input()` / `.has_input`; `has_output()` / `.has_output` | Bool | Endpoint directions |
| `default_note()` / `.default_note` | Int | -1 when unset |
| `get_channel()` / `.channel` | Int | Internal zero-based 0..15 value |
| `channel(channel: Int)` | MidiDevice | Takes musician-facing 1..16, clamps, returns a new handle |
| `note(note: Dynamic)` | MidiDevice | Int or note String; invalid input warns and falls back to C4 |

A lookup miss does not error: it returns a warning sentinel with ID
`u32::MAX`. Resolve and inspect a device before wiring it.

### Simple and legacy routes

| Exact signature | Return / effect |
|---|---|
| `route_to(voice: Voice)`; `route_to(name: String)` | MidiDevice; stores simple route |
| `route_to_channel(channel: Int, voice: Voice)` | MidiDevice; channel route |
| `route_cc(cc: Int, voice: Voice, param: String, min: Float, max: Float)` | MidiDevice; deprecated |
| `route_cc_to_group(cc: Int, group_path: String, param: String, min: Float, max: Float)` | MidiDevice; deprecated; the String is converted directly to a group ID |
| `open_input()`; `open_output()` | MidiDevice; deprecated, only mark desired state |

Voice also has feature-gated `on(device)`, `channel(Int)`, and
`cc_map(param,cc)`. Voice’s legacy channel method uses internal 0..15, unlike
the 1..16 convention on `MidiDevice.channel` and output methods.

## Direct output

These methods return Unit and queue desired output; they do not synchronously
transmit. Channels use 1..16 and clamp. MIDI 1 note/controller/program values
clamp 0..127, and bend clamps -8192..8191.

| Protocol | Exact methods |
|---|---|
| MIDI 1 | `note_on(channel:Int,note:Int,velocity:Int)`; `note_off(channel,note)`; `cc(channel,cc,value)`; `program_change(channel,program)`; `pitch_bend(channel,value)` |
| MIDI 2 high resolution | `note_on_hires(channel,note,velocity)`; `note_off_hires(channel,note,velocity)`; `cc_hires(channel,cc,value)`; `pitch_bend_hires(channel,value)` |
| MIDI 2 per-note | `send_per_note_bend(channel,note,value)`; `send_per_note_cc(channel,note,controller,value)`; `poly_pressure_hires(channel,note,pressure)` |

High-resolution velocity clamps 0..65535; other high-resolution values clamp
to unsigned 32-bit range. The MIDI 2 group for direct methods is fixed to zero.

## MIDI 1 route builders

All builder terminals append/replace ScriptState and mark the input endpoint;
they do not validate target existence or target parameter names.

### KeyboardRoute

Factory `device.keys()`; deprecated alias `device.keyboard_route()`.

| Exact member | Return / validation |
|---|---|
| `channel(Int)` | Chain; clamps 1..16; omitted means all |
| `range_midi(min:Int,max:Int)` | Chain; note endpoints clamp 0..127 |
| `range(min:String,max:String)` | Chain; permissive note parsing |
| `transpose(semitones:Int)` | Chain; clamps -128..127 |
| `octave(octaves:Int)` | Chain; clamps -10..10 |
| `velocity(curve:String)`; deprecated `velocity_curve` alias | Chain; linear, soft, hard, exponential, compressed delegated to core |
| `fixed_velocity(value:Int)` | Chain; clamps 0..127 |
| `to(voice: Voice)`; `to(name: String)` | KeyboardRoute; commits route |

Defaults: all channels, full note range, transpose 0, linear velocity.

### NoteRoute

Factory `device.pad(note: Dynamic)`; deprecated alias `note_route`. Invalid
note values warn and use C4.

| Exact member | Return / validation |
|---|---|
| `channel(Int)` | Chain; clamps 1..16 |
| `choke(group:String)`; deprecated `choke_group` alias | Chain |
| `velocity_to(param:String,min:Float,max:Float)` | Chain |
| `fixed_velocity(Int)` | Chain; clamps 0..127 |
| `to(voice: Voice)` | NoteRoute; commits route |

### CcMapping, BendMapping, and legacy CcRoute

Factories are `device.map_cc(cc: Int)`, `device.map_bend()`, and deprecated
`device.cc_route(cc: Int)`.

`CcMapping` and `BendMapping` both expose `channel(Int)`, `curve(String)`, and
terminal `to(target: Dynamic, param: String, min: Float, max: Float) -> Unit`.
Target may be Voice, GroupHandle, Fx, or String. A String resolves voice, then
group, then a new voice name. Unsupported Dynamic warns and targets a voice
named `unknown`. Curves recognize linear, `log`/`logarithmic`, and
`exp`/`exponential`; unknown becomes linear. Range/parameter values are not
validated.

Legacy CcRoute exposes `channel(Int)`, `curve(String)`, and
`to_param(voice: Voice or String, param, min, max) -> Unit`; it is voice-only.

### LooperBuilder

Factory `device.looper()`.

| Exact member | Return / behavior |
|---|---|
| `channel(Int)` | Chain; optional channel |
| `silence(bars: Float)` | Chain; default 1, minimum 0.25 |
| `quantize(beats: Float)` | Chain; default 0/off, calling sets minimum 0.0625 |
| `to(voice: Voice)` | Unit; replaces a prior looper with the same key |

## MIDI 2 route builders

| Factory and type | Exact chain members | Terminal |
|---|---|---|
| `device.group(group:Int)` → GroupRoute | `channel`, `range_midi`, `range`, `transpose`, `velocity_curve` | `route_to(Voice or String) -> Unit` |
| `device.per_note_pitch_bend()` → PerNotePitchBendBuilder | `group`, `channel`, `range` | `to(Voice or String,param,min,max) -> Unit` |
| `device.per_note_controller(controller:Int)` → PerNoteControllerBuilder | `group`, `channel`, `curve` | `to(Voice or String,param,min,max) -> Unit` |
| `device.per_note_pressure()` → PerNotePressureBuilder | `group`, `channel`, `curve` | `to(Voice or String,param,min,max) -> Unit` |
| `device.cc32(cc:Int)` → Cc32Route | `group`, `channel`, `curve` | `to(Voice or String,param,min,max) -> Unit` |

Group factories clamp group 0..15; builder channel uses 1..16; note/controller
numbers clamp 0..127. GroupRoute defaults all channels/notes, transpose 0, and
linear velocity; invalid note range names become 0/127 and curve text is not
validated. Per-note pitch range defaults 48 semitones and clamps 1..96. Other
MIDI 2 curve strings are stored without validation.

Source: [`routing.rs`](../../crates/vibelang-rhai/src/api/midi/routing.rs) and
[`midi2.rs`](../../crates/vibelang-rhai/src/api/midi/midi2.rs).

## Callbacks

Each method returns MidiDevice and stores the FnPtr plus current AST for reload.
The CLI polls callback dispatch about every 2 ms; callback errors only warn.

| Exact registration | Callback arguments actually dispatched |
|---|---|
| `on_note(callback)` | `(note, velocity, is_on)` |
| `on_note_channel(channel, callback)` | `(note, velocity, is_on)` for selected channel |
| `on_cc(callback)` | `(cc, value)` |
| `on_cc_num(cc, callback)` | `(cc, value)`, not value-only |
| `on_clock_sync(callback)` | One String event tag for clock/transport |
| `on_midi(callback)` | Event-specific tuple: note, CC, bend, or clock shape; not one universal tag |

Dispatcher source:
[`midi_dispatcher.rs`](../../crates/vibelang-cli/src/midi_dispatcher.rs).

## Clock and transport

`enable_clock()` and `disable_clock()` return a chainable MidiDevice or Rhai
error. `send_start()`, `send_stop()`, and `send_continue()` return Unit or an
error. These operations require a strictly resolved output endpoint and error
on unknown/ambiguous names.

## Recording declarations and result handles

`device.start_recording()` and
`device.start_recording_channel(channel: Int)` return MidiDevice and append a
recording-start request. There is no registered Rhai
stop/retrieve global.

When a host supplies a `MidiRecordingHandle`, it exposes:

| Exact member | Return |
|---|---|
| `note_count()` / `.note_count`; `cc_count()` / `.cc_count` | Int |
| `duration()` / `.duration` | Float beats |
| `notes()` / `.notes` | Array of maps `{beat,note,velocity,duration}` |
| `to_pattern(quantize: Float)` | Step-pattern String |

Nonpositive quantize becomes 0.25. Conversion rounds up to four-beat bars,
rounds hits to grid, uses the maximum stacked velocity as digit 1..9, and
returns `.` when no notes exist.

## Example

```rhai
let keys = midi_device("Launchkey");
let lead = voice("lead").synth("lead_bright");

keys.keys()
    .channel(1)
    .range("C2", "C6")
    .velocity("soft")
    .to(lead);

keys.map_cc(74).channel(1).to(lead, "cutoff", 100.0, 8000.0);
keys.on_note(|note, velocity, is_on| {
    print(`${note}: ${velocity}, on=${is_on}`);
});
```
