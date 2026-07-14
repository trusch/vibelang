# `.vibe` authoring reference

The VibeLang authoring surface is the union of functions registered by
`vibelang-rhai`, DSP registrations from `vibelang-dsp`, and functions provided
by explicitly imported standard-library modules. Rust `pub` visibility alone
does not make an item callable from `.vibe`.

Registration roots:

- [`ScriptEngine::new`](../../crates/vibelang-rhai/src/engine.rs#L164-L191)
- [`api::register_api`](../../crates/vibelang-rhai/src/api/mod.rs#L38-L82)
- [`register_dsp_api`](../../crates/vibelang-dsp/src/lib.rs#L67-L72)

## Find an API

| Area | Reference |
|---|---|
| Tempo, time signature, notes, chords, scales, arrays, random, math, assertions | [Globals](globals.md) |
| Group, Voice, routing, Pattern, Melody, Sequence, Fade, Fx, Sample, Buffer, SFZ, Record | [Runtime objects](runtime-objects.md) |
| `NodeRef`, envelopes, `define_synthdef`, `define_fx`, DSP helpers | [DSP](dsp.md) |
| Every generated rate-suffixed UGen function | [UGen index](generated/ugens.md) |
| Every shipped synthdef/effect and intended public imported function | [Standard-library index](generated/stdlib.md) |
| MIDI devices, mappings, routes, output, callbacks, clock, looper | [MIDI](midi.md) |
| Optional fs, exec, environment, HTTP, URL, and JSON functions | [Extensions](extensions.md) |

## Lifecycle is part of every signature

Fluent syntax does not imply one common state model. Read the lifecycle column
before assuming a call has taken effect.

| Mark | Meaning |
|---|---|
| Builder | Changes only the returned builder until a terminal operation |
| Snapshot | Writes the current script declaration for later reconciliation |
| Runtime command | Queues a live operation; completion is asynchronous |
| Handle mutation | Updates an already inserted snapshot entry immediately |
| No-op / stub | Accepted syntax that currently has no represented effect |

The full per-type matrix is in [Execution and state lifecycle](runtime-model.md).
The important correction to older project prose is that there is no universal
“every builder method syncs” rule. Pattern, Melody, Sequence, Fade, Fx, and
Record builders defer; `Fade.apply()` is a no-op; some accepted fields are not
copied into state.

## Rhai language boundary

VibeLang creates a standard `rhai::Engine`, so normal Rhai literals, control
flow, arrays, maps, functions, strings, and default packages come from Rhai
1.23.6. Use the [Rhai book](https://rhai.rs/book/) for the base language. The
VibeLang-specific limits are 4096 expression depth and 4096 call depth, plus the
music and DSP APIs documented here. Imports add VibeLang’s extracted stdlib and
configured include paths.

## Minimal example

```rhai
set_tempo(124);

import "stdlib/drums/kicks/kick_808.vibe";

let drums = define_group("drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    pattern("four").on(kick).step("x... x... x... x...").start();
});

drums.gain(0.9);
```

`voice("kick")` synchronizes once its source is complete; the Pattern remains a
builder until `start()`, which stores it and marks it playing; `drums.gain()`
updates the existing group snapshot immediately.
