---
title: Group
id: group
status: open
tags:
- concept
labels:
  area: core
  topic: group
created: 2026-03-11T08:35:45.143708354+01:00
updated: 2026-03-11T08:35:45.143708354+01:00
---

# Group

A **group** bundles voices, patterns, and melodies into a logical unit — like a bus/submix in a DAW. Groups enable shared effects and volume control.

## Creation

```rhai
define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));

    pattern("kick").on(kick).step("x... x... x... x...").start();
    pattern("snare").on(snare).step(".... x... .... x...").start();

    // Apply effects to the entire group bus
    fx("drum_reverb").synth("reverb").param("room", 0.2).param("mix", 0.15).apply();
});
```

## Group Controls

```rhai
let drums = get_group("Drums");
drums.gain(db(-3));      // adjust group volume
drums.mute();            // mute entire group
drums.unmute();
drums.solo(true);        // solo (mute all others)
```

## Audio Routing

- Bus 0: main audio output (JACK/hardware)
- Buses 16+: group buses for submixing
- Link synths route group buses to main output automatically

## Naming Convention

Capitalized: `"Drums"`, `"Bass"`, `"Synth"`, `"Pad"`
