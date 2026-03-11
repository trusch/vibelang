---
title: Pattern API
id: pattern-api
status: open
tags:
- reference
labels:
  area: api
  topic: pattern
created: 2026-03-11T08:36:06.277507179+01:00
updated: 2026-03-11T08:36:06.277507179+01:00
---

# Pattern API

## Functions

- `pattern("name")` → PatternBuilder — create a named pattern

## PatternBuilder Methods

- `.on(voice)` → self — assign voice to trigger
- `.step("notation")` → self — set step notation string
- `.euclid(hits, steps)` → self — euclidean rhythm
- `.start()` — start immediately (loop forever)

## Euclidean Rhythms

```rhai
pattern("clave").on(perc).euclid(5, 16).start();
// Distributes 5 hits evenly across 16 steps
```
