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

## Example

```rhai
let bass = voice("bass")
    .synth("acid_303_classic")
    .gain(db(-10))
    .poly(1)
    .set_param("cutoff", 800.0);
```
