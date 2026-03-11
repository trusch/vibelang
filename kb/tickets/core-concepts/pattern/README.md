---
title: Pattern
id: pattern
status: open
tags:
- concept
labels:
  area: core
  topic: pattern
created: 2026-03-11T08:35:44.948805582+01:00
updated: 2026-03-11T08:35:44.948805582+01:00
---

# Pattern

A **pattern** is a step sequencer that triggers a voice rhythmically. Patterns define _when_ a sound plays, not _which_ sound.

## Creation

```rhai
pattern("kick_pattern")
    .on(kick_voice)
    .step("x... x... x..x ....")
    .start();
```

## Methods

- `pattern("name")` — create with unique name
- `.on(voice)` — assign a voice to trigger
- `.step("notation")` — define rhythm using step notation
- `.euclid(hits, steps)` — euclidean rhythm instead of step notation
- `.start()` — start immediately (loops forever)

## Step Notation

- `x` — trigger (full velocity)
- `.` — rest (silence)
- `1`-`9` — trigger with velocity level (1=quiet, 9=loud)
- Spaces — optional, for readability

## Pattern Length

Token count determines resolution per bar (in 4/4):
- 4 tokens = quarter notes
- 8 tokens = eighth notes
- 16 tokens = sixteenth notes
- 32 tokens = thirty-second notes

## Without .start()

Patterns without `.start()` are controlled by a **sequence**:

```rhai
let kick_basic = pattern("kick_basic").on(kick).step("x... x... x... x...");
// no .start() — managed by sequence
```
