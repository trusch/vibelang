---
title: Global Functions
id: global-functions
status: open
tags:
- reference
labels:
  area: api
  topic: global
created: 2026-03-11T08:36:06.648749737+01:00
updated: 2026-03-11T08:36:06.648749737+01:00
---

# Global Functions

## Tempo & Time

- `set_tempo(bpm)` — set tempo in beats per minute
- `get_tempo()` → f64 — get current tempo
- `set_time_signature(num, denom)` — set time signature (e.g. 4, 4)
- `set_quantization(beats)` — set quantization grid
- `get_quantization()` → f64 — get current quantization
- `get_current_bar()` → i64 — get current bar number

## Example

```rhai
set_tempo(120);
set_time_signature(4, 4);
```
