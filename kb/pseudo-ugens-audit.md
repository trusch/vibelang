# Pseudo-UGen Audit

Date: 2026-05-15

Ticket: `research-audit-pseudo-ugens-in-vibelang-manifest-sclang-only-constructs`

## Scope

This audit checks every unique `name` in `crates/vibelang-dsp/ugen_manifests/*.json`
against the scsynth units registered by the installed SuperCollider plugins in
`/usr/lib/SuperCollider/plugins/*.so`.

The goal is to identify manifest entries that vibelang must not emit as literal
GraphDef UGen names because they are sclang-side constructors, aliases, helpers,
or wrappers that sclang normally lowers before sending a SynthDef to scsynth.

## Method

- Extracted manifest names:

  ```sh
  jq -r '.[].name' crates/vibelang-dsp/ugen_manifests/*.json | sort -u
  ```

- Enumerated binary UGen registrations by loading each non-supernova plugin with
  a fake `InterfaceTable` and recording calls to `fDefineUnit`. This is more
  accurate than `strings` because several real core UGens are not present as
  exact standalone strings in stripped shared objects.
- Diffed manifest names against the registered binary unit names.
- Checked each diff candidate against:
  - `/usr/share/SuperCollider/SCClassLibrary/`
  - `/usr/share/SuperCollider/Extensions/SC3plugins/`

One plugin, `UIUGens.so`, aborts on process teardown in the fake loader, but it
flushes its `fDefineUnit` calls before aborting. None of the final missing names
depends on a UI UGen registration.

## Totals

| Item | Count |
|---|---:|
| Unique manifest names | 875 |
| Registered scsynth binary unit names | 901 |
| Manifest names with no registered binary unit | 84 |
| Confirmed sclang-side pseudo/alias/helper names | 44 |
| Missing for non-pseudo or unresolved reasons | 40 |

## Confirmed Pseudo-UGens And Lowerings

These entries are not valid server UGen names. sclang lowers them to other UGens
or to operator UGens before GraphDef emission.

| Manifest name | Manifest file | Source | sclang lowering |
|---|---|---|---|
| `AMClip`, `AbsDif`, `Atan2`, `Clip2`, `DifSqr`, `Excess`, `FirstArg`, `Fold2`, `Hypot`, `HypotApx`, `Ring1`, `Ring2`, `Ring3`, `Ring4`, `ScaleNeg`, `SqrDif`, `SqrSum`, `SumSqr`, `Thresh`, `Wrap2` | `math.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/BasicOpsUGen.sc`; operator methods in `Common/Math/SimpleNumber.sc`, `Common/Math/Signal.sc`, `Common/Core/AbstractFunction.sc`, `Common/Collections/SequenceableCollection.sc` | Lower to `BinaryOpUGen(selector, a, b)` with selectors `amclip`, `absdif`, `atan2`, `clip2`, `difsqr`, `excess`, `firstArg`, `fold2`, `hypot`, `hypotApx`, `ring1`, `ring2`, `ring3`, `ring4`, `scaleneg`, `sqrdif`, `sqrsum`, `sumsqr`, `thresh`, `wrap2`. |
| `LinLin` | `math.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Line.sc`; `Common/Audio/UGen.sc` | `ar`: `MulAdd(in, (dsthi - dstlo) / (srchi - srclo), dstlo - scale * srclo)`. `kr`: `(in * scale + offset)`. |
| `ExpLin` | `math.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/UGen.sc` | There is no `ExpLin` class. UGen method `explin` lowers to `log(in / inMin) / log(inMax / inMin) * (outMax - outMin) + outMin`. |
| `ExpExp` | `math.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/UGen.sc` | There is no `ExpExp` class. UGen method `expexp` lowers to `pow(outMax / outMin, log(in / inMin) / log(inMax / inMin)) * outMin`. |
| `Changed` | `triggers.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Filter.sc` | `kr`: `HPZ1.kr(input).abs > threshold`; `ar`: `HPZ1.ar(input).abs > threshold`. |
| `BLowPass4` | `filters.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/BEQSuite.sc` | Explicitly marked "pseudo UGens"; lowers to two cascaded `SOS.ar` filters using `BLowPass.sc` coefficients. |
| `BHiPass4` | `filters.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/BEQSuite.sc` | Explicitly marked "pseudo UGens"; lowers to two cascaded `SOS.ar` filters using `BHiPass.sc` coefficients. |
| `FFTCentroid` | `sc3_mcld.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDFFTUGens.sc` | Deprecated alias: `SpecCentroid.kr(buffer)`. |
| `PV_DiffMags` | `sc3_mcld.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDFFTUGens.sc` | Deprecated alias: `PV_MagSubtract(bufferA, bufferB)`. |
| `Greyhole` | `sc3_deind.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/DEINDUGens/Greyhole.sc` | Wrapper around `GreyholeRaw.ar(in.first, in.last, damp, delayTime, diff, feedback, modDepth, modFreq, size)`. |
| `JPverb` | `sc3_deind.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/DEINDUGens/JPverb.sc` | Wrapper around `JPverbRaw.ar(in.first, in.last, damp, earlyDiff, highcut, high, lowcut, low, modDepth, modFreq, mid, size, t60)`. |
| `Mix` | `multichannel.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Mix.sc` | Collection helper. Lowers arrays to `Sum4`, `Sum3`, and binary sums recursively, with `K2A`, `A2K`, or `DC` coercion in `ar`/`kr` wrappers. |
| `PMOsc` | `oscillators.json` | `/usr/share/SuperCollider/SCClassLibrary/backwards_compatibility/PMOsc.sc` | SC2 compatibility pseudo UGen: `SinOsc.ar(carfreq, SinOsc.ar(modfreq, modphase, pmindex), mul, add)` or `kr` equivalent. |
| `OnsetsDS` | `sc3_mcld.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/OnsetsDS.sc` | Source comment calls it a pseudo-UGen. Lowers to `FFT`, `PV_Whiten`, one of `FFTComplexDev`/`FFTPhaseDev`/`FFTMKL`/`FFTPower`, `MedianTriggered`, comparisons, and `Trig1`. Deprecated in favor of `Onsets.kr`. |
| `PanX2D` | `sc3_josh_spectral.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/JoshUGens/classes/PanX.sc` | Lowers to nested `PanX`: pan over Y, then pan the result over X. |
| `PulseDPW` | `sc3_mcld.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDOscUGens.sc` | Lowers to `SawDPW(rate, freq, 0) - SawDPW(rate, freq, (width + width).wrap(-1, 1))`, then `madd`. |
| `Rotate` | `sc3_josh_spectral.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/JoshUGens/classes/Ambisonics.sc` | Ambisonic helper. Lowers to `Rotate2.ar(x, y, rotate * -0.31830988618379)` and returns `[w, xout, yout, z]`. |
| `Tilt` | `sc3_josh_spectral.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/JoshUGens/classes/Ambisonics.sc` | Ambisonic helper. Lowers to `Rotate2.ar(x, z, tilt * -0.31830988618379)` and returns `[w, xout, y, zout]`. |
| `Tumble` | `sc3_josh_spectral.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/JoshUGens/classes/Ambisonics.sc` | Ambisonic helper. Lowers to `Rotate2.ar(y, z, tilt * -0.31830988618379)` and returns `[w, x, yout, zout]`. |
| `SelectX` | `control.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Osc.sc` | Crossfade helper. Lowers to `XFade2` over `Select(which.round(2), array)`, `Select(which.trunc(2) + 1, array)`, and `(which * 2 - 1).fold2(1)`. |
| `SoundIn` | `inout.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/SoundIn.sc` | Input helper. Lowers to `In.ar(NumOutputBuses.ir + bus, channels)` plus `madd`. |
| `Splay` | `multichannel.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Splay.sc` | Spatialization helper. Lowers to `Mix(Pan2.ar/kr(inArray, positions)) * level`. |
| `SplayAz` | `panning.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Splay.sc` | Spatialization helper. Lowers to `PanAz.ar/kr(...).flop.collect(Mix(_))`. |
| `TWChoose` | `random.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Osc.sc` | Weighted choose helper. Lowers to `Select.ar/kr(TWindex.ar/kr(trig, weights, normalize), array)`. |
| `Silence` | `conversion.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/Line.sc`; `Common/Audio/UGen.sc` | Upstream class is `Silent`, not `Silence`; `Silent.ar(numChannels)` lowers to `DC.ar(0)` duplicated per channel. Treat manifest `Silence` as a non-binary alias/mismatch. |

## Missing But Not Confirmed As Pseudo-UGens

These names are also absent from the installed scsynth binary registry, but they
are not confirmed sclang pseudo-UGens with a clear lowering in the installed
sources. Do not emit them as literal GraphDef UGen names either; they need
separate manifest cleanup or plugin/source verification.

| Manifest name | Manifest file | Installed source status | Classification |
|---|---|---|---|
| `AtsFile` | `sc3_josh_spectral.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/JoshUGens/classes/AtsFile.sc` defines `AtsFile : File`. | Data/helper class, not a UGen. |
| `BigArity24` | `_test_arity_stub.json` | No upstream SC source. | Local test stub, not a real UGen. |
| `CQ_Diff` | `sc3_mcld.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLD_CQ_UGens.sc` defines `CQ_Diff : MultiOutUGen` and calls `multiNew`, but no installed binary registers `CQ_Diff`. | Suspected stale or missing binary UGen. |
| `FFTSubbandFlux` | `sc3_mcld.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDFFTUGens.sc` defines `FFTSubbandFlux : MultiOutUGen` and calls `multiNew`, but no installed binary registers `FFTSubbandFlux`. | Suspected stale or missing binary UGen. |
| `MIDelay` | `sc3_mcld.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDBufferUGens.sc` defines `MIDelay : UGen` and calls `multiNew`, but no installed binary registers `MIDelay`. | Suspected stale or missing binary UGen. |
| `RMAFoodChainL` | `sc3_mcld.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDChaosUGens.sc` defines `RMAFoodChainL : MultiOutUGen` and calls `multiNew`, but no installed binary registers `RMAFoodChainL`. | Suspected stale or missing binary UGen. |
| `RosslerResL` | `sc3_mcld.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/MCLDUGens/MCLDChaosUGens.sc` defines `RosslerResL : MCLDChaosGen` and calls `multiNew`, but no installed binary registers `RosslerResL`. | Suspected stale or missing binary UGen. |
| `SimpleLoopBuf` | `buffers.json` | `/usr/share/SuperCollider/SCClassLibrary/Common/Audio/BufIO.sc` contains the class only inside a comment that says `exception in GrafDef_Load: UGen 'SimpleLoopBuf' not installed`. | Known removed/commented-out missing UGen. |
| `FaustGreyholeRaw` | `sc3_deind.json` | No installed class source and no binary registration. Installed DEIND binaries expose `GreyholeRaw` and `JPverbRaw`, not `FaustGreyholeRaw`. | Suspected stale manifest alias. |
| `HOAEncLebedev061` | `sc_hoa.json` | No installed class source and no binary registration. Installed HOA binaries include `HOADecLebedev061` and `HOAPanLebedev061`; encoders start at `HOAEncLebedev501`. | Suspected stale or wrong SC-HOA name. |
| `HOALibEnc3D1`, `HOALibEnc3D2`, `HOALibEnc3D3`, `HOALibEnc3D4`, `HOALibEnc3D5` | `sc_hoa.json` | No installed class source and no binary registrations. | Suspected non-installed HOALibrary wrapper family, manual verification needed. |
| `HOAmbiPanner1`, `HOAmbiPanner2`, `HOAmbiPanner3`, `HOAmbiPanner4`, `HOAmbiPanner5` | `sc_hoa.json` | No installed class source and no binary registrations. | Suspected non-installed HOALibrary wrapper family, manual verification needed. |
| `ITU5001`, `ITU5002` | `sc_hoa.json` | No installed class source and no binary registrations. | Suspected sclang decoder helper or stale SC-HOA wrapper, manual verification needed. |
| `LinkTempo`, `LinkPhase`, `LinkJump` | `link.json` | No installed class source and no binary registrations. | Requires separate Ableton Link plugin/source verification; absent on this install. |
| `MiBraids`, `MiClouds`, `MiElements`, `MiGrids`, `MiMu`, `MiOmi`, `MiPlaits`, `MiRings`, `MiRipples`, `MiTides`, `MiVerb`, `MiWarps` | `mi_ugens.json` | No installed class source and no `Mi*.so` plugin binaries on this system. | Missing Mutable Instruments plugin family, not pseudo-confirmed from installed sources. |
| `VBAPSpeaker`, `VBAPSpeakerArray` | `sc3_vbap.json` | `/usr/share/SuperCollider/Extensions/SC3plugins/VBAPUGens/vbap.sc` defines speaker configuration classes, not UGens. | Data/helper classes, not server UGens. |
| `envelope` | `envelopes.json` | No upstream SC UGen class. Manifest description says it is a fluent envelope builder. | vibelang DSL helper, not a server UGen. |

## Notes

- The runtime failure for `Changed` is representative of the confirmed pseudo
  list: if vibelang emits these names as GraphDef UGen names, scsynth rejects
  the SynthDef with `UGen '<name>' not installed`.
- The `math.json` operator names are especially risky because the server does
  have `BinaryOpUGen`, not individual `Ring1`, `Clip2`, `Atan2`, etc. names.
  Correct codegen should emit `BinaryOpUGen` with the selector/special index or
  lower to equivalent supported graph operations.
- `Greyhole` and `JPverb` are convenient wrappers. Their raw forms,
  `GreyholeRaw` and `JPverbRaw`, are registered binary UGens on this install.
- `Silence` is not an upstream class name. Upstream has `Silent`, which is
  itself a helper around `DC.ar(0)`.
- Several absent names appear to be manifest coverage for plugins not installed
  on this host (`Mi*`, Link, some SC-HOA wrappers), stale aliases, or local DSL
  helpers. They are still unsafe as literal server UGen names, but they are not
  confirmed sclang pseudo-UGens from the installed upstream sources.

