# UGen Coverage Audit (post expansion)

> Source-of-truth for SC core: `supercollider/server/plugins/*.cpp` at `main` (cloned `--depth=1` to `/tmp/sc-source`, 2026-04-28).
> Source-of-truth for sc3-plugins: `supercollider/sc3-plugins/source/*/*.cpp` at `main` (cloned to `/tmp/sc3-plugins`, 2026-04-28).
> Coverage scan: `crates/vibelang-dsp/ugen_manifests/*.json` (67 files, post-expansion).
>
> Method:
> 1. Canonical names extracted from three SC registration forms — `Define*Unit(Ident)`, `Define*Unit("Name", ...)`, and `registerUnit<...>(ft, "Name", ...)`.
> 2. Vibelang names extracted with `jq -r '.[].name' ugen_manifests/*.json | sort -u`.
> 3. Set diffs computed via `comm`. Per-pack groupings derived from manifest filenames.

## Summary

| Metric | Pre-expansion | Post-expansion | Δ |
|---|---:|---:|---:|
| Manifest files | 24 | **67** | +43 |
| Total UGen entries (deduped) | 232 | **787** | +555 |
| Canonical SC core covered | 215 | **376** | +161 |
| Missing from SC core | 185 | **24** | −161 |
| sc3-plugins canonical covered | n/a | **351** | — |
| **SC core coverage** | **54 %** | **94 %** | +40 pp |
| **Effective core coverage** (excl. intentional skips) | n/a | **96 %** | — |
| **sc3-plugins coverage** | n/a | **85 %** | — |

The expansion added 43 new manifest files and ~555 new UGen entries. SC core canonical coverage rose from 54 % to 94 %; the 24 still-missing names are dominated by intentional skips (Bela hardware, operator wrappers, synthdef-compiler plumbing, server-test artifacts), leaving only **2 latent core gaps worth chasing** (`Sum3`, `Sum4` from `MulAddUGens.cpp`).

## SC core — what's still missing (24)

Every gap below is either intentionally out-of-scope or already exposed via another path. The table groups by reason for the skip.

### Intentional / out-of-scope (10)

| UGen | Source file | Why skipped |
|---|---|---|
| AnalogIn, AnalogOut, BelaScopeOut, DigitalIn, DigitalIO, DigitalOut, MultiplexAnalogIn | BelaUGens.cpp | Compile-time conditional (`SC_BELA`); only built on the Bela platform. |
| BinaryOpUGen | BinaryOpUGens.cpp | Exposed via Rust `+ - * / %` operator overloads on signal nodes. |
| UnaryOpUGen | UnaryOpUGens.cpp | Exposed via `math.json` / `conversion.json` (abs, neg, reciprocal, sin, cos, …). |
| UnitCmdDemo | DemoUGens.cpp | Server-test artifact, not a real UGen. |

### Compiler-emitted plumbing (3)

| UGen | Source file | Why skipped |
|---|---|---|
| Control, AudioControl, LagControl | IOUGens.cpp | Synthdef compiler emits these for declared parameters; user code never names them. |

### Niche / IPC / diagnostic (8)

| UGen | Source file | Notes |
|---|---|---|
| LagIn, InTrig | IOUGens.cpp | Niche bus-read variants; trivially achievable via `Lag.kr(In.kr(...))` etc. |
| SharedIn, SharedOut | IOUGens.cpp | scsynth-only IPC bus access; supernova omits these entirely. |
| CheckBadValues, Sanitize | TestUGens.cpp | NaN/Inf debugging — useful but not yet wired up. |
| FFTTrigger, PackFFT, Unpack1FFT | FFT_UGens.cpp / UnpackFFTUGens.cpp | FFT bin-level synthesis primitives — interesting if/when bin-rate user code is supported. |

### Real gaps (2)

| UGen | Source file | Notes |
|---|---|---|
| Sum3, Sum4 | MulAddUGens.cpp | Fused 3- and 4-input adders — a small but easy optimisation win for `+` chains. |

## SC core — per-source-file post-fill status

All formerly red rows from the prior audit are now ✅ unless flagged as intentional skip (⏭) or trivially low-priority (📋).

| Source file | Canonical | Covered | Status |
|---|---:|---:|---|
| BelaUGens.cpp | 7 | 0 | ⏭ out-of-scope (hardware) |
| BinaryOpUGens.cpp | 1 | 0 | ⏭ via Rust ops |
| UnaryOpUGens.cpp | 1 | 0 | ⏭ via math/conversion |
| ChaosUGens.cpp | 22 | 22 | ✅ |
| Convolution.cpp + PartitionedConvolution.cpp | 6 | 6 | ✅ |
| DelayUGens.cpp | 56 | 56 | ✅ |
| DemandUGens.cpp | 25 | 25 | ✅ |
| DemoUGens.cpp | 1 | 0 | ⏭ test artifact |
| DiskIO_UGens.cpp | 3 | 3 | ✅ |
| DynNoiseUGens.cpp | 4 | 4 | ✅ |
| FFT_UGens.cpp | 3 | 2 | 📋 missing FFTTrigger |
| FeatureDetection.cpp | 3 | 3 | ✅ |
| FilterUGens.cpp | 57 | 57 | ✅ |
| GendynUGens.cpp | 3 | 3 | ✅ |
| GrainUGens.cpp | 5 | 5 | ✅ |
| IOUGens.cpp | 16 | 9 | 📋 7 plumbing/IPC missing |
| LFUGens.cpp | 30 | 30 | ✅ |
| LinkUGen.cpp | 3 | 3 | ✅ |
| ML.cpp / ML_SpecStats.cpp | 12 | 9 | ✅ (3 dupes between files) |
| MulAddUGens.cpp | 3 | 1 | 📋 Sum3/Sum4 missing |
| NoiseUGens.cpp | 26 | 26 | ✅ |
| OscUGens.cpp | 25 | 25 | ✅ |
| PV_UGens.cpp | 31 | 31 | ✅ |
| PV_ThirdParty.cpp | 1 | 1 | ✅ |
| PanUGens.cpp | 12 | 12 | ✅ |
| PhysicalModelingUGens.cpp | 3 | 3 | ✅ |
| ReverbUGens.cpp | 3 | 3 | ✅ |
| TestUGens.cpp | 2 | 0 | 📋 diagnostic |
| TriggerUGens.cpp | 33 | 33 | ✅ |
| UIUGens.cpp | 4 | 4 | ✅ |
| UnpackFFTUGens.cpp | 2 | 0 | 📋 FFT bin-level |

✅ rows: every previously-flagged high-value category (PV_, chaos, demand, convolution, disk, ML, lifecycle, filters, oscillators, ambisonic pan, link, noise) is now fully covered.

## Manifest entries by category (current state)

The 67 manifest files split cleanly into SC-core-aligned packs and community/sc3-plugins packs. Counts are deduplicated `name` entries, not function-variant rows.

### Core-aligned manifests (28 files, ~423 entries)

| File | Entries | Maps to |
|---|---:|---|
| filters.json | 47 | FilterUGens.cpp + DynNoiseUGens.cpp |
| oscillators.json | 41 | OscUGens.cpp + LFUGens.cpp (subset) |
| pv_spectral.json | 32 | PV_UGens.cpp + PV_ThirdParty.cpp |
| triggers.json | 30 | TriggerUGens.cpp |
| demand.json | 25 | DemandUGens.cpp |
| noise.json | 23 | NoiseUGens.cpp + DynNoiseUGens.cpp |
| chaos.json | 22 | ChaosUGens.cpp |
| buffers.json | 18 | DelayUGens.cpp (buffer ops) |
| analysis.json | 17 | ML.cpp + FeatureDetection.cpp + LFUGens analysis |
| info.json | 14 | InfoUGens.cpp + IOUGens info |
| delays.json | 14 | DelayUGens.cpp (delays) |
| panning.json | 13 | PanUGens.cpp |
| envelopes.json | 11 | EnvGen + LFUGens shape |
| control.json | 11 | TriggerUGens lifecycle + IOUGens control |
| inout.json | 9 | IOUGens.cpp |
| bufdelays.json | 9 | DelayUGens.cpp (BufDelay family) |
| random.json | 8 | NoiseUGens.cpp (RNG scalars) |
| math.json | 8 | MulAddUGens.cpp + sclang range maps |
| convolution.json | 6 | Convolution.cpp + PartitionedConvolution.cpp |
| conversion.json | 6 | UnaryOpUGens (math/conversion) |
| granular.json | 5 | GrainUGens.cpp |
| physical.json | 4 | PhysicalModelingUGens.cpp |
| dynamics.json | 4 | FilterUGens (Compander, Limiter…) |
| reverb.json | 3 | ReverbUGens.cpp |
| link.json | 3 | LinkUGen.cpp |
| disk_io.json | 3 | DiskIO_UGens.cpp |
| multichannel.json | 2 | PanUGens (Splay) |
| fft.json | 2 | FFT_UGens.cpp |
| pitchtime.json | 1 | PV pitch-shift |

### Community / sc3-plugins manifests (39 files, ~364 entries)

| File | Entries | Source pack |
|---|---:|---|
| sc3_josh_spectral.json | 69 | JoshUGens (PV_*, Greyhole, JPverb) |
| sc3_mcld.json | 64 | MCLD (Onsets/MFCC variants, FFT helpers) |
| sc3_bhob.json | 59 | bhob (Berlin School / RFW companion) |
| sc3_sl.json | 30 | SL UGens |
| atk_foa.json | 27 | atk-sc3 first-order ambisonic |
| sc3_berlach.json | 19 | Berlach (Berlin School) |
| sc3_stk.json | 17 | STK physical modelling |
| mi_ugens.json | 12 | mi-UGens (Mutable Instruments ports) |
| sc3_scmir.json | 9 | SCMIR feature extractors |
| sc3_deind.json | 8 | Deind synthesis |
| sc3_bat.json | 7 | BatUGens |
| sc3_dwg.json | 7 | DWG (digital waveguide) |
| sc3_aa_oscillators.json | 6 | AntiAliasingOscillators |
| sc3_rmeqsuite.json | 6 | RMEQSuite parametric EQ |
| sc3_ncanalysis.json | 6 | NCAnalysis |
| sc3_distortion.json | 5 | Distortion UGens |
| sc3_glitch.json | 4 | Glitch |
| sc3_blackrain.json | 4 | BlackRain |
| sc3_vbap.json | 4 | VBAP panning |
| sc3_chaos.json | 3 | sc3 chaos (NLFilt, Rossler, RMA) |
| sc3_auditory.json | 3 | Auditory modelling |
| sc3_tag_system.json | 3 | Tag system |
| sc3_pitch_detection.json | 2 | Pitch detection |
| sc3_membrane.json | 2 | Membrane (2D mesh) |
| sc3_concat.json | 2 | Concat |
| sc3_betablocker.json | 2 | Betablocker |
| sc3_bbcut2.json | 2 | BBCut2 |
| sc3_summer.json | 2 | Summer |
| sc3_quantity.json | 2 | Quantity |
| sc3_rfw.json | 2 | RFW |
| sc3_loopbuf.json | 1 | LoopBuf |
| sc3_fm7.json | 1 | FM7 |
| sc3_dfm1.json | 1 | DFM1 |
| sc3_ay.json | 1 | AY chip |
| sc3_mda.json | 1 | mda (Piano) |
| sc3_neuromodules.json | 1 | NeuroModules |
| sc3_nh_hall.json | 1 | NHHall (Sean Costello) |
| sc3_vosim.json | 1 | VOSIM |

## sc3-plugins coverage

Of the 411 names registered across `sc3-plugins/source/*/*.cpp`, vibelang covers **351 (85 %)**. The 60 still-missing names are mostly:

- **Grain* family duplicates** (GrainBufJ, GrainFMJ, BufGrain, FMGrain, InGrain, MonoGrain, SinGrain, plus their `B`, `BBF`, `BF`, `I`, `IBF` variants) — JoshUGens grain families. ~30 entries; could collapse into a single `sc3_grains.json` pack if demand surfaces.
- **STK extras** (StkGlobals, StkInst, StkMesh2D, OteyPiano, OteyPianoStrings, OteySoundBoard, DWGBowedStk).
- **Ambisonic extras** (BFFreeVerb, DistanceB, NFC, ProximityB).
- **Misc** (LADSPA bridge, NovaDiskIn/Out, MultiFilt, CubicDelay, HermiteDelay, FLoopBuf, TALReverb, Xover2, Push, PMHPF/PMLPF, Dominate, Dbeta, AmplitudeMod, ATSSynth, AnalyseEvents2, ArneodoCoulletTresser, AudioMSG, AverageOutput, Betablocker, DetaBlocker01, PitchNoteUGen).

These are tracked separately from this audit; see follow-on tickets in the `vibelang-synthdef-catalog` epic.

## Non-canonical / DSL composites (60 names)

The remaining 60 vibelang entries that aren't in either canonical list split as:

- **Class-library / DSL composites** (16): `BHiPass4`, `BLowPass4`, `Changed`, `envelope`, `ExpExp`, `ExpLin`, `LinLin`, `Mix`, `PMOsc`, `SelectX`, `Silence`, `SoundIn`, `Splay`, `SplayAz`, `TWChoose`, `Tilt`/`Tumble`/`Rotate` (atk-foa transforms surfaced as primitives).
- **mi-UGens** (12): `MiBraids`, `MiClouds`, `MiElements`, `MiGrids`, `MiMu`, `MiOmi`, `MiPlaits`, `MiRings`, `MiRipples`, `MiTides`, `MiVerb`, `MiWarps` — Mutable Instruments ports, separate plugin pack.
- **JoshUGens-aliased**: `JPverb`, `JPverbRaw`, `Greyhole`, `GreyholeRaw`, `FaustGreyholeRaw` (different binding names than the sc3 source).
- **PV sub-namespace**: `PV_DiffMags`, `PV_MagExp`, `PV_MagLog`, `PV_MagMulAdd` (vibelang-only convenience PV ops).
- **Misc**: `AtsFile`, `CQ_Diff`, `FFTCentroid`/`FFTFlux`/`FFTFluxPos`/`FFTPower`/`FFTSubbandFlux`/`FFTDiffMags`, `MembraneCircle`, `MembraneHexagon`, `NLFiltC/L/N`, `OnsetsDS`, `PanX2D`, `PulseDPW`, `RMAFoodChainL`, `RosslerResL`, `Streson`, `VBAPSpeaker`, `VBAPSpeakerArray`.

These are all intentional surface area, not gaps.

## Appendix — raw lists

Regenerable in seconds; commands at the top of this file.

- canonical SC core list: `/tmp/sc-canonical-v2.txt` (400 names)
- canonical sc3-plugins list: `/tmp/sc3-canonical.txt` (411 names)
- vibelang covered: `/tmp/vibelang-covered-v3.txt` (787 names)
- set diffs: `/tmp/missing-v3.txt` (24 SC-core gaps), `/tmp/extras-v3.txt` (411 non-core), `/tmp/intersect-v3.txt` (376 covered), `/tmp/non-sc3-extras.txt` (60 community-pack/DSL names)
