# Make Noise ReSynthesizer module behavior matrix

> Research note for turning the Make Noise ReSynthesizer system into practical
> VibeLang implementation work. This is a behavior matrix, not an attempt to
> clone panel graphics or vendor firmware.

## Scope

The official ReSynthesizer manual lists 13 module positions: MATHS,
Morphagene, Spectraphon, X-PAN, QPAS, a second MATHS, Wogglebug, TEMPI, CV Bus,
Rene, PrssPnt, DXG, and Mimeophon. This matrix treats MATHS as one module
behavior with two system instances.

Decision labels:

| Label | Meaning |
|---|---|
| Exact-ish | The behavior maps cleanly to VibeLang primitives or already has a stdlib implementation. It should be close enough to preserve patch semantics. |
| Approximate | The musical role is implementable, but analog quirks, hidden firmware details, or hardware UI behavior should be modeled rather than copied exactly. |
| Deferred | Not needed for a first useful recreation, requires host/hardware integration, or depends on deeper DSP/runtime work. |

## System idioms to preserve

| Idiom | Patch behavior | VibeLang implication |
|---|---|---|
| Time-domain into frequency-domain resynthesis | Record or play Morphagene, mult its stereo outputs into Spectraphon SAM inputs, blend original and resynthesized signals through X-PAN/DXG. Source: ReSynthesizer manual pp.11,20. | Make Morphagene and Spectraphon first-class voices with named outputs. Keep dry Morphagene and Spectraphon Odd/Even routable independently. |
| Motion from CV feedback | Use Morphagene EOSG, TEMPI clocks, Rene gates, MATHS functions, Wogglebug random, and PrssPnt gestures to modulate all sound modules. Source: ReSynthesizer manual pp.9-19. | Prefer kr output ports and `.to_param(...)` routing over hard-coded modulation slots. |
| Stereo as a core signal path | X-PAN, QPAS, DXG, Mimeophon, Morphagene, and Spectraphon all expect stereo or multi-output routing. Source: X-PAN manual p.4; DXG manual pp.4-7; QPAS manual pp.6-8. | Use named stereo inputs/outputs and processors, not mono-only effects with implicit downmixing. |
| Patchable clock/state layer | TEMPI clocks Rene, Rene can coordinate state with TEMPI through Select Bus, and CV Bus distributes global control. Source: TEMPI product page; Rene manual pp.5-6; CV Bus case page. | Model clocks and state as script-level pattern/control utilities first. Select Bus behavior is a higher-level scene/state API, not audio DSP. |

## Module matrix

| Module | Role in ReSynthesizer | Key controls | Audio/CV I/O to model | Patch idioms | Practical VibeLang strategy | Sources |
|---|---|---|---|---|---|---|
| MATHS (two instances) | Central CV/function utility: envelope/LFO, slew, attenuverting, mixing, logic-style timing. | Ch.1/4 Rise, Fall, Response, Cycle; Ch.2/3 attenuverters/offsets; SUM, OR, INV; EOR/EOC. | Ch.1/4 signal and trigger inputs; Rise/Both/Fall CV; Cycle gate; Ch.2/3 signal inputs; Ch.1/4 unity and variable outs; SUM, INV, OR, EOR, EOC outputs. | Slew/portamento, triggered envelopes, cycling LFOs, CV inversion/attenuation, mixed modulation buses, delayed gates. | Exact-ish: existing `stdlib/instruments/eurorack/maths.vibe` covers ch1/ch4 envelopes, `sum`, `inv`, `eor1`, `eoc1` kr ports. Approximate: variable Response/curve currently accepted but not curve-modulating. Deferred: full ch2/ch3 offset/attenuverter channels, OR bus, audio-rate use, and complete ch4 EOC/cycle parity. Instantiate two voices for the ReSynthesizer layout. | ReSynthesizer manual pp.5,9-10,14-15; MATHS manual pp.4-9; `crates/vibelang-std/stdlib/instruments/eurorack/maths.vibe`. |
| Morphagene | Primary time-domain sound source: tape/microsound recorder, splicer, granulator, and reel player. | REC, Splice, Shift; Organize, Vari-Speed, Gene-Size, Slide, Morph, SOS; Clock; Reel selection. | Stereo audio in/out; CV over sound parameters; Play, REC, Splice, Shift, Clock inputs; EOSG trigger out; envelope-follower CV out; microSD reel storage. | Record external/system audio, split into Splices, shrink into Genes, jumble with Organize/Slide/Morph, clock recording/playback, send EOSG to Wogglebug or TEMPI, feed Spectraphon SAM. | Exact-ish: existing `morphagene` synthdef/howto covers Reel/Splice/Gene hierarchy, playback, varispeed, granular windowing, and script-tier reel helpers. Approximate: dynamic enveloping, Morph extremes, time-stretch feel, and SOS decay can be modeled musically. Deferred: exact destructive microSD behavior, hardware file workflows, and any undocumented Morph edge modes. | ReSynthesizer manual pp.11,20; Morphagene manual pp.5-8; `kb/morphagene-howto.md`; `kb/morphagene-synthdef-plan.md`. |
| Spectraphon | Primary frequency-domain source: dual spectral oscillator/resynthesizer. | Per side: Frequency/Fine, Slide, Focus, Partials, SAM/SAO mode, Array capture/selection; FM Index; Follow/Sync; Spectranoise Chaos/Noise firmware modes. | 2 audio inputs; 8 outputs total: Sine, Sub/CV, Odd, Even per side; 1V/oct, Slide, Focus, Partials, FM, mode/gate/clock-style CV/gate inputs. | Analyze Morphagene or external audio in SAM, capture Arrays, play Arrays in SAO, split Odd/Even through X-PAN/QPAS/DXG, use Follow/FM for chordal or complex-oscillator behavior. | Exact-ish: existing `spectraphon_side` and `spectraphon_dual` synthdefs model additive odd/even banks, SAM/SAO-oriented control, named outputs, FM/follow subset, and Spectranoise modes. Approximate: FFT analyzer and Array interpolation are practical models, not firmware copies. Deferred: exact hardware Array storage format, full Sync behavior, and complete per-side dual SAM/SAO plus shared FM in one synthdef. | ReSynthesizer manual pp.11,20; Spectraphon product page; Spectraphon manual; `kb/spectraphon-howto.md`; `kb/spectraphon-synthdef-plan.md`. |
| X-PAN | Voltage-controlled stereo mixer, crossfader, panner, and CV router. | Ch.1/2 X-Fade and Pan controls with CV; Aux stereo VCA level/CV. | Ch.1A/B and Ch.2A/B mono inputs; X-Fade CV and Pan CV per channel; Aux L/R with mono normalization and VCA CV; stereo SUM L/R outputs; DC-coupled I/O. | Pan Spectraphon sine or Odd/Even outputs, crossfade Odd/Even dynamically, blend dry Morphagene with resynthesized Spectraphon, use as a DC CV router. | Exact-ish: implement as a patchable processor with two mono crossfaders, equal-power or linear pan, aux stereo VCA, and stereo sum. Approximate: analog VCA headroom and audio-rate sideband character. Deferred: no need to model panel-only gain staging beyond useful clipping/level controls. | ReSynthesizer manual p.12; X-PAN manual pp.4-7; X-PAN product page. |
| QPAS | Stereo quad-peak filter and mono/stereo image animator. | Frequency, Radiate-L/R with attenuverters/CV, Q and Q CV, input level/VCA, `!!¡¡` inputs. | Stereo or mono-normalled inputs; frequency CV inputs; Radiate L/R CV; Q CV; `!!¡¡` modulation inputs; simultaneous stereo LP, BP, HP, and Smile Pass outputs. | Filter Morphagene or Spectraphon, animate stereo peaks with slow or random CV, excite high-Q ringing with gates, route different output types into DXG/Mimeophon. | Exact-ish: four parallel state-variable or resonant filter cores with shared base frequency, L/R Radiate offsets, Q feedback, and simultaneous output ports. Approximate: Smile Pass can be modeled as a musically tuned LP/HP/BP blend; `!!¡¡` can start as broad audio/CV perturbation of cutoff/Q. Deferred: exact analog core response, non-self-oscillation edge behavior, and the hidden `!!¡¡` circuit details. | ReSynthesizer manual p.13; QPAS manual pp.6-8; QPAS product page. |
| Wogglebug | Random CV/audio source and patch destabilizer. | Speed/Chaos, Ego/Id, Woggle, Disturb, Speed CV attenuator; external clock. | Smooth VCO, Woggle VCO, Ring-Mod audio outs; Stepped, Smooth, Woggle CV outs; Burst gate out; Ego, Influence, Speed CV, External Clock inputs. | Clock from Morphagene EOSG; send Stepped to Gene-Size or Organize; send Smooth/Woggle to QPAS Radiate, Spectraphon Slide/Focus, Mimeophon Rate/Zone; use Burst for irregular gates. | Exact-ish: existing `cv_wogglebug` models stepped, smooth, and woggle CV modes. Approximate: a multi-output `wogglebug` should fan out all CV/audio/gate ports from one shared chaos state. Deferred: faithful PLL/two-VCO/ring-mod interaction, Disturb hold/sample semantics, and high-rate audio noise behavior. | ReSynthesizer manual p.16; Wogglebug manual pp.4-6; Wogglebug product page; `crates/vibelang-std/stdlib/cv/eurorack/cv_wogglebug.vibe`. |
| TEMPI | Six-channel playable clock divider/multiplier and state source. | Tap tempo, Channel 1-6 buttons, PGM_A/B, Mute, Mod, Run/Stop, state/bank selection, tempo CV. | Tempo input; Mod input; State select CV/gate/Select Bus behavior; six clock outputs. | Clock Rene X/Y, automate Morphagene REC/Play/Splice, trigger MATHS/DXG strikes, recall clock arrangements alongside Rene states. | Exact-ish: script-level clock generator can emit six kr/gate outputs with ratios, phase offsets, mute, run/stop, and state presets. Approximate: human-programming gestures and non-integer phase/division feel. Deferred: full Select Bus electrical/protocol emulation and LED/button UI. | ReSynthesizer manual p.17; TEMPI manual pp.5,12,17; TEMPI product page. |
| CV Bus / 4 Zone CV Bus Case | Case-level integration: voltage math, signal distribution, line/headphone output, power/select-bus context. | Voltage Math attenuverters/mix/offset-style controls; multiple section; output volume. | Distributed CV bus/multiple jacks; voltage math inputs/outputs; stereo line/headphone output; Select Bus/power are non-audio infrastructure. | Distribute Morphagene outputs to Spectraphon and DXG, mult clocks/CV, attenuate or invert modulation, final stereo output. | Exact-ish: model patch multiples with VibeLang routing fan-out, voltage math with `cv_mixer4`/scale-offset utilities, and final output with groups/main routing. Approximate: output soft limiting/headphone drive. Deferred: power-zone isolation, LEDs, physical Select Bus, and case hardware behavior. | ReSynthesizer manual pp.14-15; 4 Zone CV Bus Case product page/manual. |
| Rene | Three-channel Cartesian sequencer and state-performance controller. | 4x4 touch/knob grid; X/Y/C channel buttons; X/Y clocks, MOD/CV inputs; Program Pages; Z-axis 64 State selection; quantization/glide/access/gate pages. | X/Y/Z clocks and CV/mod inputs; three CV outputs; three gate outputs; Select Bus coordination with TEMPI. | Clock X/Y from TEMPI, send X-CV to Spectraphon 1V/oct, use gates for DXG strikes or MATHS triggers, traverse stored states for macro patch changes. | Exact-ish: script-level sequencer can model three CV/gate streams, quantized values, glide flags, access masks, and state presets. Approximate: Cartesian/Snake traversal and MESH can be data structures rather than panel UI. Deferred: capacitive touch programming, Select Bus protocol, firmware-specific editing pages. | ReSynthesizer manual p.18; Rene manual pp.5-8; Rene product page. |
| PrssPnt | Human touch controller: one playable pressure/touch node. | Sensitivity, Slew, touch plate, momentary/toggle behavior. | Momentary Gate out; Toggled Gate out; Pressure CV out; Smooth Touch/slewed pressure CV out. | Hand-open DXG, freeze/flip Mimeophon, trigger MATHS, pressure-modulate Spectraphon Partials or QPAS Radiate, perform macro scene changes. | Exact-ish if driven by MIDI/OSC/HID: gate, toggle, pressure CV, and slew are simple control processors. Approximate without hardware: provide a `touch_macro` utility that maps an external control source to these four ports. Deferred: capacitive sensing and physical response curve. | ReSynthesizer manual p.19; PrssPnt product page/manual. |
| DXG | Dual stereo low-pass gate and stereo mixer. | Ch.1/2 CTRL combo pots/CV attenuators, Strike inputs, Aux stereo input, sum routing. | Ch.1/2 stereo L/R inputs with mono normalization; CTRL CV and Strike per channel; Ch.2 individual outs; Aux L/R; stereo SUM outs. | Strike any stereo signal into plucks, gate Spectraphon/QPAS/Morphagene, mix dry/resynth channels, feed Mimeophon as final effect, use Ch.2 outs for parallel processing. | Exact-ish: stereo LPG processor as paired VCA plus one-pole lowpass per side, with strike impulse envelope and aux sum. Approximate: vactrol-free memory/decay and analog saturation. Deferred: exact CTRL response and per-unit analog feel. | ReSynthesizer manual p.19; DXG manual pp.4-7; DXG product page. |
| Mimeophon | Stereo multi-zone color audio repeater and final echo/space processor. | Zone, Rate, Skew, Repeats, Halo, Color, Mix; Hold, Flip, Tempo, microRate. | Stereo input/output; Rate CV/attenuverter; microRate CV; Tempo, Hold, Flip gate inputs; Rate pulse output. | End-of-chain stereo echo, tempo-synced repeats from TEMPI, Zone 0 Karplus/flange/chorus, Hold loops for non-destructive manipulation, Flip for reverse repeats, Rate Out as clock source. | Exact-ish: multi-zone stereo delay with feedback, tempo sync, hold freeze, reverse/flip, ping-pong/skew, wet/dry mix, and Rate pulse. Approximate: Color/Halo can begin as filtered feedback plus diffusion/reverb; zone morphing can crossfade delay ranges. Deferred: exact Soundhack algorithm, firmware noise-floor details, and every hidden edge case of Hold/Flip in shortest zones. | ReSynthesizer manual p.19; Mimeophon manual pp.7-17; Mimeophon product page. |

## Implementation ordering

| Priority | Work | Rationale |
|---|---|---|
| 1 | Keep `morphagene`, `spectraphon_side`, `spectraphon_dual`, `maths`, and `cv_wogglebug` aligned with this matrix. | They are the core "resynthesis plus modulation" behavior and already have stdlib footholds. |
| 2 | Add patchable processors for X-PAN, QPAS, DXG, and Mimeophon. | These define the stereo signal path. They can be useful approximations before exact analog/firmware details are known. |
| 3 | Add script-level modules/utilities for TEMPI and Rene. | Their core value is musical clock/state/CV generation rather than audio DSP. |
| 4 | Add PrssPnt and CV Bus as control/routing helpers. | They are important to the instrument feel, but they can initially be lightweight wrappers around existing routing, CV, and external-control APIs. |

## Source index

Official Make Noise sources used for this matrix:

- ReSynthesizer product page: https://www.makenoisemusic.com/synthesizers-and-controllers/resynthesizer/
- ReSynthesizer manual: https://www.makenoisemusic.com/wp-content/uploads/2024/10/resynthesizer-manual.pdf
- MATHS product page: https://www.makenoisemusic.com/modules/maths
- MATHS manual: https://www.makenoise-manuals.com/maths/maths-manual.pdf
- Morphagene product page: https://www.makenoisemusic.com/modules/morphagene/
- Morphagene manual: https://www.makenoise-manuals.com/morphagene/morphagene-manual.pdf
- Spectraphon product page: https://www.makenoisemusic.com/modules/spectraphon/
- Spectraphon manual: https://www.makenoisemusic.com/wp-content/uploads/2024/03/spectraphon-manual.pdf
- X-PAN product page: https://www.makenoisemusic.com/modules/x-pan/
- X-PAN manual: https://www.makenoise-manuals.com/x-pan/x-pan-manual.pdf
- QPAS product page: https://www.makenoisemusic.com/modules/qpas/
- QPAS manual: https://www.makenoise-manuals.com/qpas/qpas-manual.pdf
- Wogglebug product page: https://www.makenoisemusic.com/modules/wogglebug/
- Wogglebug manual: https://www.makenoise-manuals.com/wogglebug/wogglebug-manual.pdf
- TEMPI product page: https://www.makenoisemusic.com/modules/tempi/
- TEMPI manual: https://www.makenoise-manuals.com/tempi/tempi-manual.pdf
- 4 Zone CV Bus Case product page: https://www.makenoisemusic.com/cases/4-zone-cv-bus-case/
- Rene product page: https://www.makenoisemusic.com/modules/rene/
- Rene manual: https://www.makenoise-manuals.com/rene/rene-manual.pdf
- PrssPnt product page: https://www.makenoisemusic.com/modules/prsspnt/
- PrssPnt manual: https://www.makenoise-manuals.com/prsspnt/prsspnt-manual.pdf
- DXG product page: https://www.makenoisemusic.com/modules/dxg/
- DXG manual: https://www.makenoise-manuals.com/dxg/dxg-manual.pdf
- Mimeophon product page: https://www.makenoisemusic.com/modules/mimeophon/
- Mimeophon manual: https://www.makenoise-manuals.com/mimeophon/mimeophon-manual.pdf

Internal VibeLang references:

- `kb/morphagene-howto.md`
- `kb/morphagene-synthdef-plan.md`
- `kb/spectraphon-howto.md`
- `kb/spectraphon-synthdef-plan.md`
- `kb/voice-multioutput-howto.md`
- `crates/vibelang-std/stdlib/instruments/eurorack/maths.vibe`
- `crates/vibelang-std/stdlib/cv/eurorack/cv_wogglebug.vibe`
