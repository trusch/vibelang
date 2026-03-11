---
title: Fade
id: fade
status: open
tags:
- concept
labels:
  area: core
  topic: fade
created: 2026-03-11T08:35:45.315130507+01:00
updated: 2026-03-11T08:35:45.315130507+01:00
---

# Fade

A **fade** automates a parameter over time — filter sweeps, volume ramps, effect transitions.

## Creation

```rhai
let filter_sweep = fade("filter_sweep")
    .on_voice("bass")
    .param("cutoff")
    .from(400.0)
    .to(4000.0)
    .over_bars(8)
    .apply();
```

## Targets

- `.on_voice("name")` — automate a voice parameter
- `.on_group("name")` — automate a group parameter
- `.on_effect("name")` — automate an effect parameter

## Methods

- `fade("name")` — create
- `.param("key")` — which parameter to automate
- `.from(value)` — start value
- `.to(value)` — end value
- `.over_bars(n)` — duration in bars
- `.apply()` — activate

## Usage in Sequences

Place fades in sequence clips to synchronize with other elements:

```rhai
sequence("seq")
    .loop_bars(8)
    .clip(0..bars(8), bass_melody)
    .clip(0..bars(8), filter_sweep)  // fade plays alongside melody
    .start();
```
