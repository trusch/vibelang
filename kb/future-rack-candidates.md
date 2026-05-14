# Future Rack Candidates

Research date: 2026-05-14.

Purpose: scout hardware modular systems that could become future VibeLang
stdlib rack epics, using the Make Noise ReSynthesizer work as the template:
practical module recreations, a coherent rack namespace, and one `main.vibe`
patch that demonstrates the system as a musical instrument.

## 1. Erica Synths Techno System

Source identity: Erica Synths' Techno System is a complete rhythm and bassline
Eurorack system for techno, industrial, electro, DnB, and live performance.
Erica describes it as an "ultimate tool for rhythm based music production" and
the FAQ lists a travel case plus the Drum Series, Bassline, effects, mixers,
Link, Modulator, and Drum Sequencer as the included ecosystem.

Defining modules and availability:

| Module | Role | Availability posture |
|---|---|---|
| Drum Sequencer | X0X-style live trigger/CV sequencing | Current Drum Series listing |
| Bass Drum2 / Bass Drum | Analog kick voice with pitch, decay, drive/accent CV | Current Drum Series listing |
| Snare Drum | 909-inspired analog snare voice | Current Drum Series listing |
| Toms | Low/mid/high tom analog voice module | Current Drum Series listing |
| Clap | Classic analog clap voice | Current Drum Series listing |
| Hi-Hats D | Digital hi-hat source | Current Drum Series listing |
| Cymbals | Digital/metallic cymbal source | Current Drum Series listing |
| Sample Drum | Two-channel sample playback/record/slicing | Current Drum Series listing |
| Bassline | Analog AS3340 bass/lead voice with Acidbox-style filter | Current Drum Series listing |
| Dual Drive | Stereo/dual saturation stage for drums | Current Drum Series listing |
| Dual FX | Performance delay/reverb/effects block | Current Drum Series listing |
| Drum Mixer / Stereo Mixer / Link | Performance mix, output, and external sync plumbing | Current Drum Series/listed in system |

Official sources:

- System page: https://www.ericasynths.lv/shop/eurorack-systems/techno-system/?v=493
- System manual / patch book: https://www.ericasynths.lv/media/TECHNO_SYSTEM_usermanual_eng_web.pdf
- Techno System FAQ module list: https://www.ericasynths.lv/news/post/faq-techno-system
- Drum Series catalogue: https://www.ericasynths.lv/shop/eurorack-modules/by-series/drum-series/
- Sample Drum manual: https://www.ericasynths.lv/media/sample_drum_user_manual.pdf

Copyright posture: Erica product pages and manuals are proprietary and explicitly
copyrighted. Use them as URL citations and behavioral references only; do not
copy diagrams, panel art, patch-book text, or PDFs into the repo.

Current stdlib audit:

- Strong generic drum overlap: `drums/kicks/kick_909.vibe`,
  `drums/kicks/kick_techno_deep.vibe`,
  `drums/kicks/kick_techno_hard.vibe`, `drums/snares/snare_909.vibe`,
  `drums/claps/clap_808.vibe`, `drums/hihats/hihat_909_closed.vibe`,
  `drums/hihats/hihat_909_open.vibe`, `drums/toms/tom_808.vibe`,
  `drums/percussion/*`.
- Bassline approximations already exist via `synths/tb303.vibe`,
  `bass/acid/*`, `bass/genre/bass_techno.vibe`, and
  `synths/sh101.vibe`.
- Effects/mixing overlap exists through `effects/distortion/*`,
  `effects/delays/dub_delay.vibe`, `effects/delays/delay_bbd_analog.vibe`,
  `effects/reverbs/*`, `processors/mixers/mixer4_stereo.vibe`, and
  `processors/mixers/crossfade_stereo.vibe`.
- CV/sequencer helpers exist in `cv/triggers/cv_clock.vibe`,
  `cv/triggers/cv_euclidean.vibe`, `cv/seq/cv_seq_step.vibe`, and
  `utility/metronome.vibe`.
- Missing: an Erica-specific Drum Sequencer surface, Sample Drum-style slicing
  voice, Bassline panel voice, Dual FX/Dual Drive named processors, and a
  coherent performance-rack import/preset layer.

Rack sp estimate: 21 if scoped as wrappers plus 2-3 new hero modules; 34 if the
epic tries to model Drum Sequencer, Sample Drum slicing, Bassline, Dual FX, and
mix/output routing with high patch fidelity.

`main.vibe` patch: a 128 BPM live techno performance with Drum Sequencer tracks
driving kick/snare/toms/hats/clap, Sample Drum loop slices, Bassline acid riffs,
Dual Drive bus saturation, Dual FX throws, and mixer mutes for a build/drop
arrangement.

Build next? Yes. This is the clearest user-priority rack, maps well to existing
stdlib drums, and would produce an immediately useful VibeLang demo. Scope it as
"Techno System performance rack" rather than exact analog circuit emulation.

## 2. Mutable Instruments Full Lineup

Source identity: a discontinued but heavily archived/open-source digital
Eurorack ecosystem for macro oscillators, resonators, granular processing,
probabilistic CV, function generation, and keyframe modulation. The target
genre is broad: ambient, generative, melodic techno, experimental, and modular
education.

Defining modules and availability:

| Module | Role | Availability posture |
|---|---|---|
| Plaits | 16-model macro oscillator with LPG and aux output | Discontinued 11/2022 |
| Rings | Modal resonator / sympathetic strings | Discontinued 04/2022 |
| Beads | Clouds successor, stereo granular texture synth | Discontinued 12/2022 |
| Marbles | Random trigger/CV source with Deja Vu memory | Discontinued 04/2022 |
| Stages | 6-segment jack-sensed envelope/LFO/sequencer | Discontinued 07/2022 |
| Tides 2018 | Multi-output function generator / oscillator | Discontinued 06/2022 |
| Frames | Four-channel keyframe mixer/morpher | Discontinued 06/2021 |
| Clouds | Original granular texture synth | Discontinued 10/2017 |
| Elements | Modal voice with exciter/resonator structure | Discontinued 01/2022 |
| Warps | Meta-modulator / cross-modulation processor | Discontinued 03/2022 |
| Blades / Ripples | Dual/mono filter voices | Discontinued 04/2022 and 11/2022 |
| Veils / Blinds / Shades | VCAs, polarizers, utilities | Discontinued 2020-2022 |

Official/archive sources:

- Mutable documentation archive: https://pichenettes.github.io/mutable-instruments-documentation/
- Plaits: https://pichenettes.github.io/mutable-instruments-documentation/modules/plaits/
- Rings manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/rings/manual/
- Beads: https://pichenettes.github.io/mutable-instruments-documentation/modules/beads/
- Marbles: https://pichenettes.github.io/mutable-instruments-documentation/modules/marbles/
- Stages: https://pichenettes.github.io/mutable-instruments-documentation/modules/stages/
- Tides 2018: https://pichenettes.github.io/mutable-instruments-documentation/modules/tides_2018/
- Frames manual: https://pichenettes.github.io/mutable-instruments-documentation/modules/frames/manual/
- Clouds: https://pichenettes.github.io/mutable-instruments-documentation/modules/clouds/
- Open-source links are per-module, for example Plaits: https://pichenettes.github.io/mutable-instruments-documentation/modules/plaits/open_source/

Copyright posture: firmware/hardware are largely open-source via the archived
Mutable resources, but docs, manuals, names, graphics, and panel designs still
need attribution and URL-only citation. A VibeLang rack should avoid using
Mutable trademarks as exact product claims unless framed as "inspired by" or
"practical approximation".

Current stdlib audit:

- Dedicated/near-dedicated coverage: `instruments/eurorack/marbles.vibe`,
  `cv/eurorack/cv_marbles.vibe`, `cv/eurorack/cv_stages.vibe`,
  `cv/eurorack/cv_tides.vibe`, and `effects/granular/clouds_processor.vibe`.
- Generic coverage for Rings/Elements/Plaits territory exists through
  `effects/filters/resonator.vibe`, `leads/pluck/*`, `bass/pluck/*`,
  `bells/*`, `world/hang_drum.vibe`, `textures/ambient/texture_granular.vibe`,
  and the granular effect family.
- The existing hotlist notes direct UGen support for `MiPlaits`, `MiBraids`,
  `MiRings`, `MiElements`, `MiClouds`, `MiTides`, `MiWarps`, and `MiGrids`,
  but the prior audit did not find dedicated preset/wrapper files for most of
  these under `stdlib/`.
- Missing: first-class Plaits/Rings/Beads/Clouds/Tides wrappers, a multi-output
  Stages, Frames keyframe morphing, Warps/Ripples/Blades wrappers, and a
  coherent Mutable rack preset/demo layer.

Rack sp estimate: 21 for wrapper-first coverage around existing MI UGens and
CV modules; 34+ if Frames, Stages jack-sensing, Marbles Deja Vu, and Beads
buffer behavior are rebuilt in detail.

`main.vibe` patch: Marbles generates a looping-but-mutating clock and pitch
field, Plaits and Rings form the voice pair, Stages/Tides shape modulation,
Frames morphs four scenes, and Beads/Clouds freezes the texture into an ambient
performance.

Build next? Probably. It has the best DSP feasibility because much of the MI
engine coverage is already available, but it is less distinctive from existing
hotlist work unless the epic commits to rack-level UX: wrappers, presets,
multi-output routing, and a polished generative `main.vibe`.

## 3. 4ms Ambient Rack

Source identity: 4ms' strongest rack identity for VibeLang is clocked ambient
processing: resonator banks, looping delays, sample playback, clockable
envelopes, and performance clock manipulation. The user-specified spine is SMR,
Phaseur, DLD, PEG, and STS; in practice the rack should add 4ms clock modules
and current delay/sample modules to make it musically complete.

Defining modules and availability:

| Module | Role | Availability posture |
|---|---|---|
| Spectral Multiband Resonator (SMR) | 6-band scale-quantized resonator bank | Official product/manual page |
| Dual Looping Delay (DLD) | Dual clocked long delay/looper | Official product/manual page |
| Pingable Envelope Generator (PEG) | Dual clock-synced envelope/function generator | Official product/manual page |
| Stereo Triggered Sampler (STS) | Two-channel stereo sample recorder/player | Official product/manual page |
| Sampler | Newer stereo recorder/player derived from STS ideas | Current 12HP/16HP product page |
| Tapographic Delay | 32-tap performance delay with saved tapographies | Official product/manual page |
| Ensemble Oscillator | Scale-centered oscillator bank | Official product/manual page |
| Shuffling Clock Multiplier (SCM/SCM+) | Multiplied shuffled/skipped clock outputs | Official product/manual page |
| Rotating Clock Divider (RCD) | Rotating clock division | Official product/manual page |
| VCA Matrix | 4x4 playable VCA routing matrix | Official product/manual page |
| Listen Four / mixer/output | Compact performance mix/output layer | Current 4ms ecosystem role |
| Phaseur Fleur | Legacy pedal phaser, not a current Eurorack core | Official legacy pedal page |

Official sources:

- SMR page: https://4mscompany.com/smr.php
- SMR manual: https://4mscompany.com/SMR/manual/SMR-manual-1.1.pdf
- DLD page: https://4mscompany.com/dld.php
- DLD manual: https://4mscompany.com/DLD/manual/DLD-manual-1.1c-v5.pdf
- PEG page: https://4mscompany.com/peg.php
- STS page: https://4mscompany.com/sts.php
- Sampler page/manual: https://4mscompany.com/sampler
- Tapographic Delay: https://4mscompany.com/tapo.php
- Ensemble Oscillator: https://4mscompany.com/enosc.php
- SCM: https://4mscompany.com/scm.php
- RCD: https://4mscompany.com/rcd.php
- VCA Matrix: https://4mscompany.com/vcam.php
- Phaseur Fleur legacy pedal: https://4mscompany.com/phaseurpedal.php

Copyright posture: 4ms manuals/product pages are proprietary. Use URL citations
and behavioral summaries only. Do not import PDF text or patch diagrams.

Current stdlib audit:

- Close but generic effects: `effects/delays/multi_tap_delay.vibe`,
  `effects/delays/delay.vibe`, `effects/delays/granular_delay.vibe`,
  `effects/delays/reverse_delay.vibe`, `processors/delays/mimeophon.vibe`,
  and `effects/modulation/phaser.vibe`.
- Resonator/spectral overlap: `effects/filters/resonator.vibe`,
  `effects/filters/comb_filter.vibe`, `effects/spectral/*`, and
  `textures/drone/drone_resonant.vibe`.
- Clock/CV overlap: `cv/triggers/cv_clock.vibe`,
  `cv/triggers/cv_euclidean.vibe`, `cv/envelopes/cv_env_complex.vibe`,
  `cv/seq/cv_seq_step.vibe`, and `cv/util/cv_mixer4.vibe`.
- Sampler overlap: `instruments/sampler/morphagene.vibe` and granular
  processors cover buffer playback ideas, but not STS bank/sample-file UX.
- Missing: SMR scale-bank resonator, DLD clock-locked loop delay, PEG pingable
  envelope semantics, STS/Sampler banked playback, Tapographic Delay
  tapography memory, and 4ms clock rotation/shuffle modules.

Rack sp estimate: 34. SMR plus DLD/STS/PEG is a real rack epic; even with
generic delay and resonator primitives already present, the musical value is in
clock-synced state, scale banks, loop memory, and multi-output routing.

`main.vibe` patch: Ensemble Oscillator drones feed SMR, DLD and Tapographic
Delay build clocked feedback beds, STS drops field-recording fragments, PEG
shapes filter/delay motion, and SCM/RCD generate evolving but locked rhythmic
events.

Build next? Probably, after Erica. It is highly distinctive from
ReSynthesizer and would improve VibeLang's clocked ambient/effect ecosystem,
but it needs a bigger implementation lane than Mutable wrapper work.

## 4. Verbos Electronics

Source identity: a West Coast/Buchla-influenced Eurorack system centered on
discrete analog tone, harmonic scanning, touch/control performance, random
voltages, multistage sequencing, and immediate panel playability. Target genres:
drone, additive performance, experimental techno, electroacoustic, and
improvised modular.

Defining modules and availability:

| Module | Role | Availability posture |
|---|---|---|
| Harmonic Oscillator | 8 sine harmonics with scan/tilt/center VC mixer | Current Verbos module page |
| Complex Oscillator | West Coast dual oscillator with waveshaping | Current Verbos module page |
| Multi-Delay Processor | 8-tap voltage-controlled delay with followers/mix taps | Current Verbos module page |
| Random Sampling | Fluctuating/stored random, noise, 4-stage analog shift register | Current Verbos module page |
| Voltage Multistage / VMS 16 | Sequencer, multistage envelope, LFO, quantizer | Current Verbos module page |
| Touchplate Keyboard | 32-key touch controller plus bender and tunable keys | Current Verbos module page |
| Scan & Pan | 4-channel VC level/pan scanner | Current Verbos module page |
| Amp & Tone | Discrete low-pass/VCA voice shaper | Current Verbos module page |
| Bark Filter Processor | 12-band Bark-scale filter bank with followers | Current Verbos module page |
| Multi-Envelope | Dual multi-shape envelope generator | Current Verbos module page |
| Sequence Selector | 5-stage sequencer/router | Current Verbos module page |
| Control Voltage Processor | Slew/processing utility layer | Current Verbos module page |

Official sources:

- Modules catalogue: https://www.verboselectronics.com/modules/
- Systems/configurations: https://www.verboselectronics.com/systems
- Harmonic Oscillator: https://www.verboselectronics.com/modules/harmonic-oscillator
- Multi-Delay Processor: https://www.verboselectronics.com/modules/multi-delay-processor
- Random Sampling: https://www.verboselectronics.com/modules/random-sampling
- Voltage Multistage: https://www.verboselectronics.com/modules/voltagemultistage
- Touchplate Keyboard: https://www.verboselectronics.com/modules/touchplate-keyboard
- Complex Oscillator: https://www.verboselectronics.com/modules/complex-oscillator
- Bark Filter Processor: https://www.verboselectronics.com/modules/bark-filter-processor
- Scan & Pan: https://www.verboselectronics.com/modules/scan-pan
- Touchplate Keyboard tech card: https://verboselectronics.com/wp-content/uploads/2025/10/TouchplateKeyboardTechCard.pdf
- Multi-Delay Processor tech card: https://static1.squarespace.com/static/52cddaa9e4b0999c86f84b8a/t/5bcdc61c085229820ce051a7/1540212255908/Multi-Delay%2BProcessor%2BTech%2BCard.pdf

Copyright posture: Verbos product pages and tech cards are proprietary. Use
URL citations and high-level behavioral summaries. Avoid panel-art or tech-card
copying. The design goal should be West Coast behavior, not exact Verbos
branding.

Current stdlib audit:

- Harmonic/additive overlap: `effects/filters/resonator.vibe`,
  `textures/drone/drone_harmonic.vibe`, `bells/*`, and generic oscillator
  primitives in `modular/mod_vco.vibe`, but no Harmonic Oscillator scan/tilt
  wrapper.
- Complex-oscillator/waveshaping overlap: `effects/distortion/waveshaper.vibe`,
  `effects/distortion/saturator.vibe`, `effects/modulation/ring_mod.vibe`, and
  `leads/synth/*`.
- Control overlap: `cv/seq/cv_seq_step.vibe`,
  `cv/envelopes/cv_env_complex.vibe`, `cv/util/cv_sample_hold.vibe`,
  `cv/lfo/cv_lfo_smooth_random.vibe`, `cv/lfo/cv_lfo_chaos.vibe`,
  `cv/util/cv_scale_offset.vibe`, and `cv/util/cv_mixer4.vibe`.
- Delay/filter/mix overlap: `effects/delays/multi_tap_delay.vibe`,
  `effects/filters/bandpass.vibe`, `effects/filters/resonator.vibe`,
  `processors/mixers/x_pan.vibe`, and `processors/mixers/mixer4_stereo.vibe`.
- Missing: Harmonic Oscillator's 8 partial outputs plus scan/tilt mixer,
  Multi-Delay's tap/follower/preset mix behavior, Bark filter follower matrix,
  Random Sampling's stored random/shift-register fanout, Touchplate performance
  controller, and Verbos-style multistage sequencing.

Rack sp estimate: 34+. The sonic identity is strong, but many modules are
multi-output control/performance surfaces, not simple synthdefs.

`main.vibe` patch: Touchplate/VMS addresses a Harmonic Oscillator drone whose
partials are scanned through Scan & Pan and Bark filtering, Random Sampling
perturbs tilt/delay taps, and Multi-Delay turns the additive voice into a
shimmering West Coast performance texture.

Build next? Only if user asks. It is artistically strong and different from
ReSynthesizer, but the touch/control semantics and analog scan behavior make it
a heavier lift than Erica, Mutable, or a focused 4ms rack.

## 5. Intellijel Performance Rack

Source identity: a precise, performance-oriented Eurorack system around
high-spec digital effects, physical-modeling percussion, programmable function
generation, tactile touch control, compact sequencing, and reliable mixing/VCA
utility. Target genres: live techno, IDM, ambient, performance modular, and
hybrid drum/melodic patches.

Defining modules and availability:

| Module | Role | Availability posture |
|---|---|---|
| Rainmaker | 16-tap stereo rhythm delay plus 64-tap comb resonator | Current catalog/product page |
| Plonk | AAS physical-modeling percussion voice | Product page, out of stock in current listing |
| Quadrax | 4-channel function/burst/LFO generator with CV matrix | Current product page |
| Qx | Quadrax EOR/EOF expander | Current product ecosystem |
| Tetrapad | 4-pad, 8-output touch controller | Product page, in stock in current listing |
| Tete | Tetrapad looper/sequencer/preset expander | Product page, in stock in current listing |
| Mixup | Chainable mono/stereo performance mixer | Product page, out of stock in current listing |
| Quad VCA | 4-channel VCA/cascaded mixer | Product page, out of stock in current listing |
| uFold II | Legacy wavefolder/waveshaper | Legacy/manual-only; Bifold is current successor |
| Steppy | 4-track 64-step gate sequencer | Product page, in stock in current listing |
| Metropolix / Scales | Larger sequencing/quantizing performance layer | Current catalog modules |
| Sealegs / Multigrain | Current stereo delay/granular alternatives | Current catalog modules |

Official sources:

- Eurorack catalogue: https://intellijel.com/shop/eurorack/
- Rainmaker page: https://intellijel.com/shop/eurorack/cylonix-rainmaker/
- Rainmaker manual: https://intellijel.com/downloads/manuals/cylonix-rainmaker_manual_v1.09-143.pdf
- Plonk page: https://intellijel.com/shop/eurorack/plonk/
- Quadrax page/manual: https://intellijel.com/shop/eurorack/quadrax/
- Tetrapad page/manual: https://intellijel.com/shop/eurorack/tetrapad/
- Steppy page/manual: https://intellijel.com/shop/eurorack/steppy/
- Mixup page/manual: https://intellijel.com/shop/eurorack/mixup/
- Quad VCA page/manual: https://intellijel.com/shop/eurorack/quad-vca/
- uFold II manual: https://intellijel.com/downloads/manuals/ufold-2_manual_2018.09.13.pdf
- Support/manual index: https://intellijel.com/support/

Copyright posture: Intellijel product pages/manuals are proprietary. Use URLs,
high-level behavior, and independently written summaries only. Avoid copying
manual tables or UI graphics.

Current stdlib audit:

- Rainmaker overlap: `effects/delays/multi_tap_delay.vibe`,
  `effects/delays/delay.vibe`, `effects/filters/comb_filter.vibe`,
  `effects/pitch/pitch_shift.vibe`, and `effects/filters/resonator.vibe`;
  no dedicated Rainmaker module exists.
- Plonk overlap: `bells/marimba.vibe`, `bells/vibraphone.vibe`,
  `world/hang_drum.vibe`, `effects/filters/resonator.vibe`, `leads/pluck/*`,
  and drum/percussion modules. No AAS-style exciter/object Plonk surface exists.
- Quadrax/Tetrapad/Steppy overlap: `cv/envelopes/cv_env_ad.vibe`,
  `cv/envelopes/cv_env_complex.vibe`, `cv/lfo/*`, `cv/triggers/*`,
  `cv/seq/cv_seq_step.vibe`, and `cv/seq/cv_seq_random.vibe`.
- Mix/VCA/fold overlap: `processors/mixers/mixer4_stereo.vibe`,
  `cv/util/cv_mixer4.vibe`, `processors/mixers/crossfade_stereo.vibe`,
  `effects/distortion/waveshaper.vibe`, and `effects/distortion/saturator.vibe`.
- Missing: Rainmaker's preset/tap/comb architecture, Plonk's exciter-resonator
  macro model, Quadrax CV matrix/burst modes, Tetrapad/Tete touch-loop surface,
  Steppy performance gate sequencer, and Intellijel rack integration.

Rack sp estimate: 34+. Rainmaker alone is ReSynthesizer-class or larger; adding
Plonk, Quadrax, Tetrapad/Tete, and Steppy makes the full rack heroic.

`main.vibe` patch: Steppy clocks Plonk percussion and Quadrax modulation,
Tetrapad/Tete morphs delay scenes and Plonk model parameters, Rainmaker turns
short hits into rhythmic comb clouds, and Mixup/Quad VCA manage performance
mutes and dynamics.

Build next? Probably not as a full rack; yes as a focused Rainmaker or Plonk
epic. The full performance rack is compelling, but too broad for the next
stdlib rack unless the user specifically wants Intellijel.

## Honorable Mentions

| Candidate | Why it matters | Stdlib fit | Recommendation |
|---|---|---|---|
| Noise Engineering percussion/noise rack | Basimilus Iteritas Alia, Loquelic Iteritas, Cursus, Desmodus/Versio, Mimetic sequencing define aggressive digital techno/IDM. Official docs are at https://manuals.noiseengineering.us/ and product pages such as https://noiseengineering.us/products/basimilus-iteritas-alia/. | Good overlap with additive/FM drums, wavefolding, distortion, and sequencers, but NE's aliasing/folder character is the hard part. | Strong future techno rack after Erica; maybe a focused Basimilus/Loquelic lane first. |
| Buchla Music Easel / 208C | Portable West Coast instrument: complex oscillator, modulation oscillator, pulser, envelope, random, sequencer, LPGs, spring reverb, touch keyboard. Sources: https://buchla.com/music-easel/, https://buchla.com/musiceasel/, https://buchla.com/download/. | Great conceptual fit for a single coherent `main.vibe`, but not Eurorack and carries strong product identity. | Build only if the user wants a Buchla-style instrument epic. |
| Befaco utility/performance rack | Rampage, Muxlicer, VCMC, Instrument Interface, mixers, and DIY/open documentation make a practical utility/control rack. Source root: https://www.befaco.org/. | Utility-heavy; would improve CV/routing ergonomics more than sonic identity. | Useful later, but not a top rack epic. |
| Roland System-500 | Classic Roland System-100m/System-700-inspired analog Eurorack: 510, 521, 530, 540, 555, 572. Official pages: https://www.roland.com/global/promos/system-500/ and module pages such as https://www.roland.com/us/products/system-500_530/. | Existing subtractive synths, filters, VCAs, envelopes, and phaser/delay already approximate most of it. | Only if user asks for classic Roland workflow. |

## Comparison

| Rank | Candidate | Rack sp | Distinctiveness from ReSynthesizer | Current stdlib leverage | Build next? | Why |
|---|---:|---:|---|---|---|---|
| 1 | Erica Synths Techno System | 21-34 | High: drum/bassline live techno rack instead of spectral/granular Make Noise | Very high generic drum/effects overlap | Yes | User priority; fast path to a strong performance demo. |
| 2 | Mutable Instruments full lineup | 21-34+ | Medium: generative macro-digital ecosystem, but several MI ideas overlap existing hotlist | High UGen/CV/granular leverage | Probably | Best feasibility if scoped as wrappers and presets. |
| 3 | 4ms ambient rack | 34 | High: clocked resonator/delay/sample ambient system | Medium generic delay/resonator/CV leverage | Probably | Distinctive, but SMR/DLD/STS state makes it a larger epic. |
| 4 | Verbos Electronics | 34+ | Very high: West Coast additive/touch/control performance | Medium primitives, low dedicated wrappers | Only if user asks | Musically strong, but heavy control-surface semantics. |
| 5 | Intellijel performance rack | 34+ | Medium-high: precise digital effects plus physical modeling/performance control | Medium-high primitives, no hero wrappers | Only if user asks | Full rack is too broad; Rainmaker or Plonk alone would be better. |

Recommended sequencing:

1. Build the Erica Techno System rack next, scoped around a performance
   namespace, sequencer surface, Sample Drum/Bassline/Dual FX hero modules, and
   a polished `main.vibe`.
2. Follow with Mutable wrapper/preset rack if the goal is fast library breadth.
3. Treat 4ms as the next larger ambient epic once clocked delay/resonator state
   work has a clear lane.
4. Keep Verbos and Intellijel as user-triggered epics or split them into smaller
   hero-module tickets.
