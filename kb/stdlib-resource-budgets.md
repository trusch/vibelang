# Stdlib LocalBuf And Grain Resource Budgets

This note audits the stdlib synthdefs that can explain these scsynth runtime
warnings:

- `warning: LocalBuf tried to allocate too many local buffers.`
- `ClearBuf: no valid buffer`
- `Too many grains!`

Scope: `crates/vibelang-std/stdlib/`, searched for `local_buf_ir`,
`grain_buf_*`, `t_grains_*`, and adjacent granular helpers.

## scsynth Limits Relevant Here

VibeLang boots scsynth through `ScsynthConfig` with these relevant values:

| Resource | VibeLang boot value | SC default | Notes |
|---|---:|---:|---|
| Global sample buffers, `-b` / `numBuffers` | 4096 | 1024 | VibeLang intentionally raises this for script buffers in `2048..4096`; this is not the LocalBuf limit. |
| RT memory, `-m` / `memSize` | 8192 KB | 8192 KB | Used for synth/unit allocations and delay UGens, not server sample buffers. |
| Wire buffers, `-w` / `numWireBufs` | 64 | 64 | SynthDef graph interconnect complexity. |
| Max synthdefs, `-d` / `maxSynthDefs` | 1024 | 1024 | Count of loaded synth definitions. |

SC documentation lists `numBuffers` default 1024, `numWireBufs` default 64,
and `memSize` default 8192 KB in `ServerOptions`:
https://doc.sccode.org/Classes/ServerOptions.html

LocalBuf has a separate synth-local limit. In SC's language layer, every
`LocalBuf` increments a `MaxLocalBufs` UGen for the SynthDef. The server starts
each graph with `localMaxBufNum = 0`; `LocalBuf_Ctor` warns when
`localBufNum >= localMaxBufNum` and returns `-1`, and `ClearBuf` then prints
`ClearBuf: no valid buffer` for that invalid bufnum. Relevant upstream files:

- `MaxLocalBufs` docs: https://doc.sccode.org/Classes/MaxLocalBufs.html
- SC `BufIO.sc`: `LocalBuf.new1` increments `UGen.buildSynthDef.maxLocalBufs`
- SC `DelayUGens.cpp`: `LocalBuf_Ctor`, `MaxLocalBufs_Ctor`, `ClearBuf_Ctor`

VibeLang has a `max_local_bufs_ir` manifest entry, but no stdlib file in this
audit calls it, and a repo grep found no Rust-side auto-insertion. Therefore,
for current VibeLang-generated SynthDefs, any `local_buf_ir` demand is likely
to exceed the undeclared synth-local budget of 0, independent of `-b 4096`.

## LocalBuf Demand By Synthdef

Sample count is `channels * frames` summed across all `local_buf_ir` calls.
Memory is approximate per synth instance at 32-bit float samples.

| Synthdef / FX | File | LocalBufs | Samples | Approx memory | Recommendation |
|---|---|---:|---:|---:|---|
| `dld` | `processors/delays/dld.vibe` | 2 | 768000 | 2.9 MiB | Keep demand, but use persistent server buffers or raise RT memory if many instances are expected; first fix missing `MaxLocalBufs(2)`. |
| `beat_repeater` | `effects/glitch/beat_repeater.vibe` | 2 | 192000 | 750 KiB | OK for one instance; prefer persistent buffers if stacked heavily; needs `MaxLocalBufs(2)`. |
| `density_grain` | `effects/granular/density_grain.vibe` | 2 | 192000 | 750 KiB | Same as above; also see TGrains budget. |
| `granular_freeze` | `effects/granular/granular_freeze.vibe` | 2 | 192000 | 750 KiB | Same as above; also see TGrains budget. |
| `granular_pitch_shift` | `effects/granular/granular_pitch_shift.vibe` | 2 | 192000 | 750 KiB | Same as above; also see TGrains budget. |
| `granular_processor` | `effects/granular/granular_processor.vibe` | 2 | 192000 | 750 KiB | Same as above; also see TGrains budget. |
| `granular_time_stretch` | `effects/granular/granular_time_stretch.vibe` | 2 | 192000 | 750 KiB | Same as above; also see TGrains budget. |
| `reverb_reverse` | `effects/reverbs/reverb_reverse.vibe` | 2 | 192000 | 750 KiB | Same as above. |
| `reverse_buffer` | `effects/glitch/reverse_buffer.vibe` | 2 | 192000 | 750 KiB | Same as above. |
| `scatter` | `effects/granular/scatter.vibe` | 2 | 192000 | 750 KiB | Same as above; also see TGrains budget. |
| `warp1_processor` | `effects/granular/warp1_processor.vibe` | 2 | 192000 | 750 KiB | Same as above; Warp1 has its own internal grain pool. |
| `stutter_repeat` | `effects/glitch/stutter_repeat.vibe` | 2 | 96000 | 375 KiB | Needs `MaxLocalBufs(2)`; size is moderate. |
| `spectral_compressor` | `effects/spectral/spectral_compressor.vibe` | 4 | 8192 | 32 KiB | Needs `MaxLocalBufs(4)`; otherwise safe. |
| `spectral_morph` | `effects/spectral/spectral_morph.vibe` | 4 | 8192 | 32 KiB | Needs `MaxLocalBufs(4)`; otherwise safe. |
| `spectraphon_dual` | `instruments/spectral/spectraphon_dual.vibe` | 3 | 4097 | 16 KiB | Plausible resynthesizer rack offender; needs `MaxLocalBufs(3)`. |
| `reverb_infinite` | `effects/reverbs/reverb_infinite.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `reverb_shimmer_pro` | `effects/reverbs/reverb_shimmer_pro.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `spectral_blur` | `effects/spectral/spectral_blur.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `spectral_brickwall` | `effects/spectral/spectral_brickwall.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `spectral_denoise` | `effects/spectral/spectral_denoise.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `spectral_diffuser` | `effects/spectral/spectral_diffuser.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `spectral_freeze` | `effects/spectral/spectral_freeze.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `spectral_gate` | `effects/spectral/spectral_gate.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `spectral_phase_shift` | `effects/spectral/spectral_phase_shift.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `spectral_scrambler` | `effects/spectral/spectral_scrambler.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `vocoder_pv` | `effects/spectral/vocoder_pv.vibe` | 2 | 4096 | 16 KiB | Needs `MaxLocalBufs(2)`. |
| `spectraphon_side` | `instruments/spectral/spectraphon_side.vibe` | 2 | 2049 | 8 KiB | Needs `MaxLocalBufs(2)`. |
| `morphagene` | `instruments/sampler/morphagene.vibe` | 1 | 1 | 4 B | Plausible `ClearBuf` offender: it calls `clear_buf_ir` on this LocalBuf. Needs `MaxLocalBufs(1)` or removal of the LocalBuf feedback cell. |

### LocalBuf Mitigation

Primary mitigation should be compiler/helper-side: make `local_buf_ir` emit or
cooperate with exactly one `MaxLocalBufs` declaration per SynthDef, matching
the count of LocalBuf UGens. Manually adding `max_local_bufs_ir(n)` to each
stdlib synthdef is a usable short-term patch, but is fragile because the
required value changes whenever LocalBuf calls are added or removed.

Raising `-b` does not fix this warning. Raising `-m` can be useful only after
`MaxLocalBufs` is declared, for synthdefs with large local memories such as
`dld` or stacked 96k-frame granular/glitch effects.

## Grain Demand By Synthdef

SC `GrainBuf` exposes `maxGrains`, defaulting to 512 active overlapping grains:
https://doc.sccode.org/Classes/GrainBuf.html

SC `TGrains` has no `maxGrains` argument in the helpfile. In current scsynth
source, `TGrains` uses a fixed `kMaxGrains = 64` array per UGen instance and
stops starting new grains when full:
https://doc.sccode.org/Classes/TGrains.html

Estimated active grains are `trigger_rate_hz * grain_duration_sec` per UGen.
The stdlib TGrains effects instantiate two TGrains UGens, one for each side.

| Synthdef / FX | Grain helper | Trigger rate | Grain duration | Peak active grains | Limit | Recommendation |
|---|---|---:|---:|---:|---:|---|
| `morphagene` | 1 x `grain_buf_ar(..., maxGrains=64)` | autonomous `min(factor / gene_dur, 4000)` or external CLK | `gene_dur` in `[50us, splice_len]` | Nominal autonomous demand <= 4; external CLK demand is `clk_hz * gene_dur` | 64 explicit | Planned autonomous path should not exceed 64. If `Too many grains!` appears here, suspect trigger storm/noisy CLK or a non-edge trigger signal; harden trigger shaping or raise explicit maxGrains only after measuring. |
| `density_grain` | 2 x `t_grains_ar` | `density_min..density_max`, defaults 5..60 | default 0.1, documented 0.01..0.5 | default peak 6 each; documented peak 30 each | 64 fixed | Under limit. Clamp density/grain_size product if allowing user values outside docs. |
| `granular_freeze` | 2 x `t_grains_ar` | `density`, default 30, documented 1..100 | default 0.15, documented 0.01..0.5 | default 4.5 each; documented peak 50 each | 64 fixed | Under limit but close at max settings. Clamp `density * grain_size <= 48` for margin. |
| `granular_processor` | 2 x `t_grains_ar` | `density`, default 30, documented 1..100 | default 0.1, documented 0.01..0.5 | default 3 each; documented peak 50 each | 64 fixed | Same as `granular_freeze`. |
| `scatter` | 2 x `t_grains_ar` | `density`, default 30, documented 1..100 | default 0.1, documented 0.01..0.5 | default 3 each; documented peak 50 each | 64 fixed | Same as `granular_freeze`. |
| `granular_pitch_shift` | 2 x `t_grains_ar` | constant 40 | default 0.1, documented 0.01..0.5 | default 4 each; documented peak 20 each | 64 fixed | Safe. |
| `granular_time_stretch` | 2 x `t_grains_ar` | constant 30 | default 0.15, documented 0.01..0.5 | default 4.5 each; documented peak 15 each | 64 fixed | Safe. |

Other granularly named stdlib processors do not hit the SC GrainBuf/TGrains
budget directly:

- `warp1_processor` uses `warp1_ar`, two calls, each with `overlaps=4`.
- `clouds_processor` uses six `mi_clouds_ar` calls because it precomputes high
  and lo-fi variants for L/R/mono before selecting. This is likely CPU-heavy,
  but it is not the SC `Too many grains!` warning path.
- `beads` uses one `mi_clouds_ar` call.
- `pitch_shift_ar` users rely on PitchShift internals, not GrainBuf/TGrains.
- `bass_granular`, `lead_granular`, and `texture_granular` are oscillator/noise
  textures, not SC granular buffer UGens.

## Resynthesizer Rack Diagnosis

The observed rack warnings are most plausibly:

- `spectraphon_dual`: three LocalBufs and no declared MaxLocalBufs. It can
  print `LocalBuf tried to allocate too many local buffers` even though its
  sample memory is tiny.
- `morphagene`: one 1-frame LocalBuf, immediately passed to `clear_buf_ir`.
  If LocalBuf returns `-1`, `ClearBuf` prints `ClearBuf: no valid buffer`.
- `morphagene` is the only rack synthdef using `grain_buf_ar`; its nominal
  autonomous overlap is <= 4 against `maxGrains=64`, so `Too many grains!`
  points to trigger-shaping failure or noisy external clock rather than normal
  Morph/Gene-Size settings.

Recommended order:

1. Fix LocalBuf accounting in the compiler/helper layer or add explicit
   `max_local_bufs_ir(n)` to each offender as a stopgap.
2. For large LocalBuf users, consider replacing per-instance LocalBuf delay
   memory with script/server buffers when the effect is expected to be stacked.
3. Harden Morphagene's grain trigger input before raising `maxGrains`: clamp
   external clock rate, convert to one-sample/one-control pulse, and consider
   a minimum inter-trigger interval.
4. Only raise scsynth `-m` after LocalBuf accounting is fixed and runtime memory
   allocation actually fails. Do not raise `-b` for LocalBuf warnings.
