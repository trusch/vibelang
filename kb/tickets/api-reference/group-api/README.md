---
title: Group API
id: group-api
status: open
tags:
- reference
labels:
  area: api
  topic: group
created: 2026-03-11T08:36:06.401468223+01:00
updated: 2026-03-11T08:36:06.401468223+01:00
---

# Group API

## Functions

- `define_group("Name", || { ... })` — create a group with a closure
- `get_group("Name")` → Group — retrieve existing group

## Group Methods

- `.gain(db(n))` — set group volume
- `.mute()` — mute entire group
- `.unmute()` — unmute
- `.solo(bool)` — solo this group

## Example

```rhai
define_group("Drums", || {
    // voices, patterns, effects go here
    fx("verb").synth("reverb").param("room", 0.2).apply();
});

get_group("Drums").gain(db(-3));
```
