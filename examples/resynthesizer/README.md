# ReSynthesizer Rack Patch

This example is a Make Noise ReSynthesizer-inspired rack patch built from the
VibeLang stdlib modules. It is a musical approximation of the system topology,
not a hardware, firmware, panel, or analog-circuit clone.

## Run

Build the CLI first if needed, then run the patch from this example directory:

```bash
bash -c "cd examples/resynthesizer && ../../target/release/vibe main.vibe"
```

For a source-tree smoke run without watch mode:

```bash
bash -c "cargo run -q -p vibelang-cli -- run --no-watch --no-api --no-jack-connect -I crates/vibelang-std examples/resynthesizer/main.vibe"
```

## Smoke Verification

Build the release CLI, then run the bounded host smoke harness from the project
root:

```bash
cargo build --release -p vibelang-cli
bash examples/resynthesizer/smoke.sh
```

The harness runs the release CLI against scsynth in deterministic no-watch mode:

```bash
RUST_LOG=info ./target/release/vibe run --no-watch --no-api --no-jack-connect examples/resynthesizer/main.vibe
```

It runs with a temporary `XDG_DATA_HOME` so the release binary extracts the
current embedded stdlib instead of reusing a stale user-local stdlib cache. It
sends `SIGINT` after 20 seconds by default
(`VIBE_RESYNTH_SMOKE_SECONDS=N` overrides the duration). Expected clean output
reaches the `Transport started` log line and does not contain any of these
regression strings:

- `UGen not installed`
- `SynthDef not found`
- `LocalBuf tried to allocate too many local buffers`
- `Too many grains`

For the full live command with watching/API/default JACK behavior, run:

```bash
RUST_LOG=info ./target/release/vibe examples/resynthesizer/main.vibe
```

## Topology

`main.vibe` instantiates one public Spectraphon voice. The `spectraphon`
synthdef contains both the real FFT/BinData analyzer and the 64-partial additive
bank; this demo runs it as an SAO oscillator (mode = 2.0) with the built-in
fallback spectrum. Its clean sine also drives a Morphagene SOS/granular leg
whose direct playback path is muted because the worktree's Morphagene buffer
reader still fails standalone.

Spectraphon's odd, sub, even, and sine outputs feed X-PAN's four named inputs.
The main X-PAN output runs through QPAS, DXG, and Mimeophon; Mimeophon is the
final processor on the main rack material before the rack group reaches the
main bus. TEMPI, Rene, MATHS, and Wogglebug run as control-rate voices that
clock and bend the source, blend, filter, gate, and delay parameters. All
running voices live in the single `rack` group.

```text
SOURCE LAYER
  spectraphon_voice (mode=2.0 SAO)
                              --+--> sine ----> Morphagene SOS leg
                                +--> odd -----> X-PAN ch1_a
                                +--> sub -----> X-PAN ch1_b
                                +--> even ----> X-PAN ch2_a
                                +--> sine ----> X-PAN ch2_b

MODULATION LAYER (kr)
  TEMPI -> Rene clocks, MATHS triggers, Morphagene clk, Mimeophon repeats
  Rene  -> Spectraphon pitch, Morphagene organize, QPAS radiate
  MATHS -> Morphagene gene/morph, Spectraphon slide/focus, QPAS Q, DXG ctrl
  Wogglebug -> Spectraphon partials, X-PAN fade, QPAS cutoff, Mimeophon color

STEREO PROCESSOR CHAIN
  X-PAN -> QPAS -> DXG -> Mimeophon -> rack group -> main bus
```

## Live-Tweak Quick Reference

These are the most useful parameters to edit while `-w` hot reload is running:

| Voice | Params | What changes |
|---|---|---|
| `morphagene_reel` | `organize`, `vari_speed`, `gene_size`, `slide`, `morph` | Source splice position, speed, grain size, and smear. |
| `spectraphon_voice` | `freq`, `partials`, `slide`, `focus`, `amp` | Resynthesized pitch, partial density, articulation, and spectral motion. |
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
