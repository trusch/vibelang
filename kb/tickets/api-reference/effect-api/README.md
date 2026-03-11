---
title: Effect API
id: effect-api
status: open
tags:
- reference
labels:
  area: api
  topic: effect
created: 2026-03-11T08:36:06.585202715+01:00
updated: 2026-03-11T08:36:06.585202715+01:00
---

# Effect API

## Functions

- `fx("name")` → EffectBuilder — create a named effect (inside a group)

## EffectBuilder Methods

- `.synth("effect_synthdef")` → self — assign effect synthdef
- `.param("key", value)` → self — set parameter
- `.apply()` — apply to group bus

## Example

```rhai
fx("delay")
    .synth("ping_pong_delay")
    .param("time", 0.375)
    .param("feedback", 0.4)
    .param("mix", 0.3)
    .apply();
```
