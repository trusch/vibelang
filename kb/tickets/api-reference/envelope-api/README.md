---
title: Envelope API
id: envelope-api
status: open
tags:
- reference
labels:
  area: api
  topic: envelope
created: 2026-03-11T08:36:07.012652471+01:00
updated: 2026-03-11T08:36:07.012652471+01:00
---

# Envelope API

Envelopes shape amplitude (or other parameters) over time.

## Envelope Types

```rhai
// ADSR — Attack, Decay, Sustain level, Release
envelope().adsr("10ms", "100ms", 0.7, "200ms")

// ASR — Attack, Sustain level, Release
envelope().asr("15ms", 0.7, "100ms")

// Percussive — Attack, Release (no sustain)
envelope().perc("1ms", "50ms")

// Simple attack + release
envelope().attack("5ms").release("200ms")
```

## Required Chain

Always end with `.gate(gate).cleanup_on_finish().build()`:

```rhai
let env = envelope()
    .adsr("10ms", "100ms", 0.7, "200ms")
    .gate(gate)
    .cleanup_on_finish()   // free synth when envelope finishes
    .build();
```

## Time Specs

Strings: `"10ms"`, `"500ms"`, `"1s"`

## Filter Envelope

Use a separate envelope (without cleanup) to modulate cutoff:

```rhai
let filter_env = envelope().perc("1ms", "150ms").gate(gate).build();
let cutoff = 300.0 + (filter_env * 4000.0);
```
