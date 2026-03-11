---
title: UGen Filters
id: ugen-filters
status: open
tags:
- reference
labels:
  area: api
  topic: ugen
created: 2026-03-11T08:36:06.957367175+01:00
updated: 2026-03-11T08:36:06.957367175+01:00
---

# UGen Filters

## Filter UGens

- `lpf_ar(input, freq)` — low-pass filter
- `hpf_ar(input, freq)` — high-pass filter
- `rlpf_ar(input, freq, res)` — resonant low-pass (res: 0.0–1.0)
- `rhpf_ar(input, freq, res)` — resonant high-pass (res: 0.0–1.0)

## Parameters

- `input` — audio signal to filter
- `freq` — cutoff frequency in Hz
- `res` — resonance (0.0 = no resonance, approaching 1.0 = self-oscillation)

## Example

```rhai
let osc = saw_ar(freq);
let filtered = rlpf_ar(osc, 800.0, 0.3);  // resonant low-pass at 800 Hz
```
