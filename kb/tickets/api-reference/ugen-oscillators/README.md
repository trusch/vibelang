---
title: UGen Oscillators
id: ugen-oscillators
status: open
tags:
- reference
labels:
  area: api
  topic: ugen
created: 2026-03-11T08:36:06.900141307+01:00
updated: 2026-03-11T08:36:06.900141307+01:00
---

# UGen Oscillators

UGens (Unit Generators) are building blocks for custom synthdefs. Suffix `_ar` = audio rate, `_kr` = control rate.

## Audio Rate Oscillators

- `sin_osc_ar(freq)` — sine wave
- `saw_ar(freq)` — sawtooth wave
- `pulse_ar(freq, width)` — pulse wave (width: 0.0–1.0)
- `lf_tri_ar(freq)` — triangle wave
- `white_noise_ar()` — white noise
- `pink_noise_ar()` — pink noise

## Control Rate Oscillators (LFOs)

- `sin_osc_kr(freq)` — sine LFO (for modulation, not audio)

## Signal Combination

UGens combine with arithmetic operators:

```rhai
let osc = saw_ar(freq) + saw_ar(freq * 1.01);  // layer (detuned)
let scaled = osc * 0.5;                          // scale amplitude
let output = filtered * env * amp;                // chain processing
```
