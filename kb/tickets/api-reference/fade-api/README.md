---
title: Fade API
id: fade-api
status: open
tags:
- reference
labels:
  area: api
  topic: fade
created: 2026-03-11T08:36:06.527434686+01:00
updated: 2026-03-11T08:36:06.527434686+01:00
---

# Fade API

## Functions

- `fade("name")` → FadeBuilder — create a named fade

## FadeBuilder Methods

- `.on_voice("name")` → self — target a voice parameter
- `.on_group("name")` → self — target a group parameter
- `.on_effect("name")` → self — target an effect parameter
- `.param("key")` → self — which parameter to automate
- `.from(value)` → self — start value
- `.to(value)` → self — end value
- `.over_bars(n)` → self — duration in bars
- `.apply()` — activate the fade
