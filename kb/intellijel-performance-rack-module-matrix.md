# Intellijel performance rack module matrix

> Research note for rebuilding a percussion/FX-focused Intellijel performance
> rack as VibeLang stdlib modules and a later demonstration patch. This is a
> behavior matrix, not a panel clone or firmware clone.

## Scope

Target rack modules: Plonk, Rainmaker, Quadrax, Tetrapad, Mixup, Quad VCA,
uFold, Steppy, Quadratt, and Buff Mult.

Decision labels:

| Label | Meaning |
| --- | --- |
| Extend | Existing VibeLang stdlib modules or primitives cover a meaningful part of the behavior, but a named Intellijel-facing module should add missing routing, parameter, or DSP structure. |
| Rebuild | Existing modules are only adjacent; implement a dedicated module to preserve patch semantics. |
| Alias | Existing VibeLang routing, group, processor, or CV utilities already cover the core behavior closely enough; expose an alias or recipe instead of new DSP. |

## Existing stdlib overlap audit

| Area | Relevant files | Findings |
| --- | --- | --- |
| Delay and comb | `stdlib/effects/delays/multi_tap_delay.vibe`, `stdlib/processors/delays/mimeophon.vibe`, `stdlib/effects/filters/comb_filter.vibe`, `stdlib/effects/filters/resonator.vibe` | Rainmaker can reuse the delay-line, feedback, diffusion, comb, and processor patterns, but existing `multi_tap_delay` has six fixed taps while Rainmaker needs a 16-tap per-tap rhythm delay plus a 1-64 tap comb/resonator layer. |
| Percussion and modal/pluck synthesis | `stdlib/drums/**`, `stdlib/bass/genre/bass_karplus.vibe`, `stdlib/bass/pluck/**`, `stdlib/leads/pluck/**`, `stdlib/effects/filters/resonator.vibe`, `stdlib/strings/acoustic_guitar.vibe` | Plonk can reuse the exciter/resonator vocabulary and Karplus/comb primitives, but no current stdlib module is a preset-morphing modal percussion synth. No `rings` or `elements` stdlib file is present in this checkout; if the Mutable Rings work lands first, Plonk should share its modal bank, damping, and exciter infrastructure. |
| Envelopes, LFOs, triggers | `stdlib/instruments/eurorack/maths.vibe`, `stdlib/cv/envelopes/cv_env_ad.vibe`, `stdlib/cv/envelopes/cv_env_adsr.vibe`, `stdlib/cv/envelopes/cv_slew.vibe`, `stdlib/cv/triggers/cv_burst.vibe`, `stdlib/cv/lfo/*_kr.vibe` | Quadrax should extend this layer: four function channels, selectable AD/AHR/cycle/burst/LFO modes, CV matrix assignments, channel linking, and optional EOR/EOF trigger ports. |
| Touch and pressure control | `stdlib/instruments/eurorack/prss_pnt.vibe` | Tetrapad has the same four-pad pressure surface class as PrssPnt, but adds vertical position, eight output mappings, keyboard/chord/voltage/fader/drum/switch/LFO modes, stored mode presets, and slew. Extend PrssPnt-style kr ports rather than starting from raw DSP. |
| Gate sequencing and clocks | `stdlib/instruments/eurorack/tempi.vibe`, `stdlib/instruments/eurorack/rene.vibe`, `stdlib/cv/triggers/cv_clock.vibe`, `stdlib/cv/triggers/cv_euclidean.vibe`, `stdlib/theory/rhythm.vibe` | Steppy should be a dedicated four-track gate sequencer, but can reuse clock/reset/gate output conventions and script-level pattern helpers. |
| Mixers and VCAs | `stdlib/processors/mixers/mixer4_stereo.vibe`, `stdlib/processors/mixers/x_pan.vibe`, `stdlib/processors/dynamics/dxg.vibe`, `stdlib/effects/dynamics/comp_vca.vibe` | Mixup aliases mostly to stereo mixer/group routing. Quad VCA needs a small utility recipe around gain, CV modulation, response curve, boost, and cascaded mix outputs; use existing gain/mixer primitives unless a patchable VCA processor is needed later. |
| CV utilities and multiples | `stdlib/cv/util/cv_mixer4.vibe`, `stdlib/cv/util/cv_scale_offset.vibe`, `stdlib/instruments/eurorack/cv_bus.vibe`, named-port additive routing from `kb/voice-multioutput-howto.md` | Quadratt aliases to `cv_scale_offset`, `cv_mixer4`, or `cv_bus`. Buff Mult aliases directly to additive named-port routing for fan-out; a named helper can document the 2x3/1x6 normalized topology. |
| Wave shaping | `stdlib/effects/distortion/waveshaper.vibe`, `stdlib/effects/distortion/saturator.vibe`, `stdlib/effects/distortion/bitcrush.vibe` | uFold is not covered by a true wavefolder. Rebuild as a dedicated DC-coupled processor with fold amount, symmetry bias, CV attenuators, and 2/4/6-stage model; it can reuse waveshaper/saturation safety patterns. |

## Module matrix

| Module | Controls to model | I/O to model | DSP signature | Stdlib overlap and gaps | Decision | Build priority |
| --- | --- | --- | --- | --- | --- | --- |
| Plonk | Preset/load/save abstraction; pitch, decay, X, Y; trigger button; exciter menu; object menu; MOD destination/depth; saturation; bitcrush; preset morph/randomize/choke behaviors. | Mono audio out; `pitch` 1V/oct; `trig`; `vel`; `x`, `y`, and `mod` CV inputs with assignable destinations. | Modal-Karplus physical-modeling percussion: shaped mallet/noise exciter into resonator objects for string, beam, marimba, drumhead, membrane, and plate-like modes. Include two-voice decays if practical. | Existing drums/plucks and `resonator`/`comb_filter` cover simple pieces only. Mutable Rings, if present later, should share modal-bank and exciter infrastructure with Plonk. | Rebuild with shared modal-resonator core. | 1 |
| Rainmaker | Rhythm delay global time, grid/groove/division, feedback tap, feedback pitch/tone, reverse/wet-dry; per-tap mute, level, pan, filter cutoff/Q/type, pitch shift, detune; comb size, feedback, tap pattern, tap count/density/slope, nonlinear structure; MOD A/B assignment; trigger action; preset slots. | Stereo in/out; delay clock in; comb clock in; clock out; trigger in; delay feedback/pitch/tone CV; comb size/feedback CV; MOD A/B CV. | 16-tap stereo rhythm delay plus stereo comb resonator. Each rhythm tap has filter and pitch-shift behavior; comb section sums 1-64 taps with tunable resonant delay patterns. | Strong delay overlap exists, especially `multi_tap_delay`, `mimeophon`, `comb_filter`, and `resonator`; current code lacks per-tap editing, 16 taps, pitch-shift-per-tap, trigger actions, and comb-pattern bank. | Extend existing delay/comb infrastructure into a named processor. | 1 |
| Quadrax | Four channels with Rise, Fall, Shape, Mode, Link, and CV matrix assignment; modes AD, AHR, Cycle, Burst, standard LFO, alternate LFV; channel linking by trigger/EOR/EOF; optional Qx EOR/EOF behavior. | Four trigger inputs; four CV inputs assignable across rise/fall/shape/level and mode parameters; four CV outputs; optional eight EOR/EOF trigger outputs. | Four independent function generators with per-channel mode switching: envelopes, cycling functions, bursts, and LFOs at kr rate. | `maths`, `cv_env_ad`, `cv_env_adsr`, `cv_burst`, `cv_slew`, and kr LFO modules cover primitives. Missing piece is the Quadrax-style four-channel mode/CV-matrix/link wrapper. | Extend CV primitives into a dedicated `quadrax` control voice. | 2 |
| Tetrapad | Four pressure/position pads; four push encoders; shift/edit; pressure response/filter; per-mode slew; mode presets; Combo pad assignments; keyboard/chord/voltage/fader/drum/switch/LFO modes. | Eight kr outputs that can emit pitch CV, position CV, pressure CV, triggers, gates, switches, LFOs, or stored voltages depending on mode. | Four touch surfaces mapped to control-rate output generators; no audio DSP except possible CV/LFO generation. | `prss_pnt` already models four pressure ports plus gates. Tetrapad needs vertical position, paired outputs per pad, mode-dependent mapping, keyboard/chord quantization, voltage memory, LFO generation, and stored presets. | Extend PrssPnt-style surface into `tetrapad`. | 2 |
| Mixup | Level 1-3, mute 1-3; stereo chain in/out behavior; clipping indicator if modeled. | Mono inputs 1-2 normalized to stereo, stereo input 3 with mono-left normalization, unity stereo input 4, stereo mix out, rear stereo chain in/out. | AC-coupled stereo summing mixer with simple level and mute controls. | `mixer4_stereo` and group routing already cover mix behavior. Missing AC coupling, mute switches, and normalled mono-to-stereo conveniences are script/alias details. | Alias to mixer/group recipe; optional thin named processor. | 4 |
| Quad VCA | Four Level knobs; four CV attenuators; linear-to-exponential response knobs; +6 dB boost switches; cascaded CV and cascaded mix behavior. | Four signal inputs, four CV inputs, four outputs with normalled cascade to final mix output. DC-coupled audio/CV processing. | Four voltage-controlled amplifiers with response-curve shaping and optional boosted output, plus normalized summing mixer. | Existing gain, mixer, and `comp_vca` naming are adjacent, but no exact four-channel CV VCA utility is required for the first patch if VibeLang params and routing handle amplitude. | Alias or thin utility processor if patch syntax benefits. | 4 |
| uFold | Folds amount, folds CV attenuator, symmetry bias, symmetry CV attenuator, 2/4/6 stages, fold-trim and symmetry-trim approximations. | DC-coupled mono input; mono folded output; folds CV input; symmetry CV input. Accept audio or CV. | Wavefolder/waveshaper that folds peaks back around threshold, with symmetry offset and selectable number of fold stages. | Existing `waveshaper`/distortion effects are clipping/saturation-oriented, not true foldback. | Rebuild as `ufold` processor. | 2 |
| Steppy | Four tracks A-D; 1-64 step length; gate length; clock divider; swing; delay; probability; rotate/shift; mutes; Loopy performance window; ratchets; reset/run mode; eight presets. | Clock input; reset/run input; four +5 V gate outputs. | Four-track gate sequencer with per-track state and per-step/per-track timing modifiers. | `tempi`, `rene`, `cv_clock`, `cv_euclidean`, and rhythm helpers cover clock/gate idioms, not Steppy-style track memory and Loopy performance. | Rebuild as script-level or kr control module. | 1 |
| Quadratt | Four knobs; four uni/bipolar switches; internal +5 V normalled source; cascaded output mixing. | Four CV/audio inputs; four outputs that can be independent or normalled into downstream channel sums. | Active attenuator/attenuverter/mixer and DC voltage source. | `cv_scale_offset`, `cv_mixer4`, and `cv_bus` cover the core math. Named-port routing handles multiple downstream destinations. | Alias to existing CV utilities. | 5 |
| Buff Mult | Upper and lower section normalization; 2x3 or 1x6 topology. | Two inputs, six buffered outputs; top section normalled into lower input if lower input unpatched. | Unity-gain signal duplication; in VibeLang this is routing fan-out, not DSP. | `kb/voice-multioutput-howto.md` documents additive `.to(group)` fan-out and kr param fan-out. No new synthdef needed unless documenting a hardware-shaped helper. | Alias to routing fan-out. | 5 |

## Priority comparison

| Priority | Modules | Why |
| --- | --- | --- |
| 1 | Plonk, Rainmaker, Steppy | These define the rack's percussion voice, complex FX identity, and playable rhythmic sequencing. Plonk and Rainmaker also carry the largest DSP risk and should shape shared infrastructure early. |
| 2 | Quadrax, Tetrapad, uFold | These make the patch expressive: steppable envelopes/LFOs, touch performance control, and timbre shaping. Each can reuse existing primitives but needs an Intellijel-facing wrapper. |
| 3 | Integration patch surface | Once priority 1-2 modules exist, build `examples/intellijel_perform/main.vibe` around named inputs/outputs: Steppy gates into Plonk/Quadrax, Quadrax and Tetrapad kr ports into params, Plonk through uFold and Rainmaker. |
| 4 | Mixup, Quad VCA | Useful rack utilities, but VibeLang groups, gains, and mixer processors already express most behavior. Add only if the example patch reads better with named Intellijel aliases. |
| 5 | Quadratt, Buff Mult | Best treated as aliases/recipes over `cv_scale_offset`, `cv_mixer4`, `cv_bus`, and named-port fan-out. |

## Implementation notes

- Use named inputs and named outputs throughout. For control modules, prefer
  `output_kr` ports that patch with `.to_param(...)` or `.param(...).modulate_by(...)`.
- Plonk and a future Mutable Rings module should not duplicate modal resonator
  internals. Extract shared exciter, damping, modal bank, and Karplus/comb
  helpers if the first implementation makes that practical.
- Rainmaker should start from the existing delay/comb code path rather than a
  clean-room delay engine. The first useful version can expose fewer edit pages
  while still preserving 16 logical taps and a separate comb resonator section.
- Tetrapad is the closest analog to `prss_pnt`: start with four pressure inputs,
  add vertical position, then implement output modes one at a time.
- Mixup, Quad VCA, Quadratt, and Buff Mult should stay utilities unless a real
  patch authoring problem appears. VibeLang routing already covers their main
  hardware reason to exist.

## Source index

Official Intellijel sources used for this matrix:

- Plonk product page: https://intellijel.com/shop/eurorack/plonk/
- Plonk manual: https://intellijel.com/downloads/manuals/plonk_manual_v1.16_2020.11.08.pdf
- Rainmaker product page: https://intellijel.com/shop/eurorack/cylonix-rainmaker/
- Rainmaker manual: https://intellijel.com/downloads/manuals/cylonix-rainmaker_manual_v1.09-143.pdf
- Quadrax product page: https://intellijel.com/shop/eurorack/quadrax/
- Quadrax manual: https://intellijel.com/downloads/manuals/quadrax_manual_1.4_2023.04.27.pdf
- Tetrapad product page: https://intellijel.com/shop/eurorack/tetrapad/
- Tetrapad manual: https://intellijel.com/downloads/manuals/tetrapad_manual_v3.1_2021.07.01.pdf
- Mixup product page: https://intellijel.com/shop/eurorack/mixup/
- Mixup manual: https://intellijel.com/downloads/manuals/mixup_manual_2023.01.19.pdf
- Quad VCA product page: https://intellijel.com/shop/eurorack/quad-vca/
- Quad VCA manual: https://intellijel.com/downloads/manuals/quad-vca_manual_2021.08.02.pdf
- uFold legacy page: https://intellijel.com/eurorack-modules/plog/
- uFold manual: https://intellijel.com/downloads/manuals/ufold-2_manual_2018.09.13.pdf
- Steppy product page: https://intellijel.com/shop/eurorack/steppy/
- Steppy manual: https://intellijel.com/downloads/manuals/steppy_manual_1.2_2020.10.26.pdf
- Quadratt 1U product page: https://intellijel.com/shop/eurorack/1u/quadratt-1u/
- Quadratt 1U manual: https://intellijel.com/downloads/manuals/quadratt-1u_manual_2021.07.26.pdf
- Buff Mult product page: https://intellijel.com/shop/eurorack/buff-mult/
- Buff Mult manual: https://intellijel.com/downloads/manuals/buff-mult_manual_2018.09.13.pdf

Internal VibeLang references:

- `kb/voice-multioutput-howto.md`
- `crates/vibelang-std/CLAUDE.md`
- `crates/vibelang-std/stdlib/instruments/eurorack/prss_pnt.vibe`
- `crates/vibelang-std/stdlib/instruments/eurorack/maths.vibe`
- `crates/vibelang-std/stdlib/instruments/eurorack/tempi.vibe`
- `crates/vibelang-std/stdlib/instruments/eurorack/rene.vibe`
- `crates/vibelang-std/stdlib/instruments/eurorack/cv_bus.vibe`
- `crates/vibelang-std/stdlib/effects/delays/multi_tap_delay.vibe`
- `crates/vibelang-std/stdlib/processors/delays/mimeophon.vibe`
- `crates/vibelang-std/stdlib/effects/filters/comb_filter.vibe`
- `crates/vibelang-std/stdlib/effects/filters/resonator.vibe`
- `crates/vibelang-std/stdlib/effects/distortion/waveshaper.vibe`
- `crates/vibelang-std/stdlib/processors/mixers/mixer4_stereo.vibe`
- `crates/vibelang-std/stdlib/cv/util/cv_mixer4.vibe`
- `crates/vibelang-std/stdlib/cv/util/cv_scale_offset.vibe`
- `crates/vibelang-std/stdlib/cv/envelopes/cv_env_ad.vibe`
- `crates/vibelang-std/stdlib/cv/envelopes/cv_env_adsr.vibe`
- `crates/vibelang-std/stdlib/cv/triggers/cv_burst.vibe`
- `crates/vibelang-std/stdlib/cv/triggers/cv_euclidean.vibe`
