---
title: Sequence API
id: sequence-api
status: open
tags:
- reference
labels:
  area: api
  topic: sequence
created: 2026-03-11T08:36:06.464981387+01:00
updated: 2026-03-11T08:36:06.464981387+01:00
---

# Sequence API

## Functions

- `sequence("name")` → SequenceBuilder — create a named sequence

## SequenceBuilder Methods

- `.loop_bars(n)` → self — total length in bars (loop point)
- `.clip(range, element)` → self — place pattern/melody/fade in time range
- `.start()` — start the sequence

## Range Syntax

```rhai
0..bars(8)             // bar 0 to 8
bars(8)..bars(16)      // bar 8 to 16
```

## Example

```rhai
sequence("arrangement")
    .loop_bars(32)
    .clip(0..bars(8), intro_hat)
    .clip(bars(8)..bars(16), kick_basic)
    .clip(bars(16)..bars(32), full_drums)
    .start();
```
