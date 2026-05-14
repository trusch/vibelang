# Mutable Instruments Manual And Firmware Sources

Retrieved: 2026-05-14T23:48:00+02:00

This is the source manifest for the Mutable Instruments rack research wave. It
records manual URLs, community documentation mirrors, and firmware source
locations for the target modules only. Do not commit downloaded manuals, panel
art, diagrams, or firmware copies into this repository.

## License Context

Mutable Instruments' archived `pichenettes/eurorack` repository states the
license split for the public source tree:

- Code for STM32F projects: MIT license.
- Code for AVR projects: GPL-3.0.
- Hardware: CC-BY-SA 3.0.
- Attribution: Emilie Gillet.
- Trademark guidance: do not use "Mutable Instruments" or original module names
  as product names for derivative works.

All target firmware sources below, when present in `pichenettes/eurorack`, are
STM32F projects and should be treated as MIT-licensed firmware with hardware
design files under CC-BY-SA 3.0. Beads is the exception in this list: its manual
is archived, but no official Beads firmware directory exists in
`pichenettes/eurorack`.

Repository license source:

- https://github.com/pichenettes/eurorack
- https://raw.githubusercontent.com/pichenettes/eurorack/master/README.md

## Source Manifest

| Module | Archived official manual URL | Community mirror/manual URL | Firmware source URL | Key DSP/source files | License note |
|---|---|---|---|---|---|
| Plaits | https://mutable-instruments.net/modules/plaits/manual/ | https://pichenettes.github.io/mutable-instruments-documentation/modules/plaits/manual/ | https://github.com/pichenettes/eurorack/tree/master/plaits | `plaits/dsp/voice.cc`, `plaits/dsp/voice.h`, `plaits/dsp/engine/`, `plaits/dsp/engine2/` | STM32F firmware is MIT; hardware files are CC-BY-SA 3.0. |
| Rings | https://mutable-instruments.net/modules/rings/manual/ | https://pichenettes.github.io/mutable-instruments-documentation/modules/rings/manual/ | https://github.com/pichenettes/eurorack/tree/master/rings | `rings/dsp/part.h`, `rings/dsp/resonator.h`, `rings/dsp/string.h`, `rings/dsp/fm_voice.h`, `rings/dsp/fx/reverb.h` | STM32F firmware is MIT; hardware files are CC-BY-SA 3.0. |
| Beads | https://mutable-instruments.net/modules/beads/manual/ | https://pichenettes.github.io/mutable-instruments-documentation/modules/beads/manual/ | Not published in `pichenettes/eurorack`; root tree contains no `beads/` directory. Closest open firmware reference for VibeLang approximation is Clouds: https://github.com/pichenettes/eurorack/tree/master/clouds | Manual-grounded only for Beads behavior. Use Clouds files only as a related granular DSP reference, not as Beads source. | No official Beads firmware source found in `pichenettes/eurorack`; do not claim MIT firmware parity for Beads itself. Clouds firmware is MIT and Clouds hardware is CC-BY-SA 3.0. |
| Clouds | https://mutable-instruments.net/modules/clouds/manual/ | https://pichenettes.github.io/mutable-instruments-documentation/modules/clouds/manual/ | https://github.com/pichenettes/eurorack/tree/master/clouds | `clouds/dsp/granular_processor.h`, `clouds/dsp/granular_processor.cc`, `clouds/dsp/parameters.h`, `clouds/dsp/grain.h`, `clouds/dsp/pvoc/` | STM32F firmware is MIT; hardware files are CC-BY-SA 3.0. |
| Marbles | https://mutable-instruments.net/modules/marbles/manual/ | https://pichenettes.github.io/mutable-instruments-documentation/modules/marbles/manual/ | https://github.com/pichenettes/eurorack/tree/master/marbles | `marbles/random/t_generator.h`, `marbles/random/x_y_generator.h`, `marbles/random/random_sequence.h`, `marbles/random/distributions.h`, `marbles/marbles.cc` | STM32F firmware is MIT; hardware files are CC-BY-SA 3.0. |
| Stages | https://mutable-instruments.net/modules/stages/manual/ | https://pichenettes.github.io/mutable-instruments-documentation/modules/stages/manual/ | https://github.com/pichenettes/eurorack/tree/master/stages | `stages/segment_generator.h`, `stages/segment_generator.cc`, `stages/chain_state.h`, `stages/chain_state.cc`, `stages/variable_shape_oscillator.h` | STM32F firmware is MIT; hardware files are CC-BY-SA 3.0. |
| Tides | https://mutable-instruments.net/modules/tides/manual/ | https://pichenettes.github.io/mutable-instruments-documentation/modules/tides/manual/ | https://github.com/pichenettes/eurorack/tree/master/tides | `tides/generator.h`, `tides/generator.cc`, `tides/cv_scaler.h`, `tides/tides.cc` | STM32F firmware is MIT; hardware files are CC-BY-SA 3.0. |
| Frames | https://mutable-instruments.net/modules/frames/manual/ | https://pichenettes.github.io/mutable-instruments-documentation/modules/frames/manual/ | https://github.com/pichenettes/eurorack/tree/master/frames | `frames/keyframer.h`, `frames/keyframer.cc`, `frames/poly_lfo.h`, `frames/frames.cc` | STM32F firmware is MIT; hardware files are CC-BY-SA 3.0. |
| Tides2 / Tides 2018 | https://mutable-instruments.net/modules/tides_2018/manual/ | https://pichenettes.github.io/mutable-instruments-documentation/modules/tides_2018/manual/ | https://github.com/pichenettes/eurorack/tree/master/tides2 | `tides2/poly_slope_generator.h`, `tides2/poly_slope_generator.cc`, `tides2/ramp_generator.h`, `tides2/ramp_shaper.h`, `tides2/tides.cc` | STM32F firmware is MIT; hardware files are CC-BY-SA 3.0. |

## Additional Reference Pages

Per-module open-source pages in the documentation mirror are useful for build
instructions and license confirmation:

- Plaits: https://pichenettes.github.io/mutable-instruments-documentation/modules/plaits/open_source/
- Rings: https://pichenettes.github.io/mutable-instruments-documentation/modules/rings/open_source/
- Clouds: https://pichenettes.github.io/mutable-instruments-documentation/modules/clouds/open_source/
- Marbles: https://pichenettes.github.io/mutable-instruments-documentation/modules/marbles/open_source/
- Stages: https://pichenettes.github.io/mutable-instruments-documentation/modules/stages/open_source/
- Tides original: https://pichenettes.github.io/mutable-instruments-documentation/modules/tides/open_source/
- Frames: https://pichenettes.github.io/mutable-instruments-documentation/modules/frames/open_source/
- Tides 2018: https://pichenettes.github.io/mutable-instruments-documentation/modules/tides_2018/open_source/

Quickstart PDF mirrors are linked from each module index page in the
documentation archive. Keep them as external references only.

## Notes For Implementation Waves

- Use firmware sources for DSP signatures and parameter naming, but implement
  original VibeLang stdlib modules rather than copying source code.
- Avoid product/trademark language in user-facing APIs when practical. The
  epic/ticket names can keep the historical reference, but shipped stdlib docs
  should describe these as MI-inspired practical recreations.
- Beads must be marked manual-grounded unless a later official source release is
  found. Do not cite Clouds source as Beads firmware.
