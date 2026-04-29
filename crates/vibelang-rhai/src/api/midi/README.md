# MIDI API (Rhai / `.vibe` scripts)

This folder implements the **user-facing MIDI API** for VibeLang Rhai scripts (`vibelang-rhai`, `midi` feature). Scripts do not talk to hardware directly: they build a [`ScriptState`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/struct.ScriptState.html) (routes, device IDs, pending output messages, etc.) that the native runtime applies on reload and each tick.

## Feature availability

| Build | Notes |
|--------|--------|
| **Native** (default) | `midi` is on by default. Device listing uses [`midir`](https://crates.io/crates/midir). |
| **`midi` disabled** | No MIDI symbols are registered; `api::midi::register` is not called. |
| **WASM** | `midir` is not linked; `list_midi_devices` / `midi_device` behavior is limited or unavailable depending on the host. |

Rust entry point: `register()` in [`mod.rs`](./mod.rs) wires all functions and custom types into the Rhai [`Engine`](https://docs.rs/rhai/latest/rhai/struct.Engine.html).

---

## Mental model

1. **Resolve a device** — `midi_device(...)` or `list_midi_devices()` yields a [`MidiDevice`](./device.rs) (name, capabilities, internal ID).
2. **Declare intent** — Routing methods and builders append entries to `ScriptState` (`midi_keyboard_routes`, `advanced_*_routes`, `midi2_*`, `loopers`, …). **Inputs and outputs are opened automatically** when you use routing, `note_on`, `enable_clock`, etc. Deprecated `open_input` / `open_output` only mark IDs in state; you can remove them from scripts.
3. **Immediate output** — `note_on`, `cc`, `send_start`, … append [`MidiOutputMessage`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/enum.MidiOutputMessage.html) values to `midi_output_messages` for the next runtime flush.
4. **Voices** — `voice("x").on(midi_device)` (see [`voice.rs`](../voice.rs)) sends note/CC traffic to an external device instead of a synth/sample, using the channel stored on the `MidiDevice`.

---

## Global functions

| Function | Signature (conceptual) | Description |
|----------|-------------------------|-------------|
| `list_midi_devices()` | `() -> Array` | Enumerates ports. Merges input and output names where both exist. Each element is a `MidiDevice` dynamic. |
| `midi_device(name_or_idx)` | `string -> MidiDevice` | If `name_or_idx` parses as a **non‑negative integer**, treats it as an **input port index**. Otherwise **case‑insensitive substring** match on input port names, then output port names. |
| `midi_device(id)` | `i64 -> MidiDevice` | Same as `midi_device(id.to_string())` (index path). |

### Sentinel / missing device

If no port matches, `midi_device` returns a placeholder with `id == u32::MAX`, `has_input == false`, `has_output == false`, and name `Unknown: …`. A warning is logged. Most operations become no-ops or skip safely; avoid relying on silent success.

### Device list vs `midi_device` indices

`list_midi_devices` assigns sequential IDs when merging outputs that were not seen as inputs. **`midi_device(n)` always uses the raw midir input port index `n`**, not necessarily the `id` field from a prior `list_midi_devices` entry. Prefer **name-based** `midi_device("Keystep")` for stable scripts across machines.

---

## Type: `MidiDevice`

Defined in [`device.rs`](./device.rs). Exposed to Rhai as a custom type with getters and methods.

### Fields / getters

| Property / call | Meaning |
|-----------------|--------|
| `id` | Internal `MidiDeviceId` as integer (for debugging; routing uses this under the hood). |
| `name` | Port name string. |
| `has_input`, `has_output` | Capabilities from enumeration. |
| `default_note` | Getter returns MIDI note 0–127, or **`-1`** if unset. |
| `channel` | **Getter returns internal 0–15** (default `0`). **Not** the same convention as the `channel(n)` *setter* (see below). |

### Channel and default note (fluent setters)

Both return a **new** `MidiDevice` with updated fields (Rhai chaining).

| Method | Description |
|--------|-------------|
| `channel(ch)` | **User convention: `ch` is 1–16** (musician MIDI channels). Stored internally as 0–15. |
| `note(x)` | Default note for pattern-driven MIDI output: `x` may be an int 0–127 or a string like `"C#4"`. Invalid names fall back to C4 with a warning. |

**Important:** After `midi_device("X").channel(2)`, property read `dev.channel` is **`1`** (internal index for “MIDI channel 2”). Prefer remembering you set “channel 2” in the 1–16 sense rather than reading `.channel` unless you want the internal index.

---

## Simple keyboard routing (MIDI 1)

| Method | Description |
|--------|-------------|
| `route_to(voice)` | All channels from this device → triggers the given [`Voice`](../voice.rs). |
| `route_to(voice_name)` | Overload: `voice_name` is a `string`. |
| `route_to_channel(ch, voice)` | **`ch` in 1–16**; only that channel is routed. |

These append a [`MidiKeyboardRoute`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/struct.MidiKeyboardRoute.html) and register the device for input.

### Deprecated CC helpers

| Method | Replacement |
|--------|-------------|
| `route_cc(cc, voice, param, min, max)` | `map_cc(cc).to(voice, param, min, max)` |
| `route_cc_to_group(cc, group_path, param, min, max)` | `map_cc(cc).to(group_handle_or_name, param, min, max)` via [`CcMapping`](./cc_mapping.rs) |

Legacy routes still write [`MidiCcRoute`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/struct.MidiCcRoute.html) (no per-route curve).

---

## Advanced routing builders

### `keys()` → `KeyboardRoute`

[`routing.rs`](./routing.rs) — full keyboard zone → voice.

| Method | Description |
|--------|-------------|
| `channel(ch)` | 1–16. |
| `range_midi(min, max)` | Note numbers 0–127. |
| `range(min, max)` | Note names, e.g. `"C2"`, `"C6"`. |
| `transpose(semitones)`, `octave(octaves)` | Pitch shift before triggering. |
| `velocity(name)` | Curve: `"linear"`, `"soft"`, `"hard"`, `"exponential"`, `"compressed"`. |
| `fixed_velocity(v)` | 0–127; constant velocity. |
| `to(voice)` / `to(voice_name)` | Commits an [`AdvancedMidiKeyboardRoute`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/struct.AdvancedMidiKeyboardRoute.html). |

Deprecated alias: `keyboard_route()` → use `keys()`.

### `pad(note)` → `NoteRoute`

Single pad/drum note → voice. `note` accepts int or note name string.

| Method | Description |
|--------|-------------|
| `channel(ch)` | 1–16. |
| `choke(group)` | Choke group name (open/closed hat, etc.). |
| `velocity_to(param, min, max)` | Map hit velocity to a synth parameter. |
| `to(voice)` | Commits [`AdvancedMidiNoteRoute`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/struct.AdvancedMidiNoteRoute.html). |

Deprecated: `note_route(note)` → `pad(note)`.

### `map_cc(cc)` → `CcMapping`

[`cc_mapping.rs`](./cc_mapping.rs) — polymorphic CC destination.

| Method | Description |
|--------|-------------|
| `channel(ch)` | Optional 1–16 filter. |
| `curve(name)` | `"linear"`, `"log"` / `"logarithmic"`, `"exp"` / `"exponential"`. |
| `to(target, param, min, max)` | `target` may be a `Voice`, [`GroupHandle`](../../group.rs), [`Fx`](../../sequence.rs), or **string** (resolved as voice name, then group name, else voice created). |

Writes [`AdvancedMidiCcRoute`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/struct.AdvancedMidiCcRoute.html).

### `cc_route(cc)` → `CcRoute` (deprecated)

Same end state as advanced CC route but **voice-only** (`to_param` / `to_param_name`). Prefer `map_cc(cc)`.

---

## Looper

[`looper_builder.rs`](./looper_builder.rs)

```rhai
midi_device("Keystep")
    .looper()
    .channel(1)           // optional, 1-16
    .silence(2.0)        // bars of silence before playback, default 1.0
    .quantize(0.25)      // beat grid, default 0.25 (16ths)
    .to(piano_voice);
```

Pushes [`LooperConfig`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/struct.LooperConfig.html); replaces any previous looper on the same device ID.

---

## Direct MIDI output (`MidiDevice` methods)

All use **channel 1–16** in the script. Values are clamped to valid MIDI ranges. Each call also ensures the device is marked for **output** in `ScriptState`.

### MIDI 1.0

| Method | Arguments |
|--------|-----------|
| `note_on(channel, note, velocity)` | |
| `note_off(channel, note)` | |
| `cc(channel, cc, value)` | |
| `program_change(channel, program)` | |
| `pitch_bend(channel, value)` | `value`: **−8192 … +8191**, 0 = center. |

### MIDI 2.0 / high resolution (UMP-style payloads)

Encoded for output according to runtime/backend. MIDI 1 gear may receive downscaled traffic.

| Method | Notes |
|--------|--------|
| `note_on_hires`, `note_off_hires` | Velocity **0–65535**. |
| `cc_hires`, `pitch_bend_hires`, `send_per_note_bend`, `send_per_note_cc`, `poly_pressure_hires` | **32‑bit** values clamped to `0 … 0xFFFFFFFF` where applicable. |
| | Group is fixed **0** on these output helpers in the current API. |

---

## MIDI clock and transport

| Method | Effect |
|--------|--------|
| `enable_clock()` | Request 24 PPQN (and related) clock to this output device. No-op with warning if the handle is the “unknown” sentinel. |
| `disable_clock()` | Turn clock off for this device. |
| `send_start()`, `send_stop()`, `send_continue()` | System realtime; queued for the next reconciliation so they align with transport. |

---

## MIDI recording (script side)

| Method | Description |
|--------|-------------|
| `start_recording()` | Requests recording from this device (all channels). Opens input. |
| `start_recording_channel(ch)` | **`ch` 1–16**; channel filter. |

These append [`MidiRecordingRequest`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/struct.MidiRecordingRequest.html) with `start: true`. Stopping and retrieving takes is handled by the **runtime** (e.g. `Midi` trait `stop_recording` in `vibelang-core`), not by additional Rhai helpers in this module.

### Type: `MidiRecordingHandle`

[`recording.rs`](./recording.rs) — registered in Rhai for **property-style access** to captured data when the host supplies a handle:

| Member / method | Meaning |
|-----------------|--------|
| `note_count`, `cc_count`, `duration` | Counters / length in **beats**. |
| `notes` | Array of maps: `beat`, `note`, `velocity`, `duration`. |
| `to_pattern_string(quantize)` | Builds a step pattern string (`'.'`, `'1'…'9'`) from recorded hits; `quantize` is grid in beats (e.g. `0.25`). |

---

## Event callbacks (`FnPtr`)

Register Rhai functions as MIDI listeners. Each registration stores a [`MidiCallbackConfig`](https://docs.rs/vibelang-core/latest/vibelang_core/reload/struct.MidiCallbackConfig.html) on `ScriptState` **and** stashes the `FnPtr` in the per-run script context (`register_midi_callback` in [`context.rs`](../../context.rs)).

| Method | Filter |
|--------|--------|
| `on_note(fn)` | Note on/off, all channels. |
| `on_note_channel(ch, fn)` | **ch: 1–16**. |
| `on_cc(fn)` | All control changes. |
| `on_cc_num(cc, fn)` | Single CC number 0–127. |
| `on_clock_sync(fn)` | Clock / start / stop / continue. |
| `on_midi(fn)` | Broad “all data” class on the core side (`CallbackType::AllData`). |

**Integration note:** Delivering these callbacks into Rhai requires the embedding app to connect runtime [`MidiEventNotification`](https://docs.rs/vibelang-core/latest/vibelang_core/struct.MidiEventNotification.html) (or equivalent) with the stored `FnPtr` map (`get_midi_callback` / `take_midi_callbacks` in `context`). The script API records **what** to subscribe to; **when** your function runs depends on host wiring.

---

## MIDI 2.0 routing (builders)

Implemented in [`midi2.rs`](./midi2.rs). All routes insert the device into `midi_inputs`.

### `group(g)` → `GroupRoute`

UMP **group 0–15** (passed as integer). Methods: `channel`, `range_midi`, `range`, `transpose`, `velocity_curve`, `route_to` / `route_to_name`.

### Per-note controllers

| Entry | Builder methods | `to` / `to_name` |
|-------|-----------------|------------------|
| `per_note_pitch_bend()` | `group`, `channel`, `range` (semitones, default bend range 48) | `(voice, param, min, max)` |
| `per_note_controller(cc)` | `group`, `channel`, `curve` | same |
| `per_note_pressure()` | `group`, `channel`, `curve` | same |

### `cc32(cc)` → `Cc32Route`

High-resolution CC routing: `group`, `channel`, `curve`, `to` / `to_name`.

---

## Voices and MIDI output

From [`voice.rs`](../voice.rs) (also `midi` feature):

| API | Role |
|-----|------|
| `voice("name").on(midi_device)` | Use external MIDI instead of a synth/sample. Uses `midi_device.id` and **`midi_device.channel` (internal 0–15)**. |
| `voice(...).channel(n)` (**deprecated for MIDI**) | Sets internal channel **0–15** directly; prefer `midi_device("…").channel(1–16)` then `.on(device)`. |
| `cc_map(param, cc)` | When the parameter changes, emit **CC** on that voice’s MIDI output. |

If the device has `default_note` set, pattern triggers that send pitch through this voice can use that note (see doc comment on `MidiDevice::note` in `device.rs`).

---

## Source layout

| File | Responsibility |
|------|----------------|
| [`mod.rs`](./mod.rs) | `list_midi_devices`, `midi_device`, `register`, MIDI 2 registration. |
| [`device.rs`](./device.rs) | `MidiDevice` struct and all device methods. |
| [`routing.rs`](./routing.rs) | `KeyboardRoute`, `NoteRoute`, `CcRoute`. |
| [`cc_mapping.rs`](./cc_mapping.rs) | `CcMapping` polymorphic `.to(...)`. |
| [`looper_builder.rs`](./looper_builder.rs) | `LooperBuilder`. |
| [`recording.rs`](./recording.rs) | `MidiRecordingHandle`. |
| [`midi2.rs`](./midi2.rs) | MIDI 2 route builders. |

---

## Quick reference cheat sheet

```rhai
// Discovery
let all = list_midi_devices();
let kbd = midi_device("KeyStep");   // substring, case-insensitive
let first_in = midi_device(0);    // input port index

// Simple play
let v = voice("synth").synth("pad").apply();
kbd.route_to(v);

// Zone + curves
kbd.keys()
    .channel(1)
    .range("C3", "C7")
    .transpose(-12)
    .velocity("soft")
    .to(v);

// Drum pad
kbd.pad("A2").channel(10).choke("hat").to(voice("hat").on(sample("hat")).apply());

// CC to cutoff (log taper)
kbd.map_cc(74).curve("logarithmic").to(v, "cutoff", 200.0, 12000.0);

// External hardware as output (melodies/patterns on this voice send MIDI)
let out = midi_device("Hydrasynth").channel(1);
let bass = voice("midi_bass").on(out).gain(db(0)).apply();
melody("bass_line").on(bass).notes("C2 E2 G2").apply();

// One-shot output
out.note_on(1, 36, 100);
out.note_off(1, 36);

// Clock slave
out.enable_clock();
```

---

## See also

- `vibelang-core` MIDI stack: `crate::midi`, `MidiHandler`, reload types in `vibelang_core::reload`.
- Rhai engine: `ScriptEngine::run` / `execute_file` in [`engine.rs`](../../engine.rs) returns `ScriptState` for the runtime to apply.
