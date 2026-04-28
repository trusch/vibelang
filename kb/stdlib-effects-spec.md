# Stdlib Effects Spec

The contract every effect in `crates/vibelang-std/stdlib/effects/` must follow.
The reference implementation is
[`reverbs/plate_reverb.vibe`](../crates/vibelang-std/stdlib/effects/reverbs/plate_reverb.vibe).

## The contract

### 1. Header metadata

The first line of every effect file is a single-line metadata comment:

```
// kind=fx | category=reverb | flavor=plate | stereo=true | params=time,damp,mix,level
```

Fields (pipe-separated `key=value`):

| key        | meaning                                                               |
|------------|-----------------------------------------------------------------------|
| `kind`     | always `fx` for stdlib effects                                        |
| `category` | top-level group: `reverb`, `delay`, `filter`, `dynamics`, `distortion`, `modulation`, `spatial`, `utility`, `character` |
| `flavor`   | the specific algorithm variant — `plate`, `hall`, `tape_wow`, `opto`, ... |
| `stereo`   | `true` if the effect accepts and returns `[L, R]`; `false` for `_mono` variants |
| `params`   | comma-separated list of public params, in declaration order, including `mix` and `level` |

Tooling (LSP, completion, the EFFECTS_INDEX) reads this header. Keep it on
line 1, no leading blank lines, and keep the field set exact.

A free-form description block follows the header.

### 2. Stereo by default

An effect accepts `input` (a stereo array `[L, R]`) and returns `[L, R]`.

If the effect is fundamentally mono — only sensible operating on a single
channel — name it `<base>_mono` and document why. Mono variants set
`stereo=false` in the header.

### 3. Always have `mix`

Every effect declares a `mix` param: `0.0 = full dry, 1.0 = full wet`.

Default by category:

| category                                   | default `mix` |
|--------------------------------------------|---------------|
| reverbs, delays, modulation (chorus, etc.) | `0.3`         |
| insert effects (compressor, EQ, filter, distortion, gate, limiter) | `1.0`         |

The `mix` blend is the *last* signal-shaping stage — it is applied after the
wet path's DC blocker and before the `level` trim.

### 4. Always have `level`

Every effect declares a `level` param with default `1.0`. It is the final
output trim, applied **after** the dry/wet mix:

```rhai
[
    ((1.0 - mix) * input[0] + mix * wet_l) * level,
    ((1.0 - mix) * input[1] + mix * wet_r) * level
]
```

`level` lets users compensate for wet-path gain changes (a heavy reverb makes
things louder; a compressor makes things quieter) without rewriting the
effect.

### 5. DC-blocked wet path

The wet path ends with `leak_dc_ar(signal, 0.995)` *before* the dry/wet mix.
This applies to anything that can introduce DC (feedback delays, comb
filters, asymmetric distortion, allpass chains, fold/clip):

```rhai
let wet_l = leak_dc_ar(processed_l, 0.995);
let wet_r = leak_dc_ar(processed_r, 0.995);
```

Pure-mix utility effects (e.g. `dc_blocker` itself, `stereo_width`) may skip
this if they cannot introduce DC.

### 6. Modulatable params

Declare params with sensible defaults but write the body so each param is
read once per audio block — the runtime feeds kr-rate signals in, so users
can fade them with `fade(...)` without the effect re-baking constants.
Avoid hoisting param-derived constants outside the closure body.

### 7. Self-contained

The effect body only reads from `input` and from its declared params. It
never touches specific bus IDs, never reads global state, never assumes a
particular sample rate beyond what UGens already handle.

## Naming

Effects follow `category_flavor` form:

- `reverb_plate`, `reverb_hall`, `reverb_spring`
- `delay_tape_wow`, `delay_ping_pong`
- `comp_opto`, `comp_fet`
- `dist_overdrive`, `dist_bitcrush`

Mono variants append `_mono`: `reverb_plate_mono`.

Existing names that already match a sensible convention stay as they are —
do not rename `dc_blocker` to `utility_dc_blocker`. New effects should
follow `category_flavor` from day one.

## Param order in `.body(...)`

The `.body(|input, p1, p2, ...|)` argument order must match the
declaration order of `.param("p1", ...).param("p2", ...)`. The header's
`params=` list mirrors that order.

`mix` and `level` are conventionally declared last, in that order, so the
output formula at the bottom of the body reads:

```rhai
.param("flavor_param_a", ...)
.param("flavor_param_b", ...)
.param("mix", 0.3)
.param("level", 1.0)
.body(|input, flavor_param_a, flavor_param_b, mix, level| {
    // ... wet path ...
    let wet_l = leak_dc_ar(processed_l, 0.995);
    let wet_r = leak_dc_ar(processed_r, 0.995);
    [
        ((1.0 - mix) * input[0] + mix * wet_l) * level,
        ((1.0 - mix) * input[1] + mix * wet_r) * level
    ]
})
```

## Reference implementation

[`crates/vibelang-std/stdlib/effects/reverbs/plate_reverb.vibe`](../crates/vibelang-std/stdlib/effects/reverbs/plate_reverb.vibe)
is the canonical example. It demonstrates the header, `mix`/`level` params,
DC-blocked wet path, and the final mix+trim formula. Copy its skeleton when
adding new effects.
