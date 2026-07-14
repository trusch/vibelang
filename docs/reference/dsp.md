# DSP graph, envelopes, synthdefs, and effects

DSP code runs while a synthdef/effect body is being built. It creates an
in-memory graph; it does not process audio during script evaluation. The
registration order is NodeRef operations, handwritten helpers, generated
UGens, then synthdef/effect builders. Source:
[`register_dsp_api`](../../crates/vibelang-dsp/src/lib.rs#L67-L72).

## NodeRef

`NodeRef` is opaque in `.vibe`; its Rust constructors and output-index accessors
are not registered. Every operation below builds graph nodes and returns a
NodeRef.

| Exact forms | Notes |
|---|---|
| `NodeRef` with `+`, `-`, `*`, or `/` and `NodeRef` | Binary graph operation |
| `NodeRef` with `+`, `-`, `*`, or `/` and `Float` | Constant right operand |
| `Float` with `+`, `-`, `*`, or `/` and `NodeRef` | Constant left operand |
| `tanh(x)`, `abs(x)`, `sign(x)`, `squared(x)`, `cubed(x)`, `sqrt(x)`, `exp(x)`, `ln(x)`, `distort(x)`, `softclip(x)`, `floor(x)`, `ceil(x)`, `round(x)` | One NodeRef argument |
| `x.clip(lo: Float, hi: Float)`; `wrap`; `fold` | Numeric bounds |
| `min(x: NodeRef, y: NodeRef or Float)`; `max(...)` | Exact NodeRef/NodeRef and NodeRef/Float overloads |
| `pow(NodeRef, Float)`; `pow(NodeRef, NodeRef)`; `pow(Float, NodeRef)` | Power overloads |
| `modulo(NodeRef, Float)`; `modulo(NodeRef, NodeRef)` | Modulo overloads |
| `round_to(NodeRef, step: Float)` | Quantized graph signal |
| `lerp(a: NodeRef, b: NodeRef, t: NodeRef)` | Graph interpolation |

Source: [`register_node_ref_api`](../../crates/vibelang-dsp/src/rhainodes.rs#L407-L506).
Many registration wrappers end in `unwrap()`: an invalid Dynamic conversion or
graph builder error can panic rather than become a recoverable Rhai error.

## Handwritten DSP helpers

### Envelopes and EnvGen

| Exact signature | Return / defaults |
|---|---|
| `Env(levels: Array, times: Array, curve: Float)` | Env; level values that cannot convert become 0; times accept numeric/NodeRef, but empty or mismatched arrays and negative duration are not validated up front |
| `env_perc()`; `env_perc(attack: Float)`; `env_perc(attack,release: Float)` | Env; defaults 0.01/1 |
| `env_adsr(attack,decay,sustain,release: Float)` | Env |
| `env_asr(attack,sustain,release: Float)` | Env |
| `env_triangle(duration: Float)` | Env |
| `env_gen(gate: NodeRef, done_action: Int)` | NodeRef |
| `env_gen(env: Env, gate: NodeRef, level_scale, level_bias, time_scale, done_action: Float)` | NodeRef, all four controls Float |
| same six-argument form with the final four controls all NodeRef | NodeRef; mixed Float/NodeRef forms are not registered |

`NewEnvGenBuilder(env: Env, gate: NodeRef)` is the exact legacy factory name.
It defaults level scale 1, bias 0, time scale 1, and done action 0. Chainable
members `with_level_scale`, `with_level_bias`, `with_time_scale`, and
`with_done_action` each have a Float and a NodeRef overload; `build()` returns
NodeRef or a Rhai error.

The preferred builder is `envelope()`:

| Member | Exact overloads and behavior |
|---|---|
| `attack(value)`; `decay(value)`; `release(value)` | Float overload plus Dynamic accepting numeric seconds, a humantime String, or NodeRef |
| `sustain(level: Float)` | Clamps 0..1 |
| `gate(value: NodeRef)` | Default is audio-rate DC 1 |
| `cleanup_on_finish()` | Selects free-self done action |
| `done_action`, `level_scale`, `level_bias`, `time_scale` | Each accepts Float or NodeRef |
| `perc(attack,release)` | Both Dynamic or both Float |
| `asr(attack,sustain,release)` | Dynamic/Float/Dynamic or all Float |
| `adsr(attack,decay,sustain,release)` | Dynamic/Dynamic/Float/Dynamic or all Float |
| `triangle(duration)` | Dynamic or Float; a NodeRef duration is rejected in the current implementation |
| `build()` | NodeRef or Rhai error |

Defaults are attack 0.01, no decay, no sustain, release 0.1, scale 1, bias 0,
time scale 1, and done action 0. The selected envelope kind follows whether
decay/sustain are present. Invalid durations can remain in the builder and fail
only during build. Source:
[`helpers.rs` registration](../../crates/vibelang-dsp/src/helpers.rs#L1442-L1669).

### Buses, channels, and hardware input

| Exact signature | Return |
|---|---|
| `in_ar(bus: Float or Int or NodeRef, channels: Float or Int)` | NodeRef with the requested output shape |
| `replace_out_ar(bus: Float or NodeRef, channels: Array)` | NodeRef side-effect graph |
| `sound_in(channels: Float or Int)` | Hardware input signal(s) |
| `sound_in_channel(channel: Float or Int or NodeRef)` | One hardware input channel |
| `sound_in_ar()`; `sound_in_ar(channel: Float or Int or NodeRef)` | Alias for channel input; no-arg uses channel 0 |
| `mix(array: Array)`; `sum(array: Array)` | NodeRef; exact aliases |
| `dup(signal: NodeRef, count: Int)`; `channels(signal,count)` | Array of NodeRef channels |
| `channel(signal: NodeRef, index: Int)` | One channel |
| `detune_spread(voices: Int, amount: Float)` | Array of Float detune offsets |
| `zip(a: Array, b: Array)` | DSP-local registration of the same visible name as the core array helper |

Counts are cast with limited validation and wrapper errors often unwrap. Prefer
small positive integral counts.

### Numeric DSP globals

`db_to_amp(Float)`, `amp_to_db(Float)`, `pow(Float,Float)`, `log(Float)`,
`log10(Float)`, `log2(Float)`, `sqrt(Float)`, `abs(Float)`, `floor(Float)`,
`ceil(Float)`, `round(Float)`, `min(Float,Float)`, `max(Float,Float)`, and
`clamp(Float,Float,Float)` return Float. `dc_ar(Float)` and `dc_kr(Float)` return
one-channel NodeRef signals. These overload the core helper names where their
argument types differ.

## SynthDefBuilderHandle

### Factories and terminal behavior

| Exact signature | Return |
|---|---|
| `define_synthdef(name: String)` | SynthDefBuilderHandle |
| `define_synthdef(name: String, body: FnPtr)` | Unit; legacy closure receives the builder |
| `builder.body(body: FnPtr)` | Unit or Rhai error; positional body finalizes, registers metadata, encodes, deploys |
| `builder.body_map(body: FnPtr)` | Unit or Rhai error; Map body required when named inputs are declared |

`body` receives parameters in declaration order. `body_map` receives one Map
containing parameters and named inputs. The returned NodeRef/multichannel graph
is partitioned across declared output ports. Byte-identical encoded bodies are
hash-skipped on redeploy.

### Builder members

| Exact signature | Return | Validation/effect |
|---|---|---|
| `param(name: String, default: Float)` | Chain | Adds control parameter; duplicates/default ranges are not checked here |
| `glide_ms(name: String, milliseconds: Float)` | Chain | Unknown name creates parameter default 0 |
| `out_bus(tag: String)` | Chain | Tags the output-bus control used by code generation |
| `input(name: String)`; `input(name: String, channels: Int)` | Chain or error | Audio input; default mono, explicit only 1 or 2 |
| `output(name: String)`; `output(name: String, channels: Int)` | Chain or error | Audio (`ar`); default mono, channels 1..255 |
| `output_kr(name: String)`; channels overload | Chain or error | Control output, same width limits |
| `output_tr(name: String)`; channels overload | Chain or error | Trigger output, same width limits |
| `outputs(entries: Array)` | Chain or error | Only `outputs([])` is accepted, declaring a side-effect-only synthdef |

Empty or duplicate port names error. The first explicit output declaration
replaces the implicit stereo `out`. Named input routing supports only mono or
stereo audio and therefore requires `body_map`.

```rhai
define_synthdef("filtered_box")
    .input("in", 2)
    .param("cutoff", 1200.0)
    .output("out", 2)
    .output_kr("env")
    .body_map(|p| {
        let body = rlpf_ar(p.inputs.in, p.cutoff, 0.3);
        [body, envelope().perc(0.01, 0.5).build()]
    });
```

## FxBuilderHandle

| Exact signature | Return / behavior |
|---|---|
| `define_fx(name: String)` | FxBuilderHandle, default 2 channels |
| `define_fx(name: String, body: FnPtr)` | Unit; legacy closure receives builder |
| `param(name: String, default: Float)` | Chain |
| `glide_ms(name: String, milliseconds: Float)` | Chain |
| `channels(count: Int)` | Chain; changes only for positive count, so zero/negative silently leave 2 |
| `body(body: FnPtr)`; `body_map(body: FnPtr)` | Unit or Rhai error; finalizes and deploys |

The later explicit zero-channel validation is unreachable through the public
builder because nonpositive values leave the default unchanged.

Source for both builders:
[`api.rs`](../../crates/vibelang-dsp/src/api.rs#L304-L542) and
[`register_synthdef_api`](../../crates/vibelang-dsp/src/api.rs#L702-L766).

## Generated UGens

Every generated rate function, input/default list, overload arity, output shape,
plugin requirement, and manifest source is in the
[generated UGen index](generated/ugens.md). The rule is intentionally unusual:
every function accepts each positional arity from zero through its full input
count (capped at 20), filling omissions from manifest numeric defaults or 0.
Read that contract before assuming an input is required.

`VIBELANG_DUMP_SYNTHDEFS` is native process configuration that writes dumps to
`/tmp`; it is not a `.vibe` function.
