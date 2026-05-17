# Synthdef Authoring Conventions

How to choose between positional `.body(...)` and the map-style
`.body_map(...)` when writing synthdefs, FX, and kr-output CV sources in the
stdlib (or in user `.vibe` files).

## TL;DR

| Param count | Recommended body style                           |
|-------------|--------------------------------------------------|
| **0–6**     | Positional `.body(\|a, b, ...\|)` — preferred    |
| **7–10**    | Positional acceptable; `.body_map` if helpful    |
| **>10**     | `.body_map(\|p\| ...)` — **required** (hard error otherwise) |

The cap on positional bodies is not a vibelang style preference — it
is a Rhai-side constraint. `Rhai::FnPtr::call` only has `IntoFuncArgs`
tuple impls up to length 10, so the synthdef builder's `match
param_nodes.len()` arms top out at 10 and the 11+ arm returns a hard
error pointing the author here. See
[`kb/synthdef-arity-limits-plan.md`](synthdef-arity-limits-plan.md) for
the full background.

## ≤6 params — positional preferred

Short positional signatures read like ordinary function definitions
and put parameter order right next to parameter use. This is how the
overwhelming majority of stdlib instruments and FX are written today.

Reference: [`drums/kicks/kick_808.vibe`](../crates/vibelang-std/stdlib/drums/kicks/kick_808.vibe).

```rhai
define_synthdef("kick_808")
    .param("freq", 60.0)
    .param("amp", 1.0)
    .body(|freq, amp| {
        let env = envelope().perc(0.001, 0.5).cleanup_on_finish().build();
        sin_osc_ar(freq) * env * amp
    });
```

Use positional whenever the param count fits and the body is short
enough that the closure header doesn't dominate the reader's attention.

## 7–10 params — positional acceptable

Positional still works at 7–10 params and stays the default. Switch to
`.body_map` only if the closure header has become noisy enough to
hurt readability (long names, nested patches, lots of parameters
shadowing each other).

The hard cap is at 11 — 10 is the last positional-friendly number.
Going from 10 to 11 means a refactor to `body_map`, so if a synthdef
is sitting at exactly 10 and you expect it to grow, pre-empt the
migration.

## >10 params — `.body_map` is required

`.body(|...|)` at >10 params returns a `SynthDefError::RhaiError`
identifying the synthdef name, the param count, and the fix:

```
Synthdef `<name>` declares <N> parameters; positional `.body(|a, b, ...|)`
is capped at 10 (Rhai's tuple-arg limit). Switch to `.body_map(|p| ...)`
and read params as `p.<name>` (e.g. `p.freq`, `p.cutoff`).
```

The same error fires in matching shape for FX (`.body_map(|p|`,
input is `p.input`) and other synthdefs that declare kr outputs. There is
no override — every synthdef authoring path with 11+ user params must use
`body_map`.

### Migrating positional → map

Mechanical translation:

1. Change `.body(|a, b, c, ...| { ... })` → `.body_map(|p| { ... })`.
2. At the top of the closure, unpack the params into local bindings
   matching the original positional names. This keeps the rest of the
   body diff-clean and preserves LSP-friendly local names.

   ```rhai
   .body_map(|p| {
       let freq   = p.freq;
       let cutoff = p.cutoff;
       let res    = p.res;
       // ... rest of body unchanged ...
   })
   ```

3. For FX bodies, `input` lives at `p.input` (same shape as before:
   array of NodeRefs, length matches the FX's channel count).
4. For kr-output CV-source bodies, the same unpack pattern applies.

Reference migration:
[`instruments/spectral/spectraphon_dual.vibe`](../crates/vibelang-std/stdlib/instruments/spectral/spectraphon_dual.vibe)
went from a 10-param positional body (with parameters folded away to
fit) to a 17-param `body_map` body that restores the previously-folded
`mode_a`/`mode_b`, `array_idx_a`/`array_idx_b`, per-side
`partials`/`slide`/`focus`. Read its header comment for the full list
of restored params and the unpack pattern.

### Why unpack into locals rather than read `p.foo` inline?

Both are valid; the stdlib convention is to unpack so:

- The unpack block doubles as a parameter index for readers: you can
  see at a glance what the synthdef takes without scrolling the
  `.param(...)` chain.
- The body diff vs the previous positional version stays small.
- Future LSP work that completes `p.<param>` in `body_map` closures
  (per `kb/synthdef-arity-limits-plan.md` Phase 5) will benefit
  authors writing _new_ map-style bodies; for migrated stdlib code
  the local-binding form already reads cleanly.

Inline `p.freq` is fine for one-off uses or trivial bodies — there is
no lint enforcing the unpack pattern.

## Where this is enforced

- **Hard error (>10 positional):** `crates/vibelang-dsp/src/builder.rs`
  in `build_body_closure_with_options_inner`,
  `build_effect_closure_inner`, and related synthdef closure paths.
  They return a `SynthDefError::RhaiError` naming the synthdef
  and pointing at `body_map`.
- **No automated lint for the 6 / 7–10 thresholds.** This document is
  the source of truth; review enforces it.
- **`body_map` API:** `SynthDefBuilder::body_map`,
  `EffectBuilder::body_map`, and kr-output synthdef bodies — all
  shipped alongside the existing positional
  `body(...)`.

## Related

- [`synthdef-arity-limits-plan.md`](synthdef-arity-limits-plan.md) —
  full architecture rationale, the rhai 10/20-arity cap analysis, and
  the phased plan this convention belongs to.
- [`stdlib-effects-spec.md`](stdlib-effects-spec.md) — the contract
  for stdlib FX (header metadata, stereo, mix/level/leak_dc_ar). The
  param-count rules above apply on top of the effects spec.
- [`voice-multioutput-howto.md`](voice-multioutput-howto.md) — port
  routing on multi-output synthdefs (orthogonal to body style).
