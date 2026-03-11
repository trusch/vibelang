---
title: Synthdef
id: synthdef
status: open
tags:
- concept
labels:
  area: core
  topic: synthdef
created: 2026-03-11T08:35:45.069880172+01:00
updated: 2026-03-11T08:35:45.069880172+01:00
---

# Synthdef

A **synthdef** (synthesizer definition) describes _how_ a sound is generated. Synthdefs are reusable blueprints that voices instantiate.

## Sources

1. **Standard library** — imported from `stdlib/`
2. **Custom synthdef** — defined with `define_synthdef()`
3. **SFZ instruments** — sample-based via `.sfz` files

## Importing

```rhai
import "stdlib/drums/kicks/kick_808.vibe";
// Registers synthdef "kick_808" in the global registry
```

## Custom Definition

```rhai
define_synthdef("my_bass")
    .param("freq", 110.0)
    .param("amp", 0.5)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        let env = envelope().adsr("10ms", "100ms", 0.7, "200ms")
            .gate(gate).cleanup_on_finish().build();
        let osc = saw_ar(freq) + saw_ar(freq * 1.01);
        rlpf_ar(osc, 800.0, 0.3) * env * amp
    });
```

## Required Parameters

Every synthdef should define: `freq` (Hz), `amp` (0.0–1.0), `gate` (1.0=on, 0.0=off).

## Naming Convention

snake_case: `"kick_808"`, `"acid_303_classic"`, `"my_bass"`
