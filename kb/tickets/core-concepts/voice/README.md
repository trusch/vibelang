---
title: Voice
id: voice
status: open
tags:
- concept
labels:
  area: core
  topic: voice
created: 2026-03-11T08:35:44.884432594+01:00
updated: 2026-03-11T08:35:44.884432594+01:00
---

# Voice

A **voice** is a named instance of a synthdef. It connects a sound definition to volume, polyphony, and parameter settings. Think of it as an instrument you can play.

## Creation

```rhai
let kick = voice("kick")
    .synth("kick_808")
    .gain(db(-6))
    .poly(4);
```

## Methods

- `voice("name")` — create a new voice with a unique name
- `.synth("synthdef_name")` — assign a synthdef (sound source)
- `.gain(db(n))` — set volume in decibels
- `.poly(n)` — set polyphony (1 = monophonic)
- `.set_param("key", value)` — set a synthdef parameter
- `.mute()` / `.unmute()` — mute/unmute
- `.input("name").from(source)` — patch a declared synthdef input from a voice or group

## Relationships

- Assigned to a **pattern** or **melody** via `.on(voice)`
- Uses a **synthdef** as its sound source
- Optionally belongs to a **group** (submix bus)
- Can patch declared named inputs on its synthdef from other voices, groups,
  the current group, or silence
- Can receive **MIDI** input via `midi.keyboard().to(voice)`

## Named Inputs

Named inputs act like patchable input jacks on synthdefs that declare them.
Routes are target-first:

```rhai
filter.input("audio").from(osc);
filter.input("audio").from(group("submix"));
filter.input("audio").from_current_group();
filter.input("audio").disconnect();
```

Leaving a declared input unpatched is valid; the runtime connects a shared
silent bus. Stereo or group sources must match the declared input width, so
stereo-to-mono routing is rejected instead of implicitly downmixed.

## Naming Convention

Voice names are lowercase, descriptive: `"kick"`, `"bass"`, `"lead"`, `"pad"`
