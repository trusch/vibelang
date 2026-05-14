# Mutable Instruments Module Behavior Matrix

Research date: 2026-05-14.

This matrix turns the target Mutable Instruments lineup into VibeLang stdlib
work. It is firmware-grounded where official source exists in
`pichenettes/eurorack`; Beads is manual-grounded because no official Beads
firmware directory is present in that repository.

Decision labels:

| Label | Meaning |
|---|---|
| Extend | Keep existing stdlib or UGen foundation and add wrapper, ports, params, presets, or fidelity gaps. |
| Rebuild | Existing coverage is too generic or absent; implement a first-class module surface. |
| Manual-grounded | No official firmware source is available for the target module; use manuals and adjacent open firmware only as a behavioral reference. |

## Current VibeLang Audit

Dedicated or near-dedicated coverage already present:

- `crates/vibelang-std/stdlib/instruments/eurorack/marbles.vibe`: six-port
  Marbles-style voice with `t1/t2/t3` trigger outs and `x1/x2/x3` pitch CV outs.
- `crates/vibelang-std/stdlib/cv/eurorack/cv_marbles.vibe`: legacy single-output
  Marbles-style V/oct generator.
- `crates/vibelang-std/stdlib/cv/eurorack/cv_stages.vibe`: single-output
  six-step Stages-style sequencer.
- `crates/vibelang-std/stdlib/cv/eurorack/cv_tides.vibe`: single-output
  Tides-style morphable LFO/function generator.
- `crates/vibelang-std/stdlib/effects/granular/clouds_processor.vibe`: stereo
  `mi_clouds_ar` wrapper fixed to Clouds granular mode.
- `crates/vibelang-dsp/ugen_manifests/mi_ugens.json`: direct UGens for
  `MiPlaits`, `MiRings`, `MiClouds`, and `MiTides`.

Generic related coverage:

- `effects/filters/resonator.vibe`, `leads/pluck/*`, `bass/pluck/*`, `bells/*`,
  and `world/hang_drum.vibe` overlap with Rings/Plaits modal territory.
- `effects/granular/*` overlaps with Clouds/Beads roles, but not the specific
  module controls.
- No first-class stdlib files were found for Plaits, Rings, Beads, Frames,
  Clouds-as-voice, Tides multi-output, or Tides2 multi-output wrappers.

## System Idioms To Preserve

| Idiom | Firmware/manual basis | VibeLang implication |
|---|---|---|
| Named alternate outputs | Plaits has OUT plus AUX; Rings produces out/aux resonator streams; Beads and Clouds are stereo; Marbles, Stages, Tides, and Frames are multi-jack CV modules. | Use named ports rather than implicit stereo: `out`, `aux`, `left`, `right`, `odd`, `even`, `t1`, `x1`, etc. |
| Internal modulation only when jacks are absent | Plaits' trigger/envelope/LPG behavior and Beads' attenurandomizers change meaning depending on patching. | Model with explicit params first; add ergonomic wrapper presets for internal-env/randomized modes. |
| Source-specific output relationships | Tides output modes, Frames keyframe interpolation, Marbles related T/X streams, and Rings odd/even-ish resonator split are the module identity. | Do not collapse to one output unless maintaining old single-output compatibility. |
| Firmware source is strongest for DSP shape, not API naming | Firmware exposes classes such as `Voice`, `GranularProcessor`, `TGenerator`, `XYGenerator`, `SegmentGenerator`, `PolySlopeGenerator`, and `Keyframer`. | Use those classes to define DSP signatures, then expose VibeLang-native parameter names and routeable ports. |

## Module Matrix

| Module | Role | Controls to expose | I/O to model | Firmware-grounded DSP signature | Existing stdlib audit | Decision | Priority |
|---|---|---|---|---|---|---|---|
| Plaits | Macro oscillator voice with internal LPG and per-model AUX side output. | `pitch`/`freq`, `engine`, `harmonics`, `timbre`, `morph`, `trigger`, `level`, `fm_mod`, `timbre_mod`, `morph_mod`, `decay`, `lpg_colour`, `gain`. | Inputs: trigger, pitch, model CV-style params. Outputs: `out`, `aux`. | Firmware `plaits/dsp/voice.h` declares `Patch` with note/harmonics/timbre/morph/engine/decay/LPG colour and `Modulations`; `Voice::Frame` has `out` and `aux`. `plaits/dsp/voice.cc` registers 24 engines in current source: 8 original pitched, 8 original noise/percussion, plus 8 v1.2 engines (`virtual_analog_vcf`, `phase_distortion`, three `six_op` banks, `wave_terrain`, `string_machine`, `chiptune`). The module manual documents the original 16-panel model layout and OUT/AUX behavior. | `MiPlaits` UGen exists with 24 engines and 2 outputs, but no stdlib wrapper file was found. Generic oscillator/pluck/bell coverage is not enough. | Extend via wrapper around `mi_plaits_ar`; no custom DSP rebuild. Add `plaits` with `out`/`aux` ports and preset helpers for the original 16-model panel plus optional v1.2 engines. | P1 |
| Rings | Modal resonator / sympathetic string processor and exciter. | `pit`, `structure`, `brightness`, `damping`, `position`, `model`, `polyphony`, `internal_exciter`, `bypass`, `gain`. | Input: excitation audio plus trigger. Outputs: `odd`, `even`, and optionally `sum`/`out` compatibility. | Firmware `rings/dsp/part.h` defines resonator models: modal, sympathetic string, string, FM voice, quantized sympathetic string, string+reverb. `rings/dsp/resonator.h` uses up to 64 modal filters with frequency, structure, brightness, damping, position, and resolution. `MiRings` exposes two audio outputs. | `MiRings` UGen exists. Generic `effects/filters/resonator.vibe` and pluck instruments overlap but do not expose Rings model/polyphony/odd-even behavior. No Rings wrapper file found. | Extend via `mi_rings_ar` wrapper; add named input `in` and ports `odd`, `even`, `sum` if sum is cheap. Preserve internal exciter mode. | P1 |
| Beads | Stereo granular texture processor, delay/slicer, and granular wavetable voice. | `time`, `pitch`, `size`, `shape`, `density`, `seed_mode`, `quality`, `freeze`, `feedback`, `dry_wet`, `space`, randomization/spread amounts. | Stereo input. Stereo outputs `left`, `right`; optional `seed_trig`/`aux` trigger mode on right output should be deferred or explicit. | Manual signature: continuous recording buffer, up to 30 replay heads/grains, density modes (latched, clocked, gated/triggered), time/pitch/size/shape sampled at grain start, quality modes (Cold digital, Sunny tape, Scorched cassette), feedback/dry-wet/reverb, delay/slicer mode at infinite size, internal wavetable mode when inputs are unpatched. No official Beads source exists in `pichenettes/eurorack`; Clouds firmware is only an adjacent granular reference. | No Beads wrapper found. Clouds processor and generic granular effects exist. `MiClouds` cannot fully express Beads quality/attenurandomizer behavior, but can seed a useful approximation. | Rebuild/approximate as manual-grounded. Start with a Beads-inspired processor using native granular UGens or `mi_clouds_ar`; document fidelity limits. | P2 |
| Clouds | Original granular texture processor and Beads predecessor. | `position`, `size`, `pitch`, `density`, `texture`, `dry_wet`, `spread`, `feedback`, `reverb`, `freeze`, `mode`, `quality`, `trigger`. | Stereo input. Stereo outputs `left`, `right`. Optional modes: granular, stretch, looping delay, spectral. | Firmware `clouds/dsp/granular_processor.h` defines `PlaybackMode` as granular, stretch, looping delay, spectral and manages freeze, mono/stereo quality, lo-fi downsampling, buffers, diffuser, reverb, pitch shifter, WSOLA, looper, and phase vocoder. `clouds/dsp/parameters.h` maps position/size/pitch/density/texture/dry_wet/spread/feedback/reverb/freeze/trigger/gate. | `clouds_processor.vibe` exists as an FX wrapper around `mi_clouds_ar`, but it fixes pitch, reverb, feedback, freeze, mode, lo-fi, and trigger. | Extend existing wrapper into a full named-input processor; keep old defaults. Add mode/freeze/pitch/reverb/feedback/trigger params. | P2 |
| Marbles | Random sampler: T trigger generator plus X/Y random CV generator with Deja Vu memory. | `clock`, `rate`, `bias_t`, `spread_t`, `jitter`, `deja_vu_t`, `bias_x`, `spread_x`, `steps`, `scale`, `length`, `range`, `external_clock`. | Outputs: `t1`, `t2`, `t3`, `x1`, `x2`, `x3`; optional `y` if extending beyond hardware front-panel target. | Firmware `marbles/random/t_generator.h` defines trigger models including complementary/independent Bernoulli, clusters, drums, divider, three-states, and Markov; it has range, rate, bias, jitter, Deja Vu, and length. `marbles/random/x_y_generator.h` defines 3 X channels plus 1 Y channel, voltage ranges, clock sources, control modes, scale index, spread, bias, steps, Deja Vu, length, and ratios. | `marbles.vibe` already exposes six ports and approximates T/X behavior. `cv_marbles.vibe` is single-output legacy support. Current implementation uses simplified LFNoise/Markov approximation, not full firmware T/XY models. | Extend existing `marbles.vibe`; keep six-port surface, add kr output ports if appropriate, add Deja Vu/range/scale improvements when runtime supports it. | P1 |
| Stages | Six-segment function generator: envelope, LFO, step sequencer, S&H, delay, oscillator depending on segment type/grouping. | Per segment: type (`ramp`, `step`, `hold`, `alt`), loop, primary, secondary, gate/trigger, grouping. Global: clock, reset, scale. | Six segment outputs plus optional group outputs/EOR/EOC-style triggers where useful. | Firmware `stages/segment_generator.h` defines segment types ramp/step/hold/alt, per-segment primary/secondary params, single/multi-segment configuration, sequencer configuration, slave mode, and process functions for decay envelope, timed pulse, gate, sample-and-hold, tap/free LFO, PLL/free oscillator, delay, portamento, clocked S&H, and zero. `chain_state` handles grouping/topology. | `cv_stages.vibe` is a single-output six-step sequencer only. It explicitly does not model jack-sensed grouping or segment modes. | Rebuild first-class `stages` multi-output module; retain `cv_stages` as legacy/simple sequencer. | P2 |
| Tides | Original tidal modulator/function generator/oscillator. | `rate`/`freq`, `shape`, `slope`, `smoothness`, `shift`, `range`, `mode` (`ad`, `looping`, `ar`), `trigger`, `clock`, `freeze`. | Outputs for original behavior: bipolar/unipolar slope plus end-of-attack/end-of-release gates; if emulating panel, map low/slope/shape/high or named mode outputs. | Firmware `tides/generator.h` defines generator ranges high/medium/low, modes AD/looping/AR, control bits for freeze/gate/clock, and flags for end-of-attack/end-of-release. `generator.cc` implements audio/control-rate rendering, integrated BLEP, clock sync/frequency ratios, low-pass smoothing, and wavefolding under smoothness. | `cv_tides.vibe` is a single-output morphable LFO approximation. `MiTides` UGen exists but maps better to Tides2-style four-output behavior. | Extend/rebuild wrapper. Keep `cv_tides` compatibility, add a first-class multi-output `tides` using `mi_tides_ar` if possible or native approximation otherwise. | P3 |
| Frames | Quad keyframer/mixer/morpher and poly-LFO alternate role. | `frame`, per-channel levels, keyframe list, interpolation/easing curve, response, add/remove keyframes, sequencer/poly-LFO mode. | Four inputs and four outputs for VCA/mix/keyframed CV; optional mix output and LFO outs. | Firmware `frames/keyframer.h` defines 4 channels, up to 64 keyframes, easing curves (step, linear, in quartic, out quartic, sine, bounce), channel response, timestamped keyframes, DAC levels, and persistent extra settings. `frames/poly_lfo.*` supports alternate modulation behavior. | No Frames wrapper found. Generic mixers/crossfaders exist but do not model keyframes or easing. | Rebuild. Implement script-level keyframe data plus a four-output processor/voice surface; avoid trying to persist hardware-style panel state initially. | P2 |
| Tides2 / Tides 2018 | Four related slope generators with output relationship modes. | `freq`, `shape`, `slope`, `smoothness`, `shift`, `ramp_mode`, `output_mode`, `range`, `trigger`, `clock`, `ratio`. | Four outputs. Recommended names by mode: `low`, `slope`, `shape`, `high` for shape-chain mode; `ch1..ch4` for generic mode; gates/amps/phases/freqs for UGen-compatible mode. | Firmware `tides2/poly_slope_generator.h` defines 4 channels, ramp modes AD/AR/looping, output modes gates/amplitude/slope-phase/frequency, audio/control ranges, ramp shaping, folding, smoothing, per-channel phase/slope/frequency relationships, and ratio tables. The manual documents output modes: waveshape chain, amplitude distribution, slope/phase/time shift, and frequency ratios. | `MiTides` UGen exists with 4 outputs and params matching Tides2. `cv_tides.vibe` does not expose the four-output behavior. | Extend via `mi_tides_ar` wrapper. This is lower risk than original Tides because the UGen manifest already matches the four-output firmware signature. | P1 |

## Per-Module Implementation Notes

### Plaits

The build wave should make the wrapper thin and faithful to `MiPlaits`:

- Declare `out` and `aux` audio ports.
- Expose `engine` as numeric first, then provide helper constants or presets for
  named engines.
- Document the panel-original 16 models separately from the source-current 24
  `MiPlaits` engines.
- Keep the internal LPG controls explicit: `trigger`, `level`, `decay`,
  `lpg_colour`.

### Rings

Use Rings as both processor and voice:

- Named audio input `in` should accept external excitation.
- Trigger/internal exciter should work with `in = 0`.
- Ports should allow odd/even style routing; if the UGen output is simply two
  channels, name them `odd` and `even` and optionally offer `sum`.
- Start with 6 `MiRings` models, not only the manual's main 3 panel modes,
  because the available UGen already exposes the source-current model enum.

### Beads

The first useful VibeLang Beads should be honest about fidelity:

- Implement time/pitch/size/shape/density/freeze/feedback/dry-wet/space.
- Add a `quality` parameter for Cold/Sunny/Scorched character, but approximate
  with filtering, sample-rate/bit-depth, saturation, wow/flutter, and limiter
  behavior.
- Grain params should latch per grain where possible; do not make every current
  grain follow parameter changes lock-step.
- Treat granular wavetable mode as a later extension unless the implementation
  wave already has a Plaits wavetable source available.

### Clouds

The current wrapper is too narrow:

- Add `pitch`, `spread`, `reverb`, `feedback`, `freeze`, `mode`, `lofi`, and
  `trigger` params to the existing `clouds_processor` or a new full wrapper.
- Preserve old default behavior for compatibility.
- Use named stereo input/output conventions from `kb/voice-multioutput-howto.md`.

### Marbles

Existing `marbles.vibe` is the right starting point:

- Upgrade outputs to control-rate ports if the build wave targets CV-to-param
  routing directly.
- Improve Deja Vu from LFNoise memory toward explicit repeat buffers if the
  synthdef DSL/runtime can support variable-length history.
- Add scale/quantizer integration rather than only `quant_steps`.
- Keep `cv_marbles.vibe` untouched except for docs/import compatibility.

### Stages

`cv_stages.vibe` should not be stretched into the full module:

- Build a new `stages` surface with six outputs.
- Encode grouping explicitly in params or helper constructors; there is no
  physical jack sensing in a script file.
- Prioritize step/ramp/hold groups, looping envelopes, S&H, and sequencer mode.
- Defer exact alt mode and every firmware state combination until after import
  and routing tests pass.

### Tides And Tides2

Split the wrappers deliberately:

- `tides2` should map closely to `MiTides` and expose four outputs immediately.
- `tides` can be a compatibility/original-flavor surface if needed, focused on
  AD/AR/looping, smoothness LPF/fold, sync, and EOA/EOR gates.
- Do not collapse `MiTides` four outputs to one `out`; the output relationship
  modes are the signature.

### Frames

Frames needs a state surface more than new DSP:

- Represent keyframes as script-level arrays/tables first.
- Provide four channel outputs with selectable easing curves.
- Add optional four audio inputs for VCA/mixer use after CV keyframing works.
- Defer hardware persistence and LED/color behavior.

## Build-Wave Priority Table

| Priority | Module(s) | Decision | Why |
|---|---|---|---|
| P1 | Plaits, Rings, Tides2 | Extend UGen wrappers | High musical value, direct `MiPlaits`/`MiRings`/`MiTides` support, clear named outputs. |
| P1 | Marbles | Extend existing stdlib | Already multi-output and central to the rack patch; improve CV-rate routing and fidelity rather than rebuilding. |
| P2 | Clouds | Extend existing processor | Existing wrapper works but hides most firmware/manual controls needed for a rack demo. |
| P2 | Frames | Rebuild | Missing first-class module; keyframe morphing is central and feasible with script-level data. |
| P2 | Beads | Manual-grounded rebuild/approximation | Important end-of-chain texture module, but no official firmware source; be explicit about approximation. |
| P2 | Stages | Rebuild | Existing single-output sequencer is too narrow; six-output segment grouping is the module identity. |
| P3 | Tides original | Extend/rebuild after Tides2 | Useful, but Tides2/MiTides gives faster four-output payoff for the rack. |

## Source Index

Manuals and mirrors:

- Plaits manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/plaits/manual/
- Rings manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/rings/manual/
- Beads manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/beads/manual/
- Clouds manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/clouds/manual/
- Marbles manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/marbles/manual/
- Stages manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/stages/manual/
- Tides original manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/tides/manual/
- Frames manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/frames/manual/
- Tides 2018 manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/tides_2018/manual/

Firmware/source references:

- Root repository and license: https://github.com/pichenettes/eurorack
- Plaits voice and engines: https://github.com/pichenettes/eurorack/tree/master/plaits
- Rings resonator/part: https://github.com/pichenettes/eurorack/tree/master/rings
- Clouds granular processor: https://github.com/pichenettes/eurorack/tree/master/clouds
- Marbles random generators: https://github.com/pichenettes/eurorack/tree/master/marbles
- Stages segment generator: https://github.com/pichenettes/eurorack/tree/master/stages
- Tides original generator: https://github.com/pichenettes/eurorack/tree/master/tides
- Frames keyframer: https://github.com/pichenettes/eurorack/tree/master/frames
- Tides2 poly slope generator: https://github.com/pichenettes/eurorack/tree/master/tides2

Internal VibeLang references:

- `kb/voice-multioutput-howto.md`
- `kb/future-rack-candidates.md`
- `kb/eurorack-recreation-hotlist.md`
- `crates/vibelang-dsp/ugen_manifests/mi_ugens.json`
- `crates/vibelang-std/stdlib/instruments/eurorack/marbles.vibe`
- `crates/vibelang-std/stdlib/cv/eurorack/cv_marbles.vibe`
- `crates/vibelang-std/stdlib/cv/eurorack/cv_stages.vibe`
- `crates/vibelang-std/stdlib/cv/eurorack/cv_tides.vibe`
- `crates/vibelang-std/stdlib/effects/granular/clouds_processor.vibe`
