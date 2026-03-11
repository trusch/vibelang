---
title: Custom Synthdef API
id: custom-synthdef-api
status: open
tags:
- reference
labels:
  area: api
  topic: synthdef
created: 2026-03-11T08:36:07.077596029+01:00
updated: 2026-03-11T08:36:07.077596029+01:00
---

# Custom Synthdef API

## Functions

- `define_synthdef("name")` → SynthdefBuilder

## SynthdefBuilder Methods

- `.param("name", default_value)` → self — declare parameter
- `.body(|params...| { ... })` → registers the synthdef

## Structure

```rhai
define_synthdef("name")
    .param("freq", 440.0)    // required
    .param("amp", 0.5)       // required
    .param("gate", 1.0)      // required
    .param("cutoff", 1200.0) // optional custom params
    .body(|freq, amp, gate, cutoff| {
        // 1. envelope
        let env = envelope().adsr(...).gate(gate).cleanup_on_finish().build();
        // 2. oscillators
        let osc = saw_ar(freq);
        // 3. filters
        let filtered = rlpf_ar(osc, cutoff, 0.3);
        // 4. output
        filtered * env * amp
    });
```

## Tips

- Always define `freq`, `amp`, `gate`
- `.cleanup_on_finish()` frees the synth after envelope ends
- Detune: `saw_ar(freq * 1.01)` for width
- Sub octave: `sin_osc_ar(freq * 0.5)` for low end
- Filter envelope: separate envelope without cleanup for time-varying cutoff
