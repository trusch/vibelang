---
title: CV Source
id: cv-source
status: open
tags:
- concept
labels:
  area: core
  topic: cv-source
created: 2026-03-11T08:35:45.373894201+01:00
updated: 2026-03-11T08:35:45.373894201+01:00
---

# CV Source

A **CV source** is a regular voice whose synthdef declares a control-rate
output port with `.output_kr(...)`. Route that kr output into another voice's
parameter with `.modulate_by(...)` or `.to_param(...)` for continuously changing
values. Unlike fades (one-shot ramps), LFO-style CV sources repeat.

## Typical Uses

- **Vibrato** — LFO on voice frequency
- **Tremolo** — LFO on voice amplitude
- **Filter wobble** — LFO on cutoff
- **Auto-pan** — LFO on stereo position

## In Custom Synthdefs

LFOs are built directly as UGens:

```rhai
let lfo = sin_osc_kr(2.0);                  // 2 Hz LFO
let cutoff = 800.0 + (lfo * 600.0);          // cutoff: 200–1400 Hz
let filtered = rlpf_ar(osc, cutoff, 0.3);
```

## Fade vs CV Source

| | Fade | CV Source |
|---|---|---|
| Shape | One-shot ramp (A→B) | Cyclic (LFO) |
| Use | Build-ups, transitions | Vibrato, wobble, tremolo |
