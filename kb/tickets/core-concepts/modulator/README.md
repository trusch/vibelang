---
title: Modulator
id: modulator
status: open
tags:
- concept
labels:
  area: core
  topic: modulator
created: 2026-03-11T08:35:45.373894201+01:00
updated: 2026-03-11T08:35:45.373894201+01:00
---

# Modulator

A **modulator** generates cyclic control signals (LFOs) that continuously change parameters. Unlike fades (one-shot ramp), modulators repeat.

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

## Fade vs Modulator

| | Fade | Modulator |
|---|---|---|
| Shape | One-shot ramp (A→B) | Cyclic (LFO) |
| Use | Build-ups, transitions | Vibrato, wobble, tremolo |
