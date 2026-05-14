# 4ms Ambient Rack Module Matrix

Research date: 2026-05-14

Scope: behavior matrix for rebuilding a 4ms-flavored ambient/IDM rack as VibeLang stdlib modules. This is research only; it audits existing stdlib overlap and recommends whether to extend existing work or rebuild dedicated 4ms-style modules.

## Existing Stdlib Overlap

| Area | Existing stdlib coverage | Relevance |
|---|---|---|
| Delays | `stdlib/effects/delays/delay.vibe`, `ping_pong_delay.vibe`, `multi_tap_delay.vibe`, `dub_delay.vibe`, `tape_delay.vibe`, `reverse_delay.vibe`, `granular_delay.vibe`; processor `stdlib/processors/delays/mimeophon.vibe` | Good raw material for generic delay lines, feedback tone, stereo crossfeed, allpass smear, and long-zone behavior. Not enough for DLD hold/windowing or TAPO tapography semantics. |
| Reverbs | `stdlib/effects/reverbs/*`, including `reverb_jpverb`, `reverb_fdn8`, shimmer, infinite, room/hall/plate, convolution reverbs | Reuse for demo patch ambience and SMR tails. Do not rebuild generic reverb as part of this rack. |
| Filters/resonators | `stdlib/effects/filters/resonator.vibe`, `comb_filter.vibe`, `bandpass.vibe`, `formant_filter.vibe`, `qpas.vibe`, low/high-pass processors | Useful building blocks, but SMR needs a dedicated multi-band resonator/filter-bank model with per-band CV, scale rotation, stereo odd/even routing, and spectral envelope outputs. |
| Spectral tools | `stdlib/instruments/spectral/spectraphon_side.vibe`, `spectraphon_dual.vibe`; `stdlib/effects/spectral/*` | Spectraphon already covers FFT analysis, partial magnitudes, multi-output routing, and spectral/morphing idioms. SMR should complement it as a resonant filter-bank, not duplicate additive oscillator memory. |
| Phase/modulation | `stdlib/effects/modulation/phaser.vibe`, `phaser_8stage.vibe`, `barber_phaser.vibe`, `chorus.vibe`, `ensemble_chorus.vibe`, `dimension_*` | Strong overlap. Phaseur can likely be a thin dedicated wrapper/preset around `phaser_8stage` + `barber_phaser` + optional chorus/vibrato blend. |
| Clock/CV | `stdlib/cv/triggers/cv_clock.vibe`, `cv_trigger.vibe`, `cv_gate.vibe`, `cv_euclidean.vibe`; `stdlib/instruments/eurorack/tempi.vibe` | Strong overlap for QCD-style clock outputs. QCD still deserves a four-output divider/multiplier module because the panel behavior is specific and useful. |
| Envelopes | `stdlib/cv/envelopes/cv_env_ad.vibe`, `cv_env_adsr.vibe`, `cv_env_complex.vibe`, `cv_slew.vibe`; `stdlib/instruments/eurorack/maths.vibe` | Strong overlap for PEG internals. PEG should rebuild the ping-locked envelope behavior, but reuse these conventions: kr outputs, named output ports, scale/offset routing, and clock-param modulation. |
| Sampling/granular | `stdlib/instruments/sampler/morphagene.vibe`, buffer APIs, sample/reel concepts | Strong enough for implementation patterns. STS should probably remain optional because true STS behavior depends on a file/bank UX rather than just sample playback DSP. |

## SMR - Spectral Multiband Resonator

Official manual summary: the hardware SMR is a resonant filter bank with six bandpass resonators/filters, variable Q, stereo odd/even inputs and outputs, quantized scale rotation/spread, per-channel level CV, spectral envelope outputs, and optional 1V/oct outputs. The epic asks for an eight-band VibeLang centerpiece. Treat this as an intentional VibeLang extension: a "4ms-flavored SMR8" that preserves the panel idea while giving eight bands for better software patching symmetry.

| Topic | Matrix |
|---|---|
| Core controls | `freq_root`, `scale`, `bank`, `rotate`, `spread`, `morph`, `q`, `two_pass`, `level1..level8`, `lock1..lock8`, `fine1..fine8`, `transpose1..transpose8`, `slew`, `env_mode`, `noise_norm`, `amp`. |
| Inputs | Named audio inputs `odd` and `even` or `in` stereo; CV params for rotate/spread/morph/q/scale; per-band level CV via params first, optionally named kr inputs later if the stdlib surface supports patchable CV inputs cleanly. |
| Outputs | Audio outputs `odd`, `even`, and `mix`; kr outputs `env1..env8` for per-band magnitude/envelope tracking. Optional `voct1..voct8` can be a later extension if patch authors need chord-machine behavior. |
| DSP signature | Eight parallel tunable resonant bands. In SuperCollider terms: split/mono-sum the input, feed an `rlpf_ar` or `bpf_ar` bank per band, map each band frequency from `root * scale_degree_ratio[rotate + spread * i]`, multiply by per-band level CV, and sum odd/even-indexed bands to separate stereo outputs. High-Q mode should allow long ringing/pinged behavior; `two_pass` can cascade a second `rlpf_ar` per band for the v5 "lusher/tighter" behavior. |
| Magnitude tracking | For each band, compute band envelope after the resonator using `amplitude_ar` or `amplitude_kr` plus smoothing. Expose these as kr outputs `env1..env8` so `.to_param(target, "param")` and target-first `.param(...).modulate_by(...)` can patch spectral energy into other voices. Pre/post behavior can be approximated with `env_prepost`: pre tracks the resonator before the level VCA, post tracks after the level VCA. |
| Scale/rotation model | Start pragmatic: built-in arrays or unrolled constants for a few useful banks: chromatic, major, minor, pentatonic, pelog/slendro-ish, Bohlen-Pierce-ish. `rotate` advances the starting scale index; `spread` chooses the interval step between neighboring bands. `morph` should lag frequency and level changes to crossfade motion rather than jump. |
| Existing overlap | `spectraphon_side` has FFT magnitude and multi-output patterns; `effects/filters/resonator.vibe` is only a single comb resonator; `effects/spectral/*` are FFT effects. None provide a scale-quantized resonant filter-bank with per-band CV outputs. |
| Extend vs rebuild | Rebuild as a dedicated stdlib module. Reuse routing conventions and maybe helper idioms from Spectraphon, but do not extend Spectraphon itself. |
| Build priority | P0. This is the rack identity and should land before the demo patch is meaningful. |

Implementation notes for the SC/Rhai authoring pass:

- Keep the first public version fixed at eight bands rather than dynamically sized. Rhai and synthdef message limits are easier to reason about with unrolled bands.
- Use named ports from the start: `odd`, `even`, `mix`, and `env1..env8`. Default routing will only route the first ports; the example patch must explicitly route/mute all ports.
- Keep level and frequency smoothing short but audible: `lag_kr` on levels and frequencies, with `morph` scaling the lag time.
- Use clipping and soft limiting after summed bands. Eight high-Q filters can get hot quickly, especially with normalized noise input.
- Prefer bandpass/resonator clarity over exact hardware UI replication. Fidelity target should be "scale-locked resonant spectral motion" first, advanced settings slots/forbidden notes later.

## DLD - Dual Looping Delay

| Topic | Matrix |
|---|---|
| Core controls | `ping_bpm` or `ping_hz`, per-channel `time`, `time_range`, `feedback`, `delay_feed`, `mix`, `hold`, `reverse`, `ping_lock`, `quantize_changes`, `cft_ms`, `clear`. |
| Inputs | Audio `in_a`, `in_b`, optional `return_a`, `return_b`; kr/trigger params `ping`, `hold_a`, `hold_b`, `reverse_a`, `reverse_b`. |
| Outputs | Audio `out_a`, `out_b`, optional `send_a`, `send_b`; kr trigger outputs `clock`, `loop_a`, `loop_b`. |
| DSP signature | Two independent synchronized delay/loop channels with a shared ping base. Delay time is a musical multiple/division of the ping clock. Hold freezes writing while reading a loop; reverse flips read direction; feedback can exceed unity carefully with soft limiting. |
| Existing overlap | Generic delay and ping-pong effects cover delay primitives. `mimeophon` covers stereo long-zone delay, feedback tone, allpass smear, and half-time pitch trick, but not dual independent looping, hold, reverse, or clock outputs. |
| Extend vs rebuild | Build a dedicated `dld` processor using existing delay idioms. Do not rebuild generic delay/reverb; factor only if a common helper already exists. |
| Build priority | P1. Needed for the rack demo after SMR and QCD/PEG clocks exist. |

## Phaseur / Phaseur Fleur

| Topic | Matrix |
|---|---|
| Core controls | `speed`, `depth`, `height`, `blend`, `ring`, optional `barber`, `chorus_width`, `level`. |
| Inputs | Stereo or mono/stereo `in`; optional CV params for speed/depth/height/ring. |
| Outputs | Stereo `out`. |
| DSP signature | Multi-stage allpass phaser with LFO sweep, regenerative feedback, manual center frequency, and wet/dry blend that can move from vibrato-ish wobble to chorus-like phasing. Barber-pole mode maps naturally to existing `barber_phaser`. |
| Existing overlap | `phaser.vibe`, `phaser_8stage.vibe`, and `barber_phaser.vibe` already cover most behavior. `chorus.vibe` and `ensemble_chorus.vibe` can cover the chorus side of the blend. |
| Extend vs rebuild | Extend/wrap. A dedicated `phaseur` stdlib effect can compose the 8-stage phaser character and expose Phaseur-named params. |
| Build priority | P3. It colors the patch, but existing phasers can stand in while SMR/DLD/PEG/QCD are built. |

## PEG - Pingable Envelope Generator

| Topic | Matrix |
|---|---|
| Core controls | Two channels: `ping`, `div_mult`, `skew`, `curve`, `scale`, `bipolar`, `cycle`, `quantized_trigger`, `async_trigger`, `peak_v`, output mode gates/triggers. |
| Inputs | `ping_a`, `ping_b`, `qnt_a`, `qnt_b`, `async_a`, `async_b`, optional cycle toggle; CV params for div/mult, skew, curve. |
| Outputs | kr envelope outputs `env_a`, `env_b`, `env5_a`, `env5_b`, `or`, plus gates/triggers `eor_a`, `eof_a`, `half_r_a`, `eor_b`, `eof_b`, `half_r_b`. |
| DSP signature | Ping period measures the interval between clock pulses; envelope length is ping period multiplied/divided by `div_mult`. Skew divides total time into rise/fall portions; curve morphs exponential/linear/log behavior; cycle mode self-triggers in sync. |
| Existing overlap | `cv_env_ad` provides AD envelope conventions and kr output style; `cv_env_complex` and `maths` provide richer envelope ideas. None are explicitly ping-period locked like PEG. |
| Extend vs rebuild | Rebuild a dedicated `peg` CV voice. Reuse envelope/CV output conventions, not generic ADSR behavior. |
| Build priority | P1. PEG is the modulation glue for clock-locked SMR level/frequency movement and DLD changes. |

## STS - Stereo Triggered Sampler

| Topic | Matrix |
|---|---|
| Core controls | Two playback channels: `sample`, `bank`, `pitch`, `start`, `length`, `play`, `reverse`, `loop`, `stereo_mode`, `gain`; recording controls if implemented later. |
| Inputs | Trigger params `play_l`, `play_r`, `reverse_l`, `reverse_r`, `record`; audio record inputs `rec_l`, `rec_r` for later. |
| Outputs | Stereo `out_l`, `out_r`; kr `end_l`, `end_r` trigger outputs. |
| DSP signature | Two stereo sample players with CV-controlled sample selection, start position, length/window, pitch, reverse, looping, and end triggers. Full hardware behavior includes SD card banks and on-module editing, which is more of a runtime/file UX than a synthdef. |
| Existing overlap | `morphagene` already handles buffer/reel, splice, granular playback, clocking, EOSG, and persistence patterns. VibeLang also has sample/buffer APIs. |
| Extend vs rebuild | Optional rebuild. For this epic, start with a simpler triggered sample player or adapt Morphagene patterns; defer full bank management unless the user-facing sample API can support it cleanly. |
| Build priority | P4. Useful for clocked source material, but not required for the core SMR -> DLD -> Phaseur demo. |

## TD/TAPO - Tapographic Delay

| Topic | Matrix |
|---|---|
| Core controls | `tapography`, `morph`, `feedback`, `repeat`, `clock_div_mult`, `sequencer_mode`, `tap_filter_mode`, per-tap time/amp/pan/filter/resonance. |
| Inputs | Mono audio `in`; clock input/param; tap entry trigger or pre-authored tapography params. |
| Outputs | Stereo `out`; kr `gate` that emits the current tap rhythm. |
| DSP signature | Up to 32 taps, each with delay time, amplitude/filter, and pan. Tapographies morph between stored configurations, can be clock-synchronized, and can sequence through memories. Per-tap low-pass or resonant band-pass behavior is part of the signature. |
| Existing overlap | `multi_tap_delay` has six fixed taps; generic delays and `mimeophon` cover feedback and smear; no existing module supports editable tapography memory, per-tap filter modes, or tap rhythm gate output. |
| Extend vs rebuild | Rebuild only if TD becomes in-scope. A minimal "tapographic_delay" can extend `multi_tap_delay` to 8 or 16 fixed params first; true 32-tap memory is higher effort. |
| Build priority | P5. Optional/nice-to-have after DLD exists, because both are delay-heavy and DLD better serves the main rack demo. |

## QCD - Quad Clock Distributor

| Topic | Matrix |
|---|---|
| Core controls | `bpm`, per-channel `div_mult`, `reset`, `run`, `pulse_width`, `trigger_delay`, `mute`, `tap`. |
| Inputs | Optional external `clock_1..clock_4` normalized downward conceptually; reset triggers per channel; CV params for div/mult. |
| Outputs | kr/tr outputs `tap`, `red`, `black`, `blue`, `green` or `ch1..ch4`, plus optional inverted outputs if expander behavior is included. |
| DSP signature | Four clock divider/multiplier channels from /32 to x16, normalized clock flow from tap/top channel downward, reset per channel, auto-stop when source clock stops, square/gate or trigger pulse output. |
| Existing overlap | `cv_clock` is a single clock; `tempi` has six phase-locked clock outputs and scenes. QCD has a simpler and very specific divider/multiplier panel that maps well to a dedicated CV voice. |
| Extend vs rebuild | Rebuild dedicated `qcd`, borrowing ideas from `tempi` for multi-output kr clocks. |
| Build priority | P1. It drives PEG, SMR rotation/level pings, and DLD sync. |

## Comparison And Priority

| Priority | Module | Decision | Why |
|---|---|---|---|
| P0 | SMR | Rebuild dedicated `smr`/`smr8` | Defines the rack identity; no existing stdlib module provides scale-rotating multi-band resonant filtering with per-band magnitude CV. |
| P1 | QCD | Rebuild dedicated `qcd` | Needed as shared clock source; feasible with existing `tempi`/`cv_clock` patterns. |
| P1 | PEG | Rebuild dedicated `peg` | Needed for clock-locked modulation; existing envelopes are not ping-period locked. |
| P1 | DLD | Rebuild dedicated `dld` using existing delay idioms | Needed for the main patch; generic delays do not cover looping hold/reverse/window behavior. |
| P3 | Phaseur | Extend/wrap existing phaser/barber phaser | Existing modulation effects are close enough for first audio color pass. |
| P4 | STS | Optional simplified rebuild | Useful as a clocked sample source but file/bank UX is larger than this rack's core. |
| P5 | TD/TAPO | Optional later rebuild | Interesting but overlaps DLD in the demo. True tapography memory is higher effort than a simple multi-tap delay. |

Recommended first implementation wave:

1. `qcd` as a four-output kr/tr clock voice.
2. `peg` as a dual ping-locked kr envelope voice.
3. `smr8` as the centerpiece resonant filter bank with `odd`, `even`, `mix`, and `env1..env8` ports.
4. `dld` as a two-channel clocked looping delay.
5. Example patch using existing `phaser_8stage` or `barber_phaser` first; add a `phaseur` wrapper after the core patch is audible.
