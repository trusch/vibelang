# VibeLang Tutorials

A hands-on course in 23 short lessons. Each tutorial is a runnable `.vibe`
script — open it, run it, tweak it. Run any tutorial from the repo root:

```bash
vibelang run -w -I . examples/tutorials/01_first_beat.vibe
```

## Core Course (recommended order)

| # | Tutorial | Difficulty | Duration | What you learn |
|---|----------|------------|----------|----------------|
| 01 | [Your First Beat](01_first_beat.vibe) | beginner | 3 min | Create a drum pattern using the stdlib sounds |
| 02 | [Your First Melody](02_first_melody.vibe) | beginner | 4 min | Add a bassline using note names |
| 03 | [Combining Elements](03_combining_elements.vibe) | beginner | 5 min | Build a complete track with drums, bass, and lead |
| 04 | [Step Patterns Deep Dive](04_step_patterns.vibe) | intermediate | 5 min | Master velocity, accents, and complex rhythms |
| 05 | [Euclidean Rhythms](05_euclidean_rhythms.vibe) | intermediate | 5 min | Generate world music rhythms algorithmically |
| 06 | [Polyrhythms](06_polyrhythms.vibe) | intermediate | 5 min | Create complex interlocking rhythms |
| 07 | [Notes & Scales](07_notes_scales.vibe) | intermediate | 5 min | Write melodies in any key |
| 08 | [Chords & Progressions](08_chords_progressions.vibe) | intermediate | 6 min | Build emotional chord progressions |
| 09 | [Arpeggios](09_arpeggios.vibe) | intermediate | 5 min | Create flowing arpeggiated sequences |
| 10 | [Groups & Routing](10_groups_routing.vibe) | intermediate | 5 min | Organize sounds with mixer groups |
| 11 | [Effects: Reverb & Delay](11_effects_reverb_delay.vibe) | intermediate | 6 min | Add space and depth with effects |
| 12 | [Sequences & Arrangement](12_sequences.vibe) | intermediate | 5 min | Arrange patterns over time to create song structures |
| 13 | [Fades & Automation](13_fades.vibe) | intermediate | 7 min | Automate parameters over time with different curves |
| 14 | [Custom Synthesizers](14_custom_synths.vibe) | advanced | 10 min | Build your own instruments with DSP code |
| 15 | [Custom Effects](15_custom_effects.vibe) | advanced | 8 min | Build your own audio effects processors |
| 16 | [CV Sources](16_cv_sources.vibe) | intermediate | 5 min | Add dynamic movement with control-rate modulation (LFOs, envelope followers) |

Natural continuation after the core course:

| # | Tutorial | Difficulty | Duration | What you learn |
|---|----------|------------|----------|----------------|
| 20 | [Scale Degree Notation](20_scale_degree_notation.vibe) | intermediate | 5 min | Write melodies using scale degrees instead of note names |
| 21 | [Multi-Output Voices & CV Routing](21_multi_output_voices.vibe) | advanced | 10 min | Route named synth outputs to groups, main, and parameters |
| 22 | [Sample Playback](22_sample_playback.vibe) | intermediate | 6 min | Load audio files and play them back with pitch, slicing, and warp |
| 23 | [SFZ Instruments](23_sfz_instruments.vibe) | intermediate | 5 min | Play multi-sampled instruments from .sfz files (experimental — see the caveat in the file) |

## Advanced: Script Extensions

These tutorials cover optional runtime extensions that reach outside the
audio sandbox. **Security note:** each one is disabled by default and must
be opted into with its feature flag (`--ext-fs`, `--ext-exec`, `--ext-net`).
In particular, `ext-exec` allows arbitrary command execution — only enable
it for scripts you trust.

| # | Tutorial | Difficulty | Duration | What you learn |
|---|----------|------------|----------|----------------|
| 17 | [Filesystem Extension](17_filesystem_extension.vibe) | intermediate | 5 min | Use file system operations in your scripts (`--ext-fs`) |
| 18 | [Shell Execution Extension](18_exec_extension.vibe) | advanced | 5 min | Execute shell commands and interact with the system (`--ext-exec`) |
| 19 | [Networking Extension](19_networking_extension.vibe) | advanced | 5 min | Make HTTP requests and work with web APIs (`--ext-net`; HTTPS needs `ext-net-tls`) |
