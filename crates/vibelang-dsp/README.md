# vibelang-dsp

SynthDef generation and UGen DSL for VibeLang.

## Overview

`vibelang-dsp` provides the synthesis definition layer for VibeLang. It enables creating SuperCollider-compatible SynthDefs using a Rust DSL that's also exposed to Rhai scripts.

Key features:

- **Graph IR** - Intermediate representation for synthesis graphs
- **UGen Library** - 100+ SuperCollider UGens as Rust functions
- **Binary Encoder** - Encodes graphs to SuperCollider's scsyndef format
- **Rhai Integration** - Full API available in `.vibe` scripts

## Architecture

```
┌─────────────────────────────────────────┐
│          define_synthdef()              │
│       (Rhai or Rust closure)            │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│           GraphBuilder                  │
│  (Thread-local active builder pattern)  │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│             GraphIR                     │
│    (UGens, Inputs, Parameters)          │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│         encode_synthdef()               │
│    (Binary scsyndef format)             │
└─────────────────────────────────────────┘
```

## Key Modules

### `graph/` - Synthesis Graph IR

Core types for representing synthesis graphs:

```rust
use vibelang_dsp::graph::{Rate, GraphIR, UGenNode};

// Rates: Scalar < Control < Audio
let rate = Rate::Audio;

// Build graphs with the builder pattern
```

### `rhainodes/` - NodeRef Operations

`NodeRef` wraps graph nodes with operator overloading:

```rust
use vibelang_dsp::NodeRef;

// Arithmetic operations create new graph nodes
let mixed = osc1 + osc2;
let scaled = osc * 0.5;
let detuned = freq * 1.01;
```

### `builder/` - SynthDef Builder

Create synthdefs with closures:

```rust
use vibelang_dsp::SynthDef;

let synthdef = SynthDef::new("my_synth")
    .param("freq", 440.0)
    .param("amp", 0.5)
    .body(|freq, amp| {
        let osc = sin_osc_ar(freq);
        osc * amp
    });
```

### `ugens/` - UGen Library

Auto-generated from `ugen_manifests/*.json`:

```rust
// Oscillators
sin_osc_ar(freq)
saw_ar(freq)
pulse_ar(freq, width)

// Filters
rlpf_ar(input, freq, q)
rhpf_ar(input, freq, q)
moog_ff_ar(input, freq, gain)

// Envelopes
env_gen_ar(env, gate)

// Delays & Effects
delay_l_ar(input, max_delay, delay_time)
comb_l_ar(input, max_delay, delay_time, decay)

// And 100+ more...
```

### `helpers/` - High-Level DSP Functions

```rust
use vibelang_dsp::helpers::*;

// Envelope generation
let env = env_adsr(0.01, 0.1, 0.7, 0.3);
let eg = env_gen(env, gate, 2.0);  // done_action=2

// Signal utilities
let sum = mix(vec![osc1, osc2, osc3]);
let channels = channels(stereo_ugen, 2);

// Audio I/O
let input = in_ar(0);
replace_out_ar(0, output);
```

### `encoder/` - Binary Encoding

```rust
use vibelang_dsp::encode_synthdef;

let graph_ir = synthdef.build()?;
let binary = encode_synthdef("name", &graph_ir)?;
// binary is SuperCollider-compatible scsyndef format
```

## Rhai API

In `.vibe` scripts:

```rhai
define_synthdef("bass")
    .param("freq", 110.0)
    .param("amp", 0.5)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        let osc = saw_ar(freq) + saw_ar(freq * 1.01);
        let filt = rlpf_ar(osc, 800.0, 0.3);

        let env = env_adsr(0.01, 0.1, 0.5, 0.2);
        let env = NewEnvGenBuilder(env, gate)
            .with_done_action(2.0)
            .build();

        filt * env * amp
    });
```

## Build System

UGens are generated at build time from JSON manifests:

```
ugen_manifests/
├── oscillators.json
├── filters.json
├── envelopes.json
├── delays.json
└── ...
```

The `build.rs` script generates `src/ugens/generated.rs` containing all UGen wrapper functions.

## Adding New UGens

1. Add to appropriate manifest in `ugen_manifests/`
2. Rebuild - the UGen will be available in Rust and Rhai

## Dependencies

- **rhai** - Scripting integration
- **byteorder** - Binary encoding

## License

MIT OR Apache-2.0
