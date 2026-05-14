# CLAUDE.md — vibelang-std

> Standard-library synthdefs and effects for VibeLang. Authoring context
> for new instruments / FX in `stdlib/`.

## What This Is

`vibelang-std` ships compiled-in `.vibe` synthdef and effect definitions
(see `README.md` for the catalogue). Files under `stdlib/` are imported
by user scripts via `import "stdlib/..."`. New synthdefs land here when
they're general-purpose enough to belong in the shipped library.

## Synthdef Authoring

- Use `define_synthdef("name", |builder| { ... })`.
- Patchable stdlib processors live at
  `stdlib/processors/<category>/<name>.vibe`. Use
  `.input("name")` or `.input("name", channels)` for audio-rate mono/stereo
  jacks, `body_map(|p| { ... })` for bodies that read `p.inputs.<name>`, and
  `out` as the primary output. Use primary input `in` for single-input
  passthroughs/filters where parent-group autofeed is useful; use role names
  for multi-input processors.
- Declare named output ports up front via `.output("name")` (1-channel)
  or `.output("name", channels)` (explicit width). The first `.output(...)`
  call replaces the implicit legacy `("out", 2)` port; subsequent calls
  append.
- Body returns one signal per declared port, in declared order. Mono
  ports take a scalar signal, stereo ports take `[L, R]`.
- Voices instantiating the synthdef route per-port from the script via
  `voice.output("name").to(group)` / `.to_main()` / `.to_current_group()`
  / `.mute()`. Plural sugar: `voice.outputs(["a", "b"]).to(...)`.
- Single-port stereo synthdefs without an explicit `.output(...)` call
  keep the legacy implicit `("out", 2)` shape and continue to work.

## Routing

Named-port routing is documented end-to-end in
`kb/voice-multioutput-howto.md` — declaring output ports and input jacks,
default output routing, target-first input patching, terminal verbs, plural /
index sugar, and reload semantics (rename = remove + add with warning).

A worked four-port example lives at
`crates/vibelang-std/stdlib/instruments/spectral/spectraphon_side.vibe`
(synthdef) and `examples/spectraphon_multiout.vibe` (script-side
routing patch).

## References

- `kb/voice-multioutput-howto.md` — full multi-output routing surface.
- `design-named-input-stdlib-primitive-catalogue` — approved first-batch
  stdlib processor catalogue and named-input jack conventions.
- `kb/voice-multi-output-cv-routing-plan.md` — architecture proposal
  with Phase 2/3 (CV ports, modulator obsolescence) plans.
- `kb/spectraphon-howto.md` — Spectraphon synthdef user manual; see
  §8e for the multi-output routing patch.
- `crates/vibelang-dsp/src/builder.rs::SynthDefBuilder::output` —
  port declaration validator.
