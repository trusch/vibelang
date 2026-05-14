---
title: Voice API
id: voice-api
status: open
tags:
- reference
labels:
  area: api
  topic: voice
created: 2026-03-11T08:36:06.215141834+01:00
updated: 2026-03-11T08:36:06.215141834+01:00
---

# Voice API

## Functions

- `voice("name")` → VoiceBuilder — create a named voice

## VoiceBuilder Methods

- `.synth("synthdef_name")` → self — assign a synthdef
- `.gain(db(n))` → self — set volume in decibels
- `.poly(n)` → self — set polyphony (default 1)
- `.set_param("key", value)` → self — set synthdef parameter
- `.mute()` → self — mute voice
- `.unmute()` → self — unmute voice
- `.input("name")` → InputHandle — target a declared named input port

## Example

```rhai
let bass = voice("bass")
    .synth("acid_303_classic")
    .gain(db(-10))
    .poly(1)
    .set_param("cutoff", 800.0);
```

## Named Inputs

Named inputs are patchable input jacks declared by a synthdef. A voice
routes a source into one of those jacks with target-first syntax:

```rhai
let osc = voice("osc").synth("oscillator");
let filter = voice("filter").synth("filter_module");

filter.input("audio").from(osc);
filter.input("audio").from(group("submix"));
filter.input("audio").from_current_group();
filter.input("audio").disconnect();
```

Current safe code generation surface:

- `.from(source_voice)` reads the source voice's default output port
  named `"out"`.
- `.from(group("name"))` reads that group's stereo mix bus.
- `.from_current_group()` reads the target voice's explicit group. It
  errors if the target voice has no explicit `.group("...")`.
- `.disconnect()` replaces the route with silence.
- Repeating `.from(...)` on the same `(target voice, input name)` replaces
  the prior cable. It does not add fan-in.
- Routes are materialized atomically: the runtime advances the materialized
  input-route state only after backend link creation succeeds, so a later
  no-change reload can retry a failed link.

Compatibility:

- Synthdefs with no declared `.input(...)` ports keep the legacy effect
  behavior: the runtime wires their legacy `in` parameter to the parent
  group pre-fader bus.
- Synthdefs with no declared `.output(...)` ports keep the legacy implicit
  stereo output port named `"out"`.
- Named-input behavior applies only when the target synthdef declares
  input ports.

Code generation constraints:

- The stable synthdef declaration surface is currently Rust-side
  `SynthDef::input(name, channels)`. Do not generate Rhai synthdef
  `.input(...)` declarations until that authoring surface exists.
- Declared audio/control named inputs may be left unpatched. They receive
  valid shared silent buses at the matching rate, so an unplugged jack is
  not an invalid or missing voice.
- `.input(name).from(source)` is a patch cable. It does not mute or consume
  the source; the source can still route to its own outputs/groups.
- Current input-link synthdefs support audio-rate mono or stereo inputs.
  Routing a group bus or stereo source into a mono named input is rejected
  as a width mismatch. There is no implicit downmix; model downmixing as an
  explicit synth/module before the mono input.

Source anchors:

- Rhai target-first entry point:
  `crates/vibelang-rhai/src/api/voice.rs` (`Voice::input`)
- Input handle terminal verbs:
  `crates/vibelang-rhai/src/api/route.rs` (`InputHandle`)
- Synthdef input declaration:
  `crates/vibelang-dsp/src/builder.rs` (`SynthDef::input`)
- Runtime route materialization:
  `crates/vibelang-core/src/handlers/routes.rs`
  (`RoutesHandler::finalize_input_routes`)
- Runtime stabilization coverage:
  `crates/vibelang-rhai/tests/named_input_routes_runtime.rs`
