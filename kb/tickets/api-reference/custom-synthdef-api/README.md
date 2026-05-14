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
- `.input("name")` → self — declare a mono audio-rate named input
- `.input("name", channels)` → self — declare a mono/stereo audio-rate
  named input (`channels` is `1` or `2`)
- `.body(|params...| { ... })` → registers the synthdef
- `.body_map(|p| { ... })` → registers the synthdef with map-style
  parameter and input access

## Named Inputs

Synthdefs can expose declared named audio inputs for script-side patching with
`voice("target").input("name").from(source)`. Declare those jacks on the
`define_synthdef` builder before `body_map`:

```rhai
define_synthdef("stereo_trim")
    .input("in", 2)       // stereo audio-rate input
    .output("out", 2)     // stereo audio-rate output
    .param("level", 1.0)
    .body_map(|p| {
        [
            p.inputs.in[0] * p.level,
            p.inputs.in[1] * p.level,
        ]
    });
```

Rules:

- Named inputs are **audio-rate only** in the public Rhai surface. There is
  no `.input_kr`, `.input_tr`, or rate argument.
- `.input(name)` declares a mono input. `.input(name, channels)` accepts
  `1` (mono) or `2` (stereo). Wider named-input routing is not public yet.
- Inputs require `.body_map(...)`. A synthdef with declared inputs cannot use
  positional `.body(...)`, because the body needs the `p.inputs` map.
- Read declared inputs as `p.inputs.<name>` inside `body_map`. Mono inputs
  are scalar audio signals; stereo inputs are `[left, right]` arrays.
- Input names must be non-empty and unique within the synthdef input set.
- Unpatched inputs receive silence, except an audio-rate stereo input named
  `in` autofeeds from the parent group when no explicit route overrides it
  (implemented in sibling ticket
  `task-implement-parent-group-in-autofeed-for-declared-in-input`).

Patch inputs from voices with the target-first Voice API documented in
[`voice-api`](../voice-api/README.md).

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

Named-input synthdefs use `body_map`:

```rhai
define_synthdef("mono_source")
    .output("out")
    .param("freq", 220.0)
    .body_map(|p| {
        sin_osc_ar(p.freq)
    });

define_synthdef("lowpass_mono")
    .input("in")
    .output("out")
    .param("cutoff", 1200.0)
    .param("resonance", 0.3)
    .param("level", 1.0)
    .body_map(|p| {
        let filtered = rlpf_ar(p.inputs.in, p.cutoff, p.resonance);
        filtered * p.level
    });

let source = voice("source").synth("mono_source");
let filter = voice("filter")
    .synth("lowpass_mono")
    .set_param("cutoff", 900.0);

filter.input("in").from(source);  // source default output port is "out"
```

## Tips

- Always define `freq`, `amp`, `gate`
- `.cleanup_on_finish()` frees the synth after envelope ends
- Detune: `saw_ar(freq * 1.01)` for width
- Sub octave: `sin_osc_ar(freq * 0.5)` for low end
- Filter envelope: separate envelope without cleanup for time-varying cutoff
- Use `body_map` for named-input processors so params live at `p.<param>` and
  declared inputs live at `p.inputs.<name>`.
