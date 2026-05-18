# ReSynthesizer implementation status

This note summarizes the current VibeLang stdlib implementation of the Make
Noise ReSynthesizer-inspired module set. It is a short status document; the
longer behavior and source rationale live in
[`resynthesizer-module-behavior-matrix.md`](resynthesizer-module-behavior-matrix.md)
and [`make-noise-resynthesizer-manual-sources.md`](make-noise-resynthesizer-manual-sources.md).

## Implemented stdlib modules

| Area | Modules | Status |
|---|---|---|
| Spectral/time-domain core | `spectraphon`, `morphagene` | Landed as practical audio instruments with named outputs, named SAM/SOS inputs, and buffer-backed workflows where needed. |
| Control and modulation | `maths`, `wogglebug`, `tempi`, `rene`, `prss_pnt`, `cv_bus` | Landed as control-rate voices with patchable params and kr output ports for `.to_param(...)` / `.modulate_by(...)` workflows. |
| Stereo processors | `x_pan`, `qpas`, `dxg`, `mimeophon` | Landed as named-input processors for patching stereo signal paths around the core voices. |

## Fidelity boundary

The current goal is patch-semantic coverage: useful signal roles, named routing
surfaces, and stable imports. The stdlib does not claim exact Make Noise panel,
firmware, storage, analog VCA/vactrol, Select Bus, capacitive-touch, or hidden
mode fidelity. Module-specific caveats are listed in the ReSynthesizer
catalogue in
[`../crates/vibelang-std/stdlib/README.md`](../crates/vibelang-std/stdlib/README.md#approximation-caveats).

SAM analysis and Morphagene SOS recording now read declared named inputs
(`analyze`, `sos`) instead of hardwired hardware input 0. Unpatched named
inputs are silent, so legacy scripts that depended on implicit hardware input 0
need an explicit input route.

## Reference imports

| Module | Import path |
|---|---|
| Spectraphon | `stdlib/instruments/spectral/spectraphon.vibe` |
| Morphagene | `stdlib/instruments/sampler/morphagene.vibe` |
| MATHS | `stdlib/instruments/eurorack/maths.vibe` |
| Wogglebug | `stdlib/instruments/eurorack/wogglebug.vibe` |
| TEMPI | `stdlib/instruments/eurorack/tempi.vibe` |
| Rene | `stdlib/instruments/eurorack/rene.vibe` |
| PrssPnt | `stdlib/instruments/eurorack/prss_pnt.vibe` |
| CV Bus | `stdlib/instruments/eurorack/cv_bus.vibe` |
| X-PAN | `stdlib/processors/mixers/x_pan.vibe` |
| QPAS | `stdlib/processors/filters/qpas.vibe` |
| DXG | `stdlib/processors/dynamics/dxg.vibe` |
| Mimeophon | `stdlib/processors/delays/mimeophon.vibe` |
