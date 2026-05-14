# ReSynthesizer Rack Patch

This example is a Make Noise ReSynthesizer-inspired rack patch built from the
VibeLang stdlib modules. It is a musical approximation of the system topology,
not a hardware, firmware, panel, or analog-circuit clone.

## Run

Build the CLI first if needed, then run the patch from this example directory:

```bash
bash -c "cd examples/resynthesizer && ../../target/release/vibelang run -w -I ../.. main.vibe"
```

For a source-tree smoke run without watch mode:

```bash
bash -c "cargo run -q -p vibelang-cli -- run --no-watch --no-api --no-jack-connect -I crates/vibelang-std examples/resynthesizer/main.vibe"
```

## Topology

`main.vibe` bakes a short Morphagene reel, plays it as a granular stereo source,
and also taps it into Spectraphon dual SAM inputs for frequency-domain
resynthesis. The dry Morphagene layer and Spectraphon odd/even partial banks are
collapsed through mono taps into X-PAN, then processed by QPAS, DXG, and
Mimeophon before the rack group reaches the main bus. TEMPI, Rene, MATHS,
Wogglebug, PrssPnt, and CV Bus run as control-rate voices that clock and bend
the source, blend, filter, gate, and delay parameters.

```text
SOURCE LAYER
  Morphagene reel/grains -----> raw_morphagene group --+--> morph tap
         |                                             |
         +--> Spectraphon analyze_a/analyze_b          +--> X-PAN aux
                      |
  Spectraphon dual odd/even banks
         |                  |
         +--> raw_spectral_odd  --> odd tap  --+
         +--> raw_spectral_even --> even tap --+--> X-PAN ch1/ch2

MODULATION LAYER (kr)
  TEMPI -> Rene clocks, MATHS triggers, Morphagene clk, Mimeophon repeats
  Rene  -> Spectraphon pitch, Morphagene organize, CV Bus
  MATHS -> Morphagene gene/morph, Spectraphon FM, QPAS Q, DXG ctrl
  Wogglebug + PrssPnt + CV Bus -> pan, fade, radiate, cutoff, zone, mix

STEREO PROCESSOR CHAIN
  X-PAN -> QPAS -> DXG -> Mimeophon -> rack group -> main bus
```

## Live-Tweak Quick Reference

These are the most useful parameters to edit while `-w` hot reload is running:

| Voice | Params | What changes |
|---|---|---|
| `morphagene_reel` | `organize`, `vari_speed`, `gene_size`, `slide`, `morph` | Source splice position, speed, grain size, and smear. |
| `spectraphon_resynth` | `freq_a`, `freq_b`, `partials_a`, `partials_b`, `slide_a`, `focus_b` | Resynthesized pitch, partial density, and spectral motion. |
| `woggle_motion` | `rate`, `depth_v`, `chaos` | Amount and speed of random drift across the whole rack. |
| `maths_functions` | `rise1`, `fall1`, `rise4`, `fall4`, `cycle1`, `cycle4` | Envelope/LFO timing for grain, FM, Q, and gate movement. |
| `touch_macro` | `press1`, `press2`, `press3`, `press4`, `slew` | Manual macro pressure for aux blend, delay mix, and DXG opening. |
| `xpan_motion` | `ch1_gain`, `ch2_gain`, `aux_gain`, `level` | Balance between odd/even spectral material and dry Morphagene. |
| `qpas_filter` | `cutoff`, `q`, `radiate`, `mode`, `mix` | Brightness, stereo filter spread, resonance, and filter output flavor. |
| `dxg_gate` | `ctrl1`, `ctrl2`, `strike_level`, `strike_decay` | Low-pass gate openness and pluck decay. |
| `mimeophon_space` | `time`, `zone`, `repeats`, `skew`, `color`, `halo`, `mix` | Delay range, feedback, stereo skew, tone, smear, and wet level. |

## Honest Caveats

The current implementation target is patch-semantic coverage: useful signal
roles, named routing surfaces, stable imports, and musical behavior. It does
not claim exact Make Noise panel, firmware, storage, analog VCA/vactrol, Select
Bus, capacitive-touch, or hidden-mode fidelity.

- Morphagene uses an evenly divided buffer-backed reel model. Exact microSD
  workflows, marker storage, destructive reel behavior, and undocumented Morph
  edge modes are not modeled.
- Spectraphon SAM/SAO behavior is a practical additive/analysis model. The FFT
  analyzer, Array interpolation, dual-side storage, Sync behavior, Chaos/Noise
  modes, and hardware Array format are approximated or deferred.
- MATHS, TEMPI, Rene, Wogglebug, PrssPnt, and CV Bus are control-rate voices.
  They preserve useful CV routing roles but not audio-rate patching, touch UI,
  Select Bus, firmware programming pages, or detailed analog response.
- X-PAN, QPAS, DXG, and Mimeophon are named-input stereo processors with the
  right musical roles. Analog headroom, Smile/hidden QPAS circuits, vactrol
  memory, separate DXG channel outputs, Mimeophon Hold/Flip/Tempo/microRate
  ports, Rate Out, and Soundhack firmware details are not exposed exactly.
- Named Spectraphon analysis and Morphagene SOS inputs must be patched
  explicitly. Unpatched named inputs are silent rather than falling back to a
  hardware input.

For module-by-module caveats and source notes, see
[`../../kb/resynthesizer-implementation-status.md`](../../kb/resynthesizer-implementation-status.md),
[`../../kb/resynthesizer-module-behavior-matrix.md`](../../kb/resynthesizer-module-behavior-matrix.md),
and
[`../../kb/make-noise-resynthesizer-manual-sources.md`](../../kb/make-noise-resynthesizer-manual-sources.md).
