---
title: Sequence
id: sequence
status: open
tags:
- concept
labels:
  area: core
  topic: sequence
created: 2026-03-11T08:35:45.257870158+01:00
updated: 2026-03-11T08:35:45.257870158+01:00
---

# Sequence

A **sequence** arranges patterns, melodies, and fades over time — like the arrangement view in a DAW. Enables song structures with intro, drop, breakdown, etc.

## Creation

```rhai
sequence("drums_seq")
    .loop_bars(32)
    .clip(0..bars(8), hihat_basic)
    .clip(bars(8)..bars(16), kick_basic)
    .clip(bars(16)..bars(24), kick_busy)
    .clip(bars(16)..bars(24), snare_basic)
    .start();
```

## Methods

- `sequence("name")` — create with unique name
- `.loop_bars(n)` — set total length in bars (loop point)
- `.clip(range, element)` — place pattern/melody/fade in a time range
- `.start()` — start the sequence

## Time Ranges

Use `bars()` helper with range operator `..`:

```rhai
0..bars(8)            // bar 0 to 8
bars(8)..bars(16)     // bar 8 to 16
```

## Important Rule

- Pattern/melody with `.start()` → plays immediately, loops forever
- Pattern/melody without `.start()` → controlled by sequence
- Never use both on the same pattern/melody
