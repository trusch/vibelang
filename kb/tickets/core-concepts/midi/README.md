---
title: MIDI
id: midi
status: open
tags:
- concept
labels:
  area: core
  topic: midi
created: 2026-03-11T08:35:45.557258289+01:00
updated: 2026-03-11T08:35:45.557258289+01:00
---

# MIDI

VibeLang supports **MIDI input** and **MIDI output** (experimental).

## MIDI Input

```rhai
let midi = midi_open("vibe");
midi_monitor(true);                   // print MIDI messages to console
midi.keyboard().to(voice_object);     // route keyboard to voice
```

## Status

- MIDI input: experimental
- MIDI output: planned/in development

## Feature Flag

MIDI requires a Cargo feature flag:

```bash
cargo install vibelang-cli --features midi
```
