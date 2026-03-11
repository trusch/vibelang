---
title: Helper Functions
id: helper-functions
status: open
tags:
- reference
labels:
  area: api
  topic: helpers
created: 2026-03-11T08:36:06.712976799+01:00
updated: 2026-03-11T08:36:06.712976799+01:00
---

# Helper Functions

## Audio Helpers

- `db(n)` → f64 — convert decibels to linear gain. `db(-6)` → 0.5
- `note("C4")` → i64 — note name to MIDI number. `note("C4")` → 60
- `chord("Cm")` → Array — chord as MIDI number array
- `chord("Cm", 3)` → Array — chord with specific octave
- `scale("C", "minor")` → Array — scale as MIDI array
- `scale("C", "dorian", 4)` → Array — scale with octave
- `scale_degree("C", "minor", 3)` → i64 — single scale degree
- `bars(n)` → f64 — convert bars to beats. `bars(4)` → 16.0 in 4/4
- `midi_to_freq(note)` → f64 — MIDI to Hz. `midi_to_freq(69)` → 440.0
- `freq_to_midi(freq)` → f64 — Hz to MIDI number

## Conversion

- `to_int(float)` → i64
- `to_float(int)` → f64
- `to_string(value)` → String
- `timestamp()` → f64 — current time in seconds
- `timestamp_ms()` → i64 — current time in milliseconds
