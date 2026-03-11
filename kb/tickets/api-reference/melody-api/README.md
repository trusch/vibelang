---
title: Melody API
id: melody-api
status: open
tags:
- reference
labels:
  area: api
  topic: melody
created: 2026-03-11T08:36:06.337998632+01:00
updated: 2026-03-11T08:36:06.337998632+01:00
---

# Melody API

## Functions

- `melody("name")` → MelodyBuilder — create a named melody

## MelodyBuilder Methods

- `.on(voice)` → self — assign voice
- `.notes("notation")` → self — set note notation string
- `.start()` — start immediately (loop)
