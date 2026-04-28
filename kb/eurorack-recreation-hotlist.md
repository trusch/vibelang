# Eurorack Recreation Hotlist

A ranked, annotated survey of eurorack modules worth recreating as vibelang
synthdefs, given the current UGen toolbox (24-engine MiPlaits, MiBraids,
MiRings, MiElements, MiClouds, MiTides, MiWarps, full SC3 oscillator/filter
suite, STK + DWG physical models, granular, chaos, vosim, MDA, etc.).

Bias is toward **sonically distinctive** modules rather than the most popular —
a generic VCA isn't worth a synthdef, but Wogglebug's woggle CV is.

Star ratings are a composite of (impact × uniqueness × feasibility).
Effort is T-shirt sized: S ≈ 1 SP, M ≈ 2-3 SP, L ≈ 5 SP, XL ≈ 8 SP.

## Top 10 (ranked priority)

1. **★★★★★ Mutable Marbles** — random/Markov CV — entirely missing from vibelang — **M**
2. **★★★★★ Make Noise Maths** — canonical function generator, log/lin/exp slope — **L**
3. **★★★★★ Make Noise Wogglebug** — Buchla-265 chaos with woggle edges — **M**
4. **★★★★☆ Intellijel Rainmaker** — 16-tap stereo rhythm delay + comb resonator — **L**
5. **★★★★☆ Make Noise QPAS** — quad-peak stereo state-variable filter — **M**
6. **★★★★☆ Mannequins Cold Mac** — utility w/ Crease wavefolder + survey CV — **S**
7. **★★★★☆ Noise Engineering Loquelic Iteritas** — VOSIM + summation + PM + infinifold — **L**
8. **★★★★☆ Make Noise Morphagene** — tape-granular w/ splices/genes/organize — **L**
9. **★★★☆☆ 4ms Spectral Multiband Resonator (SMR)** — scale-quantized resonator bank — **M**
10. **★★★☆☆ Mutable Frames** — 4-channel keyframe morpher (up to 64 frames) — **M**

## By category

### Oscillators

| Module | Mfg | Sonic | Closest UGens | Effort | Priority | Notes |
|---|---|---|---|---|---|---|
| Plaits | Mutable | 16-engine macro-oscillator | `MiPlaits` (24 engines) | S | ★★★★ | Already covered; wrap as preset bank synthdef |
| Loquelic Iteritas | NE | Wavefolder + VOSIM/SS/PM hybrid | `Vosim`, `PMOsc`, `Shaper` | L | ★★★★ | "Infinifold" wavefolder + multisampled aliasing — distinctive |
| Cursus Iteritas | NE | Additive / wavetable rotated by harmonic | `VOsc`, `Klang`, `Gendy` | M | ★★★ | Spectral-rotation behavior is the catch |
| E370 Quad Morph VCO | Synthtech | Wavetable w/ cloud (supersaw) + thru-zero FM | `VOsc`, `LFSaw`, `Mix` | L | ★★★ | Cloud mode + linear thru-zero FM are the unique parts |
| Harmonic Oscillator | Verbos | 8-partial additive w/ scan/tilt CV | `Klang` + custom scanning mixer | M | ★★★ | Engine exists; UI behavior (scan/tilt) is the synthdef value |
| 0-Coast | Make Noise | West-Coast semi-modular voice | `MiPlaits` + `Pluck` + `MiTides` env | M | ★★★ | Bundled signal-flow patch, not a single new ugen |
| Bastl SoftPop | Bastl | Squelchy semi-modular with PoP cross-mod | Triangle osc + `MoogFF` + S&H | M | ★★ | Unique squelch comes from filter pinging via PoP |
| Braids | Mutable | 48-model multi-osc | `MiBraids` | S | ★★ | Already covered; deserves preset wrappers |

### Filters

| Module | Mfg | Sonic | Closest UGens | Effort | Priority | Notes |
|---|---|---|---|---|---|---|
| QPAS | Make Noise | Stereo quad-peak SVF, animated peaks | 4× `Resonz` / `BPF` w/ Radiate spread + Smile-Pass mix | M | ★★★★ | Smile Pass = LP+HP summed; stereo Radiate is the trick |
| Spectral Multiband Resonator | 4ms | Scale-quantized 6-band resonator | 6× `Resonz` w/ scale tables + Rotate | M | ★★★ | Built-in scale banks (major/Pelog/Bohlen-Pierce) are core |
| Ripples | Mutable | 4-pole Sallen-Key VCF | `MiRipples`, `MoogFF` | S | ★★ | Already covered |
| MS-20 / Korg LP/HP | classic | Resonant w/ self-osc bite | `MoogFF`, `RLPF` | S | ★★ | stdlib likely already has this in `synths/` |
| Doepfer A-124 Wasp | Doepfer | CMOS-screech multimode | `Decimator`, `RLPF` + saturation | M | ★★ | Niche but distinctive |

### Modulators (LFO / envelope / random / function gen)

| Module | Mfg | Sonic | Closest UGens | Effort | Priority | Notes |
|---|---|---|---|---|---|---|
| **Maths** | Make Noise | 4-ch function gen, voltage-controlled slope shape, 25min–1kHz | `EnvGen` + slope-shape mapping + `Slew` + `MiTides` | L | ★★★★★ | Canonical. Need to model log/lin/exp slope morph and EOC/EOR triggers |
| **Marbles** | Mutable | t-section jittery clock, X-section random V w/ controllable distribution + Deja Vu loop history | `Dust`, `TRand`, `LFNoise`, custom Markov state | M | ★★★★★ | No equivalent in vibelang; fills a real hole |
| **Wogglebug** | Make Noise | Smooth + stepped + woggle CVs (Buchla 265 + Wiard) | `LFNoise1/2`, `Latch`, decaying-sine custom | M | ★★★★★ | Woggle mode (stepped voltages w/ decaying-sinusoidal edges) is the signature |
| Stages | Mutable | 6-segment configurable env/seq/LFO | `EnvGen` + `Demand` + segmented patcher | M | ★★★ | Auto-grouping by jack detection is the cleverness |
| Tides | Mutable | Function generator / 4 modes (gates/amps/phases/freqs) | `MiTides` | S | ★★ | Already covered; expose 4-out variant |
| PoliMATHS (2025) | Make Noise | 8-channel Maths derivative | once we have Maths, instantiate ×8 | M | ★★ | Wait until Maths is done |
| Pamela's New Workout | ALM | Clocked LFO/env/euclidean rhythms | `Demand` + `EnvGen` | M | ★★ | Useful but utilitarian |

### Effects

| Module | Mfg | Sonic | Closest UGens | Effort | Priority | Notes |
|---|---|---|---|---|---|---|
| **Rainmaker** | Intellijel/Cylonix | 16-tap stereo rhythm delay (per-tap filter + pitch) + 64-tap comb resonator | `Tap` × 16 + `Resonz` per-tap + `PitchShift` + comb bank | L | ★★★★ | Two-section design — both worth implementing |
| Clouds / Beads | Mutable | Granular textures / pitch-time | `MiClouds`, `GrainBuf` | S | ★★ | Already covered |
| BigSky / Strymon-style shimmer | external | Shimmer reverb | `MiVerb` + `PitchShift` + feedback | S | ★★ | Likely already in stdlib effects/ |
| Erbe-Verb | Make Noise | Flexible reverb size/decay | `MiVerb`, `GVerb`, `Allpass` chain | M | ★★ | Continuous size morph is the trick |
| Z-DSP | Tiptop | Cartridge-based effect player | bank of stdlib effects | L | ★ | Thin novelty over existing effects/ |
| Citadel FX Wizard (2025) | Bastl | 9 stereo experimental FX | grab-bag of distortion + reverb + bitcrush | M | ★★ | One synthdef per algo; quirky; open-source platform |

### Physical / Modal

| Module | Mfg | Sonic | Closest UGens | Effort | Priority | Notes |
|---|---|---|---|---|---|---|
| Plonk | Intellijel/AAS | Mallet exciter + bar/beam/membrane/plate resonator | `MiElements`, `StkModalBar`, `MembraneCircle/Hexagon` | M | ★★★ | Closer to Plonk than Elements is — drum-tuned modal |
| Elements | Mutable | Bow/blow/strike exciter + modal/string resonator | `MiElements` | S | ★★ | Already covered; preset wrapper |
| Rings | Mutable | Modal resonator + sympathetic strings | `MiRings` | S | ★★ | Already covered |

### Samplers / Granular

| Module | Mfg | Sonic | Closest UGens | Effort | Priority | Notes |
|---|---|---|---|---|---|---|
| **Morphagene** | Make Noise | Tape-style granular w/ Reels, Splices, Genes; Organize jumbles | `GrainBuf`, `LoopBuf`, `MiClouds` | L | ★★★★ | Splice-aware playback + Organize CV are the magic |
| Sample Drum | Erica | Per-step pitch + slice + AD env per channel | `PlayBuf`, `EnvGen` + slice index | M | ★★★ | Nice clean sample-player synthdef |
| Multigrain (2025) | Intellijel | Multi-mode granular processor | `MiClouds`, `GrainBuf` | M | ★★★ | DivKid #1 of 2025; worth a wrapper |
| Wave Bard (2025) | Bastl | Patchable stereo sample player | `PlayBuf`, modulation routing | M | ★★ | Open-source platform; quirky |

### Utilities (with sonic value)

| Module | Mfg | Sonic | Closest UGens | Effort | Priority | Notes |
|---|---|---|---|---|---|---|
| **Cold Mac** | Mannequins | 6-in summer + linear-VCA Survey + Crease wavefolder | `Mix` + `LinExp` + `Fold`/custom waveshaper | S | ★★★★ | Crease (translates +/- to opposite domain w/ zero-cross discontinuity) is the unique bit |
| Frames | Mutable | 4-ch mixer w/ 64 keyframes morphable along Frame knob | `Mix` × 4 + `Lag` + scene memory | M | ★★★ | Easing curves (stepped/linear/sine/bouncy) per channel |

### CV / chaos sources

| Module | Mfg | Sonic | Closest UGens | Effort | Priority | Notes |
|---|---|---|---|---|---|---|
| Wogglebug | Make Noise | 7 simultaneous random signals + clock-sync | listed above | M | ★★★★★ | Listed in modulators table; worth its own synthdef |
| Marbles | Mutable | listed above | listed above | M | ★★★★★ | |
| Turing Machine | Tom Whitwell / DIY | Looping shift-register pseudo-random | custom shift register | S | ★★★ | Cheap to build, very useful |
| Branches | Mutable | Probabilistic gate router | `CoinGate`, `Demand` | S | ★★ | Trivial |

## Already covered — don't recreate, just wrap

These have direct UGens; the work is presets/synthdef wrappers, not DSP:

- **Plaits / Braids** → `MiPlaits` (24 engines), `MiBraids` (48 models)
- **Rings / Elements** → `MiRings`, `MiElements`
- **Clouds / Beads** → `MiClouds` (4 modes)
- **Tides** → `MiTides`
- **Warps** → `MiWarps`
- **Grids** → `MiGrids`
- **Verbos Harmonic Oscillator** → `Klang` (additive bank); only the scan/tilt UI is missing
- **Doepfer A-100 VCO/VCF/VCA/LFO classics** → covered by `stdlib/modular/`

## Underserved categories

Things hardware modules cover well that vibelang's stdlib has thin coverage of:

1. **Probabilistic / Markov CV generators** — Marbles is the obvious flagship;
   Turing Machine, Branches, Wogglebug all live here. We have `Dust`,
   `LFNoise`, and the chaos UGens but nothing with built-in *distribution*
   shaping or *history-loop* state (Deja Vu).

2. **Function generators with voltage-controlled slope shape** — Maths,
   Stages, Tides, PoliMATHS. `EnvGen` covers ADSR but the morph between log
   and exp curves under CV is a missing primitive.

3. **Multi-tap rhythmic delays** — Rainmaker, Magneto. We have plenty of
   reverbs and basic delays but no per-tap-filtered/pitch-shifted multi-tap
   abstraction.

4. **Resonator banks with musical scaling** — SMR's killer feature is
   scale-quantized 6-band resonators. We have `Resonz` and `Klang` but no
   "scale bank" abstraction layered on top.

5. **Tape-style granular** — Morphagene's splice/gene workflow. `MiClouds`
   covers the granular DSP but not the tape-machine UX/automation surface.

6. **Utility with character** — Cold Mac's Crease and survey VCA combine into
   an "audio-rate utility" archetype that's missing.

7. **Keyframe morphing** — Frames-style scene-recall mixer. Useful for
   live-performance synthdef parameter morphing.

## Notes on 2025/2026 hot picks (DivKid data-driven 2025 list)

- **Intellijel Multigrain** — granular processor — addressable with `MiClouds`
- **Make Noise MultiMod** — modulation utility
- **Make Noise PoliMATHS** — 8× Maths — wait for Maths first
- **Knobula Drum Farm / Monumatic** — drum-modeling box
- **Acid Rain Ripsaw** — wavetable osc — addressable with `VOsc`
- **Instruo Seashell** — high view-count, sonic interest
- **Bastl Citadel (Wave Bard / FX Wizard)** — open-source DIY platform; Wave
  Bard worth a sample-player synthdef wrapper

These aren't top-priority for recreation but worth tracking — if any becomes a
canonical reference module the way Maths or Marbles did, revisit.

## Sources

- [DivKid – Best Eurorack Modules of 2025](https://divkidvideo.com/best2025/)
- [ModularGrid – Top 100 Eurorack Modules](https://modulargrid.net/e/modules/evaluationlists)
- [Mutable Instruments – Marbles documentation](https://pichenettes.github.io/mutable-instruments-documentation/modules/marbles/)
- [Mutable Instruments – Stages documentation](https://pichenettes.github.io/mutable-instruments-documentation/modules/stages/)
- [Mutable Instruments – Frames documentation](https://pichenettes.github.io/mutable-instruments-documentation/modules/frames/)
- [Make Noise – QPAS](https://www.makenoisemusic.com/modules/qpas/)
- [Make Noise – Wogglebug](https://www.makenoisemusic.com/modules/wogglebug/)
- [Make Noise – Maths (Midwest Modular)](https://midwestmodular.com/maths/)
- [Make Noise – Morphagene](https://www.makenoisemusic.com/modules/morphagene/)
- [Intellijel – Plonk](https://intellijel.com/shop/eurorack/plonk/)
- [Intellijel – Rainmaker](https://intellijel.com/shop/eurorack/cylonix-rainmaker/)
- [Noise Engineering – Loquelic Iteritas](https://noiseengineering.us/products/loquelic-iteritas/)
- [4ms – Spectral Multiband Resonator](https://4mscompany.com/p.php?p=707)
- [Mannequins – Cold Mac (Doudoroff patch guide)](https://doudoroff.com/cold-mac/)
- [Verbos – Harmonic Oscillator](http://www.verboselectronics.com/modules/harmonic-oscillator)
- [Synthesis Technology – E370](https://synthtech.com/eurorack/E370/)
- [Bastl – Softpop SP2](https://bastl-instruments.com/instruments/softpop2)
- [Bastl – Citadel (CDM coverage)](https://cdm.link/bastls-kastle-2-and-citadel-eurorack-now-are-diy-kits-open-source-platforms/)
- [Erica Synths – Sample Drum](https://www.ericasynths.lv/shop/eurorack-modules/by-series/drum-series/sample-drum/)
- [Perfect Circuit – Best New Eurorack 2025](https://www.perfectcircuit.com/signal/best-eurorack-modular-2025)
