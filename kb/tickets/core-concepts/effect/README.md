---
title: Effect
id: effect
status: open
tags:
- concept
labels:
  area: core
  topic: effect
created: 2026-03-11T08:35:45.200984238+01:00
updated: 2026-03-11T08:35:45.200984238+01:00
---

# Effect

An **effect** processes audio on a group bus. Created inside a group with `fx()`, using effect synthdefs from the standard library.

## Creation

```rhai
fx("reverb_name")
    .synth("reverb")
    .param("room", 0.5)
    .param("mix", 0.3)
    .apply();
```

## Methods

- `fx("name")` — create with unique name
- `.synth("effect_synthdef")` — assign effect synthdef
- `.param("key", value)` — set parameter
- `.apply()` — apply to group bus

## Common Parameters

- `mix` — wet/dry ratio (0.0=dry, 1.0=fully wet)
- `room` — room size (reverbs)
- `time` — delay time in seconds
- `feedback` — feedback amount (delays)
- `cutoff` — filter cutoff frequency (Hz)
- `drive` — distortion amount
- `rate` — LFO speed (modulation effects)
- `depth` — modulation depth

## Can be Automated

Effects can be automated via **fade** using `.on_effect("name")`.
