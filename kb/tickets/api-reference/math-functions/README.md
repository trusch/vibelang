---
title: Math Functions
id: math-functions
status: open
tags:
- reference
labels:
  area: api
  topic: helpers
created: 2026-03-11T08:36:06.835447833+01:00
updated: 2026-03-11T08:36:06.835447833+01:00
---

# Math Functions

## Random

- `random()` → f64 — random 0.0–1.0
- `random_range(min, max)` → f64/i64 — random in range
- `random_int(max)` → i64 — random integer 0 to max-1
- `random_choice(arr)` → Dynamic — random element from array
- `random_seed(n)` — set seed for reproducibility

## Math

- `clamp(value, min, max)` — constrain value to range
- `lerp(a, b, t)` → f64 — linear interpolation
- `map_range(v, in_min, in_max, out_min, out_max)` → f64 — remap value
- `smoothstep(edge0, edge1, x)` → f64 — smooth interpolation
- `wrap(value, max)` → f64 — wrap around (modulo-like)
- `quantize(value, step)` → f64 — snap to grid
