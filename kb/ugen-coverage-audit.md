# UGen Coverage Audit (SC Core)

> Source-of-truth: `supercollider/server/plugins/*.cpp` at `main` (cloned `--depth=1` to `/tmp/sc-source`, 2026-04-28).
> Coverage scan: `crates/vibelang-dsp/ugen_manifests/*.json` (24 files).
>
> Method:
> 1. Canonical names extracted from three SC registration forms — `Define*Unit(Ident)`, `Define*Unit("Name", ...)`, and `registerUnit<...>(ft, "Name", ...)`. The bare-identifier macro form contributes the bulk of the names; the string-literal and template forms add `MouseX/Y/Button`, `DC`, `K2A`, `MulAdd`, `Sum3`, `Sum4`, and the three `Link*` UGens that earlier audits missed.
> 2. Vibelang names extracted with `jq -r '.[].name' ugen_manifests/*.json | sort -u`.
> 3. Set diffs computed via `comm`. Missing UGens grouped by their source `*UGens.cpp` file.

## Summary

| Metric | Count |
|---|---|
| **Canonical SC core UGens** (server/plugins/*.cpp) | **400** |
| **Currently covered** by vibelang manifests | **215** |
| **Missing** from vibelang | **185** |
| Vibelang manifests, total entries | 232 |
| Vibelang-only entries (not in SC core) | 17 |
| Misnamed / suspicious vibelang entries | 1 (`Triangle`) |
| Class-library / DSL composites in vibelang | 16 |

Notes on the canonical count:
- The brief's "~280 distinct UGens" estimate is low — registering all three macro forms across `server/plugins/*.cpp` yields **400**. The discrepancy is mostly hardware/diagnostic UGens (Bela, Demo, Test) and helper UGens (Control, AudioControl, BlockSize, NodeID, etc.) that aren't usually counted as "user-facing".
- The brief's "461 manifest entries" is the count of *function variants* (rate-suffixed methods like `sin_osc_ar` + `sin_osc_kr`); deduplicated by `name`, vibelang has **232** UGen entries.

Coverage stands at **215/400 = 54%** of the canonical SC core surface, but several missing entries are intentional/out-of-scope (see "Out-of-scope categories" below).

## Per-category gap (by source file)

The table groups every missing UGen under its `server/plugins/*.cpp` source file, so each gap maps directly to a vibelang manifest extension or a new manifest file.

### High-value gaps (recommended priority)

#### PV_UGens.cpp — phase-vocoder (canonical 31, covered 0, missing 31)
The entire FFT/spectral PV_ family is absent. `fft.json` only carries `FFT` and `IFFT`; the spectral mutator UGens that operate between them are all missing.
- ❌ PV_Add
- ❌ PV_BinScramble
- ❌ PV_BinShift
- ❌ PV_BinWipe
- ❌ PV_BrickWall
- ❌ PV_Conj
- ❌ PV_Copy
- ❌ PV_CopyPhase
- ❌ PV_Diffuser
- ❌ PV_Div
- ❌ PV_LocalMax
- ❌ PV_MagAbove
- ❌ PV_MagBelow
- ❌ PV_MagClip
- ❌ PV_MagDiv
- ❌ PV_MagFreeze
- ❌ PV_MagMul
- ❌ PV_MagNoise
- ❌ PV_MagShift
- ❌ PV_MagSmear
- ❌ PV_MagSquared
- ❌ PV_Max
- ❌ PV_Min
- ❌ PV_Mul
- ❌ PV_PhaseShift
- ❌ PV_PhaseShift90
- ❌ PV_PhaseShift270
- ❌ PV_RandComb
- ❌ PV_RandWipe
- ❌ PV_RectComb
- ❌ PV_RectComb2

#### PV_ThirdParty.cpp — ships-with-core PV (canonical 1, covered 0, missing 1)
- ❌ PV_ConformalMap

#### UnpackFFTUGens.cpp — FFT bin-level access (canonical 2, covered 0, missing 2)
- ❌ PackFFT
- ❌ Unpack1FFT

#### FFT_UGens.cpp — FFT helpers (canonical 3, covered 2, missing 1)
- ✅ FFT, IFFT
- ❌ FFTTrigger

#### ChaosUGens.cpp — chaotic generators (canonical 22, covered 0, missing 22)
The whole non-linear/strange-attractor family is missing; vibelang's `noise.json` covers Brownian/clip/dust noise but no chaos.
- ❌ CuspL, CuspN
- ❌ FBSineC, FBSineL, FBSineN
- ❌ GbmanL, GbmanN
- ❌ HenonC, HenonL, HenonN
- ❌ LatoocarfianC, LatoocarfianL, LatoocarfianN
- ❌ LinCongC, LinCongL, LinCongN
- ❌ LorenzL
- ❌ QuadC, QuadL, QuadN
- ❌ StandardL, StandardN

#### DemandUGens.cpp — demand-rate streams (canonical 25, covered 10, missing 15)
Dseq/Dser/Drand/Dwhite/Dxrand/Dgeom/Dseries/Dbrown/Demand/TDuty are present. The rest are missing — including the demand-buffer ops, the demand-form envelope generator, switching/concat/duty UGens, and `Dpoll`.
- ✅ Dbrown, Demand, Dgeom, Drand, Dseq, Dser, Dseries, Dwhite, Dxrand, TDuty
- ❌ Dbufrd, Dbufwr
- ❌ Dconst, Ddup
- ❌ DemandEnvGen
- ❌ Dibrown, Diwhite (integer demand noise)
- ❌ Dpoll, Dreset
- ❌ Dshuf, Dstutter
- ❌ Dswitch, Dswitch1
- ❌ Duty
- ❌ Dwrand

#### DelayUGens.cpp — delays / buffer plumbing (canonical 56, covered 43, missing 13)
The bulk of buffer/delay UGens are present; the gaps are in scope plumbing, tap-style delays, and local-buf creation.
- ❌ BlockSize *(plumbing — usually unnecessary in user code)*
- ❌ ClearBuf, SetBuf
- ❌ DelTapRd, DelTapWr (de-coupled delay tap read/write — useful for multi-tap effects)
- ❌ GrainTap
- ❌ LocalBuf, MaxLocalBufs *(synthdef-internal buffer allocation)*
- ❌ NodeID *(plumbing)*
- ❌ ScopeOut, ScopeOut2 (server-side scope output)
- ❌ SimpleLoopBuf
- ❌ SubsampleOffset *(plumbing)*

#### TriggerUGens.cpp — triggers / lifecycle (canonical 33, covered 21, missing 12)
Most trigger UGens covered (Trig, TrigControl, Latch, Schmidt, Stepper, ToggleFF, Sweep, etc.). Gaps are in node-lifecycle and value-tracking UGens.
- ❌ Free, FreeSelf, FreeSelfWhenDone
- ❌ Pause, PauseSelf, PauseSelfWhenDone
- ❌ TDelay
- ❌ Poll *(diagnostic)*
- ❌ SendPeakRMS
- ❌ LastValue
- ❌ LeastChange, MostChange

#### FilterUGens.cpp — filters (canonical 57, covered 43, missing 14)
Big filter library mostly present (BPF/HPF/LPF/RLPF/RHPF/Resonz/Klank/Ringz/Median/Lag*/MoogFF/Decay/Integrator/etc.). Missing: a few exotic filters and the zero-pole DC/Nyquist family.
- ❌ BAllPass *(Butterworth all-pass — sibling of BBandPass etc.)*
- ❌ BPZ2, BRZ2 *(2-pole zero filters)*
- ❌ HPZ1, HPZ2, LPZ1, LPZ2 *(zero-only DC/Nyquist filters)*
- ❌ DetectSilence *(useful as auto-free trigger)*
- ❌ Flip *(filter)*
- ❌ FreqShift *(SSB freq-shifter — common in modulation FX)*
- ❌ Hilbert *(Hilbert transform — sin/cos pair)*
- ❌ Lag2UD, Lag3UD *(asymmetric up/down lag — counterpart to LagUD which IS covered)*
- ❌ MidEQ *(parametric mid-band EQ)*

#### OscUGens.cpp — oscillators / table lookup (canonical 25, covered 13, missing 12)
The standard band-limited oscillators are present; the wavetable lookup family and degree-mapping UGens are missing.
- ❌ Osc, OscN *(arbitrary-buffer wavetable — `Osc.ar(buf, freq, phase)`)*
- ❌ Shaper *(waveshaping via buffer lookup)*
- ❌ Index, IndexL, IndexInBetween
- ❌ DegreeToKey *(degree → MIDI mapping via scale buffer)*
- ❌ DetectIndex
- ❌ FoldIndex, WrapIndex
- ❌ TWindex *(triggered weighted index)*
- ❌ PSinGrain *(sine grain)*

#### Convolution.cpp + PartitionedConvolution.cpp — convolution (canonical 6, covered 0, missing 6)
Convolution is entirely absent. Significant for IR-based reverb, cab simulation, and time-domain FFT-mediated effects.
- ❌ Convolution
- ❌ Convolution2
- ❌ Convolution2L
- ❌ Convolution3
- ❌ StereoConvolution2L
- ❌ PartConv (PartitionedConvolution.cpp)

#### DiskIO_UGens.cpp — disk streaming (canonical 3, covered 0, missing 3)
- ❌ DiskIn (streaming-from-disk playback)
- ❌ DiskOut (streaming-to-disk recording)
- ❌ VDiskIn (variable-rate streaming playback)

#### ML.cpp / ML_SpecStats.cpp — feature detection (canonical 12, covered 0, missing 9 unique)
None of the ML feature-detection UGens are exposed. `analysis.json` covers `Amplitude`, `RunningSum`, etc., but the heavy ML units are absent.
- ❌ BeatTrack, BeatTrack2
- ❌ KeyTrack
- ❌ Loudness
- ❌ MFCC
- ❌ Onsets
- ❌ SpecCentroid, SpecFlatness, SpecPcile *(also defined in `ML_SpecStats.cpp` — same name, single binary registration)*

#### LFUGens.cpp — LFOs / lag / shape (canonical 30, covered 23, missing 7)
Vast majority of LF oscillators and shaping helpers are covered. Gaps:
- ❌ LFCub *(cubic-interpolated low-freq oscillator)*
- ❌ Vibrato *(builtin vibrato wrapper — uses `LFNoise2` + sine)*
- ❌ AmpComp, AmpCompA *(equal-loudness amplitude compensation curves)*
- ❌ ModDif *(modular difference between two values)*
- ❌ Unwrap *(phase-unwrap helper)*
- ❌ IEnvGen *(immediate envelope generator — i-rate variant of EnvGen)*

#### IOUGens.cpp — bus I/O / control (canonical 16, covered 9, missing 7)
Audio I/O (`In`, `Out`, `InFeedback`, `XOut`, `ReplaceOut`, `OffsetOut`, `LocalIn`, `LocalOut`, `SoundIn`*) is covered. Missing UGens are mostly synth-control plumbing usually generated by the synthdef compiler, so coverage is largely a UX choice.
- ❌ Control, AudioControl *(synthdef compiler emits these for declared parameters)*
- ❌ LagControl, LagIn *(lagged control input — useful for smooth parameter automation)*
- ❌ InTrig (trigger-mode bus read — useful for accumulator patterns)
- ❌ SharedIn, SharedOut *(IPC bus access — niche)*

#### NoiseUGens.cpp — noise / RNG (canonical 26, covered 20, missing 6)
Standard noise (WhiteNoise, PinkNoise, BrownNoise, GrayNoise, ClipNoise, Crackle, Dust, Dust2, LFNoise0/1/2, LFClipNoise, TRand, IRand, TIRand, TWindex *(missing — see OscUGens)*) covered. Missing:
- ❌ ExpRand, NRand *(scalar/i-rate random generators)*
- ❌ Logistic *(logistic-map noise)*
- ❌ MantissaMask *(bit-level audio degrader)*
- ❌ RandID, RandSeed *(per-node RNG state plumbing)*

### Smaller gaps

#### LinkUGen.cpp — Ableton Link sync (canonical 3, covered 0, missing 3)
Vibelang has zero coverage of Ableton Link, which exposes tempo/phase from a connected Link mesh.
- ❌ LinkTempo
- ❌ LinkPhase
- ❌ LinkJump

#### MulAddUGens.cpp — fused arithmetic (canonical 3, covered 1, missing 2)
- ✅ MulAdd
- ❌ Sum3, Sum4 *(fused 3- and 4-input adder — cheaper than chaining `+`)*

#### PanUGens.cpp — panning (canonical 12, covered 8, missing 4)
- ❌ PanB, PanB2 *(B-format Ambisonic panners)*
- ❌ BiPanB2, DecodeB2 *(B-format 2D ↔ stereo decoders)*

#### DynNoiseUGens.cpp — dynamic noise (canonical 4, covered 3, missing 1)
- ❌ LFDClipNoise *(dynamic-rate clipped noise)*

#### TestUGens.cpp — diagnostics (canonical 2, covered 0, missing 2)
Diagnostic-only — low priority.
- ❌ CheckBadValues *(NaN/inf detector)*
- ❌ Sanitize *(replace bad samples)*

#### FeatureDetection.cpp — onset detection helpers (canonical 3, covered 1, missing 2)
- ✅ RunningSum
- ❌ PV_HainsworthFoote *(spectral onset detector — Hainsworth/Foote)*
- ❌ PV_JensenAndersen *(spectral onset detector — Jensen/Andersen)*

### Out-of-scope categories (intentional skips)

These are the 11 missing UGens we shouldn't bother chasing — they're hardware-specific, internal, or already handled by the language layer.

#### BelaUGens.cpp — Bela hardware bridge (canonical 7, covered 0, missing 7)
Compile-time conditional (`SC_BELA`) — only available on the Bela platform. Out of scope unless vibelang adds Bela targeting.
- ⏭ AnalogIn, AnalogOut, BelaScopeOut, DigitalIn, DigitalIO, DigitalOut, MultiplexAnalogIn

#### BinaryOpUGens.cpp / UnaryOpUGens.cpp — operator wrappers (canonical 2, covered 0, missing 2)
- ⏭ BinaryOpUGen — exposed via Rust `+ - * /` operator overloads
- ⏭ UnaryOpUGen — exposed via the math/conversion manifests (`abs`, `neg`, `reciprocal`, `sin`, `cos`, etc.)

#### DemoUGens.cpp — example UGen (canonical 1, covered 0, missing 1)
- ⏭ UnitCmdDemo — server-test artifact, not a real UGen.

### Already complete (no action)

| Source file | Canonical | Covered | Status |
|---|---|---|---|
| GendynUGens.cpp | 3 | 3 | ✅ all present |
| GrainUGens.cpp | 5 | 5 | ✅ all present |
| PhysicalModelingUGens.cpp | 3 | 3 | ✅ all present |
| ReverbUGens.cpp | 3 | 3 | ✅ all present |
| UIUGens.cpp | 4 | 4 | ✅ all present (KeyState, MouseX, MouseY, MouseButton) |

## Vibelang-only entries

17 UGen names appear in vibelang manifests but not in SC core. Most are intentional class-library composites or DSL constructs — only one is a clear misname.

### Misnamed / non-canonical

- **`Triangle`** (`oscillators.json`) — there is no `Triangle` UGen in `server/plugins/`. SC's triangle wave is `LFTri` (low-frequency, non band-limited) at the server level; band-limited triangle is built via additive synthesis or `Blip`. The vibelang manifest declares `[ar, kr]` rates and `freq, phase` inputs, which matches `LFTri`'s signature exactly. **Recommendation: rename to `LFTri` (already covered separately) and remove `Triangle`, OR document `Triangle` explicitly as a vibelang alias for `LFTri`.**

### Class-library / DSL composites (intentional, not UGens)

These are sclang `*UGen` class methods or vibelang DSL constructs that compile down to one or more underlying UGens. They are *correctly* in the manifests (vibelang surfaces them as user-facing primitives) but they have no 1:1 server-side counterpart.

| Vibelang name | What it really is | Compiles to |
|---|---|---|
| `BHiPass4`, `BLowPass4` | sclang composite — 4th-order Butterworth via two cascaded 2nd-order sections | 2× `BHiPass` / `BLowPass` |
| `Changed` | class-library trigger helper | `HPZ1.ar(in).abs > thresh` |
| `envelope` | vibelang DSL fluent envelope builder (ADSR/ASR/perc) | `EnvGen.kr(Env(...))` |
| `ExpExp`, `ExpLin`, `LinLin` | sclang range-mapping methods | `*` + `+` ops |
| `JPverb` | **sc3-plugins** UGen (JoshUGens) — *not* SC core | external plugin |
| `Mix` | class-library — sums an array of channels | repeated `+` |
| `PMOsc` | class-library phase-modulation oscillator helper | 2× `SinOsc` |
| `SelectX` | class-library cross-faded selector | `LinXFade2` chain |
| `Silence` | class-library — constant zero output | `DC.ar(0)` |
| `SoundIn` | class-library — `In.ar` with hardware-input offset added | `In.ar(NumOutputBuses + ch)` |
| `Splay`, `SplayAz` | class-library — spread mono array across stereo / multi-channel field | `Pan2` / `PanAz` chain |
| `TWChoose` | class-library triggered weighted-choose | `Select` + `TWindex` |

Note: `JPverb` is from **sc3-plugins**, not SC core. It belongs in a separate community-plugin coverage track (a different audit).

## Suggested follow-on tickets

Each of the high-value gap groups maps cleanly to a manifest extension or new manifest file. Suggested tickets in rough priority order:

1. **`fft.json` expansion — PV_ family** (32 UGens: PV_UGens.cpp + PV_ThirdParty.cpp + UnpackFFTUGens.cpp + FFTTrigger). Single largest gap.
2. **New `chaos.json` manifest** (22 UGens: ChaosUGens.cpp). Self-contained category.
3. **`demand.json` expansion** (15 UGens: DemandUGens.cpp gaps).
4. **New `convolution.json` manifest** (6 UGens: Convolution.cpp + PartitionedConvolution.cpp).
5. **New `disk.json` manifest** (3 UGens: DiskIO_UGens.cpp).
6. **`analysis.json` expansion — ML feature detection** (9 UGens: ML.cpp). Decide if heavy DSP UGens like BeatTrack/MFCC are in vibelang's scope.
7. **`triggers.json` expansion — node lifecycle** (12 UGens: Free*, Pause*, TDelay, Poll, SendPeakRMS, LastValue, LeastChange, MostChange).
8. **`filters.json` expansion** (14 UGens: BAllPass, zero-pole filters, Hilbert, FreqShift, MidEQ, asymmetric Lag*UD).
9. **`oscillators.json` expansion — wavetable / index** (12 UGens: Osc, OscN, Shaper, Index family, DegreeToKey, TWindex, PSinGrain).
10. **`delays.json` expansion — taps & buffer plumbing** (13 UGens: DelTapRd/Wr, GrainTap, ScopeOut, ClearBuf/SetBuf, etc.).
11. **`pan.json` expansion — Ambisonic B-format** (4 UGens: PanB, PanB2, BiPanB2, DecodeB2).
12. **`oscillators.json`/`noise.json` cleanup** — rename `Triangle` to `LFTri` alias or remove.
13. **New `link.json` manifest** (3 UGens: Ableton Link).
14. **`noise.json` expansion** (6 UGens).
15. **`io.json` expansion** (7 UGens) — decide which control plumbing is worth surfacing.
16. **`math.json` expansion** (Sum3, Sum4).

A separate audit should track sc3-plugins (where `JPverb` came from) and other community plugin sets (sc3-plugins, MI-UGens, JoshUGens, BatLib, etc.) — that work is out of scope here.

## Appendix — raw lists

The data files used to produce this audit (regenerable in seconds — see Method at the top):

- canonical SC list: `/tmp/sc-canonical-v2.txt` (400 names)
- canonical with file mapping: `/tmp/sc-ugens-by-file3.tsv` (403 rows; 3 dupes from ML.cpp ↔ ML_SpecStats.cpp)
- vibelang covered: `/tmp/vibelang-covered.txt` (232 names)
- diff outputs: `/tmp/missing-v2.txt` (185), `/tmp/extras-v2.txt` (17), `/tmp/intersect-v2.txt` (215)
