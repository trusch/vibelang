# Generated standard-library index

> Inventory generated mechanically from literal DSP definitions and Rhai function declarations under `crates/vibelang-std/stdlib`. The supported-public distinction is a documented convention until explicit export metadata exists.

The shipped tree contains **829 `.vibe` files**, **890 DSP definition occurrences / 887 unique names**, and **707 function declarations**. Of those functions, **595 declarations / 594 unique names** are intended-supported public API and **112 underscore-prefixed declarations** are implementation-convention helpers. Rhai does not enforce that convention: the source contains no `private fn`, so an underscore helper remains callable after importing its module. Both sets are indexed below.

## Import and contract model

Definitions and functions are not global merely because they are shipped. Import the source module, optionally with a namespace:

```rhai
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/theory/chords.vibe" as chords;

let kick = voice("kick").synth("kick_808");
let cm = chords::minor_triad("C");
```

For a DSP definition, the linked `.vibe` source is the current exact parameter/default and named input/output contract: repeated `.param`, `.input`, `.output`, `.output_kr`, and `.output_tr` calls feed the builders documented in [DSP](../dsp.md). The catalogue does not duplicate those chains with a regex because nested Rhai expressions make that unsafe. The P0 generator plan is to parse Rhai or consume explicit metadata.

For script functions, Rhai signatures declare argument names but no static return type. The table gives every exact public name/argument list and a source-line link for dynamic return/error behavior. Wrapper names commonly encode defaults in their body while an `_ex` sibling exposes explicit controls.

## Duplicate-name warnings

DSP duplicates: `lfo_random` (2 definitions), `lfo_saw` (2 definitions), `lfo_sine` (2 definitions). Public function duplicate: `arpeggio_up_down` (theory/arpeggios.vibe and theory/bass_patterns.vibe). Import namespaces avoid function ambiguity; synthdef registry replacement remains source-order-sensitive.

## DSP definitions

### bass

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `acid_303_classic` | `define_synthdef` | `stdlib/bass/acid/acid_303_classic.vibe` | [bass/acid/acid_303_classic.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_303_classic.vibe#L2) |
| `acid_aggressive` | `define_synthdef` | `stdlib/bass/acid/acid_aggressive.vibe` | [bass/acid/acid_aggressive.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_aggressive.vibe#L2) |
| `acid_bubbly` | `define_synthdef` | `stdlib/bass/acid/acid_bubbly.vibe` | [bass/acid/acid_bubbly.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_bubbly.vibe#L2) |
| `acid_detuned` | `define_synthdef` | `stdlib/bass/acid/acid_detuned.vibe` | [bass/acid/acid_detuned.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_detuned.vibe#L2) |
| `acid_distorted` | `define_synthdef` | `stdlib/bass/acid/acid_distorted.vibe` | [bass/acid/acid_distorted.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_distorted.vibe#L2) |
| `acid_filtered_square` | `define_synthdef` | `stdlib/bass/acid/acid_filtered_square.vibe` | [bass/acid/acid_filtered_square.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_filtered_square.vibe#L2) |
| `acid_minimal` | `define_synthdef` | `stdlib/bass/acid/acid_minimal.vibe` | [bass/acid/acid_minimal.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_minimal.vibe#L2) |
| `acid_modulated` | `define_synthdef` | `stdlib/bass/acid/acid_modulated.vibe` | [bass/acid/acid_modulated.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_modulated.vibe#L2) |
| `acid_resonant_sweep` | `define_synthdef` | `stdlib/bass/acid/acid_resonant_sweep.vibe` | [bass/acid/acid_resonant_sweep.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_resonant_sweep.vibe#L2) |
| `acid_squelchy` | `define_synthdef` | `stdlib/bass/acid/acid_squelchy.vibe` | [bass/acid/acid_squelchy.vibe:2](../../../crates/vibelang-std/stdlib/bass/acid/acid_squelchy.vibe#L2) |
| `bass_101` | `define_synthdef` | `stdlib/bass/synth/bass_101.vibe` | [bass/synth/bass_101.vibe:2](../../../crates/vibelang-std/stdlib/bass/synth/bass_101.vibe#L2) |
| `bass_afrobeat` | `define_synthdef` | `stdlib/bass/genre/bass_afrobeat.vibe` | [bass/genre/bass_afrobeat.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_afrobeat.vibe#L2) |
| `bass_disco` | `define_synthdef` | `stdlib/bass/genre/bass_disco.vibe` | [bass/genre/bass_disco.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_disco.vibe#L2) |
| `bass_drill` | `define_synthdef` | `stdlib/bass/genre/bass_drill.vibe` | [bass/genre/bass_drill.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_drill.vibe#L2) |
| `bass_dx7` | `define_synthdef` | `stdlib/bass/synth/bass_dx7.vibe` | [bass/synth/bass_dx7.vibe:2](../../../crates/vibelang-std/stdlib/bass/synth/bass_dx7.vibe#L2) |
| `bass_fingered` | `define_synthdef` | `stdlib/bass/acoustic/bass_fingered.vibe` | [bass/acoustic/bass_fingered.vibe:2](../../../crates/vibelang-std/stdlib/bass/acoustic/bass_fingered.vibe#L2) |
| `bass_formant` | `define_synthdef` | `stdlib/bass/genre/bass_formant.vibe` | [bass/genre/bass_formant.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_formant.vibe#L2) |
| `bass_fretless` | `define_synthdef` | `stdlib/bass/acoustic/bass_fretless.vibe` | [bass/acoustic/bass_fretless.vibe:2](../../../crates/vibelang-std/stdlib/bass/acoustic/bass_fretless.vibe#L2) |
| `bass_funk` | `define_synthdef` | `stdlib/bass/genre/bass_funk.vibe` | [bass/genre/bass_funk.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_funk.vibe#L2) |
| `bass_garage` | `define_synthdef` | `stdlib/bass/genre/bass_garage.vibe` | [bass/genre/bass_garage.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_garage.vibe#L2) |
| `bass_granular` | `define_synthdef` | `stdlib/bass/synth/bass_granular.vibe` | [bass/synth/bass_granular.vibe:2](../../../crates/vibelang-std/stdlib/bass/synth/bass_granular.vibe#L2) |
| `bass_house` | `define_synthdef` | `stdlib/bass/genre/bass_house.vibe` | [bass/genre/bass_house.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_house.vibe#L2) |
| `bass_juno` | `define_synthdef` | `stdlib/bass/synth/bass_juno.vibe` | [bass/synth/bass_juno.vibe:2](../../../crates/vibelang-std/stdlib/bass/synth/bass_juno.vibe#L2) |
| `bass_karplus` | `define_synthdef` | `stdlib/bass/genre/bass_karplus.vibe` | [bass/genre/bass_karplus.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_karplus.vibe#L2) |
| `bass_moog` | `define_synthdef` | `stdlib/bass/synth/bass_moog.vibe` | [bass/synth/bass_moog.vibe:2](../../../crates/vibelang-std/stdlib/bass/synth/bass_moog.vibe#L2) |
| `bass_ms20` | `define_synthdef` | `stdlib/bass/synth/bass_ms20.vibe` | [bass/synth/bass_ms20.vibe:2](../../../crates/vibelang-std/stdlib/bass/synth/bass_ms20.vibe#L2) |
| `bass_muted` | `define_synthdef` | `stdlib/bass/acoustic/bass_muted.vibe` | [bass/acoustic/bass_muted.vibe:2](../../../crates/vibelang-std/stdlib/bass/acoustic/bass_muted.vibe#L2) |
| `bass_phonk` | `define_synthdef` | `stdlib/bass/genre/bass_phonk.vibe` | [bass/genre/bass_phonk.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_phonk.vibe#L2) |
| `bass_picked` | `define_synthdef` | `stdlib/bass/acoustic/bass_picked.vibe` | [bass/acoustic/bass_picked.vibe:2](../../../crates/vibelang-std/stdlib/bass/acoustic/bass_picked.vibe#L2) |
| `bass_pluck_bell` | `define_synthdef` | `stdlib/bass/pluck/pluck_bell.vibe` | [bass/pluck/pluck_bell.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_bell.vibe#L2) |
| `bass_pluck_bright` | `define_synthdef` | `stdlib/bass/pluck/pluck_bright.vibe` | [bass/pluck/pluck_bright.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_bright.vibe#L2) |
| `bass_pluck_long` | `define_synthdef` | `stdlib/bass/pluck/pluck_long.vibe` | [bass/pluck/pluck_long.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_long.vibe#L2) |
| `bass_pluck_muted` | `define_synthdef` | `stdlib/bass/pluck/pluck_muted.vibe` | [bass/pluck/pluck_muted.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_muted.vibe#L2) |
| `bass_pluck_resonant` | `define_synthdef` | `stdlib/bass/pluck/pluck_resonant.vibe` | [bass/pluck/pluck_resonant.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_resonant.vibe#L2) |
| `bass_pluck_short` | `define_synthdef` | `stdlib/bass/pluck/pluck_short.vibe` | [bass/pluck/pluck_short.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_short.vibe#L2) |
| `bass_prophet` | `define_synthdef` | `stdlib/bass/synth/bass_prophet.vibe` | [bass/synth/bass_prophet.vibe:2](../../../crates/vibelang-std/stdlib/bass/synth/bass_prophet.vibe#L2) |
| `bass_reggae` | `define_synthdef` | `stdlib/bass/genre/bass_reggae.vibe` | [bass/genre/bass_reggae.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_reggae.vibe#L2) |
| `bass_slap` | `define_synthdef` | `stdlib/bass/acoustic/bass_slap.vibe` | [bass/acoustic/bass_slap.vibe:2](../../../crates/vibelang-std/stdlib/bass/acoustic/bass_slap.vibe#L2) |
| `bass_talking` | `define_synthdef` | `stdlib/bass/genre/bass_talking.vibe` | [bass/genre/bass_talking.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_talking.vibe#L2) |
| `bass_techno` | `define_synthdef` | `stdlib/bass/genre/bass_techno.vibe` | [bass/genre/bass_techno.vibe:2](../../../crates/vibelang-std/stdlib/bass/genre/bass_techno.vibe#L2) |
| `bass_upright` | `define_synthdef` | `stdlib/bass/acoustic/bass_upright.vibe` | [bass/acoustic/bass_upright.vibe:2](../../../crates/vibelang-std/stdlib/bass/acoustic/bass_upright.vibe#L2) |
| `bass_upright_arco` | `define_synthdef` | `stdlib/bass/acoustic/bass_upright_arco.vibe` | [bass/acoustic/bass_upright_arco.vibe:2](../../../crates/vibelang-std/stdlib/bass/acoustic/bass_upright_arco.vibe#L2) |
| `bass_upright_pizz` | `define_synthdef` | `stdlib/bass/acoustic/bass_upright_pizz.vibe` | [bass/acoustic/bass_upright_pizz.vibe:2](../../../crates/vibelang-std/stdlib/bass/acoustic/bass_upright_pizz.vibe#L2) |
| `bass_wavetable` | `define_synthdef` | `stdlib/bass/synth/bass_wavetable.vibe` | [bass/synth/bass_wavetable.vibe:2](../../../crates/vibelang-std/stdlib/bass/synth/bass_wavetable.vibe#L2) |
| `fm_bass_aggressive` | `define_synthdef` | `stdlib/bass/fm/fm_bass_aggressive.vibe` | [bass/fm/fm_bass_aggressive.vibe:2](../../../crates/vibelang-std/stdlib/bass/fm/fm_bass_aggressive.vibe#L2) |
| `fm_bass_classic` | `define_synthdef` | `stdlib/bass/fm/fm_bass_classic.vibe` | [bass/fm/fm_bass_classic.vibe:2](../../../crates/vibelang-std/stdlib/bass/fm/fm_bass_classic.vibe#L2) |
| `fm_bass_deep` | `define_synthdef` | `stdlib/bass/fm/fm_bass_deep.vibe` | [bass/fm/fm_bass_deep.vibe:2](../../../crates/vibelang-std/stdlib/bass/fm/fm_bass_deep.vibe#L2) |
| `fm_bass_evolving` | `define_synthdef` | `stdlib/bass/fm/fm_bass_evolving.vibe` | [bass/fm/fm_bass_evolving.vibe:2](../../../crates/vibelang-std/stdlib/bass/fm/fm_bass_evolving.vibe#L2) |
| `fm_bass_metallic` | `define_synthdef` | `stdlib/bass/fm/fm_bass_metallic.vibe` | [bass/fm/fm_bass_metallic.vibe:2](../../../crates/vibelang-std/stdlib/bass/fm/fm_bass_metallic.vibe#L2) |
| `pluck_dark` | `define_synthdef` | `stdlib/bass/pluck/pluck_dark.vibe` | [bass/pluck/pluck_dark.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_dark.vibe#L2) |
| `pluck_elastic` | `define_synthdef` | `stdlib/bass/pluck/pluck_elastic.vibe` | [bass/pluck/pluck_elastic.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_elastic.vibe#L2) |
| `pluck_funky` | `define_synthdef` | `stdlib/bass/pluck/pluck_funky.vibe` | [bass/pluck/pluck_funky.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_funky.vibe#L2) |
| `pluck_percussive` | `define_synthdef` | `stdlib/bass/pluck/pluck_percussive.vibe` | [bass/pluck/pluck_percussive.vibe:2](../../../crates/vibelang-std/stdlib/bass/pluck/pluck_percussive.vibe#L2) |
| `reese_aggressive` | `define_synthdef` | `stdlib/bass/reese/reese_aggressive.vibe` | [bass/reese/reese_aggressive.vibe:2](../../../crates/vibelang-std/stdlib/bass/reese/reese_aggressive.vibe#L2) |
| `reese_classic` | `define_synthdef` | `stdlib/bass/reese/reese_classic.vibe` | [bass/reese/reese_classic.vibe:2](../../../crates/vibelang-std/stdlib/bass/reese/reese_classic.vibe#L2) |
| `reese_deep` | `define_synthdef` | `stdlib/bass/reese/reese_deep.vibe` | [bass/reese/reese_deep.vibe:2](../../../crates/vibelang-std/stdlib/bass/reese/reese_deep.vibe#L2) |
| `reese_distorted` | `define_synthdef` | `stdlib/bass/reese/reese_distorted.vibe` | [bass/reese/reese_distorted.vibe:2](../../../crates/vibelang-std/stdlib/bass/reese/reese_distorted.vibe#L2) |
| `reese_evolving` | `define_synthdef` | `stdlib/bass/reese/reese_evolving.vibe` | [bass/reese/reese_evolving.vibe:2](../../../crates/vibelang-std/stdlib/bass/reese/reese_evolving.vibe#L2) |
| `reese_smooth` | `define_synthdef` | `stdlib/bass/reese/reese_smooth.vibe` | [bass/reese/reese_smooth.vibe:2](../../../crates/vibelang-std/stdlib/bass/reese/reese_smooth.vibe#L2) |
| `sub_deep` | `define_synthdef` | `stdlib/bass/sub/sub_deep.vibe` | [bass/sub/sub_deep.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_deep.vibe#L2) |
| `sub_filtered` | `define_synthdef` | `stdlib/bass/sub/sub_filtered.vibe` | [bass/sub/sub_filtered.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_filtered.vibe#L2) |
| `sub_harmonic` | `define_synthdef` | `stdlib/bass/sub/sub_harmonic.vibe` | [bass/sub/sub_harmonic.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_harmonic.vibe#L2) |
| `sub_modulated` | `define_synthdef` | `stdlib/bass/sub/sub_modulated.vibe` | [bass/sub/sub_modulated.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_modulated.vibe#L2) |
| `sub_mono` | `define_synthdef` | `stdlib/bass/sub/sub_mono.vibe` | [bass/sub/sub_mono.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_mono.vibe#L2) |
| `sub_octave` | `define_synthdef` | `stdlib/bass/sub/sub_octave.vibe` | [bass/sub/sub_octave.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_octave.vibe#L2) |
| `sub_pure_sine` | `define_synthdef` | `stdlib/bass/sub/sub_pure_sine.vibe` | [bass/sub/sub_pure_sine.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_pure_sine.vibe#L2) |
| `sub_stereo` | `define_synthdef` | `stdlib/bass/sub/sub_stereo.vibe` | [bass/sub/sub_stereo.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_stereo.vibe#L2) |
| `sub_triangle` | `define_synthdef` | `stdlib/bass/sub/sub_triangle.vibe` | [bass/sub/sub_triangle.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_triangle.vibe#L2) |
| `sub_warm` | `define_synthdef` | `stdlib/bass/sub/sub_warm.vibe` | [bass/sub/sub_warm.vibe:2](../../../crates/vibelang-std/stdlib/bass/sub/sub_warm.vibe#L2) |
| `wobble_aggressive` | `define_synthdef` | `stdlib/bass/wobble/wobble_aggressive.vibe` | [bass/wobble/wobble_aggressive.vibe:2](../../../crates/vibelang-std/stdlib/bass/wobble/wobble_aggressive.vibe#L2) |
| `wobble_classic` | `define_synthdef` | `stdlib/bass/wobble/wobble_classic.vibe` | [bass/wobble/wobble_classic.vibe:2](../../../crates/vibelang-std/stdlib/bass/wobble/wobble_classic.vibe#L2) |
| `wobble_deep` | `define_synthdef` | `stdlib/bass/wobble/wobble_deep.vibe` | [bass/wobble/wobble_deep.vibe:2](../../../crates/vibelang-std/stdlib/bass/wobble/wobble_deep.vibe#L2) |
| `wobble_fm` | `define_synthdef` | `stdlib/bass/wobble/wobble_fm.vibe` | [bass/wobble/wobble_fm.vibe:2](../../../crates/vibelang-std/stdlib/bass/wobble/wobble_fm.vibe#L2) |
| `wobble_smooth` | `define_synthdef` | `stdlib/bass/wobble/wobble_smooth.vibe` | [bass/wobble/wobble_smooth.vibe:2](../../../crates/vibelang-std/stdlib/bass/wobble/wobble_smooth.vibe#L2) |
| `wobble_squelch` | `define_synthdef` | `stdlib/bass/wobble/wobble_squelch.vibe` | [bass/wobble/wobble_squelch.vibe:2](../../../crates/vibelang-std/stdlib/bass/wobble/wobble_squelch.vibe#L2) |

### bells

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `church_bell` | `define_synthdef` | `stdlib/bells/church_bell.vibe` | [bells/church_bell.vibe:6](../../../crates/vibelang-std/stdlib/bells/church_bell.vibe#L6) |
| `glockenspiel` | `define_synthdef` | `stdlib/bells/glockenspiel.vibe` | [bells/glockenspiel.vibe:6](../../../crates/vibelang-std/stdlib/bells/glockenspiel.vibe#L6) |
| `marimba` | `define_synthdef` | `stdlib/bells/marimba.vibe` | [bells/marimba.vibe:6](../../../crates/vibelang-std/stdlib/bells/marimba.vibe#L6) |
| `music_box` | `define_synthdef` | `stdlib/bells/music_box.vibe` | [bells/music_box.vibe:6](../../../crates/vibelang-std/stdlib/bells/music_box.vibe#L6) |
| `tubular_bells` | `define_synthdef` | `stdlib/bells/tubular_bells.vibe` | [bells/tubular_bells.vibe:6](../../../crates/vibelang-std/stdlib/bells/tubular_bells.vibe#L6) |
| `vibraphone` | `define_synthdef` | `stdlib/bells/vibraphone.vibe` | [bells/vibraphone.vibe:6](../../../crates/vibelang-std/stdlib/bells/vibraphone.vibe#L6) |
| `xylophone` | `define_synthdef` | `stdlib/bells/xylophone.vibe` | [bells/xylophone.vibe:6](../../../crates/vibelang-std/stdlib/bells/xylophone.vibe#L6) |

### brass

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `bass_trombone` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:264](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L264) |
| `brass_section` | `define_synthdef` | `stdlib/brass/brass_section.vibe` | [brass/brass_section.vibe:6](../../../crates/vibelang-std/stdlib/brass/brass_section.vibe#L6) |
| `brass_section_realistic` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:455](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L455) |
| `euphonium` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:408](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L408) |
| `flugelhorn` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:158](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L158) |
| `french_horn` | `define_synthdef` | `stdlib/brass/french_horn.vibe` | [brass/french_horn.vibe:6](../../../crates/vibelang-std/stdlib/brass/french_horn.vibe#L6) |
| `french_horn_realistic` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:308](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L308) |
| `synth_brass` | `define_synthdef` | `stdlib/brass/synth_brass.vibe` | [brass/synth_brass.vibe:6](../../../crates/vibelang-std/stdlib/brass/synth_brass.vibe#L6) |
| `trombone` | `define_synthdef` | `stdlib/brass/trombone.vibe` | [brass/trombone.vibe:6](../../../crates/vibelang-std/stdlib/brass/trombone.vibe#L6) |
| `trombone_realistic` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:209](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L209) |
| `trumpet` | `define_synthdef` | `stdlib/brass/trumpet.vibe` | [brass/trumpet.vibe:6](../../../crates/vibelang-std/stdlib/brass/trumpet.vibe#L6) |
| `trumpet_cup_mute` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:123](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L123) |
| `trumpet_harmon_mute` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:76](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L76) |
| `trumpet_realistic` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:12](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L12) |
| `tuba` | `define_synthdef` | `stdlib/brass/tuba.vibe` | [brass/tuba.vibe:6](../../../crates/vibelang-std/stdlib/brass/tuba.vibe#L6) |
| `tuba_realistic` | `define_synthdef` | `stdlib/brass/brass_realistic.vibe` | [brass/brass_realistic.vibe:362](../../../crates/vibelang-std/stdlib/brass/brass_realistic.vibe#L362) |

### cinematic

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `braam` | `define_synthdef` | `stdlib/cinematic/braam.vibe` | [cinematic/braam.vibe:2](../../../crates/vibelang-std/stdlib/cinematic/braam.vibe#L2) |
| `drone_tension` | `define_synthdef` | `stdlib/cinematic/drone_tension.vibe` | [cinematic/drone_tension.vibe:2](../../../crates/vibelang-std/stdlib/cinematic/drone_tension.vibe#L2) |
| `impact_cinematic` | `define_synthdef` | `stdlib/cinematic/impact_cinematic.vibe` | [cinematic/impact_cinematic.vibe:2](../../../crates/vibelang-std/stdlib/cinematic/impact_cinematic.vibe#L2) |
| `whoosh_tonal` | `define_synthdef` | `stdlib/cinematic/whoosh_tonal.vibe` | [cinematic/whoosh_tonal.vibe:2](../../../crates/vibelang-std/stdlib/cinematic/whoosh_tonal.vibe#L2) |

### cv

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `cv_arpeggio` | `define_synthdef` | `stdlib/cv/seq/cv_arpeggio.vibe` | [cv/seq/cv_arpeggio.vibe:28](../../../crates/vibelang-std/stdlib/cv/seq/cv_arpeggio.vibe#L28) |
| `cv_burst` | `define_synthdef` | `stdlib/cv/triggers/cv_burst.vibe` | [cv/triggers/cv_burst.vibe:19](../../../crates/vibelang-std/stdlib/cv/triggers/cv_burst.vibe#L19) |
| `cv_clock` | `define_synthdef` | `stdlib/cv/triggers/cv_clock.vibe` | [cv/triggers/cv_clock.vibe:9](../../../crates/vibelang-std/stdlib/cv/triggers/cv_clock.vibe#L9) |
| `cv_diff` | `define_synthdef` | `stdlib/cv/util/cv_diff.vibe` | [cv/util/cv_diff.vibe:13](../../../crates/vibelang-std/stdlib/cv/util/cv_diff.vibe#L13) |
| `cv_drift` | `define_synthdef` | `stdlib/cv/pitch/cv_drift.vibe` | [cv/pitch/cv_drift.vibe:21](../../../crates/vibelang-std/stdlib/cv/pitch/cv_drift.vibe#L21) |
| `cv_env_ad` | `define_synthdef` | `stdlib/cv/envelopes/cv_env_ad.vibe` | [cv/envelopes/cv_env_ad.vibe:28](../../../crates/vibelang-std/stdlib/cv/envelopes/cv_env_ad.vibe#L28) |
| `cv_env_adsr` | `define_synthdef` | `stdlib/cv/envelopes/cv_env_adsr.vibe` | [cv/envelopes/cv_env_adsr.vibe:43](../../../crates/vibelang-std/stdlib/cv/envelopes/cv_env_adsr.vibe#L43) |
| `cv_env_complex` | `define_synthdef` | `stdlib/cv/envelopes/cv_env_complex.vibe` | [cv/envelopes/cv_env_complex.vibe:35](../../../crates/vibelang-std/stdlib/cv/envelopes/cv_env_complex.vibe#L35) |
| `cv_euclidean` | `define_synthdef` | `stdlib/cv/triggers/cv_euclidean.vibe` | [cv/triggers/cv_euclidean.vibe:13](../../../crates/vibelang-std/stdlib/cv/triggers/cv_euclidean.vibe#L13) |
| `cv_gate` | `define_synthdef` | `stdlib/cv/triggers/cv_gate.vibe` | [cv/triggers/cv_gate.vibe:14](../../../crates/vibelang-std/stdlib/cv/triggers/cv_gate.vibe#L14) |
| `cv_inv` | `define_synthdef` | `stdlib/cv/util/cv_inv.vibe` | [cv/util/cv_inv.vibe:10](../../../crates/vibelang-std/stdlib/cv/util/cv_inv.vibe#L10) |
| `cv_lfo_chaos` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_chaos.vibe` | [cv/lfo/cv_lfo_chaos.vibe:32](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_chaos.vibe#L32) |
| `cv_lfo_chaos_kr` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_chaos_kr.vibe` | [cv/lfo/cv_lfo_chaos_kr.vibe:24](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_chaos_kr.vibe#L24) |
| `cv_lfo_random` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_random.vibe` | [cv/lfo/cv_lfo_random.vibe:20](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_random.vibe#L20) |
| `cv_lfo_random_kr` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_random_kr.vibe` | [cv/lfo/cv_lfo_random_kr.vibe:19](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_random_kr.vibe#L19) |
| `cv_lfo_sine` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_sine.vibe` | [cv/lfo/cv_lfo_sine.vibe:21](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_sine.vibe#L21) |
| `cv_lfo_sine_kr` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_sine_kr.vibe` | [cv/lfo/cv_lfo_sine_kr.vibe:22](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_sine_kr.vibe#L22) |
| `cv_lfo_smooth_random` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_smooth_random.vibe` | [cv/lfo/cv_lfo_smooth_random.vibe:23](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_smooth_random.vibe#L23) |
| `cv_lfo_smooth_random_kr` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_smooth_random_kr.vibe` | [cv/lfo/cv_lfo_smooth_random_kr.vibe:19](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_smooth_random_kr.vibe#L19) |
| `cv_lfo_square` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_square.vibe` | [cv/lfo/cv_lfo_square.vibe:26](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_square.vibe#L26) |
| `cv_lfo_square_kr` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_square_kr.vibe` | [cv/lfo/cv_lfo_square_kr.vibe:23](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_square_kr.vibe#L23) |
| `cv_lfo_tri` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_tri.vibe` | [cv/lfo/cv_lfo_tri.vibe:17](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_tri.vibe#L17) |
| `cv_lfo_tri_kr` | `define_synthdef` | `stdlib/cv/lfo/cv_lfo_tri_kr.vibe` | [cv/lfo/cv_lfo_tri_kr.vibe:18](../../../crates/vibelang-std/stdlib/cv/lfo/cv_lfo_tri_kr.vibe#L18) |
| `cv_marbles` | `define_synthdef` | `stdlib/cv/eurorack/cv_marbles.vibe` | [cv/eurorack/cv_marbles.vibe:49](../../../crates/vibelang-std/stdlib/cv/eurorack/cv_marbles.vibe#L49) |
| `cv_maths` | `define_synthdef` | `stdlib/cv/eurorack/cv_maths.vibe` | [cv/eurorack/cv_maths.vibe:35](../../../crates/vibelang-std/stdlib/cv/eurorack/cv_maths.vibe#L35) |
| `cv_max` | `define_synthdef` | `stdlib/cv/util/cv_max.vibe` | [cv/util/cv_max.vibe:9](../../../crates/vibelang-std/stdlib/cv/util/cv_max.vibe#L9) |
| `cv_min` | `define_synthdef` | `stdlib/cv/util/cv_min.vibe` | [cv/util/cv_min.vibe:9](../../../crates/vibelang-std/stdlib/cv/util/cv_min.vibe#L9) |
| `cv_mixer4` | `define_synthdef` | `stdlib/cv/util/cv_mixer4.vibe` | [cv/util/cv_mixer4.vibe:18](../../../crates/vibelang-std/stdlib/cv/util/cv_mixer4.vibe#L18) |
| `cv_quantizer` | `define_synthdef` | `stdlib/cv/pitch/cv_quantizer.vibe` | [cv/pitch/cv_quantizer.vibe:27](../../../crates/vibelang-std/stdlib/cv/pitch/cv_quantizer.vibe#L27) |
| `cv_rectify` | `define_synthdef` | `stdlib/cv/util/cv_rectify.vibe` | [cv/util/cv_rectify.vibe:17](../../../crates/vibelang-std/stdlib/cv/util/cv_rectify.vibe#L17) |
| `cv_sample_hold` | `define_synthdef` | `stdlib/cv/util/cv_sample_hold.vibe` | [cv/util/cv_sample_hold.vibe:18](../../../crates/vibelang-std/stdlib/cv/util/cv_sample_hold.vibe#L18) |
| `cv_scale_offset` | `define_synthdef` | `stdlib/cv/util/cv_scale_offset.vibe` | [cv/util/cv_scale_offset.vibe:18](../../../crates/vibelang-std/stdlib/cv/util/cv_scale_offset.vibe#L18) |
| `cv_seq_markov` | `define_synthdef` | `stdlib/cv/seq/cv_seq_markov.vibe` | [cv/seq/cv_seq_markov.vibe:36](../../../crates/vibelang-std/stdlib/cv/seq/cv_seq_markov.vibe#L36) |
| `cv_seq_random` | `define_synthdef` | `stdlib/cv/seq/cv_seq_random.vibe` | [cv/seq/cv_seq_random.vibe:30](../../../crates/vibelang-std/stdlib/cv/seq/cv_seq_random.vibe#L30) |
| `cv_seq_step` | `define_synthdef` | `stdlib/cv/seq/cv_seq_step.vibe` | [cv/seq/cv_seq_step.vibe:22](../../../crates/vibelang-std/stdlib/cv/seq/cv_seq_step.vibe#L22) |
| `cv_slew` | `define_synthdef` | `stdlib/cv/envelopes/cv_slew.vibe` | [cv/envelopes/cv_slew.vibe:23](../../../crates/vibelang-std/stdlib/cv/envelopes/cv_slew.vibe#L23) |
| `cv_stages` | `define_synthdef` | `stdlib/cv/eurorack/cv_stages.vibe` | [cv/eurorack/cv_stages.vibe:41](../../../crates/vibelang-std/stdlib/cv/eurorack/cv_stages.vibe#L41) |
| `cv_test_swept_dc` | `define_synthdef` | `stdlib/cv/calibration/cv_test_swept_dc.vibe` | [cv/calibration/cv_test_swept_dc.vibe:23](../../../crates/vibelang-std/stdlib/cv/calibration/cv_test_swept_dc.vibe#L23) |
| `cv_tides` | `define_synthdef` | `stdlib/cv/eurorack/cv_tides.vibe` | [cv/eurorack/cv_tides.vibe:39](../../../crates/vibelang-std/stdlib/cv/eurorack/cv_tides.vibe#L39) |
| `cv_trigger` | `define_synthdef` | `stdlib/cv/triggers/cv_trigger.vibe` | [cv/triggers/cv_trigger.vibe:13](../../../crates/vibelang-std/stdlib/cv/triggers/cv_trigger.vibe#L13) |
| `cv_v5_steady` | `define_synthdef` | `stdlib/cv/calibration/cv_v5_steady.vibe` | [cv/calibration/cv_v5_steady.vibe:16](../../../crates/vibelang-std/stdlib/cv/calibration/cv_v5_steady.vibe#L16) |
| `cv_voct_calib` | `define_synthdef` | `stdlib/cv/calibration/cv_voct_calib.vibe` | [cv/calibration/cv_voct_calib.vibe:24](../../../crates/vibelang-std/stdlib/cv/calibration/cv_voct_calib.vibe#L24) |
| `cv_voct_from_midi` | `define_synthdef` | `stdlib/cv/pitch/cv_voct_from_midi.vibe` | [cv/pitch/cv_voct_from_midi.vibe:6](../../../crates/vibelang-std/stdlib/cv/pitch/cv_voct_from_midi.vibe#L6) |
| `cv_voct_seq` | `define_synthdef` | `stdlib/cv/pitch/cv_voct_seq.vibe` | [cv/pitch/cv_voct_seq.vibe:14](../../../crates/vibelang-std/stdlib/cv/pitch/cv_voct_seq.vibe#L14) |
| `cv_wogglebug` | `define_synthdef` | `stdlib/cv/eurorack/cv_wogglebug.vibe` | [cv/eurorack/cv_wogglebug.vibe:41](../../../crates/vibelang-std/stdlib/cv/eurorack/cv_wogglebug.vibe#L41) |
| `envelope_follower` | `define_synthdef` | `stdlib/cv/envelopes/envelope_follower.vibe` | [cv/envelopes/envelope_follower.vibe:7](../../../crates/vibelang-std/stdlib/cv/envelopes/envelope_follower.vibe#L7) |
| `lfo_random` **(duplicate name)** | `define_synthdef` | `stdlib/cv/lfo/lfo_random.vibe` | [cv/lfo/lfo_random.vibe:4](../../../crates/vibelang-std/stdlib/cv/lfo/lfo_random.vibe#L4) |
| `lfo_random_exp` | `define_synthdef` | `stdlib/cv/lfo/lfo_random.vibe` | [cv/lfo/lfo_random.vibe:36](../../../crates/vibelang-std/stdlib/cv/lfo/lfo_random.vibe#L36) |
| `lfo_random_smooth` | `define_synthdef` | `stdlib/cv/lfo/lfo_random.vibe` | [cv/lfo/lfo_random.vibe:20](../../../crates/vibelang-std/stdlib/cv/lfo/lfo_random.vibe#L20) |
| `lfo_saw` **(duplicate name)** | `define_synthdef` | `stdlib/cv/lfo/lfo_saw.vibe` | [cv/lfo/lfo_saw.vibe:4](../../../crates/vibelang-std/stdlib/cv/lfo/lfo_saw.vibe#L4) |
| `lfo_saw_down` | `define_synthdef` | `stdlib/cv/lfo/lfo_saw.vibe` | [cv/lfo/lfo_saw.vibe:21](../../../crates/vibelang-std/stdlib/cv/lfo/lfo_saw.vibe#L21) |
| `lfo_sine` **(duplicate name)** | `define_synthdef` | `stdlib/cv/lfo/lfo_sine.vibe` | [cv/lfo/lfo_sine.vibe:4](../../../crates/vibelang-std/stdlib/cv/lfo/lfo_sine.vibe#L4) |
| `lfo_square` | `define_synthdef` | `stdlib/cv/lfo/lfo_square.vibe` | [cv/lfo/lfo_square.vibe:4](../../../crates/vibelang-std/stdlib/cv/lfo/lfo_square.vibe#L4) |
| `lfo_tri` | `define_synthdef` | `stdlib/cv/lfo/lfo_tri.vibe` | [cv/lfo/lfo_tri.vibe:4](../../../crates/vibelang-std/stdlib/cv/lfo/lfo_tri.vibe#L4) |
| `peak_follower` | `define_synthdef` | `stdlib/cv/envelopes/envelope_follower.vibe` | [cv/envelopes/envelope_follower.vibe:27](../../../crates/vibelang-std/stdlib/cv/envelopes/envelope_follower.vibe#L27) |

### drums

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `agogo_high` | `define_synthdef` | `stdlib/drums/latin/agogo_high.vibe` | [drums/latin/agogo_high.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/agogo_high.vibe#L2) |
| `agogo_low` | `define_synthdef` | `stdlib/drums/latin/agogo_low.vibe` | [drums/latin/agogo_low.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/agogo_low.vibe#L2) |
| `bell_ping` | `define_synthdef` | `stdlib/drums/cymbals/bell_ping.vibe` | [drums/cymbals/bell_ping.vibe:2](../../../crates/vibelang-std/stdlib/drums/cymbals/bell_ping.vibe#L2) |
| `berimbau` | `define_synthdef` | `stdlib/drums/latin/berimbau.vibe` | [drums/latin/berimbau.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/berimbau.vibe#L2) |
| `bongo_high` | `define_synthdef` | `stdlib/drums/latin/bongo_high.vibe` | [drums/latin/bongo_high.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/bongo_high.vibe#L2) |
| `bongo_low` | `define_synthdef` | `stdlib/drums/latin/bongo_low.vibe` | [drums/latin/bongo_low.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/bongo_low.vibe#L2) |
| `break_amen` | `define_synthdef` | `stdlib/drums/breakbeats/break_amen.vibe` | [drums/breakbeats/break_amen.vibe:2](../../../crates/vibelang-std/stdlib/drums/breakbeats/break_amen.vibe#L2) |
| `break_funky` | `define_synthdef` | `stdlib/drums/breakbeats/break_funky.vibe` | [drums/breakbeats/break_funky.vibe:2](../../../crates/vibelang-std/stdlib/drums/breakbeats/break_funky.vibe#L2) |
| `cabasa` | `define_synthdef` | `stdlib/drums/latin/cabasa.vibe` | [drums/latin/cabasa.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/cabasa.vibe#L2) |
| `cajon` | `define_synthdef` | `stdlib/drums/latin/cajon.vibe` | [drums/latin/cajon.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/cajon.vibe#L2) |
| `cajon_slap` | `define_synthdef` | `stdlib/drums/latin/cajon_slap.vibe` | [drums/latin/cajon_slap.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/cajon_slap.vibe#L2) |
| `china` | `define_synthdef` | `stdlib/drums/cymbals/china.vibe` | [drums/cymbals/china.vibe:2](../../../crates/vibelang-std/stdlib/drums/cymbals/china.vibe#L2) |
| `clap_808` | `define_synthdef` | `stdlib/drums/claps/clap_808.vibe` | [drums/claps/clap_808.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_808.vibe#L2) |
| `clap_crowd` | `define_synthdef` | `stdlib/drums/claps/clap_crowd.vibe` | [drums/claps/clap_crowd.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_crowd.vibe#L2) |
| `clap_filtered` | `define_synthdef` | `stdlib/drums/claps/clap_filtered.vibe` | [drums/claps/clap_filtered.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_filtered.vibe#L2) |
| `clap_handclap` | `define_synthdef` | `stdlib/drums/claps/clap_handclap.vibe` | [drums/claps/clap_handclap.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_handclap.vibe#L2) |
| `clap_layered` | `define_synthdef` | `stdlib/drums/claps/clap_layered.vibe` | [drums/claps/clap_layered.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_layered.vibe#L2) |
| `clap_lofi` | `define_synthdef` | `stdlib/drums/claps/clap_lofi.vibe` | [drums/claps/clap_lofi.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_lofi.vibe#L2) |
| `clap_loose` | `define_synthdef` | `stdlib/drums/claps/clap_loose.vibe` | [drums/claps/clap_loose.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_loose.vibe#L2) |
| `clap_reverb` | `define_synthdef` | `stdlib/drums/claps/clap_reverb.vibe` | [drums/claps/clap_reverb.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_reverb.vibe#L2) |
| `clap_short` | `define_synthdef` | `stdlib/drums/claps/clap_short.vibe` | [drums/claps/clap_short.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_short.vibe#L2) |
| `clap_tight` | `define_synthdef` | `stdlib/drums/claps/clap_tight.vibe` | [drums/claps/clap_tight.vibe:2](../../../crates/vibelang-std/stdlib/drums/claps/clap_tight.vibe#L2) |
| `clave` | `define_synthdef` | `stdlib/drums/percussion/clave.vibe` | [drums/percussion/clave.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/clave.vibe#L2) |
| `conga_high` | `define_synthdef` | `stdlib/drums/latin/conga_high.vibe` | [drums/latin/conga_high.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/conga_high.vibe#L2) |
| `conga_low` | `define_synthdef` | `stdlib/drums/latin/conga_low.vibe` | [drums/latin/conga_low.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/conga_low.vibe#L2) |
| `conga_muted` | `define_synthdef` | `stdlib/drums/latin/conga_muted.vibe` | [drums/latin/conga_muted.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/conga_muted.vibe#L2) |
| `cowbell` | `define_synthdef` | `stdlib/drums/percussion/cowbell.vibe` | [drums/percussion/cowbell.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/cowbell.vibe#L2) |
| `cr78_kick` | `define_synthdef` | `stdlib/drums/machines/cr78_kick.vibe` | [drums/machines/cr78_kick.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/cr78_kick.vibe#L2) |
| `cr78_snare` | `define_synthdef` | `stdlib/drums/machines/cr78_snare.vibe` | [drums/machines/cr78_snare.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/cr78_snare.vibe#L2) |
| `crash_bright` | `define_synthdef` | `stdlib/drums/cymbals/crash_bright.vibe` | [drums/cymbals/crash_bright.vibe:2](../../../crates/vibelang-std/stdlib/drums/cymbals/crash_bright.vibe#L2) |
| `crash_dark` | `define_synthdef` | `stdlib/drums/cymbals/crash_dark.vibe` | [drums/cymbals/crash_dark.vibe:2](../../../crates/vibelang-std/stdlib/drums/cymbals/crash_dark.vibe#L2) |
| `crash_reverse` | `define_synthdef` | `stdlib/drums/cymbals/crash_reverse.vibe` | [drums/cymbals/crash_reverse.vibe:2](../../../crates/vibelang-std/stdlib/drums/cymbals/crash_reverse.vibe#L2) |
| `crash_swell` | `define_synthdef` | `stdlib/drums/cymbals/crash_swell.vibe` | [drums/cymbals/crash_swell.vibe:2](../../../crates/vibelang-std/stdlib/drums/cymbals/crash_swell.vibe#L2) |
| `cuica` | `define_synthdef` | `stdlib/drums/latin/cuica.vibe` | [drums/latin/cuica.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/cuica.vibe#L2) |
| `dmx_kick` | `define_synthdef` | `stdlib/drums/machines/dmx_kick.vibe` | [drums/machines/dmx_kick.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/dmx_kick.vibe#L2) |
| `dmx_snare` | `define_synthdef` | `stdlib/drums/machines/dmx_snare.vibe` | [drums/machines/dmx_snare.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/dmx_snare.vibe#L2) |
| `foley_book_drop` | `define_synthdef` | `stdlib/drums/foley/foley_book_drop.vibe` | [drums/foley/foley_book_drop.vibe:2](../../../crates/vibelang-std/stdlib/drums/foley/foley_book_drop.vibe#L2) |
| `foley_bucket` | `define_synthdef` | `stdlib/drums/foley/foley_bucket.vibe` | [drums/foley/foley_bucket.vibe:2](../../../crates/vibelang-std/stdlib/drums/foley/foley_bucket.vibe#L2) |
| `foley_cardboard` | `define_synthdef` | `stdlib/drums/foley/foley_cardboard.vibe` | [drums/foley/foley_cardboard.vibe:2](../../../crates/vibelang-std/stdlib/drums/foley/foley_cardboard.vibe#L2) |
| `foley_door_slam` | `define_synthdef` | `stdlib/drums/foley/foley_door_slam.vibe` | [drums/foley/foley_door_slam.vibe:2](../../../crates/vibelang-std/stdlib/drums/foley/foley_door_slam.vibe#L2) |
| `foley_pot_lid` | `define_synthdef` | `stdlib/drums/foley/foley_pot_lid.vibe` | [drums/foley/foley_pot_lid.vibe:2](../../../crates/vibelang-std/stdlib/drums/foley/foley_pot_lid.vibe#L2) |
| `guiro` | `define_synthdef` | `stdlib/drums/latin/guiro.vibe` | [drums/latin/guiro.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/guiro.vibe#L2) |
| `guiro_short` | `define_synthdef` | `stdlib/drums/latin/guiro_short.vibe` | [drums/latin/guiro_short.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/guiro_short.vibe#L2) |
| `hihat_808_closed` | `define_synthdef` | `stdlib/drums/hihats/hihat_808_closed.vibe` | [drums/hihats/hihat_808_closed.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_808_closed.vibe#L2) |
| `hihat_808_open` | `define_synthdef` | `stdlib/drums/hihats/hihat_808_open.vibe` | [drums/hihats/hihat_808_open.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_808_open.vibe#L2) |
| `hihat_909_closed` | `define_synthdef` | `stdlib/drums/hihats/hihat_909_closed.vibe` | [drums/hihats/hihat_909_closed.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_909_closed.vibe#L2) |
| `hihat_909_open` | `define_synthdef` | `stdlib/drums/hihats/hihat_909_open.vibe` | [drums/hihats/hihat_909_open.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_909_open.vibe#L2) |
| `hihat_dusty` | `define_synthdef` | `stdlib/drums/hihats/hihat_dusty.vibe` | [drums/hihats/hihat_dusty.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_dusty.vibe#L2) |
| `hihat_filtered` | `define_synthdef` | `stdlib/drums/hihats/hihat_filtered.vibe` | [drums/hihats/hihat_filtered.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_filtered.vibe#L2) |
| `hihat_long` | `define_synthdef` | `stdlib/drums/hihats/hihat_long.vibe` | [drums/hihats/hihat_long.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_long.vibe#L2) |
| `hihat_metallic` | `define_synthdef` | `stdlib/drums/hihats/hihat_metallic.vibe` | [drums/hihats/hihat_metallic.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_metallic.vibe#L2) |
| `hihat_short` | `define_synthdef` | `stdlib/drums/hihats/hihat_short.vibe` | [drums/hihats/hihat_short.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_short.vibe#L2) |
| `hihat_splash` | `define_synthdef` | `stdlib/drums/hihats/hihat_splash.vibe` | [drums/hihats/hihat_splash.vibe:2](../../../crates/vibelang-std/stdlib/drums/hihats/hihat_splash.vibe#L2) |
| `kick_808` | `define_synthdef` | `stdlib/drums/kicks/kick_808.vibe` | [drums/kicks/kick_808.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_808.vibe#L2) |
| `kick_909` | `define_synthdef` | `stdlib/drums/kicks/kick_909.vibe` | [drums/kicks/kick_909.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_909.vibe#L2) |
| `kick_acoustic` | `define_synthdef` | `stdlib/drums/kicks/kick_acoustic.vibe` | [drums/kicks/kick_acoustic.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_acoustic.vibe#L2) |
| `kick_boomy` | `define_synthdef` | `stdlib/drums/kicks/kick_boomy.vibe` | [drums/kicks/kick_boomy.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_boomy.vibe#L2) |
| `kick_click` | `define_synthdef` | `stdlib/drums/kicks/kick_click.vibe` | [drums/kicks/kick_click.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_click.vibe#L2) |
| `kick_distorted` | `define_synthdef` | `stdlib/drums/kicks/kick_distorted.vibe` | [drums/kicks/kick_distorted.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_distorted.vibe#L2) |
| `kick_dnb` | `define_synthdef` | `stdlib/drums/kicks/kick_dnb.vibe` | [drums/kicks/kick_dnb.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_dnb.vibe#L2) |
| `kick_fm` | `define_synthdef` | `stdlib/drums/kicks/kick_fm.vibe` | [drums/kicks/kick_fm.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_fm.vibe#L2) |
| `kick_gabber` | `define_synthdef` | `stdlib/drums/kicks/kick_gabber.vibe` | [drums/kicks/kick_gabber.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_gabber.vibe#L2) |
| `kick_jungle` | `define_synthdef` | `stdlib/drums/kicks/kick_jungle.vibe` | [drums/kicks/kick_jungle.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_jungle.vibe#L2) |
| `kick_layered` | `define_synthdef` | `stdlib/drums/kicks/kick_layered.vibe` | [drums/kicks/kick_layered.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_layered.vibe#L2) |
| `kick_lofi` | `define_synthdef` | `stdlib/drums/kicks/kick_lofi.vibe` | [drums/kicks/kick_lofi.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_lofi.vibe#L2) |
| `kick_pitched` | `define_synthdef` | `stdlib/drums/kicks/kick_pitched.vibe` | [drums/kicks/kick_pitched.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_pitched.vibe#L2) |
| `kick_punchy` | `define_synthdef` | `stdlib/drums/kicks/kick_punchy.vibe` | [drums/kicks/kick_punchy.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_punchy.vibe#L2) |
| `kick_reggaeton` | `define_synthdef` | `stdlib/drums/kicks/kick_reggaeton.vibe` | [drums/kicks/kick_reggaeton.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_reggaeton.vibe#L2) |
| `kick_soft` | `define_synthdef` | `stdlib/drums/kicks/kick_soft.vibe` | [drums/kicks/kick_soft.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_soft.vibe#L2) |
| `kick_sub` | `define_synthdef` | `stdlib/drums/kicks/kick_sub.vibe` | [drums/kicks/kick_sub.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_sub.vibe#L2) |
| `kick_techno_deep` | `define_synthdef` | `stdlib/drums/kicks/kick_techno_deep.vibe` | [drums/kicks/kick_techno_deep.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_techno_deep.vibe#L2) |
| `kick_techno_hard` | `define_synthdef` | `stdlib/drums/kicks/kick_techno_hard.vibe` | [drums/kicks/kick_techno_hard.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_techno_hard.vibe#L2) |
| `kick_trap` | `define_synthdef` | `stdlib/drums/kicks/kick_trap.vibe` | [drums/kicks/kick_trap.vibe:2](../../../crates/vibelang-std/stdlib/drums/kicks/kick_trap.vibe#L2) |
| `linn_kick` | `define_synthdef` | `stdlib/drums/machines/linn_kick.vibe` | [drums/machines/linn_kick.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/linn_kick.vibe#L2) |
| `linn_snare` | `define_synthdef` | `stdlib/drums/machines/linn_snare.vibe` | [drums/machines/linn_snare.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/linn_snare.vibe#L2) |
| `machinedrum_kick` | `define_synthdef` | `stdlib/drums/machines/machinedrum_kick.vibe` | [drums/machines/machinedrum_kick.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/machinedrum_kick.vibe#L2) |
| `machinedrum_snare` | `define_synthdef` | `stdlib/drums/machines/machinedrum_snare.vibe` | [drums/machines/machinedrum_snare.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/machinedrum_snare.vibe#L2) |
| `maracas` | `define_synthdef` | `stdlib/drums/latin/maracas.vibe` | [drums/latin/maracas.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/maracas.vibe#L2) |
| `mpc_kick` | `define_synthdef` | `stdlib/drums/machines/mpc_kick.vibe` | [drums/machines/mpc_kick.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/mpc_kick.vibe#L2) |
| `mpc_snare` | `define_synthdef` | `stdlib/drums/machines/mpc_snare.vibe` | [drums/machines/mpc_snare.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/mpc_snare.vibe#L2) |
| `pandeiro` | `define_synthdef` | `stdlib/drums/latin/pandeiro.vibe` | [drums/latin/pandeiro.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/pandeiro.vibe#L2) |
| `ride` | `define_synthdef` | `stdlib/drums/cymbals/ride.vibe` | [drums/cymbals/ride.vibe:2](../../../crates/vibelang-std/stdlib/drums/cymbals/ride.vibe#L2) |
| `rim` | `define_synthdef` | `stdlib/drums/percussion/rim.vibe` | [drums/percussion/rim.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/rim.vibe#L2) |
| `shaker` | `define_synthdef` | `stdlib/drums/percussion/shaker.vibe` | [drums/percussion/shaker.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/shaker.vibe#L2) |
| `snap` | `define_synthdef` | `stdlib/drums/percussion/snap.vibe` | [drums/percussion/snap.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/snap.vibe#L2) |
| `snare_808` | `define_synthdef` | `stdlib/drums/snares/snare_808.vibe` | [drums/snares/snare_808.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_808.vibe#L2) |
| `snare_909` | `define_synthdef` | `stdlib/drums/snares/snare_909.vibe` | [drums/snares/snare_909.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_909.vibe#L2) |
| `snare_acoustic` | `define_synthdef` | `stdlib/drums/snares/snare_acoustic.vibe` | [drums/snares/snare_acoustic.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_acoustic.vibe#L2) |
| `snare_brush` | `define_synthdef` | `stdlib/drums/snares/snare_brush.vibe` | [drums/snares/snare_brush.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_brush.vibe#L2) |
| `snare_clap_snare` | `define_synthdef` | `stdlib/drums/snares/snare_clap_snare.vibe` | [drums/snares/snare_clap_snare.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_clap_snare.vibe#L2) |
| `snare_filtered` | `define_synthdef` | `stdlib/drums/snares/snare_filtered.vibe` | [drums/snares/snare_filtered.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_filtered.vibe#L2) |
| `snare_garage` | `define_synthdef` | `stdlib/drums/snares/snare_garage.vibe` | [drums/snares/snare_garage.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_garage.vibe#L2) |
| `snare_gated` | `define_synthdef` | `stdlib/drums/snares/snare_gated.vibe` | [drums/snares/snare_gated.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_gated.vibe#L2) |
| `snare_jungle` | `define_synthdef` | `stdlib/drums/snares/snare_jungle.vibe` | [drums/snares/snare_jungle.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_jungle.vibe#L2) |
| `snare_layered` | `define_synthdef` | `stdlib/drums/snares/snare_layered.vibe` | [drums/snares/snare_layered.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_layered.vibe#L2) |
| `snare_lofi` | `define_synthdef` | `stdlib/drums/snares/snare_lofi.vibe` | [drums/snares/snare_lofi.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_lofi.vibe#L2) |
| `snare_loose_acoustic` | `define_synthdef` | `stdlib/drums/snares/snare_loose_acoustic.vibe` | [drums/snares/snare_loose_acoustic.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_loose_acoustic.vibe#L2) |
| `snare_marching` | `define_synthdef` | `stdlib/drums/snares/snare_marching.vibe` | [drums/snares/snare_marching.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_marching.vibe#L2) |
| `snare_noise_layer` | `define_synthdef` | `stdlib/drums/snares/snare_noise_layer.vibe` | [drums/snares/snare_noise_layer.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_noise_layer.vibe#L2) |
| `snare_piccolo` | `define_synthdef` | `stdlib/drums/snares/snare_piccolo.vibe` | [drums/snares/snare_piccolo.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_piccolo.vibe#L2) |
| `snare_pitched` | `define_synthdef` | `stdlib/drums/snares/snare_pitched.vibe` | [drums/snares/snare_pitched.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_pitched.vibe#L2) |
| `snare_reverb` | `define_synthdef` | `stdlib/drums/snares/snare_reverb.vibe` | [drums/snares/snare_reverb.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_reverb.vibe#L2) |
| `snare_rimshot` | `define_synthdef` | `stdlib/drums/snares/snare_rimshot.vibe` | [drums/snares/snare_rimshot.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_rimshot.vibe#L2) |
| `snare_tight_electronic` | `define_synthdef` | `stdlib/drums/snares/snare_tight_electronic.vibe` | [drums/snares/snare_tight_electronic.vibe:2](../../../crates/vibelang-std/stdlib/drums/snares/snare_tight_electronic.vibe#L2) |
| `sp1200_kick` | `define_synthdef` | `stdlib/drums/machines/sp1200_kick.vibe` | [drums/machines/sp1200_kick.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/sp1200_kick.vibe#L2) |
| `sp1200_snare` | `define_synthdef` | `stdlib/drums/machines/sp1200_snare.vibe` | [drums/machines/sp1200_snare.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/sp1200_snare.vibe#L2) |
| `splash` | `define_synthdef` | `stdlib/drums/cymbals/splash.vibe` | [drums/cymbals/splash.vibe:2](../../../crates/vibelang-std/stdlib/drums/cymbals/splash.vibe#L2) |
| `surdo` | `define_synthdef` | `stdlib/drums/latin/surdo.vibe` | [drums/latin/surdo.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/surdo.vibe#L2) |
| `tambourine` | `define_synthdef` | `stdlib/drums/percussion/tambourine.vibe` | [drums/percussion/tambourine.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/tambourine.vibe#L2) |
| `tempest_kick` | `define_synthdef` | `stdlib/drums/machines/tempest_kick.vibe` | [drums/machines/tempest_kick.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/tempest_kick.vibe#L2) |
| `tempest_snare` | `define_synthdef` | `stdlib/drums/machines/tempest_snare.vibe` | [drums/machines/tempest_snare.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/tempest_snare.vibe#L2) |
| `timbale_high` | `define_synthdef` | `stdlib/drums/latin/timbale_high.vibe` | [drums/latin/timbale_high.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/timbale_high.vibe#L2) |
| `timbale_low` | `define_synthdef` | `stdlib/drums/latin/timbale_low.vibe` | [drums/latin/timbale_low.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/timbale_low.vibe#L2) |
| `timbale_rim` | `define_synthdef` | `stdlib/drums/latin/timbale_rim.vibe` | [drums/latin/timbale_rim.vibe:2](../../../crates/vibelang-std/stdlib/drums/latin/timbale_rim.vibe#L2) |
| `tom_808` | `define_synthdef` | `stdlib/drums/toms/tom_808.vibe` | [drums/toms/tom_808.vibe:2](../../../crates/vibelang-std/stdlib/drums/toms/tom_808.vibe#L2) |
| `tom_concert` | `define_synthdef` | `stdlib/drums/toms/tom_concert.vibe` | [drums/toms/tom_concert.vibe:2](../../../crates/vibelang-std/stdlib/drums/toms/tom_concert.vibe#L2) |
| `tom_floor` | `define_synthdef` | `stdlib/drums/toms/tom_floor.vibe` | [drums/toms/tom_floor.vibe:2](../../../crates/vibelang-std/stdlib/drums/toms/tom_floor.vibe#L2) |
| `tom_high` | `define_synthdef` | `stdlib/drums/percussion/tom_high.vibe` | [drums/percussion/tom_high.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/tom_high.vibe#L2) |
| `tom_low` | `define_synthdef` | `stdlib/drums/percussion/tom_low.vibe` | [drums/percussion/tom_low.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/tom_low.vibe#L2) |
| `tom_mid` | `define_synthdef` | `stdlib/drums/percussion/tom_mid.vibe` | [drums/percussion/tom_mid.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/tom_mid.vibe#L2) |
| `tom_roto` | `define_synthdef` | `stdlib/drums/toms/tom_roto.vibe` | [drums/toms/tom_roto.vibe:2](../../../crates/vibelang-std/stdlib/drums/toms/tom_roto.vibe#L2) |
| `tom_synth` | `define_synthdef` | `stdlib/drums/toms/tom_synth.vibe` | [drums/toms/tom_synth.vibe:2](../../../crates/vibelang-std/stdlib/drums/toms/tom_synth.vibe#L2) |
| `volca_kick` | `define_synthdef` | `stdlib/drums/machines/volca_kick.vibe` | [drums/machines/volca_kick.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/volca_kick.vibe#L2) |
| `volca_snare` | `define_synthdef` | `stdlib/drums/machines/volca_snare.vibe` | [drums/machines/volca_snare.vibe:2](../../../crates/vibelang-std/stdlib/drums/machines/volca_snare.vibe#L2) |
| `woodblock` | `define_synthdef` | `stdlib/drums/percussion/woodblock.vibe` | [drums/percussion/woodblock.vibe:2](../../../crates/vibelang-std/stdlib/drums/percussion/woodblock.vibe#L2) |

### effects

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `amp_follower` | `define_fx` | `stdlib/effects/dynamics/amp_follower.vibe` | [effects/dynamics/amp_follower.vibe:26](../../../crates/vibelang-std/stdlib/effects/dynamics/amp_follower.vibe#L26) |
| `analog_delay` | `define_fx` | `stdlib/effects/delays/analog_delay.vibe` | [effects/delays/analog_delay.vibe:27](../../../crates/vibelang-std/stdlib/effects/delays/analog_delay.vibe#L27) |
| `auto_pan` | `define_fx` | `stdlib/effects/modulation/auto_pan.vibe` | [effects/modulation/auto_pan.vibe:21](../../../crates/vibelang-std/stdlib/effects/modulation/auto_pan.vibe#L21) |
| `auto_tune_vibe` | `define_fx` | `stdlib/effects/pitch/auto_tune_vibe.vibe` | [effects/pitch/auto_tune_vibe.vibe:43](../../../crates/vibelang-std/stdlib/effects/pitch/auto_tune_vibe.vibe#L43) |
| `auto_wah` | `define_fx` | `stdlib/effects/modulation/auto_wah.vibe` | [effects/modulation/auto_wah.vibe:21](../../../crates/vibelang-std/stdlib/effects/modulation/auto_wah.vibe#L21) |
| `auto_wah_pro` | `define_fx` | `stdlib/effects/modulation/auto_wah_pro.vibe` | [effects/modulation/auto_wah_pro.vibe:31](../../../crates/vibelang-std/stdlib/effects/modulation/auto_wah_pro.vibe#L31) |
| `bandpass` | `define_fx` | `stdlib/effects/filters/bandpass.vibe` | [effects/filters/bandpass.vibe:21](../../../crates/vibelang-std/stdlib/effects/filters/bandpass.vibe#L21) |
| `barber_phaser` | `define_fx` | `stdlib/effects/modulation/barber_phaser.vibe` | [effects/modulation/barber_phaser.vibe:49](../../../crates/vibelang-std/stdlib/effects/modulation/barber_phaser.vibe#L49) |
| `beat_repeater` | `define_fx` | `stdlib/effects/glitch/beat_repeater.vibe` | [effects/glitch/beat_repeater.vibe:33](../../../crates/vibelang-std/stdlib/effects/glitch/beat_repeater.vibe#L33) |
| `bitcrush` | `define_fx` | `stdlib/effects/distortion/bitcrush.vibe` | [effects/distortion/bitcrush.vibe:21](../../../crates/vibelang-std/stdlib/effects/distortion/bitcrush.vibe#L21) |
| `bitcrush_pro` | `define_fx` | `stdlib/effects/character/bitcrush_pro.vibe` | [effects/character/bitcrush_pro.vibe:29](../../../crates/vibelang-std/stdlib/effects/character/bitcrush_pro.vibe#L29) |
| `broken_radio` | `define_fx` | `stdlib/effects/character/broken_radio.vibe` | [effects/character/broken_radio.vibe:28](../../../crates/vibelang-std/stdlib/effects/character/broken_radio.vibe#L28) |
| `cab_simulator` | `define_fx` | `stdlib/effects/convolution/cab_simulator.vibe` | [effects/convolution/cab_simulator.vibe:28](../../../crates/vibelang-std/stdlib/effects/convolution/cab_simulator.vibe#L28) |
| `cassette` | `define_fx` | `stdlib/effects/character/cassette.vibe` | [effects/character/cassette.vibe:27](../../../crates/vibelang-std/stdlib/effects/character/cassette.vibe#L27) |
| `chiptune` | `define_fx` | `stdlib/effects/character/chiptune.vibe` | [effects/character/chiptune.vibe:26](../../../crates/vibelang-std/stdlib/effects/character/chiptune.vibe#L26) |
| `chorus` | `define_fx` | `stdlib/effects/modulation/chorus.vibe` | [effects/modulation/chorus.vibe:21](../../../crates/vibelang-std/stdlib/effects/modulation/chorus.vibe#L21) |
| `chorus_dimension_3d` | `define_fx` | `stdlib/effects/modulation/chorus_dimension_3d.vibe` | [effects/modulation/chorus_dimension_3d.vibe:32](../../../crates/vibelang-std/stdlib/effects/modulation/chorus_dimension_3d.vibe#L32) |
| `clouds_processor` | `define_fx` | `stdlib/effects/granular/clouds_processor.vibe` | [effects/granular/clouds_processor.vibe:59](../../../crates/vibelang-std/stdlib/effects/granular/clouds_processor.vibe#L59) |
| `comb_filter` | `define_fx` | `stdlib/effects/filters/comb_filter.vibe` | [effects/filters/comb_filter.vibe:22](../../../crates/vibelang-std/stdlib/effects/filters/comb_filter.vibe#L22) |
| `comp_fet` | `define_fx` | `stdlib/effects/dynamics/comp_fet.vibe` | [effects/dynamics/comp_fet.vibe:27](../../../crates/vibelang-std/stdlib/effects/dynamics/comp_fet.vibe#L27) |
| `comp_opto` | `define_fx` | `stdlib/effects/dynamics/comp_opto.vibe` | [effects/dynamics/comp_opto.vibe:28](../../../crates/vibelang-std/stdlib/effects/dynamics/comp_opto.vibe#L28) |
| `comp_vari_mu` | `define_fx` | `stdlib/effects/dynamics/comp_vari_mu.vibe` | [effects/dynamics/comp_vari_mu.vibe:30](../../../crates/vibelang-std/stdlib/effects/dynamics/comp_vari_mu.vibe#L30) |
| `comp_vca` | `define_fx` | `stdlib/effects/dynamics/comp_vca.vibe` | [effects/dynamics/comp_vca.vibe:27](../../../crates/vibelang-std/stdlib/effects/dynamics/comp_vca.vibe#L27) |
| `compressor` | `define_fx` | `stdlib/effects/dynamics/compressor.vibe` | [effects/dynamics/compressor.vibe:26](../../../crates/vibelang-std/stdlib/effects/dynamics/compressor.vibe#L26) |
| `console_summing` | `define_fx` | `stdlib/effects/character/console_summing.vibe` | [effects/character/console_summing.vibe:27](../../../crates/vibelang-std/stdlib/effects/character/console_summing.vibe#L27) |
| `dc_blocker` | `define_fx` | `stdlib/effects/utility/dc_blocker.vibe` | [effects/utility/dc_blocker.vibe:22](../../../crates/vibelang-std/stdlib/effects/utility/dc_blocker.vibe#L22) |
| `de_esser` | `define_fx` | `stdlib/effects/dynamics/de_esser.vibe` | [effects/dynamics/de_esser.vibe:27](../../../crates/vibelang-std/stdlib/effects/dynamics/de_esser.vibe#L27) |
| `delay` | `define_fx` | `stdlib/effects/delays/delay.vibe` | [effects/delays/delay.vibe:23](../../../crates/vibelang-std/stdlib/effects/delays/delay.vibe#L23) |
| `delay_bbd_analog` | `define_fx` | `stdlib/effects/delays/delay_bbd_analog.vibe` | [effects/delays/delay_bbd_analog.vibe:28](../../../crates/vibelang-std/stdlib/effects/delays/delay_bbd_analog.vibe#L28) |
| `delay_modulated_tape` | `define_fx` | `stdlib/effects/delays/delay_modulated_tape.vibe` | [effects/delays/delay_modulated_tape.vibe:30](../../../crates/vibelang-std/stdlib/effects/delays/delay_modulated_tape.vibe#L30) |
| `delay_tape_wow` | `define_fx` | `stdlib/effects/delays/delay_tape_wow.vibe` | [effects/delays/delay_tape_wow.vibe:31](../../../crates/vibelang-std/stdlib/effects/delays/delay_tape_wow.vibe#L31) |
| `density_grain` | `define_fx` | `stdlib/effects/granular/density_grain.vibe` | [effects/granular/density_grain.vibe:27](../../../crates/vibelang-std/stdlib/effects/granular/density_grain.vibe#L27) |
| `dimension_chorus` | `define_fx` | `stdlib/effects/modulation/dimension_chorus.vibe` | [effects/modulation/dimension_chorus.vibe:15](../../../crates/vibelang-std/stdlib/effects/modulation/dimension_chorus.vibe#L15) |
| `dimension_d` | `define_fx` | `stdlib/effects/modulation/dimension_d.vibe` | [effects/modulation/dimension_d.vibe:35](../../../crates/vibelang-std/stdlib/effects/modulation/dimension_d.vibe#L35) |
| `distortion` | `define_fx` | `stdlib/effects/distortion/distortion.vibe` | [effects/distortion/distortion.vibe:21](../../../crates/vibelang-std/stdlib/effects/distortion/distortion.vibe#L21) |
| `drive_transistor` | `define_fx` | `stdlib/effects/distortion/drive_transistor.vibe` | [effects/distortion/drive_transistor.vibe:31](../../../crates/vibelang-std/stdlib/effects/distortion/drive_transistor.vibe#L31) |
| `drive_tube` | `define_fx` | `stdlib/effects/distortion/drive_tube.vibe` | [effects/distortion/drive_tube.vibe:30](../../../crates/vibelang-std/stdlib/effects/distortion/drive_tube.vibe#L30) |
| `dub_delay` | `define_fx` | `stdlib/effects/delays/dub_delay.vibe` | [effects/delays/dub_delay.vibe:25](../../../crates/vibelang-std/stdlib/effects/delays/dub_delay.vibe#L25) |
| `ducking` | `define_fx` | `stdlib/effects/dynamics/ducking.vibe` | [effects/dynamics/ducking.vibe:27](../../../crates/vibelang-std/stdlib/effects/dynamics/ducking.vibe#L27) |
| `ducking_delay` | `define_fx` | `stdlib/effects/delays/ducking_delay.vibe` | [effects/delays/ducking_delay.vibe:28](../../../crates/vibelang-std/stdlib/effects/delays/ducking_delay.vibe#L28) |
| `early_reflections_ir` | `define_fx` | `stdlib/effects/convolution/early_reflections_ir.vibe` | [effects/convolution/early_reflections_ir.vibe:24](../../../crates/vibelang-std/stdlib/effects/convolution/early_reflections_ir.vibe#L24) |
| `ensemble_chorus` | `define_fx` | `stdlib/effects/modulation/ensemble_chorus.vibe` | [effects/modulation/ensemble_chorus.vibe:32](../../../crates/vibelang-std/stdlib/effects/modulation/ensemble_chorus.vibe#L32) |
| `eq_neve_1073` | `define_fx` | `stdlib/effects/filters/eq_neve_1073.vibe` | [effects/filters/eq_neve_1073.vibe:34](../../../crates/vibelang-std/stdlib/effects/filters/eq_neve_1073.vibe#L34) |
| `eq_pultec_low` | `define_fx` | `stdlib/effects/filters/eq_pultec_low.vibe` | [effects/filters/eq_pultec_low.vibe:27](../../../crates/vibelang-std/stdlib/effects/filters/eq_pultec_low.vibe#L27) |
| `eq_three_band` | `define_fx` | `stdlib/effects/filters/eq_three_band.vibe` | [effects/filters/eq_three_band.vibe:27](../../../crates/vibelang-std/stdlib/effects/filters/eq_three_band.vibe#L27) |
| `exciter` | `define_fx` | `stdlib/effects/distortion/exciter.vibe` | [effects/distortion/exciter.vibe:16](../../../crates/vibelang-std/stdlib/effects/distortion/exciter.vibe#L16) |
| `flanger` | `define_fx` | `stdlib/effects/modulation/flanger.vibe` | [effects/modulation/flanger.vibe:23](../../../crates/vibelang-std/stdlib/effects/modulation/flanger.vibe#L23) |
| `formant_filter` | `define_fx` | `stdlib/effects/filters/formant_filter.vibe` | [effects/filters/formant_filter.vibe:23](../../../crates/vibelang-std/stdlib/effects/filters/formant_filter.vibe#L23) |
| `formant_shifter` | `define_fx` | `stdlib/effects/pitch/formant_shifter.vibe` | [effects/pitch/formant_shifter.vibe:37](../../../crates/vibelang-std/stdlib/effects/pitch/formant_shifter.vibe#L37) |
| `frame_freezer` | `define_fx` | `stdlib/effects/glitch/frame_freezer.vibe` | [effects/glitch/frame_freezer.vibe:31](../../../crates/vibelang-std/stdlib/effects/glitch/frame_freezer.vibe#L31) |
| `freq_shift` | `define_fx` | `stdlib/effects/modulation/freq_shift.vibe` | [effects/modulation/freq_shift.vibe:20](../../../crates/vibelang-std/stdlib/effects/modulation/freq_shift.vibe#L20) |
| `freq_shifter_ssb` | `define_fx` | `stdlib/effects/pitch/freq_shifter_ssb.vibe` | [effects/pitch/freq_shifter_ssb.vibe:36](../../../crates/vibelang-std/stdlib/effects/pitch/freq_shifter_ssb.vibe#L36) |
| `fuzz` | `define_fx` | `stdlib/effects/distortion/fuzz.vibe` | [effects/distortion/fuzz.vibe:21](../../../crates/vibelang-std/stdlib/effects/distortion/fuzz.vibe#L21) |
| `gate` | `define_fx` | `stdlib/effects/dynamics/gate.vibe` | [effects/dynamics/gate.vibe:23](../../../crates/vibelang-std/stdlib/effects/dynamics/gate.vibe#L23) |
| `granular_delay` | `define_fx` | `stdlib/effects/delays/granular_delay.vibe` | [effects/delays/granular_delay.vibe:26](../../../crates/vibelang-std/stdlib/effects/delays/granular_delay.vibe#L26) |
| `granular_freeze` | `define_fx` | `stdlib/effects/granular/granular_freeze.vibe` | [effects/granular/granular_freeze.vibe:29](../../../crates/vibelang-std/stdlib/effects/granular/granular_freeze.vibe#L29) |
| `granular_pitch_shift` | `define_fx` | `stdlib/effects/granular/granular_pitch_shift.vibe` | [effects/granular/granular_pitch_shift.vibe:25](../../../crates/vibelang-std/stdlib/effects/granular/granular_pitch_shift.vibe#L25) |
| `granular_processor` | `define_fx` | `stdlib/effects/granular/granular_processor.vibe` | [effects/granular/granular_processor.vibe:28](../../../crates/vibelang-std/stdlib/effects/granular/granular_processor.vibe#L28) |
| `granular_time_stretch` | `define_fx` | `stdlib/effects/granular/granular_time_stretch.vibe` | [effects/granular/granular_time_stretch.vibe:25](../../../crates/vibelang-std/stdlib/effects/granular/granular_time_stretch.vibe#L25) |
| `gverb` | `define_fx` | `stdlib/effects/reverbs/gverb.vibe` | [effects/reverbs/gverb.vibe:31](../../../crates/vibelang-std/stdlib/effects/reverbs/gverb.vibe#L31) |
| `haas` | `define_fx` | `stdlib/effects/spatial/haas.vibe` | [effects/spatial/haas.vibe:19](../../../crates/vibelang-std/stdlib/effects/spatial/haas.vibe#L19) |
| `hall_reverb` | `define_fx` | `stdlib/effects/reverbs/hall_reverb.vibe` | [effects/reverbs/hall_reverb.vibe:17](../../../crates/vibelang-std/stdlib/effects/reverbs/hall_reverb.vibe#L17) |
| `harmonic_tremolo` | `define_fx` | `stdlib/effects/modulation/harmonic_tremolo.vibe` | [effects/modulation/harmonic_tremolo.vibe:26](../../../crates/vibelang-std/stdlib/effects/modulation/harmonic_tremolo.vibe#L26) |
| `harmonizer_2voice` | `define_fx` | `stdlib/effects/pitch/harmonizer_2voice.vibe` | [effects/pitch/harmonizer_2voice.vibe:29](../../../crates/vibelang-std/stdlib/effects/pitch/harmonizer_2voice.vibe#L29) |
| `harmonizer_4voice` | `define_fx` | `stdlib/effects/pitch/harmonizer_4voice.vibe` | [effects/pitch/harmonizer_4voice.vibe:32](../../../crates/vibelang-std/stdlib/effects/pitch/harmonizer_4voice.vibe#L32) |
| `highpass` | `define_fx` | `stdlib/effects/filters/highpass.vibe` | [effects/filters/highpass.vibe:21](../../../crates/vibelang-std/stdlib/effects/filters/highpass.vibe#L21) |
| `hybrid_reverb` | `define_fx` | `stdlib/effects/convolution/hybrid_reverb.vibe` | [effects/convolution/hybrid_reverb.vibe:31](../../../crates/vibelang-std/stdlib/effects/convolution/hybrid_reverb.vibe#L31) |
| `ladder_filter` | `define_fx` | `stdlib/effects/filters/ladder_filter.vibe` | [effects/filters/ladder_filter.vibe:21](../../../crates/vibelang-std/stdlib/effects/filters/ladder_filter.vibe#L21) |
| `leslie_pro` | `define_fx` | `stdlib/effects/modulation/leslie_pro.vibe` | [effects/modulation/leslie_pro.vibe:33](../../../crates/vibelang-std/stdlib/effects/modulation/leslie_pro.vibe#L33) |
| `limiter` | `define_fx` | `stdlib/effects/dynamics/limiter.vibe` | [effects/dynamics/limiter.vibe:23](../../../crates/vibelang-std/stdlib/effects/dynamics/limiter.vibe#L23) |
| `lo_fi` | `define_fx` | `stdlib/effects/distortion/lo_fi.vibe` | [effects/distortion/lo_fi.vibe:25](../../../crates/vibelang-std/stdlib/effects/distortion/lo_fi.vibe#L25) |
| `lowpass` | `define_fx` | `stdlib/effects/filters/lowpass.vibe` | [effects/filters/lowpass.vibe:21](../../../crates/vibelang-std/stdlib/effects/filters/lowpass.vibe#L21) |
| `moog_filter` | `define_fx` | `stdlib/effects/filters/moog_filter.vibe` | [effects/filters/moog_filter.vibe:21](../../../crates/vibelang-std/stdlib/effects/filters/moog_filter.vibe#L21) |
| `multi_tap_delay` | `define_fx` | `stdlib/effects/delays/multi_tap_delay.vibe` | [effects/delays/multi_tap_delay.vibe:27](../../../crates/vibelang-std/stdlib/effects/delays/multi_tap_delay.vibe#L27) |
| `multiband_comp` | `define_fx` | `stdlib/effects/dynamics/multiband_comp.vibe` | [effects/dynamics/multiband_comp.vibe:32](../../../crates/vibelang-std/stdlib/effects/dynamics/multiband_comp.vibe#L32) |
| `octaver_down` | `define_fx` | `stdlib/effects/pitch/octaver_down.vibe` | [effects/pitch/octaver_down.vibe:31](../../../crates/vibelang-std/stdlib/effects/pitch/octaver_down.vibe#L31) |
| `octaver_up` | `define_fx` | `stdlib/effects/pitch/octaver_up.vibe` | [effects/pitch/octaver_up.vibe:30](../../../crates/vibelang-std/stdlib/effects/pitch/octaver_up.vibe#L30) |
| `overdrive` | `define_fx` | `stdlib/effects/distortion/overdrive.vibe` | [effects/distortion/overdrive.vibe:21](../../../crates/vibelang-std/stdlib/effects/distortion/overdrive.vibe#L21) |
| `pan` | `define_fx` | `stdlib/effects/modulation/pan.vibe` | [effects/modulation/pan.vibe:19](../../../crates/vibelang-std/stdlib/effects/modulation/pan.vibe#L19) |
| `parallel_comp` | `define_fx` | `stdlib/effects/dynamics/parallel_comp.vibe` | [effects/dynamics/parallel_comp.vibe:31](../../../crates/vibelang-std/stdlib/effects/dynamics/parallel_comp.vibe#L31) |
| `phaser` | `define_fx` | `stdlib/effects/modulation/phaser.vibe` | [effects/modulation/phaser.vibe:25](../../../crates/vibelang-std/stdlib/effects/modulation/phaser.vibe#L25) |
| `phaser_8stage` | `define_fx` | `stdlib/effects/modulation/phaser_8stage.vibe` | [effects/modulation/phaser_8stage.vibe:31](../../../crates/vibelang-std/stdlib/effects/modulation/phaser_8stage.vibe#L31) |
| `ping_pong_delay` | `define_fx` | `stdlib/effects/delays/ping_pong_delay.vibe` | [effects/delays/ping_pong_delay.vibe:21](../../../crates/vibelang-std/stdlib/effects/delays/ping_pong_delay.vibe#L21) |
| `pitch_shift` | `define_fx` | `stdlib/effects/modulation/pitch_shift.vibe` | [effects/modulation/pitch_shift.vibe:21](../../../crates/vibelang-std/stdlib/effects/modulation/pitch_shift.vibe#L21) |
| `plate_reverb` | `define_fx` | `stdlib/effects/reverbs/plate_reverb.vibe` | [effects/reverbs/plate_reverb.vibe:23](../../../crates/vibelang-std/stdlib/effects/reverbs/plate_reverb.vibe#L23) |
| `resonator` | `define_fx` | `stdlib/effects/filters/resonator.vibe` | [effects/filters/resonator.vibe:21](../../../crates/vibelang-std/stdlib/effects/filters/resonator.vibe#L21) |
| `reverb` | `define_fx` | `stdlib/effects/reverbs/reverb.vibe` | [effects/reverbs/reverb.vibe:15](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb.vibe#L15) |
| `reverb_ducked` | `define_fx` | `stdlib/effects/reverbs/reverb_ducked.vibe` | [effects/reverbs/reverb_ducked.vibe:28](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb_ducked.vibe#L28) |
| `reverb_fdn8` | `define_fx` | `stdlib/effects/reverbs/reverb_fdn8.vibe` | [effects/reverbs/reverb_fdn8.vibe:29](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb_fdn8.vibe#L29) |
| `reverb_gated` | `define_fx` | `stdlib/effects/reverbs/reverb_gated.vibe` | [effects/reverbs/reverb_gated.vibe:28](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb_gated.vibe#L28) |
| `reverb_greyhole` | `define_fx` | `stdlib/effects/reverbs/reverb_greyhole.vibe` | [effects/reverbs/reverb_greyhole.vibe:33](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb_greyhole.vibe#L33) |
| `reverb_infinite` | `define_fx` | `stdlib/effects/reverbs/reverb_infinite.vibe` | [effects/reverbs/reverb_infinite.vibe:26](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb_infinite.vibe#L26) |
| `reverb_ir` | `define_fx` | `stdlib/effects/convolution/reverb_ir.vibe` | [effects/convolution/reverb_ir.vibe:24](../../../crates/vibelang-std/stdlib/effects/convolution/reverb_ir.vibe#L24) |
| `reverb_jpverb` | `define_fx` | `stdlib/effects/reverbs/reverb_jpverb.vibe` | [effects/reverbs/reverb_jpverb.vibe:25](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb_jpverb.vibe#L25) |
| `reverb_minimal` | `define_fx` | `stdlib/effects/reverbs/reverb_minimal.vibe` | [effects/reverbs/reverb_minimal.vibe:24](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb_minimal.vibe#L24) |
| `reverb_partconv` | `define_fx` | `stdlib/effects/convolution/reverb_partconv.vibe` | [effects/convolution/reverb_partconv.vibe:26](../../../crates/vibelang-std/stdlib/effects/convolution/reverb_partconv.vibe#L26) |
| `reverb_reverse` | `define_fx` | `stdlib/effects/reverbs/reverb_reverse.vibe` | [effects/reverbs/reverb_reverse.vibe:28](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb_reverse.vibe#L28) |
| `reverb_shimmer_pro` | `define_fx` | `stdlib/effects/reverbs/reverb_shimmer_pro.vibe` | [effects/reverbs/reverb_shimmer_pro.vibe:31](../../../crates/vibelang-std/stdlib/effects/reverbs/reverb_shimmer_pro.vibe#L31) |
| `reverse_buffer` | `define_fx` | `stdlib/effects/glitch/reverse_buffer.vibe` | [effects/glitch/reverse_buffer.vibe:35](../../../crates/vibelang-std/stdlib/effects/glitch/reverse_buffer.vibe#L35) |
| `reverse_delay` | `define_fx` | `stdlib/effects/delays/reverse_delay.vibe` | [effects/delays/reverse_delay.vibe:22](../../../crates/vibelang-std/stdlib/effects/delays/reverse_delay.vibe#L22) |
| `ring_mod` | `define_fx` | `stdlib/effects/modulation/ring_mod.vibe` | [effects/modulation/ring_mod.vibe:19](../../../crates/vibelang-std/stdlib/effects/modulation/ring_mod.vibe#L19) |
| `ring_mod_modulated` | `define_fx` | `stdlib/effects/modulation/ring_mod_modulated.vibe` | [effects/modulation/ring_mod_modulated.vibe:28](../../../crates/vibelang-std/stdlib/effects/modulation/ring_mod_modulated.vibe#L28) |
| `room_reverb` | `define_fx` | `stdlib/effects/reverbs/room_reverb.vibe` | [effects/reverbs/room_reverb.vibe:15](../../../crates/vibelang-std/stdlib/effects/reverbs/room_reverb.vibe#L15) |
| `rotary` | `define_fx` | `stdlib/effects/modulation/rotary.vibe` | [effects/modulation/rotary.vibe:21](../../../crates/vibelang-std/stdlib/effects/modulation/rotary.vibe#L21) |
| `saturator` | `define_fx` | `stdlib/effects/distortion/saturator.vibe` | [effects/distortion/saturator.vibe:22](../../../crates/vibelang-std/stdlib/effects/distortion/saturator.vibe#L22) |
| `scatter` | `define_fx` | `stdlib/effects/granular/scatter.vibe` | [effects/granular/scatter.vibe:26](../../../crates/vibelang-std/stdlib/effects/granular/scatter.vibe#L26) |
| `shepherd_tone_fx` | `define_fx` | `stdlib/effects/pitch/shepherd_tone_fx.vibe` | [effects/pitch/shepherd_tone_fx.vibe:39](../../../crates/vibelang-std/stdlib/effects/pitch/shepherd_tone_fx.vibe#L39) |
| `shimmer_delay` | `define_fx` | `stdlib/effects/delays/shimmer_delay.vibe` | [effects/delays/shimmer_delay.vibe:21](../../../crates/vibelang-std/stdlib/effects/delays/shimmer_delay.vibe#L21) |
| `shimmer_reverb` | `define_fx` | `stdlib/effects/reverbs/shimmer_reverb.vibe` | [effects/reverbs/shimmer_reverb.vibe:17](../../../crates/vibelang-std/stdlib/effects/reverbs/shimmer_reverb.vibe#L17) |
| `sidechain` | `define_fx` | `stdlib/effects/dynamics/sidechain.vibe` | [effects/dynamics/sidechain.vibe:28](../../../crates/vibelang-std/stdlib/effects/dynamics/sidechain.vibe#L28) |
| `slapback` | `define_fx` | `stdlib/effects/delays/slapback.vibe` | [effects/delays/slapback.vibe:19](../../../crates/vibelang-std/stdlib/effects/delays/slapback.vibe#L19) |
| `spectral_blur` | `define_fx` | `stdlib/effects/spectral/spectral_blur.vibe` | [effects/spectral/spectral_blur.vibe:21](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_blur.vibe#L21) |
| `spectral_brickwall` | `define_fx` | `stdlib/effects/spectral/spectral_brickwall.vibe` | [effects/spectral/spectral_brickwall.vibe:25](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_brickwall.vibe#L25) |
| `spectral_compressor` | `define_fx` | `stdlib/effects/spectral/spectral_compressor.vibe` | [effects/spectral/spectral_compressor.vibe:35](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_compressor.vibe#L35) |
| `spectral_denoise` | `define_fx` | `stdlib/effects/spectral/spectral_denoise.vibe` | [effects/spectral/spectral_denoise.vibe:28](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_denoise.vibe#L28) |
| `spectral_diffuser` | `define_fx` | `stdlib/effects/spectral/spectral_diffuser.vibe` | [effects/spectral/spectral_diffuser.vibe:21](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_diffuser.vibe#L21) |
| `spectral_freeze` | `define_fx` | `stdlib/effects/spectral/spectral_freeze.vibe` | [effects/spectral/spectral_freeze.vibe:22](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_freeze.vibe#L22) |
| `spectral_gate` | `define_fx` | `stdlib/effects/spectral/spectral_gate.vibe` | [effects/spectral/spectral_gate.vibe:25](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_gate.vibe#L25) |
| `spectral_morph` | `define_fx` | `stdlib/effects/spectral/spectral_morph.vibe` | [effects/spectral/spectral_morph.vibe:27](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_morph.vibe#L27) |
| `spectral_phase_shift` | `define_fx` | `stdlib/effects/spectral/spectral_phase_shift.vibe` | [effects/spectral/spectral_phase_shift.vibe:23](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_phase_shift.vibe#L23) |
| `spectral_scrambler` | `define_fx` | `stdlib/effects/spectral/spectral_scrambler.vibe` | [effects/spectral/spectral_scrambler.vibe:24](../../../crates/vibelang-std/stdlib/effects/spectral/spectral_scrambler.vibe#L24) |
| `spring_reverb` | `define_fx` | `stdlib/effects/reverbs/spring_reverb.vibe` | [effects/reverbs/spring_reverb.vibe:15](../../../crates/vibelang-std/stdlib/effects/reverbs/spring_reverb.vibe#L15) |
| `stereo_enhancer` | `define_fx` | `stdlib/effects/spatial/stereo_enhancer.vibe` | [effects/spatial/stereo_enhancer.vibe:19](../../../crates/vibelang-std/stdlib/effects/spatial/stereo_enhancer.vibe#L19) |
| `stereo_width` | `define_fx` | `stdlib/effects/spatial/stereo_width.vibe` | [effects/spatial/stereo_width.vibe:22](../../../crates/vibelang-std/stdlib/effects/spatial/stereo_width.vibe#L22) |
| `stutter` | `define_fx` | `stdlib/effects/utility/stutter.vibe` | [effects/utility/stutter.vibe:19](../../../crates/vibelang-std/stdlib/effects/utility/stutter.vibe#L19) |
| `stutter_repeat` | `define_fx` | `stdlib/effects/glitch/stutter_repeat.vibe` | [effects/glitch/stutter_repeat.vibe:33](../../../crates/vibelang-std/stdlib/effects/glitch/stutter_repeat.vibe#L33) |
| `tape_delay` | `define_fx` | `stdlib/effects/delays/tape_delay.vibe` | [effects/delays/tape_delay.vibe:25](../../../crates/vibelang-std/stdlib/effects/delays/tape_delay.vibe#L25) |
| `tape_loop_lofi` | `define_fx` | `stdlib/effects/character/tape_loop_lofi.vibe` | [effects/character/tape_loop_lofi.vibe:32](../../../crates/vibelang-std/stdlib/effects/character/tape_loop_lofi.vibe#L32) |
| `tape_saturation` | `define_fx` | `stdlib/effects/distortion/tape_saturation.vibe` | [effects/distortion/tape_saturation.vibe:16](../../../crates/vibelang-std/stdlib/effects/distortion/tape_saturation.vibe#L16) |
| `telephone` | `define_fx` | `stdlib/effects/character/telephone.vibe` | [effects/character/telephone.vibe:25](../../../crates/vibelang-std/stdlib/effects/character/telephone.vibe#L25) |
| `transient_glitch` | `define_fx` | `stdlib/effects/glitch/transient_glitch.vibe` | [effects/glitch/transient_glitch.vibe:39](../../../crates/vibelang-std/stdlib/effects/glitch/transient_glitch.vibe#L39) |
| `transient_shaper` | `define_fx` | `stdlib/effects/dynamics/transient_shaper.vibe` | [effects/dynamics/transient_shaper.vibe:25](../../../crates/vibelang-std/stdlib/effects/dynamics/transient_shaper.vibe#L25) |
| `tremolo` | `define_fx` | `stdlib/effects/modulation/tremolo.vibe` | [effects/modulation/tremolo.vibe:21](../../../crates/vibelang-std/stdlib/effects/modulation/tremolo.vibe#L21) |
| `vibrato` | `define_fx` | `stdlib/effects/modulation/vibrato.vibe` | [effects/modulation/vibrato.vibe:21](../../../crates/vibelang-std/stdlib/effects/modulation/vibrato.vibe#L21) |
| `vibrato_drift` | `define_fx` | `stdlib/effects/modulation/vibrato_drift.vibe` | [effects/modulation/vibrato_drift.vibe:44](../../../crates/vibelang-std/stdlib/effects/modulation/vibrato_drift.vibe#L44) |
| `vinyl` | `define_fx` | `stdlib/effects/distortion/vinyl.vibe` | [effects/distortion/vinyl.vibe:27](../../../crates/vibelang-std/stdlib/effects/distortion/vinyl.vibe#L27) |
| `vinyl_dust` | `define_fx` | `stdlib/effects/character/vinyl_dust.vibe` | [effects/character/vinyl_dust.vibe:26](../../../crates/vibelang-std/stdlib/effects/character/vinyl_dust.vibe#L26) |
| `vocoder` | `define_fx` | `stdlib/effects/utility/vocoder.vibe` | [effects/utility/vocoder.vibe:19](../../../crates/vibelang-std/stdlib/effects/utility/vocoder.vibe#L19) |
| `vocoder_pv` | `define_fx` | `stdlib/effects/spectral/vocoder_pv.vibe` | [effects/spectral/vocoder_pv.vibe:37](../../../crates/vibelang-std/stdlib/effects/spectral/vocoder_pv.vibe#L37) |
| `warp1_processor` | `define_fx` | `stdlib/effects/granular/warp1_processor.vibe` | [effects/granular/warp1_processor.vibe:27](../../../crates/vibelang-std/stdlib/effects/granular/warp1_processor.vibe#L27) |
| `waveshaper` | `define_fx` | `stdlib/effects/distortion/waveshaper.vibe` | [effects/distortion/waveshaper.vibe:22](../../../crates/vibelang-std/stdlib/effects/distortion/waveshaper.vibe#L22) |

### fx

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `atmosphere_bright` | `define_synthdef` | `stdlib/fx/atmospheres/atmosphere_bright.vibe` | [fx/atmospheres/atmosphere_bright.vibe:2](../../../crates/vibelang-std/stdlib/fx/atmospheres/atmosphere_bright.vibe#L2) |
| `atmosphere_dark` | `define_synthdef` | `stdlib/fx/atmospheres/atmosphere_dark.vibe` | [fx/atmospheres/atmosphere_dark.vibe:2](../../../crates/vibelang-std/stdlib/fx/atmospheres/atmosphere_dark.vibe#L2) |
| `atmosphere_pulse` | `define_synthdef` | `stdlib/fx/atmospheres/atmosphere_pulse.vibe` | [fx/atmospheres/atmosphere_pulse.vibe:2](../../../crates/vibelang-std/stdlib/fx/atmospheres/atmosphere_pulse.vibe#L2) |
| `atmosphere_tension` | `define_synthdef` | `stdlib/fx/atmospheres/atmosphere_tension.vibe` | [fx/atmospheres/atmosphere_tension.vibe:2](../../../crates/vibelang-std/stdlib/fx/atmospheres/atmosphere_tension.vibe#L2) |
| `atmosphere_wind` | `define_synthdef` | `stdlib/fx/atmospheres/atmosphere_wind.vibe` | [fx/atmospheres/atmosphere_wind.vibe:2](../../../crates/vibelang-std/stdlib/fx/atmospheres/atmosphere_wind.vibe#L2) |
| `downer_filtered` | `define_synthdef` | `stdlib/fx/downers/downer_filtered.vibe` | [fx/downers/downer_filtered.vibe:2](../../../crates/vibelang-std/stdlib/fx/downers/downer_filtered.vibe#L2) |
| `downer_noise` | `define_synthdef` | `stdlib/fx/downers/downer_noise.vibe` | [fx/downers/downer_noise.vibe:2](../../../crates/vibelang-std/stdlib/fx/downers/downer_noise.vibe#L2) |
| `downer_pitch` | `define_synthdef` | `stdlib/fx/downers/downer_pitch.vibe` | [fx/downers/downer_pitch.vibe:2](../../../crates/vibelang-std/stdlib/fx/downers/downer_pitch.vibe#L2) |
| `downer_slow` | `define_synthdef` | `stdlib/fx/downers/downer_slow.vibe` | [fx/downers/downer_slow.vibe:2](../../../crates/vibelang-std/stdlib/fx/downers/downer_slow.vibe#L2) |
| `downer_synth` | `define_synthdef` | `stdlib/fx/downers/downer_synth.vibe` | [fx/downers/downer_synth.vibe:2](../../../crates/vibelang-std/stdlib/fx/downers/downer_synth.vibe#L2) |
| `impact_bass_drop` | `define_synthdef` | `stdlib/fx/impacts/impact_bass_drop.vibe` | [fx/impacts/impact_bass_drop.vibe:2](../../../crates/vibelang-std/stdlib/fx/impacts/impact_bass_drop.vibe#L2) |
| `impact_hard` | `define_synthdef` | `stdlib/fx/impacts/impact_hard.vibe` | [fx/impacts/impact_hard.vibe:2](../../../crates/vibelang-std/stdlib/fx/impacts/impact_hard.vibe#L2) |
| `impact_industrial` | `define_synthdef` | `stdlib/fx/impacts/impact_industrial.vibe` | [fx/impacts/impact_industrial.vibe:2](../../../crates/vibelang-std/stdlib/fx/impacts/impact_industrial.vibe#L2) |
| `impact_metallic` | `define_synthdef` | `stdlib/fx/impacts/impact_metallic.vibe` | [fx/impacts/impact_metallic.vibe:2](../../../crates/vibelang-std/stdlib/fx/impacts/impact_metallic.vibe#L2) |
| `impact_noise` | `define_synthdef` | `stdlib/fx/impacts/impact_noise.vibe` | [fx/impacts/impact_noise.vibe:2](../../../crates/vibelang-std/stdlib/fx/impacts/impact_noise.vibe#L2) |
| `impact_orchestral` | `define_synthdef` | `stdlib/fx/impacts/impact_orchestral.vibe` | [fx/impacts/impact_orchestral.vibe:2](../../../crates/vibelang-std/stdlib/fx/impacts/impact_orchestral.vibe#L2) |
| `impact_soft` | `define_synthdef` | `stdlib/fx/impacts/impact_soft.vibe` | [fx/impacts/impact_soft.vibe:2](../../../crates/vibelang-std/stdlib/fx/impacts/impact_soft.vibe#L2) |
| `impact_sub` | `define_synthdef` | `stdlib/fx/impacts/impact_sub.vibe` | [fx/impacts/impact_sub.vibe:2](../../../crates/vibelang-std/stdlib/fx/impacts/impact_sub.vibe#L2) |
| `impact_trailer` | `define_synthdef` | `stdlib/fx/impacts/impact_trailer.vibe` | [fx/impacts/impact_trailer.vibe:2](../../../crates/vibelang-std/stdlib/fx/impacts/impact_trailer.vibe#L2) |
| `riser_cinematic` | `define_synthdef` | `stdlib/fx/risers/riser_cinematic.vibe` | [fx/risers/riser_cinematic.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_cinematic.vibe#L2) |
| `riser_filtered` | `define_synthdef` | `stdlib/fx/risers/riser_filtered.vibe` | [fx/risers/riser_filtered.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_filtered.vibe#L2) |
| `riser_pink_noise` | `define_synthdef` | `stdlib/fx/risers/riser_pink_noise.vibe` | [fx/risers/riser_pink_noise.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_pink_noise.vibe#L2) |
| `riser_pitch` | `define_synthdef` | `stdlib/fx/risers/riser_pitch.vibe` | [fx/risers/riser_pitch.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_pitch.vibe#L2) |
| `riser_reverse` | `define_synthdef` | `stdlib/fx/risers/riser_reverse.vibe` | [fx/risers/riser_reverse.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_reverse.vibe#L2) |
| `riser_shepard` | `define_synthdef` | `stdlib/fx/risers/riser_shepard.vibe` | [fx/risers/riser_shepard.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_shepard.vibe#L2) |
| `riser_synth` | `define_synthdef` | `stdlib/fx/risers/riser_synth.vibe` | [fx/risers/riser_synth.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_synth.vibe#L2) |
| `riser_tension` | `define_synthdef` | `stdlib/fx/risers/riser_tension.vibe` | [fx/risers/riser_tension.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_tension.vibe#L2) |
| `riser_vocal` | `define_synthdef` | `stdlib/fx/risers/riser_vocal.vibe` | [fx/risers/riser_vocal.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_vocal.vibe#L2) |
| `riser_white_noise` | `define_synthdef` | `stdlib/fx/risers/riser_white_noise.vibe` | [fx/risers/riser_white_noise.vibe:2](../../../crates/vibelang-std/stdlib/fx/risers/riser_white_noise.vibe#L2) |
| `stinger_brass` | `define_synthdef` | `stdlib/fx/stingers/stinger_brass.vibe` | [fx/stingers/stinger_brass.vibe:2](../../../crates/vibelang-std/stdlib/fx/stingers/stinger_brass.vibe#L2) |
| `stinger_bright` | `define_synthdef` | `stdlib/fx/stingers/stinger_bright.vibe` | [fx/stingers/stinger_bright.vibe:2](../../../crates/vibelang-std/stdlib/fx/stingers/stinger_bright.vibe#L2) |
| `stinger_dark` | `define_synthdef` | `stdlib/fx/stingers/stinger_dark.vibe` | [fx/stingers/stinger_dark.vibe:2](../../../crates/vibelang-std/stdlib/fx/stingers/stinger_dark.vibe#L2) |
| `stinger_orchestral` | `define_synthdef` | `stdlib/fx/stingers/stinger_orchestral.vibe` | [fx/stingers/stinger_orchestral.vibe:2](../../../crates/vibelang-std/stdlib/fx/stingers/stinger_orchestral.vibe#L2) |
| `stinger_synth` | `define_synthdef` | `stdlib/fx/stingers/stinger_synth.vibe` | [fx/stingers/stinger_synth.vibe:2](../../../crates/vibelang-std/stdlib/fx/stingers/stinger_synth.vibe#L2) |
| `subdrop_cinematic` | `define_synthdef` | `stdlib/fx/subdrops/subdrop_cinematic.vibe` | [fx/subdrops/subdrop_cinematic.vibe:2](../../../crates/vibelang-std/stdlib/fx/subdrops/subdrop_cinematic.vibe#L2) |
| `subdrop_classic` | `define_synthdef` | `stdlib/fx/subdrops/subdrop_classic.vibe` | [fx/subdrops/subdrop_classic.vibe:2](../../../crates/vibelang-std/stdlib/fx/subdrops/subdrop_classic.vibe#L2) |
| `subdrop_click` | `define_synthdef` | `stdlib/fx/subdrops/subdrop_click.vibe` | [fx/subdrops/subdrop_click.vibe:2](../../../crates/vibelang-std/stdlib/fx/subdrops/subdrop_click.vibe#L2) |
| `subdrop_deep` | `define_synthdef` | `stdlib/fx/subdrops/subdrop_deep.vibe` | [fx/subdrops/subdrop_deep.vibe:2](../../../crates/vibelang-std/stdlib/fx/subdrops/subdrop_deep.vibe#L2) |
| `subdrop_distorted` | `define_synthdef` | `stdlib/fx/subdrops/subdrop_distorted.vibe` | [fx/subdrops/subdrop_distorted.vibe:2](../../../crates/vibelang-std/stdlib/fx/subdrops/subdrop_distorted.vibe#L2) |
| `subdrop_long` | `define_synthdef` | `stdlib/fx/subdrops/subdrop_long.vibe` | [fx/subdrops/subdrop_long.vibe:2](../../../crates/vibelang-std/stdlib/fx/subdrops/subdrop_long.vibe#L2) |
| `subdrop_tight` | `define_synthdef` | `stdlib/fx/subdrops/subdrop_tight.vibe` | [fx/subdrops/subdrop_tight.vibe:2](../../../crates/vibelang-std/stdlib/fx/subdrops/subdrop_tight.vibe#L2) |
| `subdrop_wobble` | `define_synthdef` | `stdlib/fx/subdrops/subdrop_wobble.vibe` | [fx/subdrops/subdrop_wobble.vibe:2](../../../crates/vibelang-std/stdlib/fx/subdrops/subdrop_wobble.vibe#L2) |
| `sweep_cinematic` | `define_synthdef` | `stdlib/fx/sweeps/sweep_cinematic.vibe` | [fx/sweeps/sweep_cinematic.vibe:2](../../../crates/vibelang-std/stdlib/fx/sweeps/sweep_cinematic.vibe#L2) |
| `sweep_filter_down` | `define_synthdef` | `stdlib/fx/sweeps/sweep_filter_down.vibe` | [fx/sweeps/sweep_filter_down.vibe:2](../../../crates/vibelang-std/stdlib/fx/sweeps/sweep_filter_down.vibe#L2) |
| `sweep_filter_up` | `define_synthdef` | `stdlib/fx/sweeps/sweep_filter_up.vibe` | [fx/sweeps/sweep_filter_up.vibe:2](../../../crates/vibelang-std/stdlib/fx/sweeps/sweep_filter_up.vibe#L2) |
| `sweep_laser` | `define_synthdef` | `stdlib/fx/sweeps/sweep_laser.vibe` | [fx/sweeps/sweep_laser.vibe:2](../../../crates/vibelang-std/stdlib/fx/sweeps/sweep_laser.vibe#L2) |
| `sweep_pitch` | `define_synthdef` | `stdlib/fx/sweeps/sweep_pitch.vibe` | [fx/sweeps/sweep_pitch.vibe:2](../../../crates/vibelang-std/stdlib/fx/sweeps/sweep_pitch.vibe#L2) |
| `sweep_release` | `define_synthdef` | `stdlib/fx/sweeps/sweep_release.vibe` | [fx/sweeps/sweep_release.vibe:2](../../../crates/vibelang-std/stdlib/fx/sweeps/sweep_release.vibe#L2) |
| `sweep_reverse` | `define_synthdef` | `stdlib/fx/sweeps/sweep_reverse.vibe` | [fx/sweeps/sweep_reverse.vibe:2](../../../crates/vibelang-std/stdlib/fx/sweeps/sweep_reverse.vibe#L2) |
| `sweep_tension` | `define_synthdef` | `stdlib/fx/sweeps/sweep_tension.vibe` | [fx/sweeps/sweep_tension.vibe:2](../../../crates/vibelang-std/stdlib/fx/sweeps/sweep_tension.vibe#L2) |
| `sweep_whoosh` | `define_synthdef` | `stdlib/fx/sweeps/sweep_whoosh.vibe` | [fx/sweeps/sweep_whoosh.vibe:2](../../../crates/vibelang-std/stdlib/fx/sweeps/sweep_whoosh.vibe#L2) |
| `transition_glitch` | `define_synthdef` | `stdlib/fx/transitions/transition_glitch.vibe` | [fx/transitions/transition_glitch.vibe:2](../../../crates/vibelang-std/stdlib/fx/transitions/transition_glitch.vibe#L2) |
| `transition_reverse` | `define_synthdef` | `stdlib/fx/transitions/transition_reverse.vibe` | [fx/transitions/transition_reverse.vibe:2](../../../crates/vibelang-std/stdlib/fx/transitions/transition_reverse.vibe#L2) |
| `transition_sweep` | `define_synthdef` | `stdlib/fx/transitions/transition_sweep.vibe` | [fx/transitions/transition_sweep.vibe:2](../../../crates/vibelang-std/stdlib/fx/transitions/transition_sweep.vibe#L2) |
| `whoosh_fast` | `define_synthdef` | `stdlib/fx/transitions/whoosh_fast.vibe` | [fx/transitions/whoosh_fast.vibe:2](../../../crates/vibelang-std/stdlib/fx/transitions/whoosh_fast.vibe#L2) |
| `whoosh_slow` | `define_synthdef` | `stdlib/fx/transitions/whoosh_slow.vibe` | [fx/transitions/whoosh_slow.vibe:2](../../../crates/vibelang-std/stdlib/fx/transitions/whoosh_slow.vibe#L2) |

### instruments

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `beads` | `define_synthdef` | `stdlib/instruments/eurorack/beads.vibe` | [instruments/eurorack/beads.vibe:44](../../../crates/vibelang-std/stdlib/instruments/eurorack/beads.vibe#L44) |
| `cv_bus` | `define_synthdef` | `stdlib/instruments/eurorack/cv_bus.vibe` | [instruments/eurorack/cv_bus.vibe:15](../../../crates/vibelang-std/stdlib/instruments/eurorack/cv_bus.vibe#L15) |
| `erica_bass_drum` | `define_synthdef` | `stdlib/instruments/eurorack/erica_bass_drum.vibe` | [instruments/eurorack/erica_bass_drum.vibe:20](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_bass_drum.vibe#L20) |
| `erica_bassline` | `define_synthdef` | `stdlib/instruments/eurorack/erica_bassline.vibe` | [instruments/eurorack/erica_bassline.vibe:27](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_bassline.vibe#L27) |
| `erica_black_vco2` | `define_synthdef` | `stdlib/instruments/eurorack/erica_black_vco2.vibe` | [instruments/eurorack/erica_black_vco2.vibe:28](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_black_vco2.vibe#L28) |
| `erica_hi_hats` | `define_synthdef` | `stdlib/instruments/eurorack/erica_hi_hats.vibe` | [instruments/eurorack/erica_hi_hats.vibe:19](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_hi_hats.vibe#L19) |
| `erica_snare_drum` | `define_synthdef` | `stdlib/instruments/eurorack/erica_snare_drum.vibe` | [instruments/eurorack/erica_snare_drum.vibe:18](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_snare_drum.vibe#L18) |
| `erica_vc_clock` | `define_synthdef` | `stdlib/instruments/eurorack/erica_vc_clock.vibe` | [instruments/eurorack/erica_vc_clock.vibe:22](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_vc_clock.vibe#L22) |
| `erica_vc_eg` | `define_synthdef` | `stdlib/instruments/eurorack/erica_vc_eg.vibe` | [instruments/eurorack/erica_vc_eg.vibe:44](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_vc_eg.vibe#L44) |
| `erica_wavetable_vco` | `define_synthdef` | `stdlib/instruments/eurorack/erica_wavetable_vco.vibe` | [instruments/eurorack/erica_wavetable_vco.vibe:112](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_wavetable_vco.vibe#L112) |
| `frames` | `define_synthdef` | `stdlib/instruments/eurorack/frames.vibe` | [instruments/eurorack/frames.vibe:109](../../../crates/vibelang-std/stdlib/instruments/eurorack/frames.vibe#L109) |
| `marbles` | `define_synthdef` | `stdlib/instruments/eurorack/marbles.vibe` | [instruments/eurorack/marbles.vibe:65](../../../crates/vibelang-std/stdlib/instruments/eurorack/marbles.vibe#L65) |
| `maths` | `define_synthdef` | `stdlib/instruments/eurorack/maths.vibe` | [instruments/eurorack/maths.vibe:32](../../../crates/vibelang-std/stdlib/instruments/eurorack/maths.vibe#L32) |
| `morphagene` | `define_synthdef` | `stdlib/instruments/sampler/morphagene.vibe` | [instruments/sampler/morphagene.vibe:153](../../../crates/vibelang-std/stdlib/instruments/sampler/morphagene.vibe#L153) |
| `morphagene_reel_fill` | `define_synthdef` | `stdlib/instruments/sampler/morphagene.vibe` | [instruments/sampler/morphagene.vibe:490](../../../crates/vibelang-std/stdlib/instruments/sampler/morphagene.vibe#L490) |
| `otey_piano` | `define_synthdef` | `stdlib/instruments/keyboard/otey_piano.vibe` | [instruments/keyboard/otey_piano.vibe:27](../../../crates/vibelang-std/stdlib/instruments/keyboard/otey_piano.vibe#L27) |
| `peg` | `define_synthdef` | `stdlib/instruments/eurorack/peg.vibe` | [instruments/eurorack/peg.vibe:20](../../../crates/vibelang-std/stdlib/instruments/eurorack/peg.vibe#L20) |
| `plaits` | `define_synthdef` | `stdlib/instruments/eurorack/plaits.vibe` | [instruments/eurorack/plaits.vibe:43](../../../crates/vibelang-std/stdlib/instruments/eurorack/plaits.vibe#L43) |
| `plonk` | `define_synthdef` | `stdlib/instruments/eurorack/plonk.vibe` | [instruments/eurorack/plonk.vibe:31](../../../crates/vibelang-std/stdlib/instruments/eurorack/plonk.vibe#L31) |
| `prss_pnt` | `define_synthdef` | `stdlib/instruments/eurorack/prss_pnt.vibe` | [instruments/eurorack/prss_pnt.vibe:12](../../../crates/vibelang-std/stdlib/instruments/eurorack/prss_pnt.vibe#L12) |
| `qcd` | `define_synthdef` | `stdlib/instruments/eurorack/qcd.vibe` | [instruments/eurorack/qcd.vibe:20](../../../crates/vibelang-std/stdlib/instruments/eurorack/qcd.vibe#L20) |
| `quadrax` | `define_synthdef` | `stdlib/instruments/eurorack/quadrax.vibe` | [instruments/eurorack/quadrax.vibe:120](../../../crates/vibelang-std/stdlib/instruments/eurorack/quadrax.vibe#L120) |
| `rene` | `define_synthdef` | `stdlib/instruments/eurorack/rene.vibe` | [instruments/eurorack/rene.vibe:31](../../../crates/vibelang-std/stdlib/instruments/eurorack/rene.vibe#L31) |
| `rings` | `define_synthdef` | `stdlib/instruments/eurorack/rings.vibe` | [instruments/eurorack/rings.vibe:37](../../../crates/vibelang-std/stdlib/instruments/eurorack/rings.vibe#L37) |
| `spectraphon` | `define_synthdef` | `stdlib/instruments/spectral/spectraphon.vibe` | [instruments/spectral/spectraphon.vibe:45](../../../crates/vibelang-std/stdlib/instruments/spectral/spectraphon.vibe#L45) |
| `stages` | `define_synthdef` | `stdlib/instruments/eurorack/stages.vibe` | [instruments/eurorack/stages.vibe:120](../../../crates/vibelang-std/stdlib/instruments/eurorack/stages.vibe#L120) |
| `steppy` | `define_synthdef` | `stdlib/instruments/eurorack/steppy.vibe` | [instruments/eurorack/steppy.vibe:22](../../../crates/vibelang-std/stdlib/instruments/eurorack/steppy.vibe#L22) |
| `tempi` | `define_synthdef` | `stdlib/instruments/eurorack/tempi.vibe` | [instruments/eurorack/tempi.vibe:23](../../../crates/vibelang-std/stdlib/instruments/eurorack/tempi.vibe#L23) |
| `tetrapad` | `define_synthdef` | `stdlib/instruments/eurorack/tetrapad.vibe` | [instruments/eurorack/tetrapad.vibe:254](../../../crates/vibelang-std/stdlib/instruments/eurorack/tetrapad.vibe#L254) |
| `tides2` | `define_synthdef` | `stdlib/instruments/eurorack/tides2.vibe` | [instruments/eurorack/tides2.vibe:53](../../../crates/vibelang-std/stdlib/instruments/eurorack/tides2.vibe#L53) |
| `verbos_harmonic_osc` | `define_synthdef` | `stdlib/instruments/eurorack/verbos_harmonic_osc.vibe` | [instruments/eurorack/verbos_harmonic_osc.vibe:21](../../../crates/vibelang-std/stdlib/instruments/eurorack/verbos_harmonic_osc.vibe#L21) |
| `verbos_multistage` | `define_synthdef` | `stdlib/instruments/eurorack/verbos_multistage.vibe` | [instruments/eurorack/verbos_multistage.vibe:20](../../../crates/vibelang-std/stdlib/instruments/eurorack/verbos_multistage.vibe#L20) |
| `verbos_random_sampling` | `define_synthdef` | `stdlib/instruments/eurorack/verbos_random_sampling.vibe` | [instruments/eurorack/verbos_random_sampling.vibe:20](../../../crates/vibelang-std/stdlib/instruments/eurorack/verbos_random_sampling.vibe#L20) |
| `verbos_touchplate` | `define_synthdef` | `stdlib/instruments/eurorack/verbos_touchplate.vibe` | [instruments/eurorack/verbos_touchplate.vibe:20](../../../crates/vibelang-std/stdlib/instruments/eurorack/verbos_touchplate.vibe#L20) |
| `wogglebug` | `define_synthdef` | `stdlib/instruments/eurorack/wogglebug.vibe` | [instruments/eurorack/wogglebug.vibe:31](../../../crates/vibelang-std/stdlib/instruments/eurorack/wogglebug.vibe#L31) |

### keys

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `accordion_keys` | `define_synthdef` | `stdlib/keys/accordion_keys.vibe` | [keys/accordion_keys.vibe:2](../../../crates/vibelang-std/stdlib/keys/accordion_keys.vibe#L2) |
| `celesta` | `define_synthdef` | `stdlib/keys/mallet_instruments.vibe` | [keys/mallet_instruments.vibe:45](../../../crates/vibelang-std/stdlib/keys/mallet_instruments.vibe#L45) |
| `celeste` | `define_synthdef` | `stdlib/keys/celeste.vibe` | [keys/celeste.vibe:6](../../../crates/vibelang-std/stdlib/keys/celeste.vibe#L6) |
| `clavinet` | `define_synthdef` | `stdlib/keys/clavinet.vibe` | [keys/clavinet.vibe:7](../../../crates/vibelang-std/stdlib/keys/clavinet.vibe#L7) |
| `crotales` | `define_synthdef` | `stdlib/keys/mallet_instruments.vibe` | [keys/mallet_instruments.vibe:15](../../../crates/vibelang-std/stdlib/keys/mallet_instruments.vibe#L15) |
| `electric_piano` | `define_synthdef` | `stdlib/keys/electric_piano.vibe` | [keys/electric_piano.vibe:7](../../../crates/vibelang-std/stdlib/keys/electric_piano.vibe#L7) |
| `grand_piano` | `define_synthdef` | `stdlib/keys/grand_piano.vibe` | [keys/grand_piano.vibe:12](../../../crates/vibelang-std/stdlib/keys/grand_piano.vibe#L12) |
| `grand_piano_bright` | `define_synthdef` | `stdlib/keys/grand_piano.vibe` | [keys/grand_piano.vibe:97](../../../crates/vibelang-std/stdlib/keys/grand_piano.vibe#L97) |
| `grand_piano_warm` | `define_synthdef` | `stdlib/keys/grand_piano.vibe` | [keys/grand_piano.vibe:146](../../../crates/vibelang-std/stdlib/keys/grand_piano.vibe#L146) |
| `hammond_organ` | `define_synthdef` | `stdlib/keys/hammond_organ.vibe` | [keys/hammond_organ.vibe:7](../../../crates/vibelang-std/stdlib/keys/hammond_organ.vibe#L7) |
| `harpsichord` | `define_synthdef` | `stdlib/keys/harpsichord.vibe` | [keys/harpsichord.vibe:2](../../../crates/vibelang-std/stdlib/keys/harpsichord.vibe#L2) |
| `honky_tonk` | `define_synthdef` | `stdlib/keys/honky_tonk.vibe` | [keys/honky_tonk.vibe:6](../../../crates/vibelang-std/stdlib/keys/honky_tonk.vibe#L6) |
| `mellotron_choir` | `define_synthdef` | `stdlib/keys/mellotron_choir.vibe` | [keys/mellotron_choir.vibe:2](../../../crates/vibelang-std/stdlib/keys/mellotron_choir.vibe#L2) |
| `mellotron_flute` | `define_synthdef` | `stdlib/keys/mellotron_flute.vibe` | [keys/mellotron_flute.vibe:2](../../../crates/vibelang-std/stdlib/keys/mellotron_flute.vibe#L2) |
| `mellotron_strings` | `define_synthdef` | `stdlib/keys/mellotron_strings.vibe` | [keys/mellotron_strings.vibe:2](../../../crates/vibelang-std/stdlib/keys/mellotron_strings.vibe#L2) |
| `organ_b3` | `define_synthdef` | `stdlib/keys/organ_b3.vibe` | [keys/organ_b3.vibe:2](../../../crates/vibelang-std/stdlib/keys/organ_b3.vibe#L2) |
| `piano_felt` | `define_synthdef` | `stdlib/keys/piano_felt.vibe` | [keys/piano_felt.vibe:2](../../../crates/vibelang-std/stdlib/keys/piano_felt.vibe#L2) |
| `piano_toy` | `define_synthdef` | `stdlib/keys/piano_toy.vibe` | [keys/piano_toy.vibe:2](../../../crates/vibelang-std/stdlib/keys/piano_toy.vibe#L2) |
| `pipe_organ` | `define_synthdef` | `stdlib/keys/pipe_organ.vibe` | [keys/pipe_organ.vibe:2](../../../crates/vibelang-std/stdlib/keys/pipe_organ.vibe#L2) |
| `rhodes_bright` | `define_synthdef` | `stdlib/keys/rhodes_bright.vibe` | [keys/rhodes_bright.vibe:2](../../../crates/vibelang-std/stdlib/keys/rhodes_bright.vibe#L2) |
| `rhodes_dark` | `define_synthdef` | `stdlib/keys/rhodes_dark.vibe` | [keys/rhodes_dark.vibe:2](../../../crates/vibelang-std/stdlib/keys/rhodes_dark.vibe#L2) |
| `tack_piano` | `define_synthdef` | `stdlib/keys/grand_piano.vibe` | [keys/grand_piano.vibe:233](../../../crates/vibelang-std/stdlib/keys/grand_piano.vibe#L233) |
| `upright_piano` | `define_synthdef` | `stdlib/keys/grand_piano.vibe` | [keys/grand_piano.vibe:189](../../../crates/vibelang-std/stdlib/keys/grand_piano.vibe#L189) |
| `wurlitzer` | `define_synthdef` | `stdlib/keys/wurlitzer.vibe` | [keys/wurlitzer.vibe:6](../../../crates/vibelang-std/stdlib/keys/wurlitzer.vibe#L6) |

### leads

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `lead_aggressive` | `define_synthdef` | `stdlib/leads/synth/lead_aggressive.vibe` | [leads/synth/lead_aggressive.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_aggressive.vibe#L2) |
| `lead_arp` | `define_synthdef` | `stdlib/leads/classic/lead_arp.vibe` | [leads/classic/lead_arp.vibe:2](../../../crates/vibelang-std/stdlib/leads/classic/lead_arp.vibe#L2) |
| `lead_bitcrushed` | `define_synthdef` | `stdlib/leads/organic/lead_bitcrushed.vibe` | [leads/organic/lead_bitcrushed.vibe:2](../../../crates/vibelang-std/stdlib/leads/organic/lead_bitcrushed.vibe#L2) |
| `lead_bright` | `define_synthdef` | `stdlib/leads/synth/lead_bright.vibe` | [leads/synth/lead_bright.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_bright.vibe#L2) |
| `lead_complextro` | `define_synthdef` | `stdlib/leads/modern/lead_complextro.vibe` | [leads/modern/lead_complextro.vibe:2](../../../crates/vibelang-std/stdlib/leads/modern/lead_complextro.vibe#L2) |
| `lead_cs80` | `define_synthdef` | `stdlib/leads/classic/lead_cs80.vibe` | [leads/classic/lead_cs80.vibe:2](../../../crates/vibelang-std/stdlib/leads/classic/lead_cs80.vibe#L2) |
| `lead_dark` | `define_synthdef` | `stdlib/leads/synth/lead_dark.vibe` | [leads/synth/lead_dark.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_dark.vibe#L2) |
| `lead_detuned` | `define_synthdef` | `stdlib/leads/synth/lead_detuned.vibe` | [leads/synth/lead_detuned.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_detuned.vibe#L2) |
| `lead_dx7` | `define_synthdef` | `stdlib/leads/classic/lead_dx7.vibe` | [leads/classic/lead_dx7.vibe:2](../../../crates/vibelang-std/stdlib/leads/classic/lead_dx7.vibe#L2) |
| `lead_filtered` | `define_synthdef` | `stdlib/leads/synth/lead_filtered.vibe` | [leads/synth/lead_filtered.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_filtered.vibe#L2) |
| `lead_flute` | `define_synthdef` | `stdlib/leads/organic/lead_flute.vibe` | [leads/organic/lead_flute.vibe:2](../../../crates/vibelang-std/stdlib/leads/organic/lead_flute.vibe#L2) |
| `lead_future_bass` | `define_synthdef` | `stdlib/leads/modern/lead_future_bass.vibe` | [leads/modern/lead_future_bass.vibe:2](../../../crates/vibelang-std/stdlib/leads/modern/lead_future_bass.vibe#L2) |
| `lead_granular` | `define_synthdef` | `stdlib/leads/organic/lead_granular.vibe` | [leads/organic/lead_granular.vibe:2](../../../crates/vibelang-std/stdlib/leads/organic/lead_granular.vibe#L2) |
| `lead_hardstyle` | `define_synthdef` | `stdlib/leads/modern/lead_hardstyle.vibe` | [leads/modern/lead_hardstyle.vibe:2](../../../crates/vibelang-std/stdlib/leads/modern/lead_hardstyle.vibe#L2) |
| `lead_juno_hoover` | `define_synthdef` | `stdlib/leads/classic/lead_juno_hoover.vibe` | [leads/classic/lead_juno_hoover.vibe:2](../../../crates/vibelang-std/stdlib/leads/classic/lead_juno_hoover.vibe#L2) |
| `lead_moog` | `define_synthdef` | `stdlib/leads/classic/lead_moog.vibe` | [leads/classic/lead_moog.vibe:2](../../../crates/vibelang-std/stdlib/leads/classic/lead_moog.vibe#L2) |
| `lead_ms20` | `define_synthdef` | `stdlib/leads/classic/lead_ms20.vibe` | [leads/classic/lead_ms20.vibe:2](../../../crates/vibelang-std/stdlib/leads/classic/lead_ms20.vibe#L2) |
| `lead_neuro` | `define_synthdef` | `stdlib/leads/modern/lead_neuro.vibe` | [leads/modern/lead_neuro.vibe:2](../../../crates/vibelang-std/stdlib/leads/modern/lead_neuro.vibe#L2) |
| `lead_pluck_bell` | `define_synthdef` | `stdlib/leads/pluck/pluck_bell.vibe` | [leads/pluck/pluck_bell.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_bell.vibe#L2) |
| `lead_pluck_bright` | `define_synthdef` | `stdlib/leads/pluck/pluck_bright.vibe` | [leads/pluck/pluck_bright.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_bright.vibe#L2) |
| `lead_pluck_long` | `define_synthdef` | `stdlib/leads/pluck/pluck_long.vibe` | [leads/pluck/pluck_long.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_long.vibe#L2) |
| `lead_pluck_muted` | `define_synthdef` | `stdlib/leads/pluck/pluck_muted.vibe` | [leads/pluck/pluck_muted.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_muted.vibe#L2) |
| `lead_pluck_resonant` | `define_synthdef` | `stdlib/leads/pluck/pluck_resonant.vibe` | [leads/pluck/pluck_resonant.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_resonant.vibe#L2) |
| `lead_pluck_short` | `define_synthdef` | `stdlib/leads/pluck/pluck_short.vibe` | [leads/pluck/pluck_short.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_short.vibe#L2) |
| `lead_progressive` | `define_synthdef` | `stdlib/leads/modern/lead_progressive.vibe` | [leads/modern/lead_progressive.vibe:2](../../../crates/vibelang-std/stdlib/leads/modern/lead_progressive.vibe#L2) |
| `lead_prophet` | `define_synthdef` | `stdlib/leads/classic/lead_prophet.vibe` | [leads/classic/lead_prophet.vibe:2](../../../crates/vibelang-std/stdlib/leads/classic/lead_prophet.vibe#L2) |
| `lead_psytrance` | `define_synthdef` | `stdlib/leads/modern/lead_psytrance.vibe` | [leads/modern/lead_psytrance.vibe:2](../../../crates/vibelang-std/stdlib/leads/modern/lead_psytrance.vibe#L2) |
| `lead_pwm` | `define_synthdef` | `stdlib/leads/synth/lead_pwm.vibe` | [leads/synth/lead_pwm.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_pwm.vibe#L2) |
| `lead_saw` | `define_synthdef` | `stdlib/leads/synth/lead_saw.vibe` | [leads/synth/lead_saw.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_saw.vibe#L2) |
| `lead_sh101` | `define_synthdef` | `stdlib/leads/classic/lead_sh101.vibe` | [leads/classic/lead_sh101.vibe:2](../../../crates/vibelang-std/stdlib/leads/classic/lead_sh101.vibe#L2) |
| `lead_smooth` | `define_synthdef` | `stdlib/leads/synth/lead_smooth.vibe` | [leads/synth/lead_smooth.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_smooth.vibe#L2) |
| `lead_square` | `define_synthdef` | `stdlib/leads/synth/lead_square.vibe` | [leads/synth/lead_square.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_square.vibe#L2) |
| `lead_supersaw` | `define_synthdef` | `stdlib/leads/synth/lead_supersaw.vibe` | [leads/synth/lead_supersaw.vibe:2](../../../crates/vibelang-std/stdlib/leads/synth/lead_supersaw.vibe#L2) |
| `lead_tape` | `define_synthdef` | `stdlib/leads/organic/lead_tape.vibe` | [leads/organic/lead_tape.vibe:2](../../../crates/vibelang-std/stdlib/leads/organic/lead_tape.vibe#L2) |
| `lead_theremin` | `define_synthdef` | `stdlib/leads/organic/lead_theremin.vibe` | [leads/organic/lead_theremin.vibe:2](../../../crates/vibelang-std/stdlib/leads/organic/lead_theremin.vibe#L2) |
| `lead_trance` | `define_synthdef` | `stdlib/leads/modern/lead_trance.vibe` | [leads/modern/lead_trance.vibe:2](../../../crates/vibelang-std/stdlib/leads/modern/lead_trance.vibe#L2) |
| `lead_vocal` | `define_synthdef` | `stdlib/leads/organic/lead_vocal.vibe` | [leads/organic/lead_vocal.vibe:2](../../../crates/vibelang-std/stdlib/leads/organic/lead_vocal.vibe#L2) |
| `lead_whistle` | `define_synthdef` | `stdlib/leads/organic/lead_whistle.vibe` | [leads/organic/lead_whistle.vibe:2](../../../crates/vibelang-std/stdlib/leads/organic/lead_whistle.vibe#L2) |
| `pluck_harp` | `define_synthdef` | `stdlib/leads/pluck/pluck_harp.vibe` | [leads/pluck/pluck_harp.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_harp.vibe#L2) |
| `pluck_kalimba` | `define_synthdef` | `stdlib/leads/pluck/pluck_kalimba.vibe` | [leads/pluck/pluck_kalimba.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_kalimba.vibe#L2) |
| `pluck_marimba` | `define_synthdef` | `stdlib/leads/pluck/pluck_marimba.vibe` | [leads/pluck/pluck_marimba.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_marimba.vibe#L2) |
| `pluck_piano` | `define_synthdef` | `stdlib/leads/pluck/pluck_piano.vibe` | [leads/pluck/pluck_piano.vibe:2](../../../crates/vibelang-std/stdlib/leads/pluck/pluck_piano.vibe#L2) |
| `stab_brass` | `define_synthdef` | `stdlib/leads/stabs/stab_brass.vibe` | [leads/stabs/stab_brass.vibe:2](../../../crates/vibelang-std/stdlib/leads/stabs/stab_brass.vibe#L2) |
| `stab_bright` | `define_synthdef` | `stdlib/leads/stabs/stab_bright.vibe` | [leads/stabs/stab_bright.vibe:2](../../../crates/vibelang-std/stdlib/leads/stabs/stab_bright.vibe#L2) |
| `stab_dark` | `define_synthdef` | `stdlib/leads/stabs/stab_dark.vibe` | [leads/stabs/stab_dark.vibe:2](../../../crates/vibelang-std/stdlib/leads/stabs/stab_dark.vibe#L2) |
| `stab_distorted` | `define_synthdef` | `stdlib/leads/stabs/stab_distorted.vibe` | [leads/stabs/stab_distorted.vibe:2](../../../crates/vibelang-std/stdlib/leads/stabs/stab_distorted.vibe#L2) |
| `stab_orchestral` | `define_synthdef` | `stdlib/leads/stabs/stab_orchestral.vibe` | [leads/stabs/stab_orchestral.vibe:2](../../../crates/vibelang-std/stdlib/leads/stabs/stab_orchestral.vibe#L2) |
| `stab_piano` | `define_synthdef` | `stdlib/leads/stabs/stab_piano.vibe` | [leads/stabs/stab_piano.vibe:2](../../../crates/vibelang-std/stdlib/leads/stabs/stab_piano.vibe#L2) |
| `stab_short` | `define_synthdef` | `stdlib/leads/stabs/stab_short.vibe` | [leads/stabs/stab_short.vibe:2](../../../crates/vibelang-std/stdlib/leads/stabs/stab_short.vibe#L2) |
| `stab_super` | `define_synthdef` | `stdlib/leads/stabs/stab_super.vibe` | [leads/stabs/stab_super.vibe:2](../../../crates/vibelang-std/stdlib/leads/stabs/stab_super.vibe#L2) |

### modern

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `dubstep_chord` | `define_synthdef` | `stdlib/modern/dubstep.vibe` | [modern/dubstep.vibe:145](../../../crates/vibelang-std/stdlib/modern/dubstep.vibe#L145) |
| `dubstep_growl` | `define_synthdef` | `stdlib/modern/dubstep.vibe` | [modern/dubstep.vibe:43](../../../crates/vibelang-std/stdlib/modern/dubstep.vibe#L43) |
| `dubstep_riddim` | `define_synthdef` | `stdlib/modern/dubstep.vibe` | [modern/dubstep.vibe:79](../../../crates/vibelang-std/stdlib/modern/dubstep.vibe#L79) |
| `dubstep_snare` | `define_synthdef` | `stdlib/modern/dubstep.vibe` | [modern/dubstep.vibe:114](../../../crates/vibelang-std/stdlib/modern/dubstep.vibe#L114) |
| `dubstep_wobble` | `define_synthdef` | `stdlib/modern/dubstep.vibe` | [modern/dubstep.vibe:7](../../../crates/vibelang-std/stdlib/modern/dubstep.vibe#L7) |
| `future_chord` | `define_synthdef` | `stdlib/modern/future_bass.vibe` | [modern/future_bass.vibe:7](../../../crates/vibelang-std/stdlib/modern/future_bass.vibe#L7) |
| `future_lead` | `define_synthdef` | `stdlib/modern/future_bass.vibe` | [modern/future_bass.vibe:41](../../../crates/vibelang-std/stdlib/modern/future_bass.vibe#L41) |
| `future_pad` | `define_synthdef` | `stdlib/modern/future_bass.vibe` | [modern/future_bass.vibe:97](../../../crates/vibelang-std/stdlib/modern/future_bass.vibe#L97) |
| `future_pluck` | `define_synthdef` | `stdlib/modern/future_bass.vibe` | [modern/future_bass.vibe:125](../../../crates/vibelang-std/stdlib/modern/future_bass.vibe#L125) |
| `future_wobble` | `define_synthdef` | `stdlib/modern/future_bass.vibe` | [modern/future_bass.vibe:67](../../../crates/vibelang-std/stdlib/modern/future_bass.vibe#L67) |
| `hyperpop_bass` | `define_synthdef` | `stdlib/modern/hyperpop.vibe` | [modern/hyperpop.vibe:68](../../../crates/vibelang-std/stdlib/modern/hyperpop.vibe#L68) |
| `hyperpop_chord` | `define_synthdef` | `stdlib/modern/hyperpop.vibe` | [modern/hyperpop.vibe:37](../../../crates/vibelang-std/stdlib/modern/hyperpop.vibe#L37) |
| `hyperpop_hat` | `define_synthdef` | `stdlib/modern/hyperpop.vibe` | [modern/hyperpop.vibe:133](../../../crates/vibelang-std/stdlib/modern/hyperpop.vibe#L133) |
| `hyperpop_lead` | `define_synthdef` | `stdlib/modern/hyperpop.vibe` | [modern/hyperpop.vibe:7](../../../crates/vibelang-std/stdlib/modern/hyperpop.vibe#L7) |
| `hyperpop_synth` | `define_synthdef` | `stdlib/modern/hyperpop.vibe` | [modern/hyperpop.vibe:103](../../../crates/vibelang-std/stdlib/modern/hyperpop.vibe#L103) |
| `lofi_bass` | `define_synthdef` | `stdlib/modern/lofi.vibe` | [modern/lofi.vibe:76](../../../crates/vibelang-std/stdlib/modern/lofi.vibe#L76) |
| `lofi_crackle` | `define_synthdef` | `stdlib/modern/lofi.vibe` | [modern/lofi.vibe:132](../../../crates/vibelang-std/stdlib/modern/lofi.vibe#L132) |
| `lofi_keys` | `define_synthdef` | `stdlib/modern/lofi.vibe` | [modern/lofi.vibe:7](../../../crates/vibelang-std/stdlib/modern/lofi.vibe#L7) |
| `lofi_pad` | `define_synthdef` | `stdlib/modern/lofi.vibe` | [modern/lofi.vibe:102](../../../crates/vibelang-std/stdlib/modern/lofi.vibe#L102) |
| `lofi_piano` | `define_synthdef` | `stdlib/modern/lofi.vibe` | [modern/lofi.vibe:40](../../../crates/vibelang-std/stdlib/modern/lofi.vibe#L40) |
| `serum_growl` | `define_synthdef` | `stdlib/modern/serum_style.vibe` | [modern/serum_style.vibe:8](../../../crates/vibelang-std/stdlib/modern/serum_style.vibe#L8) |
| `serum_hoover` | `define_synthdef` | `stdlib/modern/serum_style.vibe` | [modern/serum_style.vibe:146](../../../crates/vibelang-std/stdlib/modern/serum_style.vibe#L146) |
| `serum_pluck` | `define_synthdef` | `stdlib/modern/serum_style.vibe` | [modern/serum_style.vibe:114](../../../crates/vibelang-std/stdlib/modern/serum_style.vibe#L114) |
| `serum_reese` | `define_synthdef` | `stdlib/modern/serum_style.vibe` | [modern/serum_style.vibe:47](../../../crates/vibelang-std/stdlib/modern/serum_style.vibe#L47) |
| `serum_supersaw` | `define_synthdef` | `stdlib/modern/serum_style.vibe` | [modern/serum_style.vibe:81](../../../crates/vibelang-std/stdlib/modern/serum_style.vibe#L81) |
| `synthwave_arp` | `define_synthdef` | `stdlib/modern/synthwave.vibe` | [modern/synthwave.vibe:97](../../../crates/vibelang-std/stdlib/modern/synthwave.vibe#L97) |
| `synthwave_bass` | `define_synthdef` | `stdlib/modern/synthwave.vibe` | [modern/synthwave.vibe:7](../../../crates/vibelang-std/stdlib/modern/synthwave.vibe#L7) |
| `synthwave_lead` | `define_synthdef` | `stdlib/modern/synthwave.vibe` | [modern/synthwave.vibe:40](../../../crates/vibelang-std/stdlib/modern/synthwave.vibe#L40) |
| `synthwave_pad` | `define_synthdef` | `stdlib/modern/synthwave.vibe` | [modern/synthwave.vibe:67](../../../crates/vibelang-std/stdlib/modern/synthwave.vibe#L67) |
| `synthwave_snare` | `define_synthdef` | `stdlib/modern/synthwave.vibe` | [modern/synthwave.vibe:128](../../../crates/vibelang-std/stdlib/modern/synthwave.vibe#L128) |
| `trap_808_dist` | `define_synthdef` | `stdlib/modern/trap.vibe` | [modern/trap.vibe:43](../../../crates/vibelang-std/stdlib/modern/trap.vibe#L43) |
| `trap_808_long` | `define_synthdef` | `stdlib/modern/trap.vibe` | [modern/trap.vibe:7](../../../crates/vibelang-std/stdlib/modern/trap.vibe#L7) |
| `trap_bell` | `define_synthdef` | `stdlib/modern/trap.vibe` | [modern/trap.vibe:150](../../../crates/vibelang-std/stdlib/modern/trap.vibe#L150) |
| `trap_hat` | `define_synthdef` | `stdlib/modern/trap.vibe` | [modern/trap.vibe:81](../../../crates/vibelang-std/stdlib/modern/trap.vibe#L81) |
| `trap_hat_roll` | `define_synthdef` | `stdlib/modern/trap.vibe` | [modern/trap.vibe:104](../../../crates/vibelang-std/stdlib/modern/trap.vibe#L104) |
| `trap_lead` | `define_synthdef` | `stdlib/modern/trap.vibe` | [modern/trap.vibe:124](../../../crates/vibelang-std/stdlib/modern/trap.vibe#L124) |

### modular

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `mod_lfo` | `define_synthdef` | `stdlib/modular/mod_lfo.vibe` | [modular/mod_lfo.vibe:2](../../../crates/vibelang-std/stdlib/modular/mod_lfo.vibe#L2) |
| `mod_noise` | `define_synthdef` | `stdlib/modular/mod_noise.vibe` | [modular/mod_noise.vibe:2](../../../crates/vibelang-std/stdlib/modular/mod_noise.vibe#L2) |
| `mod_vco` | `define_synthdef` | `stdlib/modular/mod_vco.vibe` | [modular/mod_vco.vibe:2](../../../crates/vibelang-std/stdlib/modular/mod_vco.vibe#L2) |

### orchestral

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `brass_stab` | `define_synthdef` | `stdlib/orchestral/brass_stab.vibe` | [orchestral/brass_stab.vibe:2](../../../crates/vibelang-std/stdlib/orchestral/brass_stab.vibe#L2) |
| `strings_legato` | `define_synthdef` | `stdlib/orchestral/strings_legato.vibe` | [orchestral/strings_legato.vibe:2](../../../crates/vibelang-std/stdlib/orchestral/strings_legato.vibe#L2) |
| `strings_staccato` | `define_synthdef` | `stdlib/orchestral/strings_staccato.vibe` | [orchestral/strings_staccato.vibe:2](../../../crates/vibelang-std/stdlib/orchestral/strings_staccato.vibe#L2) |
| `timpani` | `define_synthdef` | `stdlib/orchestral/timpani.vibe` | [orchestral/timpani.vibe:2](../../../crates/vibelang-std/stdlib/orchestral/timpani.vibe#L2) |
| `wind_chimes` | `define_synthdef` | `stdlib/orchestral/wind_chimes.vibe` | [orchestral/wind_chimes.vibe:2](../../../crates/vibelang-std/stdlib/orchestral/wind_chimes.vibe#L2) |

### pads

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `pad_aggressive` | `define_synthdef` | `stdlib/pads/lush/pad_aggressive.vibe` | [pads/lush/pad_aggressive.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_aggressive.vibe#L2) |
| `pad_arp` | `define_synthdef` | `stdlib/pads/movement/pad_arp.vibe` | [pads/movement/pad_arp.vibe:2](../../../crates/vibelang-std/stdlib/pads/movement/pad_arp.vibe#L2) |
| `pad_breath` | `define_synthdef` | `stdlib/pads/textural/pad_breath.vibe` | [pads/textural/pad_breath.vibe:2](../../../crates/vibelang-std/stdlib/pads/textural/pad_breath.vibe#L2) |
| `pad_bright` | `define_synthdef` | `stdlib/pads/ambient/pad_bright.vibe` | [pads/ambient/pad_bright.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_bright.vibe#L2) |
| `pad_choir` | `define_synthdef` | `stdlib/pads/textural/pad_choir.vibe` | [pads/textural/pad_choir.vibe:2](../../../crates/vibelang-std/stdlib/pads/textural/pad_choir.vibe#L2) |
| `pad_chorus` | `define_synthdef` | `stdlib/pads/lush/pad_chorus.vibe` | [pads/lush/pad_chorus.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_chorus.vibe#L2) |
| `pad_cold` | `define_synthdef` | `stdlib/pads/ambient/pad_cold.vibe` | [pads/ambient/pad_cold.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_cold.vibe#L2) |
| `pad_cs80` | `define_synthdef` | `stdlib/pads/analog/pad_cs80.vibe` | [pads/analog/pad_cs80.vibe:2](../../../crates/vibelang-std/stdlib/pads/analog/pad_cs80.vibe#L2) |
| `pad_dark` | `define_synthdef` | `stdlib/pads/ambient/pad_dark.vibe` | [pads/ambient/pad_dark.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_dark.vibe#L2) |
| `pad_dense` | `define_synthdef` | `stdlib/pads/ambient/pad_dense.vibe` | [pads/ambient/pad_dense.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_dense.vibe#L2) |
| `pad_detuned` | `define_synthdef` | `stdlib/pads/lush/pad_detuned.vibe` | [pads/lush/pad_detuned.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_detuned.vibe#L2) |
| `pad_epic` | `define_synthdef` | `stdlib/pads/cinematic/pad_epic.vibe` | [pads/cinematic/pad_epic.vibe:2](../../../crates/vibelang-std/stdlib/pads/cinematic/pad_epic.vibe#L2) |
| `pad_evolving` | `define_synthdef` | `stdlib/pads/ambient/pad_evolving.vibe` | [pads/ambient/pad_evolving.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_evolving.vibe#L2) |
| `pad_filtered` | `define_synthdef` | `stdlib/pads/ambient/pad_filtered.vibe` | [pads/ambient/pad_filtered.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_filtered.vibe#L2) |
| `pad_gate` | `define_synthdef` | `stdlib/pads/movement/pad_gate.vibe` | [pads/movement/pad_gate.vibe:2](../../../crates/vibelang-std/stdlib/pads/movement/pad_gate.vibe#L2) |
| `pad_ghostly` | `define_synthdef` | `stdlib/pads/textural/pad_ghostly.vibe` | [pads/textural/pad_ghostly.vibe:2](../../../crates/vibelang-std/stdlib/pads/textural/pad_ghostly.vibe#L2) |
| `pad_glass` | `define_synthdef` | `stdlib/pads/textural/pad_glass.vibe` | [pads/textural/pad_glass.vibe:2](../../../crates/vibelang-std/stdlib/pads/textural/pad_glass.vibe#L2) |
| `pad_horror` | `define_synthdef` | `stdlib/pads/cinematic/pad_horror.vibe` | [pads/cinematic/pad_horror.vibe:2](../../../crates/vibelang-std/stdlib/pads/cinematic/pad_horror.vibe#L2) |
| `pad_juno` | `define_synthdef` | `stdlib/pads/analog/pad_juno.vibe` | [pads/analog/pad_juno.vibe:2](../../../crates/vibelang-std/stdlib/pads/analog/pad_juno.vibe#L2) |
| `pad_jupiter` | `define_synthdef` | `stdlib/pads/analog/pad_jupiter.vibe` | [pads/analog/pad_jupiter.vibe:2](../../../crates/vibelang-std/stdlib/pads/analog/pad_jupiter.vibe#L2) |
| `pad_metallic` | `define_synthdef` | `stdlib/pads/textural/pad_metallic.vibe` | [pads/textural/pad_metallic.vibe:2](../../../crates/vibelang-std/stdlib/pads/textural/pad_metallic.vibe#L2) |
| `pad_morphing` | `define_synthdef` | `stdlib/pads/lush/pad_morphing.vibe` | [pads/lush/pad_morphing.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_morphing.vibe#L2) |
| `pad_mysterious` | `define_synthdef` | `stdlib/pads/cinematic/pad_mysterious.vibe` | [pads/cinematic/pad_mysterious.vibe:2](../../../crates/vibelang-std/stdlib/pads/cinematic/pad_mysterious.vibe#L2) |
| `pad_narrow` | `define_synthdef` | `stdlib/pads/lush/pad_narrow.vibe` | [pads/lush/pad_narrow.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_narrow.vibe#L2) |
| `pad_oberheim` | `define_synthdef` | `stdlib/pads/analog/pad_oberheim.vibe` | [pads/analog/pad_oberheim.vibe:2](../../../crates/vibelang-std/stdlib/pads/analog/pad_oberheim.vibe#L2) |
| `pad_organ` | `define_synthdef` | `stdlib/pads/lush/pad_organ.vibe` | [pads/lush/pad_organ.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_organ.vibe#L2) |
| `pad_prophet` | `define_synthdef` | `stdlib/pads/analog/pad_prophet.vibe` | [pads/analog/pad_prophet.vibe:2](../../../crates/vibelang-std/stdlib/pads/analog/pad_prophet.vibe#L2) |
| `pad_pulsing` | `define_synthdef` | `stdlib/pads/movement/pad_pulsing.vibe` | [pads/movement/pad_pulsing.vibe:2](../../../crates/vibelang-std/stdlib/pads/movement/pad_pulsing.vibe#L2) |
| `pad_random` | `define_synthdef` | `stdlib/pads/movement/pad_random.vibe` | [pads/movement/pad_random.vibe:2](../../../crates/vibelang-std/stdlib/pads/movement/pad_random.vibe#L2) |
| `pad_scifi` | `define_synthdef` | `stdlib/pads/cinematic/pad_scifi.vibe` | [pads/cinematic/pad_scifi.vibe:2](../../../crates/vibelang-std/stdlib/pads/cinematic/pad_scifi.vibe#L2) |
| `pad_shimmer` | `define_synthdef` | `stdlib/pads/ambient/pad_shimmer.vibe` | [pads/ambient/pad_shimmer.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_shimmer.vibe#L2) |
| `pad_soft` | `define_synthdef` | `stdlib/pads/lush/pad_soft.vibe` | [pads/lush/pad_soft.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_soft.vibe#L2) |
| `pad_space` | `define_synthdef` | `stdlib/pads/ambient/pad_space.vibe` | [pads/ambient/pad_space.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_space.vibe#L2) |
| `pad_sparse` | `define_synthdef` | `stdlib/pads/ambient/pad_sparse.vibe` | [pads/ambient/pad_sparse.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_sparse.vibe#L2) |
| `pad_string` | `define_synthdef` | `stdlib/pads/lush/pad_string.vibe` | [pads/lush/pad_string.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_string.vibe#L2) |
| `pad_swelling` | `define_synthdef` | `stdlib/pads/movement/pad_swelling.vibe` | [pads/movement/pad_swelling.vibe:2](../../../crates/vibelang-std/stdlib/pads/movement/pad_swelling.vibe#L2) |
| `pad_tension` | `define_synthdef` | `stdlib/pads/cinematic/pad_tension.vibe` | [pads/cinematic/pad_tension.vibe:2](../../../crates/vibelang-std/stdlib/pads/cinematic/pad_tension.vibe#L2) |
| `pad_underwater` | `define_synthdef` | `stdlib/pads/textural/pad_underwater.vibe` | [pads/textural/pad_underwater.vibe:2](../../../crates/vibelang-std/stdlib/pads/textural/pad_underwater.vibe#L2) |
| `pad_voice` | `define_synthdef` | `stdlib/pads/lush/pad_voice.vibe` | [pads/lush/pad_voice.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_voice.vibe#L2) |
| `pad_warm` | `define_synthdef` | `stdlib/pads/ambient/pad_warm.vibe` | [pads/ambient/pad_warm.vibe:2](../../../crates/vibelang-std/stdlib/pads/ambient/pad_warm.vibe#L2) |
| `pad_wide` | `define_synthdef` | `stdlib/pads/lush/pad_wide.vibe` | [pads/lush/pad_wide.vibe:2](../../../crates/vibelang-std/stdlib/pads/lush/pad_wide.vibe#L2) |

### processors

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `bark_filter` | `define_synthdef` | `stdlib/processors/filters/bark_filter.vibe` | [processors/filters/bark_filter.vibe:7](../../../crates/vibelang-std/stdlib/processors/filters/bark_filter.vibe#L7) |
| `crossfade_stereo` | `define_synthdef` | `stdlib/processors/mixers/crossfade_stereo.vibe` | [processors/mixers/crossfade_stereo.vibe:3](../../../crates/vibelang-std/stdlib/processors/mixers/crossfade_stereo.vibe#L3) |
| `dld` | `define_synthdef` | `stdlib/processors/delays/dld.vibe` | [processors/delays/dld.vibe:33](../../../crates/vibelang-std/stdlib/processors/delays/dld.vibe#L33) |
| `dxg` | `define_synthdef` | `stdlib/processors/dynamics/dxg.vibe` | [processors/dynamics/dxg.vibe:6](../../../crates/vibelang-std/stdlib/processors/dynamics/dxg.vibe#L6) |
| `erica_black_bbd` | `define_synthdef` | `stdlib/processors/delays/erica_black_bbd.vibe` | [processors/delays/erica_black_bbd.vibe:10](../../../crates/vibelang-std/stdlib/processors/delays/erica_black_bbd.vibe#L10) |
| `erica_black_hole_dsp` | `define_synthdef` | `stdlib/processors/fx/erica_black_hole_dsp.vibe` | [processors/fx/erica_black_hole_dsp.vibe:25](../../../crates/vibelang-std/stdlib/processors/fx/erica_black_hole_dsp.vibe#L25) |
| `erica_black_output` | `define_synthdef` | `stdlib/processors/mixers/erica_black_output.vibe` | [processors/mixers/erica_black_output.vibe:17](../../../crates/vibelang-std/stdlib/processors/mixers/erica_black_output.vibe#L17) |
| `erica_polivoks_vcf` | `define_synthdef` | `stdlib/processors/filters/erica_polivoks_vcf.vibe` | [processors/filters/erica_polivoks_vcf.vibe:9](../../../crates/vibelang-std/stdlib/processors/filters/erica_polivoks_vcf.vibe#L9) |
| `erica_quad_vca2` | `define_synthdef` | `stdlib/processors/mixers/erica_quad_vca2.vibe` | [processors/mixers/erica_quad_vca2.vibe:18](../../../crates/vibelang-std/stdlib/processors/mixers/erica_quad_vca2.vibe#L18) |
| `lowpass_mono` | `define_synthdef` | `stdlib/processors/filters/lowpass_mono.vibe` | [processors/filters/lowpass_mono.vibe:3](../../../crates/vibelang-std/stdlib/processors/filters/lowpass_mono.vibe#L3) |
| `lowpass_stereo` | `define_synthdef` | `stdlib/processors/filters/lowpass_stereo.vibe` | [processors/filters/lowpass_stereo.vibe:3](../../../crates/vibelang-std/stdlib/processors/filters/lowpass_stereo.vibe#L3) |
| `mimeophon` | `define_synthdef` | `stdlib/processors/delays/mimeophon.vibe` | [processors/delays/mimeophon.vibe:15](../../../crates/vibelang-std/stdlib/processors/delays/mimeophon.vibe#L15) |
| `mixer4_stereo` | `define_synthdef` | `stdlib/processors/mixers/mixer4_stereo.vibe` | [processors/mixers/mixer4_stereo.vibe:3](../../../crates/vibelang-std/stdlib/processors/mixers/mixer4_stereo.vibe#L3) |
| `passthrough_mono` | `define_synthdef` | `stdlib/processors/utility/passthrough_mono.vibe` | [processors/utility/passthrough_mono.vibe:3](../../../crates/vibelang-std/stdlib/processors/utility/passthrough_mono.vibe#L3) |
| `passthrough_stereo` | `define_synthdef` | `stdlib/processors/utility/passthrough_stereo.vibe` | [processors/utility/passthrough_stereo.vibe:3](../../../crates/vibelang-std/stdlib/processors/utility/passthrough_stereo.vibe#L3) |
| `qpas` | `define_synthdef` | `stdlib/processors/filters/qpas.vibe` | [processors/filters/qpas.vibe:11](../../../crates/vibelang-std/stdlib/processors/filters/qpas.vibe#L11) |
| `rainmaker` | `define_synthdef` | `stdlib/processors/delays/rainmaker.vibe` | [processors/delays/rainmaker.vibe:43](../../../crates/vibelang-std/stdlib/processors/delays/rainmaker.vibe#L43) |
| `ring_mod_mono` | `define_synthdef` | `stdlib/processors/modulation/ring_mod_mono.vibe` | [processors/modulation/ring_mod_mono.vibe:3](../../../crates/vibelang-std/stdlib/processors/modulation/ring_mod_mono.vibe#L3) |
| `ring_mod_stereo` | `define_synthdef` | `stdlib/processors/modulation/ring_mod_stereo.vibe` | [processors/modulation/ring_mod_stereo.vibe:3](../../../crates/vibelang-std/stdlib/processors/modulation/ring_mod_stereo.vibe#L3) |
| `smr8` | `define_synthdef` | `stdlib/processors/filters/smr8.vibe` | [processors/filters/smr8.vibe:102](../../../crates/vibelang-std/stdlib/processors/filters/smr8.vibe#L102) |
| `ufold` | `define_synthdef` | `stdlib/processors/distortion/ufold.vibe` | [processors/distortion/ufold.vibe:10](../../../crates/vibelang-std/stdlib/processors/distortion/ufold.vibe#L10) |
| `verbos_amp_tone` | `define_synthdef` | `stdlib/processors/utility/verbos_amp_tone.vibe` | [processors/utility/verbos_amp_tone.vibe:7](../../../crates/vibelang-std/stdlib/processors/utility/verbos_amp_tone.vibe#L7) |
| `verbos_mdp` | `define_synthdef` | `stdlib/processors/delays/verbos_mdp.vibe` | [processors/delays/verbos_mdp.vibe:9](../../../crates/vibelang-std/stdlib/processors/delays/verbos_mdp.vibe#L9) |
| `x_pan` | `define_synthdef` | `stdlib/processors/mixers/x_pan.vibe` | [processors/mixers/x_pan.vibe:6](../../../crates/vibelang-std/stdlib/processors/mixers/x_pan.vibe#L6) |

### retro

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `chip_noise` | `define_synthdef` | `stdlib/retro/chip_noise.vibe` | [retro/chip_noise.vibe:2](../../../crates/vibelang-std/stdlib/retro/chip_noise.vibe#L2) |
| `chip_pulse` | `define_synthdef` | `stdlib/retro/chip_pulse.vibe` | [retro/chip_pulse.vibe:2](../../../crates/vibelang-std/stdlib/retro/chip_pulse.vibe#L2) |
| `chip_triangle` | `define_synthdef` | `stdlib/retro/chip_triangle.vibe` | [retro/chip_triangle.vibe:2](../../../crates/vibelang-std/stdlib/retro/chip_triangle.vibe#L2) |
| `sfx_coin` | `define_synthdef` | `stdlib/retro/sfx_coin.vibe` | [retro/sfx_coin.vibe:2](../../../crates/vibelang-std/stdlib/retro/sfx_coin.vibe#L2) |
| `sfx_jump` | `define_synthdef` | `stdlib/retro/sfx_jump.vibe` | [retro/sfx_jump.vibe:2](../../../crates/vibelang-std/stdlib/retro/sfx_jump.vibe#L2) |
| `sfx_laser` | `define_synthdef` | `stdlib/retro/sfx_laser.vibe` | [retro/sfx_laser.vibe:2](../../../crates/vibelang-std/stdlib/retro/sfx_laser.vibe#L2) |
| `sfx_powerup` | `define_synthdef` | `stdlib/retro/sfx_powerup.vibe` | [retro/sfx_powerup.vibe:2](../../../crates/vibelang-std/stdlib/retro/sfx_powerup.vibe#L2) |
| `synth_80s_brass` | `define_synthdef` | `stdlib/retro/synth_80s_brass.vibe` | [retro/synth_80s_brass.vibe:2](../../../crates/vibelang-std/stdlib/retro/synth_80s_brass.vibe#L2) |

### strings

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `acid_bass` | `define_synthdef` | `stdlib/strings/bass_instruments.vibe` | [strings/bass_instruments.vibe:364](../../../crates/vibelang-std/stdlib/strings/bass_instruments.vibe#L364) |
| `acoustic_guitar` | `define_synthdef` | `stdlib/strings/acoustic_guitar.vibe` | [strings/acoustic_guitar.vibe:12](../../../crates/vibelang-std/stdlib/strings/acoustic_guitar.vibe#L12) |
| `classical_guitar` | `define_synthdef` | `stdlib/strings/acoustic_guitar.vibe` | [strings/acoustic_guitar.vibe:83](../../../crates/vibelang-std/stdlib/strings/acoustic_guitar.vibe#L83) |
| `electric_bass` | `define_synthdef` | `stdlib/strings/bass_instruments.vibe` | [strings/bass_instruments.vibe:112](../../../crates/vibelang-std/stdlib/strings/bass_instruments.vibe#L112) |
| `electric_guitar_clean` | `define_synthdef` | `stdlib/strings/electric_guitar.vibe` | [strings/electric_guitar.vibe:11](../../../crates/vibelang-std/stdlib/strings/electric_guitar.vibe#L11) |
| `electric_guitar_crunch` | `define_synthdef` | `stdlib/strings/electric_guitar.vibe` | [strings/electric_guitar.vibe:74](../../../crates/vibelang-std/stdlib/strings/electric_guitar.vibe#L74) |
| `electric_guitar_distorted` | `define_synthdef` | `stdlib/strings/electric_guitar.vibe` | [strings/electric_guitar.vibe:126](../../../crates/vibelang-std/stdlib/strings/electric_guitar.vibe#L126) |
| `electric_guitar_funk` | `define_synthdef` | `stdlib/strings/electric_guitar.vibe` | [strings/electric_guitar.vibe:242](../../../crates/vibelang-std/stdlib/strings/electric_guitar.vibe#L242) |
| `electric_guitar_jazz` | `define_synthdef` | `stdlib/strings/electric_guitar.vibe` | [strings/electric_guitar.vibe:188](../../../crates/vibelang-std/stdlib/strings/electric_guitar.vibe#L188) |
| `electric_guitar_wah` | `define_synthdef` | `stdlib/strings/electric_guitar.vibe` | [strings/electric_guitar.vibe:333](../../../crates/vibelang-std/stdlib/strings/electric_guitar.vibe#L333) |
| `fingerpick_guitar` | `define_synthdef` | `stdlib/strings/acoustic_guitar.vibe` | [strings/acoustic_guitar.vibe:128](../../../crates/vibelang-std/stdlib/strings/acoustic_guitar.vibe#L128) |
| `fretless_bass` | `define_synthdef` | `stdlib/strings/bass_instruments.vibe` | [strings/bass_instruments.vibe:222](../../../crates/vibelang-std/stdlib/strings/bass_instruments.vibe#L222) |
| `guitar_harmonic` | `define_synthdef` | `stdlib/strings/acoustic_guitar.vibe` | [strings/acoustic_guitar.vibe:230](../../../crates/vibelang-std/stdlib/strings/acoustic_guitar.vibe#L230) |
| `guitar_muted` | `define_synthdef` | `stdlib/strings/acoustic_guitar.vibe` | [strings/acoustic_guitar.vibe:263](../../../crates/vibelang-std/stdlib/strings/acoustic_guitar.vibe#L263) |
| `harp` | `define_synthdef` | `stdlib/strings/harp.vibe` | [strings/harp.vibe:6](../../../crates/vibelang-std/stdlib/strings/harp.vibe#L6) |
| `jazz_bass` | `define_synthdef` | `stdlib/strings/bass_instruments.vibe` | [strings/bass_instruments.vibe:167](../../../crates/vibelang-std/stdlib/strings/bass_instruments.vibe#L167) |
| `picked_bass` | `define_synthdef` | `stdlib/strings/bass_instruments.vibe` | [strings/bass_instruments.vibe:402](../../../crates/vibelang-std/stdlib/strings/bass_instruments.vibe#L402) |
| `pizzicato` | `define_synthdef` | `stdlib/strings/pizzicato.vibe` | [strings/pizzicato.vibe:6](../../../crates/vibelang-std/stdlib/strings/pizzicato.vibe#L6) |
| `power_chord` | `define_synthdef` | `stdlib/strings/electric_guitar.vibe` | [strings/electric_guitar.vibe:287](../../../crates/vibelang-std/stdlib/strings/electric_guitar.vibe#L287) |
| `slap_bass` | `define_synthdef` | `stdlib/strings/bass_instruments.vibe` | [strings/bass_instruments.vibe:273](../../../crates/vibelang-std/stdlib/strings/bass_instruments.vibe#L273) |
| `solo_cello` | `define_synthdef` | `stdlib/strings/solo_cello.vibe` | [strings/solo_cello.vibe:6](../../../crates/vibelang-std/stdlib/strings/solo_cello.vibe#L6) |
| `solo_violin` | `define_synthdef` | `stdlib/strings/solo_violin.vibe` | [strings/solo_violin.vibe:6](../../../crates/vibelang-std/stdlib/strings/solo_violin.vibe#L6) |
| `string_ensemble` | `define_synthdef` | `stdlib/strings/string_ensemble.vibe` | [strings/string_ensemble.vibe:7](../../../crates/vibelang-std/stdlib/strings/string_ensemble.vibe#L7) |
| `synth_bass_moog` | `define_synthdef` | `stdlib/strings/bass_instruments.vibe` | [strings/bass_instruments.vibe:324](../../../crates/vibelang-std/stdlib/strings/bass_instruments.vibe#L324) |
| `tremolo_strings` | `define_synthdef` | `stdlib/strings/tremolo_strings.vibe` | [strings/tremolo_strings.vibe:6](../../../crates/vibelang-std/stdlib/strings/tremolo_strings.vibe#L6) |
| `twelve_string_guitar` | `define_synthdef` | `stdlib/strings/acoustic_guitar.vibe` | [strings/acoustic_guitar.vibe:176](../../../crates/vibelang-std/stdlib/strings/acoustic_guitar.vibe#L176) |
| `upright_bass` | `define_synthdef` | `stdlib/strings/bass_instruments.vibe` | [strings/bass_instruments.vibe:12](../../../crates/vibelang-std/stdlib/strings/bass_instruments.vibe#L12) |
| `upright_bass_bowed` | `define_synthdef` | `stdlib/strings/bass_instruments.vibe` | [strings/bass_instruments.vibe:65](../../../crates/vibelang-std/stdlib/strings/bass_instruments.vibe#L65) |

### synths

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `arp_2600_lead` | `define_synthdef` | `stdlib/synths/arp.vibe` | [synths/arp.vibe:7](../../../crates/vibelang-std/stdlib/synths/arp.vibe#L7) |
| `arp_odyssey_bass` | `define_synthdef` | `stdlib/synths/arp.vibe` | [synths/arp.vibe:35](../../../crates/vibelang-std/stdlib/synths/arp.vibe#L35) |
| `arp_solina` | `define_synthdef` | `stdlib/synths/arp.vibe` | [synths/arp.vibe:95](../../../crates/vibelang-std/stdlib/synths/arp.vibe#L95) |
| `arp_strings` | `define_synthdef` | `stdlib/synths/arp.vibe` | [synths/arp.vibe:67](../../../crates/vibelang-std/stdlib/synths/arp.vibe#L67) |
| `cs80_brass` | `define_synthdef` | `stdlib/synths/cs80.vibe` | [synths/cs80.vibe:37](../../../crates/vibelang-std/stdlib/synths/cs80.vibe#L37) |
| `cs80_lead` | `define_synthdef` | `stdlib/synths/cs80.vibe` | [synths/cs80.vibe:5](../../../crates/vibelang-std/stdlib/synths/cs80.vibe#L5) |
| `cs80_strings` | `define_synthdef` | `stdlib/synths/cs80.vibe` | [synths/cs80.vibe:70](../../../crates/vibelang-std/stdlib/synths/cs80.vibe#L70) |
| `dx7_bass` | `define_synthdef` | `stdlib/synths/dx7.vibe` | [synths/dx7.vibe:45](../../../crates/vibelang-std/stdlib/synths/dx7.vibe#L45) |
| `dx7_bells` | `define_synthdef` | `stdlib/synths/dx7.vibe` | [synths/dx7.vibe:75](../../../crates/vibelang-std/stdlib/synths/dx7.vibe#L75) |
| `dx7_brass` | `define_synthdef` | `stdlib/synths/dx7.vibe` | [synths/dx7.vibe:109](../../../crates/vibelang-std/stdlib/synths/dx7.vibe#L109) |
| `dx7_epiano` | `define_synthdef` | `stdlib/synths/dx7.vibe` | [synths/dx7.vibe:8](../../../crates/vibelang-std/stdlib/synths/dx7.vibe#L8) |
| `dx7_marimba` | `define_synthdef` | `stdlib/synths/dx7.vibe` | [synths/dx7.vibe:142](../../../crates/vibelang-std/stdlib/synths/dx7.vibe#L142) |
| `juno_bass` | `define_synthdef` | `stdlib/synths/juno.vibe` | [synths/juno.vibe:77](../../../crates/vibelang-std/stdlib/synths/juno.vibe#L77) |
| `juno_brass` | `define_synthdef` | `stdlib/synths/juno.vibe` | [synths/juno.vibe:44](../../../crates/vibelang-std/stdlib/synths/juno.vibe#L44) |
| `juno_pad` | `define_synthdef` | `stdlib/synths/juno.vibe` | [synths/juno.vibe:8](../../../crates/vibelang-std/stdlib/synths/juno.vibe#L8) |
| `juno_strings` | `define_synthdef` | `stdlib/synths/juno.vibe` | [synths/juno.vibe:108](../../../crates/vibelang-std/stdlib/synths/juno.vibe#L108) |
| `jupiter_bass` | `define_synthdef` | `stdlib/synths/jupiter.vibe` | [synths/jupiter.vibe:106](../../../crates/vibelang-std/stdlib/synths/jupiter.vibe#L106) |
| `jupiter_brass` | `define_synthdef` | `stdlib/synths/jupiter.vibe` | [synths/jupiter.vibe:72](../../../crates/vibelang-std/stdlib/synths/jupiter.vibe#L72) |
| `jupiter_pad` | `define_synthdef` | `stdlib/synths/jupiter.vibe` | [synths/jupiter.vibe:8](../../../crates/vibelang-std/stdlib/synths/jupiter.vibe#L8) |
| `jupiter_supersaw` | `define_synthdef` | `stdlib/synths/jupiter.vibe` | [synths/jupiter.vibe:39](../../../crates/vibelang-std/stdlib/synths/jupiter.vibe#L39) |
| `memorymoog_brass` | `define_synthdef` | `stdlib/synths/memorymoog.vibe` | [synths/memorymoog.vibe:36](../../../crates/vibelang-std/stdlib/synths/memorymoog.vibe#L36) |
| `memorymoog_lead` | `define_synthdef` | `stdlib/synths/memorymoog.vibe` | [synths/memorymoog.vibe:68](../../../crates/vibelang-std/stdlib/synths/memorymoog.vibe#L68) |
| `memorymoog_pad` | `define_synthdef` | `stdlib/synths/memorymoog.vibe` | [synths/memorymoog.vibe:5](../../../crates/vibelang-std/stdlib/synths/memorymoog.vibe#L5) |
| `minimoog_bass` | `define_synthdef` | `stdlib/synths/minimoog.vibe` | [synths/minimoog.vibe:8](../../../crates/vibelang-std/stdlib/synths/minimoog.vibe#L8) |
| `minimoog_lead` | `define_synthdef` | `stdlib/synths/minimoog.vibe` | [synths/minimoog.vibe:44](../../../crates/vibelang-std/stdlib/synths/minimoog.vibe#L44) |
| `minimoog_pad` | `define_synthdef` | `stdlib/synths/minimoog.vibe` | [synths/minimoog.vibe:80](../../../crates/vibelang-std/stdlib/synths/minimoog.vibe#L80) |
| `ms20_bass` | `define_synthdef` | `stdlib/synths/ms20.vibe` | [synths/ms20.vibe:5](../../../crates/vibelang-std/stdlib/synths/ms20.vibe#L5) |
| `ms20_lead` | `define_synthdef` | `stdlib/synths/ms20.vibe` | [synths/ms20.vibe:40](../../../crates/vibelang-std/stdlib/synths/ms20.vibe#L40) |
| `ms20_sync` | `define_synthdef` | `stdlib/synths/ms20.vibe` | [synths/ms20.vibe:74](../../../crates/vibelang-std/stdlib/synths/ms20.vibe#L74) |
| `oberheim_bass` | `define_synthdef` | `stdlib/synths/oberheim.vibe` | [synths/oberheim.vibe:95](../../../crates/vibelang-std/stdlib/synths/oberheim.vibe#L95) |
| `oberheim_brass` | `define_synthdef` | `stdlib/synths/oberheim.vibe` | [synths/oberheim.vibe:36](../../../crates/vibelang-std/stdlib/synths/oberheim.vibe#L36) |
| `oberheim_lead` | `define_synthdef` | `stdlib/synths/oberheim.vibe` | [synths/oberheim.vibe:69](../../../crates/vibelang-std/stdlib/synths/oberheim.vibe#L69) |
| `oberheim_pad` | `define_synthdef` | `stdlib/synths/oberheim.vibe` | [synths/oberheim.vibe:8](../../../crates/vibelang-std/stdlib/synths/oberheim.vibe#L8) |
| `odyssey_bass` | `define_synthdef` | `stdlib/synths/odyssey.vibe` | [synths/odyssey.vibe:5](../../../crates/vibelang-std/stdlib/synths/odyssey.vibe#L5) |
| `odyssey_lead` | `define_synthdef` | `stdlib/synths/odyssey.vibe` | [synths/odyssey.vibe:37](../../../crates/vibelang-std/stdlib/synths/odyssey.vibe#L37) |
| `odyssey_sh` | `define_synthdef` | `stdlib/synths/odyssey.vibe` | [synths/odyssey.vibe:70](../../../crates/vibelang-std/stdlib/synths/odyssey.vibe#L70) |
| `polysix_bass` | `define_synthdef` | `stdlib/synths/polysix.vibe` | [synths/polysix.vibe:63](../../../crates/vibelang-std/stdlib/synths/polysix.vibe#L63) |
| `polysix_pad` | `define_synthdef` | `stdlib/synths/polysix.vibe` | [synths/polysix.vibe:5](../../../crates/vibelang-std/stdlib/synths/polysix.vibe#L5) |
| `polysix_strings` | `define_synthdef` | `stdlib/synths/polysix.vibe` | [synths/polysix.vibe:36](../../../crates/vibelang-std/stdlib/synths/polysix.vibe#L36) |
| `ppg_bass` | `define_synthdef` | `stdlib/synths/ppg_wave.vibe` | [synths/ppg_wave.vibe:63](../../../crates/vibelang-std/stdlib/synths/ppg_wave.vibe#L63) |
| `ppg_lead` | `define_synthdef` | `stdlib/synths/ppg_wave.vibe` | [synths/ppg_wave.vibe:37](../../../crates/vibelang-std/stdlib/synths/ppg_wave.vibe#L37) |
| `ppg_pad` | `define_synthdef` | `stdlib/synths/ppg_wave.vibe` | [synths/ppg_wave.vibe:5](../../../crates/vibelang-std/stdlib/synths/ppg_wave.vibe#L5) |
| `pro_one_bass` | `define_synthdef` | `stdlib/synths/pro_one.vibe` | [synths/pro_one.vibe:5](../../../crates/vibelang-std/stdlib/synths/pro_one.vibe#L5) |
| `pro_one_lead` | `define_synthdef` | `stdlib/synths/pro_one.vibe` | [synths/pro_one.vibe:38](../../../crates/vibelang-std/stdlib/synths/pro_one.vibe#L38) |
| `pro_one_sync` | `define_synthdef` | `stdlib/synths/pro_one.vibe` | [synths/pro_one.vibe:71](../../../crates/vibelang-std/stdlib/synths/pro_one.vibe#L71) |
| `prophet_brass` | `define_synthdef` | `stdlib/synths/prophet.vibe` | [synths/prophet.vibe:39](../../../crates/vibelang-std/stdlib/synths/prophet.vibe#L39) |
| `prophet_lead` | `define_synthdef` | `stdlib/synths/prophet.vibe` | [synths/prophet.vibe:72](../../../crates/vibelang-std/stdlib/synths/prophet.vibe#L72) |
| `prophet_pad` | `define_synthdef` | `stdlib/synths/prophet.vibe` | [synths/prophet.vibe:8](../../../crates/vibelang-std/stdlib/synths/prophet.vibe#L8) |
| `prophet_strings` | `define_synthdef` | `stdlib/synths/prophet.vibe` | [synths/prophet.vibe:99](../../../crates/vibelang-std/stdlib/synths/prophet.vibe#L99) |
| `sh101_acid` | `define_synthdef` | `stdlib/synths/sh101.vibe` | [synths/sh101.vibe:93](../../../crates/vibelang-std/stdlib/synths/sh101.vibe#L93) |
| `sh101_bass` | `define_synthdef` | `stdlib/synths/sh101.vibe` | [synths/sh101.vibe:8](../../../crates/vibelang-std/stdlib/synths/sh101.vibe#L8) |
| `sh101_lead` | `define_synthdef` | `stdlib/synths/sh101.vibe` | [synths/sh101.vibe:41](../../../crates/vibelang-std/stdlib/synths/sh101.vibe#L41) |
| `sh101_sync` | `define_synthdef` | `stdlib/synths/sh101.vibe` | [synths/sh101.vibe:65](../../../crates/vibelang-std/stdlib/synths/sh101.vibe#L65) |
| `tb303_accent` | `define_synthdef` | `stdlib/synths/tb303.vibe` | [synths/tb303.vibe:73](../../../crates/vibelang-std/stdlib/synths/tb303.vibe#L73) |
| `tb303_bass` | `define_synthdef` | `stdlib/synths/tb303.vibe` | [synths/tb303.vibe:5](../../../crates/vibelang-std/stdlib/synths/tb303.vibe#L5) |
| `tb303_slide` | `define_synthdef` | `stdlib/synths/tb303.vibe` | [synths/tb303.vibe:43](../../../crates/vibelang-std/stdlib/synths/tb303.vibe#L43) |

### textures

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `drone_dark` | `define_synthdef` | `stdlib/textures/drone/drone_dark.vibe` | [textures/drone/drone_dark.vibe:2](../../../crates/vibelang-std/stdlib/textures/drone/drone_dark.vibe#L2) |
| `drone_evolving` | `define_synthdef` | `stdlib/textures/drone/drone_evolving.vibe` | [textures/drone/drone_evolving.vibe:2](../../../crates/vibelang-std/stdlib/textures/drone/drone_evolving.vibe#L2) |
| `drone_harmonic` | `define_synthdef` | `stdlib/textures/drone/drone_harmonic.vibe` | [textures/drone/drone_harmonic.vibe:2](../../../crates/vibelang-std/stdlib/textures/drone/drone_harmonic.vibe#L2) |
| `drone_noise` | `define_synthdef` | `stdlib/textures/drone/drone_noise.vibe` | [textures/drone/drone_noise.vibe:2](../../../crates/vibelang-std/stdlib/textures/drone/drone_noise.vibe#L2) |
| `drone_resonant` | `define_synthdef` | `stdlib/textures/drone/drone_resonant.vibe` | [textures/drone/drone_resonant.vibe:2](../../../crates/vibelang-std/stdlib/textures/drone/drone_resonant.vibe#L2) |
| `texture_field` | `define_synthdef` | `stdlib/textures/ambient/texture_field.vibe` | [textures/ambient/texture_field.vibe:2](../../../crates/vibelang-std/stdlib/textures/ambient/texture_field.vibe#L2) |
| `texture_granular` | `define_synthdef` | `stdlib/textures/ambient/texture_granular.vibe` | [textures/ambient/texture_granular.vibe:2](../../../crates/vibelang-std/stdlib/textures/ambient/texture_granular.vibe#L2) |
| `texture_rain` | `define_synthdef` | `stdlib/textures/ambient/texture_rain.vibe` | [textures/ambient/texture_rain.vibe:2](../../../crates/vibelang-std/stdlib/textures/ambient/texture_rain.vibe#L2) |
| `texture_space` | `define_synthdef` | `stdlib/textures/ambient/texture_space.vibe` | [textures/ambient/texture_space.vibe:2](../../../crates/vibelang-std/stdlib/textures/ambient/texture_space.vibe#L2) |
| `texture_wind` | `define_synthdef` | `stdlib/textures/ambient/texture_wind.vibe` | [textures/ambient/texture_wind.vibe:2](../../../crates/vibelang-std/stdlib/textures/ambient/texture_wind.vibe#L2) |

### utility

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `a440` | `define_synthdef` | `stdlib/utility/test_tones.vibe` | [utility/test_tones.vibe:41](../../../crates/vibelang-std/stdlib/utility/test_tones.vibe#L41) |
| `brown_noise_gen` | `define_synthdef` | `stdlib/utility/noise_generators.vibe` | [utility/noise_generators.vibe:43](../../../crates/vibelang-std/stdlib/utility/noise_generators.vibe#L43) |
| `click_high` | `define_synthdef` | `stdlib/utility/click_track.vibe` | [utility/click_track.vibe:4](../../../crates/vibelang-std/stdlib/utility/click_track.vibe#L4) |
| `click_low` | `define_synthdef` | `stdlib/utility/click_track.vibe` | [utility/click_track.vibe:15](../../../crates/vibelang-std/stdlib/utility/click_track.vibe#L15) |
| `click_wood` | `define_synthdef` | `stdlib/utility/click_track.vibe` | [utility/click_track.vibe:26](../../../crates/vibelang-std/stdlib/utility/click_track.vibe#L26) |
| `crackle_gen` | `define_synthdef` | `stdlib/utility/noise_generators.vibe` | [utility/noise_generators.vibe:104](../../../crates/vibelang-std/stdlib/utility/noise_generators.vibe#L104) |
| `filtered_noise_gen` | `define_synthdef` | `stdlib/utility/noise_generators.vibe` | [utility/noise_generators.vibe:83](../../../crates/vibelang-std/stdlib/utility/noise_generators.vibe#L83) |
| `hiss_gen` | `define_synthdef` | `stdlib/utility/noise_generators.vibe` | [utility/noise_generators.vibe:63](../../../crates/vibelang-std/stdlib/utility/noise_generators.vibe#L63) |
| `lfo_random` **(duplicate name)** | `define_synthdef` | `stdlib/utility/lfo.vibe` | [utility/lfo.vibe:61](../../../crates/vibelang-std/stdlib/utility/lfo.vibe#L61) |
| `lfo_saw` **(duplicate name)** | `define_synthdef` | `stdlib/utility/lfo.vibe` | [utility/lfo.vibe:44](../../../crates/vibelang-std/stdlib/utility/lfo.vibe#L44) |
| `lfo_sine` **(duplicate name)** | `define_synthdef` | `stdlib/utility/lfo.vibe` | [utility/lfo.vibe:8](../../../crates/vibelang-std/stdlib/utility/lfo.vibe#L8) |
| `lfo_triangle` | `define_synthdef` | `stdlib/utility/lfo.vibe` | [utility/lfo.vibe:25](../../../crates/vibelang-std/stdlib/utility/lfo.vibe#L25) |
| `metronome` | `define_synthdef` | `stdlib/utility/metronome.vibe` | [utility/metronome.vibe:6](../../../crates/vibelang-std/stdlib/utility/metronome.vibe#L6) |
| `metronome_accent` | `define_synthdef` | `stdlib/utility/metronome.vibe` | [utility/metronome.vibe:29](../../../crates/vibelang-std/stdlib/utility/metronome.vibe#L29) |
| `noise_pink` | `define_synthdef` | `stdlib/utility/noise_pink.vibe` | [utility/noise_pink.vibe:2](../../../crates/vibelang-std/stdlib/utility/noise_pink.vibe#L2) |
| `noise_white` | `define_synthdef` | `stdlib/utility/noise_white.vibe` | [utility/noise_white.vibe:2](../../../crates/vibelang-std/stdlib/utility/noise_white.vibe#L2) |
| `pink_noise_gen` | `define_synthdef` | `stdlib/utility/noise_generators.vibe` | [utility/noise_generators.vibe:23](../../../crates/vibelang-std/stdlib/utility/noise_generators.vibe#L23) |
| `saw_test` | `define_synthdef` | `stdlib/utility/oscilloscope_tones.vibe` | [utility/oscilloscope_tones.vibe:38](../../../crates/vibelang-std/stdlib/utility/oscilloscope_tones.vibe#L38) |
| `silence` | `define_synthdef` | `stdlib/utility/silence.vibe` | [utility/silence.vibe:4](../../../crates/vibelang-std/stdlib/utility/silence.vibe#L4) |
| `square_test` | `define_synthdef` | `stdlib/utility/oscilloscope_tones.vibe` | [utility/oscilloscope_tones.vibe:23](../../../crates/vibelang-std/stdlib/utility/oscilloscope_tones.vibe#L23) |
| `sweep_tone` | `define_synthdef` | `stdlib/utility/oscilloscope_tones.vibe` | [utility/oscilloscope_tones.vibe:4](../../../crates/vibelang-std/stdlib/utility/oscilloscope_tones.vibe#L4) |
| `test_1k` | `define_synthdef` | `stdlib/utility/test_tones.vibe` | [utility/test_tones.vibe:57](../../../crates/vibelang-std/stdlib/utility/test_tones.vibe#L57) |
| `test_saw` | `define_synthdef` | `stdlib/utility/test_tones.vibe` | [utility/test_tones.vibe:24](../../../crates/vibelang-std/stdlib/utility/test_tones.vibe#L24) |
| `test_sine` | `define_synthdef` | `stdlib/utility/test_tones.vibe` | [utility/test_tones.vibe:7](../../../crates/vibelang-std/stdlib/utility/test_tones.vibe#L7) |
| `test_sweep` | `define_synthdef` | `stdlib/utility/test_tones.vibe` | [utility/test_tones.vibe:73](../../../crates/vibelang-std/stdlib/utility/test_tones.vibe#L73) |
| `tri_test` | `define_synthdef` | `stdlib/utility/oscilloscope_tones.vibe` | [utility/oscilloscope_tones.vibe:53](../../../crates/vibelang-std/stdlib/utility/oscilloscope_tones.vibe#L53) |
| `tuner_a432` | `define_synthdef` | `stdlib/utility/tuner.vibe` | [utility/tuner.vibe:18](../../../crates/vibelang-std/stdlib/utility/tuner.vibe#L18) |
| `tuner_a440` | `define_synthdef` | `stdlib/utility/tuner.vibe` | [utility/tuner.vibe:4](../../../crates/vibelang-std/stdlib/utility/tuner.vibe#L4) |
| `tuner_e_low` | `define_synthdef` | `stdlib/utility/tuner.vibe` | [utility/tuner.vibe:32](../../../crates/vibelang-std/stdlib/utility/tuner.vibe#L32) |
| `white_noise_gen` | `define_synthdef` | `stdlib/utility/noise_generators.vibe` | [utility/noise_generators.vibe:7](../../../crates/vibelang-std/stdlib/utility/noise_generators.vibe#L7) |

### vocals

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `choir_pad` | `define_synthdef` | `stdlib/vocals/choir_pad.vibe` | [vocals/choir_pad.vibe:7](../../../crates/vibelang-std/stdlib/vocals/choir_pad.vibe#L7) |
| `formant_synth` | `define_synthdef` | `stdlib/vocals/formant_synth.vibe` | [vocals/formant_synth.vibe:7](../../../crates/vibelang-std/stdlib/vocals/formant_synth.vibe#L7) |
| `vocal_aaah` | `define_synthdef` | `stdlib/vocals/vocal_aaah.vibe` | [vocals/vocal_aaah.vibe:2](../../../crates/vibelang-std/stdlib/vocals/vocal_aaah.vibe#L2) |
| `vocal_breathy` | `define_synthdef` | `stdlib/vocals/vocal_breathy.vibe` | [vocals/vocal_breathy.vibe:2](../../../crates/vibelang-std/stdlib/vocals/vocal_breathy.vibe#L2) |
| `vocal_chop` | `define_synthdef` | `stdlib/vocals/vocal_chop.vibe` | [vocals/vocal_chop.vibe:7](../../../crates/vibelang-std/stdlib/vocals/vocal_chop.vibe#L7) |
| `vocal_eee` | `define_synthdef` | `stdlib/vocals/vocal_eee.vibe` | [vocals/vocal_eee.vibe:2](../../../crates/vibelang-std/stdlib/vocals/vocal_eee.vibe#L2) |
| `vocal_falsetto` | `define_synthdef` | `stdlib/vocals/vocal_falsetto.vibe` | [vocals/vocal_falsetto.vibe:2](../../../crates/vibelang-std/stdlib/vocals/vocal_falsetto.vibe#L2) |
| `vocal_mmm` | `define_synthdef` | `stdlib/vocals/vocal_mmm.vibe` | [vocals/vocal_mmm.vibe:2](../../../crates/vibelang-std/stdlib/vocals/vocal_mmm.vibe#L2) |
| `vocal_oooh` | `define_synthdef` | `stdlib/vocals/vocal_oooh.vibe` | [vocals/vocal_oooh.vibe:2](../../../crates/vibelang-std/stdlib/vocals/vocal_oooh.vibe#L2) |
| `vocal_operatic` | `define_synthdef` | `stdlib/vocals/vocal_operatic.vibe` | [vocals/vocal_operatic.vibe:2](../../../crates/vibelang-std/stdlib/vocals/vocal_operatic.vibe#L2) |
| `vocal_robot` | `define_synthdef` | `stdlib/vocals/vocal_robot.vibe` | [vocals/vocal_robot.vibe:2](../../../crates/vibelang-std/stdlib/vocals/vocal_robot.vibe#L2) |
| `voice_drone` | `define_synthdef` | `stdlib/vocals/voice_drone.vibe` | [vocals/voice_drone.vibe:6](../../../crates/vibelang-std/stdlib/vocals/voice_drone.vibe#L6) |
| `whisper` | `define_synthdef` | `stdlib/vocals/whisper.vibe` | [vocals/whisper.vibe:6](../../../crates/vibelang-std/stdlib/vocals/whisper.vibe#L6) |

### woodwinds

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `alto_sax` | `define_synthdef` | `stdlib/woodwinds/saxophone.vibe` | [woodwinds/saxophone.vibe:7](../../../crates/vibelang-std/stdlib/woodwinds/saxophone.vibe#L7) |
| `alto_sax_realistic` | `define_synthdef` | `stdlib/woodwinds/woodwinds_realistic.vibe` | [woodwinds/woodwinds_realistic.vibe:81](../../../crates/vibelang-std/stdlib/woodwinds/woodwinds_realistic.vibe#L81) |
| `bari_sax` | `define_synthdef` | `stdlib/woodwinds/saxophone.vibe` | [woodwinds/saxophone.vibe:111](../../../crates/vibelang-std/stdlib/woodwinds/saxophone.vibe#L111) |
| `bass_clarinet` | `define_synthdef` | `stdlib/woodwinds/clarinet.vibe` | [woodwinds/clarinet.vibe:48](../../../crates/vibelang-std/stdlib/woodwinds/clarinet.vibe#L48) |
| `bassoon` | `define_synthdef` | `stdlib/woodwinds/bassoon.vibe` | [woodwinds/bassoon.vibe:6](../../../crates/vibelang-std/stdlib/woodwinds/bassoon.vibe#L6) |
| `bassoon_realistic` | `define_synthdef` | `stdlib/woodwinds/woodwinds_realistic.vibe` | [woodwinds/woodwinds_realistic.vibe:350](../../../crates/vibelang-std/stdlib/woodwinds/woodwinds_realistic.vibe#L350) |
| `clarinet` | `define_synthdef` | `stdlib/woodwinds/clarinet.vibe` | [woodwinds/clarinet.vibe:7](../../../crates/vibelang-std/stdlib/woodwinds/clarinet.vibe#L7) |
| `clarinet_realistic` | `define_synthdef` | `stdlib/woodwinds/woodwinds_realistic.vibe` | [woodwinds/woodwinds_realistic.vibe:12](../../../crates/vibelang-std/stdlib/woodwinds/woodwinds_realistic.vibe#L12) |
| `contrabassoon` | `define_synthdef` | `stdlib/woodwinds/bassoon.vibe` | [woodwinds/bassoon.vibe:44](../../../crates/vibelang-std/stdlib/woodwinds/bassoon.vibe#L44) |
| `english_horn` | `define_synthdef` | `stdlib/woodwinds/woodwinds_realistic.vibe` | [woodwinds/woodwinds_realistic.vibe:311](../../../crates/vibelang-std/stdlib/woodwinds/woodwinds_realistic.vibe#L311) |
| `flute` | `define_synthdef` | `stdlib/woodwinds/flute.vibe` | [woodwinds/flute.vibe:7](../../../crates/vibelang-std/stdlib/woodwinds/flute.vibe#L7) |
| `flute_realistic` | `define_synthdef` | `stdlib/woodwinds/woodwinds_realistic.vibe` | [woodwinds/woodwinds_realistic.vibe:203](../../../crates/vibelang-std/stdlib/woodwinds/woodwinds_realistic.vibe#L203) |
| `oboe` | `define_synthdef` | `stdlib/woodwinds/oboe.vibe` | [woodwinds/oboe.vibe:7](../../../crates/vibelang-std/stdlib/woodwinds/oboe.vibe#L7) |
| `oboe_realistic` | `define_synthdef` | `stdlib/woodwinds/woodwinds_realistic.vibe` | [woodwinds/woodwinds_realistic.vibe:260](../../../crates/vibelang-std/stdlib/woodwinds/woodwinds_realistic.vibe#L260) |
| `pan_flute` | `define_synthdef` | `stdlib/woodwinds/woodwinds_realistic.vibe` | [woodwinds/woodwinds_realistic.vibe:437](../../../crates/vibelang-std/stdlib/woodwinds/woodwinds_realistic.vibe#L437) |
| `piccolo` | `define_synthdef` | `stdlib/woodwinds/flute.vibe` | [woodwinds/flute.vibe:45](../../../crates/vibelang-std/stdlib/woodwinds/flute.vibe#L45) |
| `recorder` | `define_synthdef` | `stdlib/woodwinds/woodwinds_realistic.vibe` | [woodwinds/woodwinds_realistic.vibe:401](../../../crates/vibelang-std/stdlib/woodwinds/woodwinds_realistic.vibe#L401) |
| `soprano_sax` | `define_synthdef` | `stdlib/woodwinds/saxophone.vibe` | [woodwinds/saxophone.vibe:79](../../../crates/vibelang-std/stdlib/woodwinds/saxophone.vibe#L79) |
| `tenor_sax` | `define_synthdef` | `stdlib/woodwinds/saxophone.vibe` | [woodwinds/saxophone.vibe:46](../../../crates/vibelang-std/stdlib/woodwinds/saxophone.vibe#L46) |
| `tenor_sax_realistic` | `define_synthdef` | `stdlib/woodwinds/woodwinds_realistic.vibe` | [woodwinds/woodwinds_realistic.vibe:147](../../../crates/vibelang-std/stdlib/woodwinds/woodwinds_realistic.vibe#L147) |

### world

| Public name | Kind | Import module | Exact source |
|---|---|---|---|
| `accordion` | `define_synthdef` | `stdlib/world/accordion.vibe` | [world/accordion.vibe:2](../../../crates/vibelang-std/stdlib/world/accordion.vibe#L2) |
| `bagpipe` | `define_synthdef` | `stdlib/world/bagpipe.vibe` | [world/bagpipe.vibe:2](../../../crates/vibelang-std/stdlib/world/bagpipe.vibe#L2) |
| `balalaika` | `define_synthdef` | `stdlib/world/balalaika.vibe` | [world/balalaika.vibe:2](../../../crates/vibelang-std/stdlib/world/balalaika.vibe#L2) |
| `banjo` | `define_synthdef` | `stdlib/world/banjo.vibe` | [world/banjo.vibe:2](../../../crates/vibelang-std/stdlib/world/banjo.vibe#L2) |
| `bouzouki` | `define_synthdef` | `stdlib/world/bouzouki.vibe` | [world/bouzouki.vibe:2](../../../crates/vibelang-std/stdlib/world/bouzouki.vibe#L2) |
| `darbuka` | `define_synthdef` | `stdlib/world/darbuka.vibe` | [world/darbuka.vibe:2](../../../crates/vibelang-std/stdlib/world/darbuka.vibe#L2) |
| `didgeridoo` | `define_synthdef` | `stdlib/world/didgeridoo.vibe` | [world/didgeridoo.vibe:2](../../../crates/vibelang-std/stdlib/world/didgeridoo.vibe#L2) |
| `djembe_bass` | `define_synthdef` | `stdlib/world/djembe.vibe` | [world/djembe.vibe:8](../../../crates/vibelang-std/stdlib/world/djembe.vibe#L8) |
| `djembe_slap` | `define_synthdef` | `stdlib/world/djembe.vibe` | [world/djembe.vibe:63](../../../crates/vibelang-std/stdlib/world/djembe.vibe#L63) |
| `djembe_tone` | `define_synthdef` | `stdlib/world/djembe.vibe` | [world/djembe.vibe:35](../../../crates/vibelang-std/stdlib/world/djembe.vibe#L35) |
| `erhu` | `define_synthdef` | `stdlib/world/erhu.vibe` | [world/erhu.vibe:2](../../../crates/vibelang-std/stdlib/world/erhu.vibe#L2) |
| `gamelan_gong` | `define_synthdef` | `stdlib/world/gamelan.vibe` | [world/gamelan.vibe:7](../../../crates/vibelang-std/stdlib/world/gamelan.vibe#L7) |
| `gamelan_metal` | `define_synthdef` | `stdlib/world/gamelan.vibe` | [world/gamelan.vibe:44](../../../crates/vibelang-std/stdlib/world/gamelan.vibe#L44) |
| `hang_drum` | `define_synthdef` | `stdlib/world/hang_drum.vibe` | [world/hang_drum.vibe:2](../../../crates/vibelang-std/stdlib/world/hang_drum.vibe#L2) |
| `kalimba` | `define_synthdef` | `stdlib/world/kalimba.vibe` | [world/kalimba.vibe:7](../../../crates/vibelang-std/stdlib/world/kalimba.vibe#L7) |
| `kora` | `define_synthdef` | `stdlib/world/kora.vibe` | [world/kora.vibe:2](../../../crates/vibelang-std/stdlib/world/kora.vibe#L2) |
| `koto` | `define_synthdef` | `stdlib/world/koto.vibe` | [world/koto.vibe:7](../../../crates/vibelang-std/stdlib/world/koto.vibe#L7) |
| `mbira` | `define_synthdef` | `stdlib/world/mbira.vibe` | [world/mbira.vibe:2](../../../crates/vibelang-std/stdlib/world/mbira.vibe#L2) |
| `ney` | `define_synthdef` | `stdlib/world/ney.vibe` | [world/ney.vibe:2](../../../crates/vibelang-std/stdlib/world/ney.vibe#L2) |
| `oud` | `define_synthdef` | `stdlib/world/oud.vibe` | [world/oud.vibe:2](../../../crates/vibelang-std/stdlib/world/oud.vibe#L2) |
| `santoor` | `define_synthdef` | `stdlib/world/santoor.vibe` | [world/santoor.vibe:2](../../../crates/vibelang-std/stdlib/world/santoor.vibe#L2) |
| `shakuhachi` | `define_synthdef` | `stdlib/world/shakuhachi.vibe` | [world/shakuhachi.vibe:7](../../../crates/vibelang-std/stdlib/world/shakuhachi.vibe#L7) |
| `sitar` | `define_synthdef` | `stdlib/world/sitar.vibe` | [world/sitar.vibe:7](../../../crates/vibelang-std/stdlib/world/sitar.vibe#L7) |
| `steel_drum` | `define_synthdef` | `stdlib/world/steel_drum.vibe` | [world/steel_drum.vibe:7](../../../crates/vibelang-std/stdlib/world/steel_drum.vibe#L7) |
| `tabla_bayan` | `define_synthdef` | `stdlib/world/tabla.vibe` | [world/tabla.vibe:8](../../../crates/vibelang-std/stdlib/world/tabla.vibe#L8) |
| `tabla_dayan` | `define_synthdef` | `stdlib/world/tabla.vibe` | [world/tabla.vibe:42](../../../crates/vibelang-std/stdlib/world/tabla.vibe#L42) |
| `tin_whistle` | `define_synthdef` | `stdlib/world/tin_whistle.vibe` | [world/tin_whistle.vibe:2](../../../crates/vibelang-std/stdlib/world/tin_whistle.vibe#L2) |
| `ukulele` | `define_synthdef` | `stdlib/world/ukulele.vibe` | [world/ukulele.vibe:2](../../../crates/vibelang-std/stdlib/world/ukulele.vibe#L2) |

## Imported public functions

### `stdlib/instruments/sampler/morphagene.vibe`

| Exact signature | Source |
|---|---|
| `reel(name, frames, channels)` | [instruments/sampler/morphagene.vibe:547](../../../crates/vibelang-std/stdlib/instruments/sampler/morphagene.vibe#L547) |
| `reel_attach(voice, reel)` | [instruments/sampler/morphagene.vibe:566](../../../crates/vibelang-std/stdlib/instruments/sampler/morphagene.vibe#L566) |
| `reel_fill_preset(reel, preset)` | [instruments/sampler/morphagene.vibe:595](../../../crates/vibelang-std/stdlib/instruments/sampler/morphagene.vibe#L595) |

### `stdlib/theory/arpeggios.vibe`

| Exact signature | Source |
|---|---|
| `alberti_bass(chord)` | [theory/arpeggios.vibe:198](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L198) |
| `apply_to_progression(progression)` | [theory/arpeggios.vibe:419](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L419) |
| `apply_to_progression_ex(progression, pattern)` | [theory/arpeggios.vibe:422](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L422) |
| `arpeggio_16th(chord)` | [theory/arpeggios.vibe:271](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L271) |
| `arpeggio_16th_ex(chord, pattern)` | [theory/arpeggios.vibe:274](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L274) |
| `arpeggio_8th(chord)` | [theory/arpeggios.vibe:258](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L258) |
| `arpeggio_8th_ex(chord, pattern)` | [theory/arpeggios.vibe:261](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L261) |
| `arpeggio_accented(chord)` | [theory/arpeggios.vibe:133](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L133) |
| `arpeggio_accented_ex(chord, pattern)` | [theory/arpeggios.vibe:136](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L136) |
| `arpeggio_down(chord)` | [theory/arpeggios.vibe:19](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L19) |
| `arpeggio_down_extended(chord)` | [theory/arpeggios.vibe:86](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L86) |
| `arpeggio_down_extended_ex(chord, octaves)` | [theory/arpeggios.vibe:89](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L89) |
| `arpeggio_down_up(chord)` | [theory/arpeggios.vibe:47](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L47) |
| `arpeggio_pattern(chord, index_pattern)` | [theory/arpeggios.vibe:216](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L216) |
| `arpeggio_repeat(chord)` | [theory/arpeggios.vibe:229](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L229) |
| `arpeggio_repeat_ex(chord, pattern, repeats)` | [theory/arpeggios.vibe:232](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L232) |
| `arpeggio_triplet(chord)` | [theory/arpeggios.vibe:296](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L296) |
| `arpeggio_triplet_ex(chord, cycles)` | [theory/arpeggios.vibe:299](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L299) |
| `arpeggio_up(chord)` | [theory/arpeggios.vibe:14](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L14) |
| `arpeggio_up_down(chord)` **(duplicate public name)** | [theory/arpeggios.vibe:30](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L30) |
| `arpeggio_up_extended(chord)` | [theory/arpeggios.vibe:68](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L68) |
| `arpeggio_up_extended_ex(chord, octaves)` | [theory/arpeggios.vibe:71](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L71) |
| `arpeggio_with_rests(chord)` | [theory/arpeggios.vibe:111](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L111) |
| `arpeggio_with_rests_ex(chord, pattern)` | [theory/arpeggios.vibe:114](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L114) |
| `broken_1_3_2_4(chord)` | [theory/arpeggios.vibe:162](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L162) |
| `broken_1_5_3_5(chord)` | [theory/arpeggios.vibe:175](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L175) |
| `cascading_arpeggio(chord)` | [theory/arpeggios.vibe:353](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L353) |
| `cascading_arpeggio_ex(chord, cascades)` | [theory/arpeggios.vibe:356](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L356) |
| `invert_arpeggio(arpeggio)` | [theory/arpeggios.vibe:378](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L378) |
| `rolling_arpeggio(chord)` | [theory/arpeggios.vibe:319](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L319) |
| `transpose_arpeggio(arpeggio, semitones)` | [theory/arpeggios.vibe:399](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L399) |
| `tremolo_arpeggio(chord)` | [theory/arpeggios.vibe:335](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L335) |
| `tremolo_arpeggio_ex(chord, note1_idx, note2_idx, repeats)` | [theory/arpeggios.vibe:338](../../../crates/vibelang-std/stdlib/theory/arpeggios.vibe#L338) |

### `stdlib/theory/bass_patterns.vibe`

| Exact signature | Source |
|---|---|
| `apply_degree_pattern(progression, degree_pattern)` | [theory/bass_patterns.vibe:575](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L575) |
| `arpeggio_ascending(progression)` | [theory/bass_patterns.vibe:150](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L150) |
| `arpeggio_ascending_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:153](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L153) |
| `arpeggio_descending(progression)` | [theory/bass_patterns.vibe:173](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L173) |
| `arpeggio_descending_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:176](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L176) |
| `arpeggio_up_down(progression)` **(duplicate public name)** | [theory/bass_patterns.vibe:197](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L197) |
| `arpeggio_up_down_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:200](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L200) |
| `bass_pattern(name, progression)` | [theory/bass_patterns.vibe:489](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L489) |
| `bass_pattern_ex(name, progression, beats_per_chord)` | [theory/bass_patterns.vibe:493](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L493) |
| `custom_degree_pattern(chord, degree_pattern)` | [theory/bass_patterns.vibe:558](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L558) |
| `disco_bass(progression)` | [theory/bass_patterns.vibe:459](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L459) |
| `disco_bass_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:462](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L462) |
| `dnb_bass(progression)` | [theory/bass_patterns.vibe:429](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L429) |
| `dnb_bass_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:432](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L432) |
| `funk_bass(progression)` | [theory/bass_patterns.vibe:369](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L369) |
| `funk_bass_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:372](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L372) |
| `house_bass(progression)` | [theory/bass_patterns.vibe:330](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L330) |
| `house_bass_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:333](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L333) |
| `pedal_point(root_note)` | [theory/bass_patterns.vibe:523](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L523) |
| `pedal_point_ex(root_note, duration)` | [theory/bass_patterns.vibe:526](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L526) |
| `pedal_rhythmic(root_note, pattern)` | [theory/bass_patterns.vibe:538](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L538) |
| `reggae_bass(progression)` | [theory/bass_patterns.vibe:404](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L404) |
| `reggae_bass_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:407](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L407) |
| `root_fifth_octave(progression)` | [theory/bass_patterns.vibe:116](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L116) |
| `root_fifth_octave_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:119](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L119) |
| `root_fifth_pattern(progression)` | [theory/bass_patterns.vibe:90](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L90) |
| `root_fifth_pattern_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:93](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L93) |
| `root_octave_pattern(progression)` | [theory/bass_patterns.vibe:57](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L57) |
| `root_octave_pattern_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:60](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L60) |
| `root_on_beats(progression)` | [theory/bass_patterns.vibe:37](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L37) |
| `root_on_beats_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:40](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L40) |
| `root_pattern(progression)` | [theory/bass_patterns.vibe:14](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L14) |
| `root_pattern_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:17](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L17) |
| `techno_bass(progression)` | [theory/bass_patterns.vibe:339](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L339) |
| `techno_bass_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:342](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L342) |
| `walking_bass_bebop(progression)` | [theory/bass_patterns.vibe:273](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L273) |
| `walking_bass_bebop_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:276](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L276) |
| `walking_bass_simple(progression)` | [theory/bass_patterns.vibe:232](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L232) |
| `walking_bass_simple_ex(progression, beats_per_chord)` | [theory/bass_patterns.vibe:235](../../../crates/vibelang-std/stdlib/theory/bass_patterns.vibe#L235) |

### `stdlib/theory/chords.vibe`

| Exact signature | Source |
|---|---|
| `add_extension(notes, semitones_from_root)` | [theory/chords.vibe:565](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L565) |
| `augmented_major7_chord(root)` | [theory/chords.vibe:196](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L196) |
| `augmented_major7_chord_ex(root, octave)` | [theory/chords.vibe:200](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L200) |
| `augmented_triad(root)` | [theory/chords.vibe:104](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L104) |
| `augmented_triad_ex(root, octave)` | [theory/chords.vibe:108](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L108) |
| `chord(root, quality)` | [theory/chords.vibe:320](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L320) |
| `chord_ex(root, quality, octave)` | [theory/chords.vibe:324](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L324) |
| `chord_inversion(root, quality)` | [theory/chords.vibe:429](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L429) |
| `chord_inversion_ex(root, quality, inversion, octave)` | [theory/chords.vibe:432](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L432) |
| `chord_root(notes)` | [theory/chords.vibe:538](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L538) |
| `chord_tones(notes)` | [theory/chords.vibe:546](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L546) |
| `close_voicing(notes)` | [theory/chords.vibe:443](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L443) |
| `diminished7_chord(root)` | [theory/chords.vibe:180](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L180) |
| `diminished7_chord_ex(root, octave)` | [theory/chords.vibe:184](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L184) |
| `diminished_triad(root)` | [theory/chords.vibe:96](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L96) |
| `diminished_triad_ex(root, octave)` | [theory/chords.vibe:100](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L100) |
| `dom7b5_chord(root)` | [theory/chords.vibe:300](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L300) |
| `dom7b5_chord_ex(root, octave)` | [theory/chords.vibe:304](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L304) |
| `dom7b9_chord(root)` | [theory/chords.vibe:284](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L284) |
| `dom7b9_chord_ex(root, octave)` | [theory/chords.vibe:288](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L288) |
| `dom7sharp5_chord(root)` | [theory/chords.vibe:308](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L308) |
| `dom7sharp5_chord_ex(root, octave)` | [theory/chords.vibe:312](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L312) |
| `dom7sharp9_chord(root)` | [theory/chords.vibe:292](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L292) |
| `dom7sharp9_chord_ex(root, octave)` | [theory/chords.vibe:296](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L296) |
| `dominant11_chord(root)` | [theory/chords.vibe:248](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L248) |
| `dominant11_chord_ex(root, octave)` | [theory/chords.vibe:252](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L252) |
| `dominant13_chord(root)` | [theory/chords.vibe:272](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L272) |
| `dominant13_chord_ex(root, octave)` | [theory/chords.vibe:276](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L276) |
| `dominant7_chord(root)` | [theory/chords.vibe:164](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L164) |
| `dominant7_chord_ex(root, octave)` | [theory/chords.vibe:168](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L168) |
| `dominant9_chord(root)` | [theory/chords.vibe:224](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L224) |
| `dominant9_chord_ex(root, octave)` | [theory/chords.vibe:228](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L228) |
| `drop2_voicing(notes)` | [theory/chords.vibe:469](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L469) |
| `drop3_voicing(notes)` | [theory/chords.vibe:491](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L491) |
| `half_diminished7_chord(root)` | [theory/chords.vibe:172](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L172) |
| `half_diminished7_chord_ex(root, octave)` | [theory/chords.vibe:176](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L176) |
| `invert_chord(notes)` | [theory/chords.vibe:406](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L406) |
| `invert_chord_ex(notes, inversion)` | [theory/chords.vibe:410](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L410) |
| `major11_chord(root)` | [theory/chords.vibe:232](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L232) |
| `major11_chord_ex(root, octave)` | [theory/chords.vibe:236](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L236) |
| `major13_chord(root)` | [theory/chords.vibe:256](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L256) |
| `major13_chord_ex(root, octave)` | [theory/chords.vibe:260](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L260) |
| `major7_chord(root)` | [theory/chords.vibe:148](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L148) |
| `major7_chord_ex(root, octave)` | [theory/chords.vibe:152](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L152) |
| `major9_chord(root)` | [theory/chords.vibe:208](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L208) |
| `major9_chord_ex(root, octave)` | [theory/chords.vibe:212](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L212) |
| `major_triad(root)` | [theory/chords.vibe:80](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L80) |
| `major_triad_ex(root, octave)` | [theory/chords.vibe:84](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L84) |
| `minor11_chord(root)` | [theory/chords.vibe:240](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L240) |
| `minor11_chord_ex(root, octave)` | [theory/chords.vibe:244](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L244) |
| `minor13_chord(root)` | [theory/chords.vibe:264](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L264) |
| `minor13_chord_ex(root, octave)` | [theory/chords.vibe:268](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L268) |
| `minor7_chord(root)` | [theory/chords.vibe:156](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L156) |
| `minor7_chord_ex(root, octave)` | [theory/chords.vibe:160](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L160) |
| `minor9_chord(root)` | [theory/chords.vibe:216](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L216) |
| `minor9_chord_ex(root, octave)` | [theory/chords.vibe:220](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L220) |
| `minor_major7_chord(root)` | [theory/chords.vibe:188](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L188) |
| `minor_major7_chord_ex(root, octave)` | [theory/chords.vibe:192](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L192) |
| `minor_triad(root)` | [theory/chords.vibe:88](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L88) |
| `minor_triad_ex(root, octave)` | [theory/chords.vibe:92](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L92) |
| `omit_note(notes, semitones_from_root)` | [theory/chords.vibe:576](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L576) |
| `open_voicing(notes)` | [theory/chords.vibe:448](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L448) |
| `power_chord(root)` | [theory/chords.vibe:128](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L128) |
| `power_chord_ex(root, octave)` | [theory/chords.vibe:132](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L132) |
| `power_chord_octave(root)` | [theory/chords.vibe:136](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L136) |
| `power_chord_octave_ex(root, octave)` | [theory/chords.vibe:140](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L140) |
| `sus2_chord(root)` | [theory/chords.vibe:112](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L112) |
| `sus2_chord_ex(root, octave)` | [theory/chords.vibe:116](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L116) |
| `sus4_chord(root)` | [theory/chords.vibe:120](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L120) |
| `sus4_chord_ex(root, octave)` | [theory/chords.vibe:124](../../../crates/vibelang-std/stdlib/theory/chords.vibe#L124) |

### `stdlib/theory/core.vibe`

| Exact signature | Source |
|---|---|
| `array_to_step_string(arr)` | [theory/core.vibe:227](../../../crates/vibelang-std/stdlib/theory/core.vibe#L227) |
| `enharmonic(note_name)` | [theory/core.vibe:298](../../../crates/vibelang-std/stdlib/theory/core.vibe#L298) |
| `interval_name(semitones)` | [theory/core.vibe:243](../../../crates/vibelang-std/stdlib/theory/core.vibe#L243) |
| `interval_semitones(note1, note2)` | [theory/core.vibe:137](../../../crates/vibelang-std/stdlib/theory/core.vibe#L137) |
| `interval_to_semitones(name)` | [theory/core.vibe:269](../../../crates/vibelang-std/stdlib/theory/core.vibe#L269) |
| `midi_to_freq(midi_num)` | [theory/core.vibe:104](../../../crates/vibelang-std/stdlib/theory/core.vibe#L104) |
| `midi_to_freq_ex(midi_num, a4_freq)` | [theory/core.vibe:107](../../../crates/vibelang-std/stdlib/theory/core.vibe#L107) |
| `midi_to_note(midi_num)` | [theory/core.vibe:74](../../../crates/vibelang-std/stdlib/theory/core.vibe#L74) |
| `midi_to_note_ex(midi_num, use_flats)` | [theory/core.vibe:77](../../../crates/vibelang-std/stdlib/theory/core.vibe#L77) |
| `note_at_interval(root_note, semitones)` | [theory/core.vibe:144](../../../crates/vibelang-std/stdlib/theory/core.vibe#L144) |
| `note_to_freq(note_name)` | [theory/core.vibe:113](../../../crates/vibelang-std/stdlib/theory/core.vibe#L113) |
| `note_to_freq_ex(note_name, a4_freq)` | [theory/core.vibe:116](../../../crates/vibelang-std/stdlib/theory/core.vibe#L116) |
| `note_to_midi(note_name)` | [theory/core.vibe:13](../../../crates/vibelang-std/stdlib/theory/core.vibe#L13) |
| `note_to_pitch_class(note)` | [theory/core.vibe:323](../../../crates/vibelang-std/stdlib/theory/core.vibe#L323) |
| `notes_to_freq(notes)` | [theory/core.vibe:210](../../../crates/vibelang-std/stdlib/theory/core.vibe#L210) |
| `notes_to_freq_ex(notes, a4_freq)` | [theory/core.vibe:213](../../../crates/vibelang-std/stdlib/theory/core.vibe#L213) |
| `notes_to_midi(notes)` | [theory/core.vibe:197](../../../crates/vibelang-std/stdlib/theory/core.vibe#L197) |
| `parse_int(str)` | [theory/core.vibe:307](../../../crates/vibelang-std/stdlib/theory/core.vibe#L307) |
| `scale_mask_from_notes(notes)` | [theory/core.vibe:356](../../../crates/vibelang-std/stdlib/theory/core.vibe#L356) |
| `set_octave(note_name, new_octave)` | [theory/core.vibe:153](../../../crates/vibelang-std/stdlib/theory/core.vibe#L153) |
| `shift_octave(note_name, octave_shift)` | [theory/core.vibe:170](../../../crates/vibelang-std/stdlib/theory/core.vibe#L170) |
| `transpose_note(note_name, semitones)` | [theory/core.vibe:127](../../../crates/vibelang-std/stdlib/theory/core.vibe#L127) |
| `transpose_notes(notes, semitones)` | [theory/core.vibe:184](../../../crates/vibelang-std/stdlib/theory/core.vibe#L184) |

### `stdlib/theory/counterpoint.vibe`

| Exact signature | Source |
|---|---|
| `analyze_counterpoint_motion(voice1_note1, voice1_note2, voice2_note1, voice2_note2)` | [theory/counterpoint.vibe:168](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L168) |
| `check_hidden_perfects(voice1, voice2)` | [theory/counterpoint.vibe:208](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L208) |
| `check_leap_treatment(melody)` | [theory/counterpoint.vibe:265](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L265) |
| `check_parallel_perfects_counterpoint(voice1, voice2)` | [theory/counterpoint.vibe:174](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L174) |
| `first_species_above(cantus_firmus, key_root)` | [theory/counterpoint.vibe:30](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L30) |
| `first_species_above_ex(cantus_firmus, key_root, mode)` | [theory/counterpoint.vibe:33](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L33) |
| `first_species_below(cantus_firmus, key_root)` | [theory/counterpoint.vibe:80](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L80) |
| `first_species_below_ex(cantus_firmus, key_root, mode)` | [theory/counterpoint.vibe:83](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L83) |
| `generate_cantus_firmus(key_root)` | [theory/counterpoint.vibe:393](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L393) |
| `generate_cantus_firmus_ex(key_root, mode, length)` | [theory/counterpoint.vibe:396](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L396) |
| `is_consonant_interval(semitones)` | [theory/counterpoint.vibe:131](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L131) |
| `is_imperfect_consonance(semitones)` | [theory/counterpoint.vibe:142](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L142) |
| `is_perfect_consonance(semitones)` | [theory/counterpoint.vibe:136](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L136) |
| `is_prepared_dissonance(prev_interval, curr_interval, step_size)` | [theory/counterpoint.vibe:148](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L148) |
| `is_proper_cadence(voice1_final, voice2_final, voice1_penult, voice2_penult)` | [theory/counterpoint.vibe:303](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L303) |
| `is_resolved_dissonance(curr_interval, next_interval, step_size)` | [theory/counterpoint.vibe:156](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L156) |
| `is_valid_first_species_interval(semitones)` | [theory/counterpoint.vibe:20](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L20) |
| `is_valid_melodic_interval(semitones)` | [theory/counterpoint.vibe:249](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L249) |
| `score_counterpoint(voice1, voice2)` | [theory/counterpoint.vibe:436](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L436) |
| `validate_counterpoint(voice1, voice2)` | [theory/counterpoint.vibe:335](../../../crates/vibelang-std/stdlib/theory/counterpoint.vibe#L335) |

### `stdlib/theory/harmony.vibe`

| Exact signature | Source |
|---|---|
| `add_tension(chord, tension_semitones)` | [theory/harmony.vibe:253](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L253) |
| `analyze_harmonic_rhythm(progression, beats_per_chord)` | [theory/harmony.vibe:353](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L353) |
| `available_tensions(chord_quality)` | [theory/harmony.vibe:218](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L218) |
| `available_tensions_ex(chord_quality, context)` | [theory/harmony.vibe:221](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L221) |
| `chord_consonance(chord)` | [theory/harmony.vibe:468](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L468) |
| `chord_degree_in_key(chord_root, key_root)` | [theory/harmony.vibe:93](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L93) |
| `chord_degree_in_key_ex(chord_root, key_root, mode)` | [theory/harmony.vibe:96](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L96) |
| `classify_melodic_note(note, prev_note, next_note, chord, key)` | [theory/harmony.vibe:312](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L312) |
| `classify_melodic_note_ex(note, prev_note, next_note, chord, key, mode)` | [theory/harmony.vibe:315](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L315) |
| `dissonance_level(semitones)` | [theory/harmony.vibe:447](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L447) |
| `harmonic_function(degree)` | [theory/harmony.vibe:149](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L149) |
| `harmonic_function_ex(degree, mode)` | [theory/harmony.vibe:152](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L152) |
| `identify_chord(notes)` | [theory/harmony.vibe:15](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L15) |
| `is_chord_tone(note, chord)` | [theory/harmony.vibe:288](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L288) |
| `is_consonant(semitones)` | [theory/harmony.vibe:437](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L437) |
| `is_functional_progression(degrees)` | [theory/harmony.vibe:181](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L181) |
| `is_scale_tone(note, key_root)` | [theory/harmony.vibe:303](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L303) |
| `is_scale_tone_ex(note, key_root, mode)` | [theory/harmony.vibe:306](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L306) |
| `parallel_key(key_root, current_mode)` | [theory/harmony.vibe:395](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L395) |
| `relative_key(key_root, current_mode)` | [theory/harmony.vibe:383](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L383) |
| `roman_numeral(chord_root, chord_quality, key_root)` | [theory/harmony.vibe:113](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L113) |
| `roman_numeral_ex(chord_root, chord_quality, key_root, mode)` | [theory/harmony.vibe:116](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L116) |
| `scale_degree(note, key_root)` | [theory/harmony.vibe:268](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L268) |
| `scale_degree_ex(note, key_root, mode)` | [theory/harmony.vibe:271](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L271) |
| `suggest_substitutes(chord_root, chord_quality)` | [theory/harmony.vibe:401](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L401) |
| `tritone_sub(chord_root)` | [theory/harmony.vibe:377](../../../crates/vibelang-std/stdlib/theory/harmony.vibe#L377) |

### `stdlib/theory/melodies/classical.vibe`

| Exact signature | Source |
|---|---|
| `blue_danube()` | [theory/melodies/classical.vibe:45](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L45) |
| `canon_in_d()` | [theory/melodies/classical.vibe:60](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L60) |
| `carmen_habanera()` | [theory/melodies/classical.vibe:35](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L35) |
| `clair_de_lune()` | [theory/melodies/classical.vibe:95](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L95) |
| `eine_kleine_nachtmusik()` | [theory/melodies/classical.vibe:30](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L30) |
| `fur_elise()` | [theory/melodies/classical.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L13) |
| `fur_elise_2()` | [theory/melodies/classical.vibe:16](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L16) |
| `gymnopedie_1()` | [theory/melodies/classical.vibe:100](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L100) |
| `hall_mountain_king()` | [theory/melodies/classical.vibe:85](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L85) |
| `hallelujah_chorus()` | [theory/melodies/classical.vibe:115](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L115) |
| `hungarian_dance_5()` | [theory/melodies/classical.vibe:90](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L90) |
| `minuet_in_g()` | [theory/melodies/classical.vibe:75](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L75) |
| `moonlight_sonata()` | [theory/melodies/classical.vibe:65](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L65) |
| `mozart_symphony_40()` | [theory/melodies/classical.vibe:120](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L120) |
| `nocturne_op9_no2()` | [theory/melodies/classical.vibe:125](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L125) |
| `ode_to_joy()` | [theory/melodies/classical.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L8) |
| `ride_of_valkyries()` | [theory/melodies/classical.vibe:50](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L50) |
| `spring_vivaldi()` | [theory/melodies/classical.vibe:55](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L55) |
| `sugar_plum_fairy()` | [theory/melodies/classical.vibe:105](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L105) |
| `swan_lake()` | [theory/melodies/classical.vibe:25](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L25) |
| `toccata_fugue()` | [theory/melodies/classical.vibe:80](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L80) |
| `turkish_march()` | [theory/melodies/classical.vibe:70](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L70) |
| `william_tell_overture()` | [theory/melodies/classical.vibe:40](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L40) |
| `winter_vivaldi()` | [theory/melodies/classical.vibe:110](../../../crates/vibelang-std/stdlib/theory/melodies/classical.vibe#L110) |

### `stdlib/theory/melodies/folk_songs.vibe`

| Exact signature | Source |
|---|---|
| `amazing_grace()` | [theory/melodies/folk_songs.vibe:68](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L68) |
| `auld_lang_syne()` | [theory/melodies/folk_songs.vibe:73](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L73) |
| `camptown_races()` | [theory/melodies/folk_songs.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L48) |
| `clementine()` | [theory/melodies/folk_songs.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L53) |
| `coming_round_the_mountain()` | [theory/melodies/folk_songs.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L33) |
| `danny_boy()` | [theory/melodies/folk_songs.vibe:78](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L78) |
| `down_by_riverside()` | [theory/melodies/folk_songs.vibe:63](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L63) |
| `greensleeves()` | [theory/melodies/folk_songs.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L8) |
| `home_on_the_range()` | [theory/melodies/folk_songs.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L28) |
| `oh_susanna()` | [theory/melodies/folk_songs.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L18) |
| `old_smokey()` | [theory/melodies/folk_songs.vibe:58](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L58) |
| `scarborough_fair()` | [theory/melodies/folk_songs.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L13) |
| `skip_to_my_lou()` | [theory/melodies/folk_songs.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L43) |
| `this_old_man()` | [theory/melodies/folk_songs.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L38) |
| `yankee_doodle()` | [theory/melodies/folk_songs.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/folk_songs.vibe#L23) |

### `stdlib/theory/melodies/holiday_songs.vibe`

| Exact signature | Source |
|---|---|
| `auld_lang_syne_holiday()` | [theory/melodies/holiday_songs.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L53) |
| `deck_the_halls()` | [theory/melodies/holiday_songs.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L18) |
| `first_noel()` | [theory/melodies/holiday_songs.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L38) |
| `happy_birthday()` | [theory/melodies/holiday_songs.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L48) |
| `hark_herald_angels()` | [theory/melodies/holiday_songs.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L43) |
| `jingle_bells()` | [theory/melodies/holiday_songs.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L8) |
| `joy_to_world()` | [theory/melodies/holiday_songs.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L33) |
| `merry_christmas()` | [theory/melodies/holiday_songs.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L23) |
| `o_christmas_tree()` | [theory/melodies/holiday_songs.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L28) |
| `silent_night()` | [theory/melodies/holiday_songs.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/holiday_songs.vibe#L13) |

### `stdlib/theory/melodies/jazz_standards.vibe`

| Exact signature | Source |
|---|---|
| `black_bottom_stomp()` | [theory/melodies/jazz_standards.vibe:63](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L63) |
| `blue_monk()` | [theory/melodies/jazz_standards.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L53) |
| `caravan()` | [theory/melodies/jazz_standards.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L18) |
| `in_the_mood()` | [theory/melodies/jazz_standards.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L38) |
| `maple_leaf_rag()` | [theory/melodies/jazz_standards.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L28) |
| `sing_sing_sing()` | [theory/melodies/jazz_standards.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L43) |
| `st_louis_blues()` | [theory/melodies/jazz_standards.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L33) |
| `summertime()` | [theory/melodies/jazz_standards.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L48) |
| `swing_low_sweet_chariot()` | [theory/melodies/jazz_standards.vibe:58](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L58) |
| `take_five()` | [theory/melodies/jazz_standards.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L13) |
| `the_entertainer()` | [theory/melodies/jazz_standards.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L23) |
| `when_saints_go_marching()` | [theory/melodies/jazz_standards.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/jazz_standards.vibe#L8) |

### `stdlib/theory/melodies/movie_themes.vibe`

| Exact signature | Source |
|---|---|
| `also_sprach_zarathustra()` | [theory/melodies/movie_themes.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L8) |
| `axel_f()` | [theory/melodies/movie_themes.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L43) |
| `back_to_future_theme()` | [theory/melodies/movie_themes.vibe:58](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L58) |
| `chariots_of_fire()` | [theory/melodies/movie_themes.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L28) |
| `close_encounters()` | [theory/melodies/movie_themes.vibe:68](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L68) |
| `ghostbusters_theme()` | [theory/melodies/movie_themes.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L53) |
| `gonna_fly_now()` | [theory/melodies/movie_themes.vibe:63](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L63) |
| `good_bad_ugly_theme()` | [theory/melodies/movie_themes.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L18) |
| `halloween_theme()` | [theory/melodies/movie_themes.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L48) |
| `imperial_march()` | [theory/melodies/movie_themes.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L33) |
| `indiana_jones_theme()` | [theory/melodies/movie_themes.vibe:78](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L78) |
| `james_bond_theme()` | [theory/melodies/movie_themes.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L38) |
| `jaws_theme()` | [theory/melodies/movie_themes.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L13) |
| `superman_theme()` | [theory/melodies/movie_themes.vibe:73](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L73) |
| `the_entertainer_movie()` | [theory/melodies/movie_themes.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/movie_themes.vibe#L23) |

### `stdlib/theory/melodies/national_anthems.vibe`

| Exact signature | Source |
|---|---|
| `advance_australia_fair()` | [theory/melodies/national_anthems.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L28) |
| `deutschlandlied()` | [theory/melodies/national_anthems.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L33) |
| `fratelli_italia()` | [theory/melodies/national_anthems.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L48) |
| `god_save_queen()` | [theory/melodies/national_anthems.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L13) |
| `kimigayo()` | [theory/melodies/national_anthems.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L38) |
| `la_brabanconne()` | [theory/melodies/national_anthems.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L43) |
| `la_marseillaise()` | [theory/melodies/national_anthems.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L18) |
| `mexican_anthem()` | [theory/melodies/national_anthems.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L53) |
| `o_canada()` | [theory/melodies/national_anthems.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L23) |
| `star_spangled_banner()` | [theory/melodies/national_anthems.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/national_anthems.vibe#L8) |

### `stdlib/theory/melodies/nursery_rhymes.vibe`

| Exact signature | Source |
|---|---|
| `baa_baa_black_sheep()` | [theory/melodies/nursery_rhymes.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L18) |
| `humpty_dumpty()` | [theory/melodies/nursery_rhymes.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L38) |
| `jack_and_jill()` | [theory/melodies/nursery_rhymes.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L43) |
| `london_bridge()` | [theory/melodies/nursery_rhymes.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L28) |
| `mary_had_little_lamb()` | [theory/melodies/nursery_rhymes.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L13) |
| `old_macdonald()` | [theory/melodies/nursery_rhymes.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L48) |
| `pop_goes_weasel()` | [theory/melodies/nursery_rhymes.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L33) |
| `row_row_row_boat()` | [theory/melodies/nursery_rhymes.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L23) |
| `twinkle_twinkle()` | [theory/melodies/nursery_rhymes.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L8) |
| `wheels_on_bus()` | [theory/melodies/nursery_rhymes.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/nursery_rhymes.vibe#L53) |

### `stdlib/theory/melodies/pop_rock.vibe`

| Exact signature | Source |
|---|---|
| `another_one_bites_dust()` | [theory/melodies/pop_rock.vibe:58](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L58) |
| `billie_jean_bass()` | [theory/melodies/pop_rock.vibe:63](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L63) |
| `come_together_riff()` | [theory/melodies/pop_rock.vibe:68](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L68) |
| `day_tripper_riff()` | [theory/melodies/pop_rock.vibe:73](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L73) |
| `house_rising_sun()` | [theory/melodies/pop_rock.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L8) |
| `iron_man_riff()` | [theory/melodies/pop_rock.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L38) |
| `la_bamba()` | [theory/melodies/pop_rock.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L18) |
| `louie_louie()` | [theory/melodies/pop_rock.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L13) |
| `pretty_woman_riff()` | [theory/melodies/pop_rock.vibe:78](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L78) |
| `satisfaction_riff()` | [theory/melodies/pop_rock.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L43) |
| `seven_nation_army()` | [theory/melodies/pop_rock.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L28) |
| `smoke_on_water()` | [theory/melodies/pop_rock.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L23) |
| `sunshine_love_riff()` | [theory/melodies/pop_rock.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L48) |
| `superstition_riff()` | [theory/melodies/pop_rock.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L53) |
| `sweet_child_opening()` | [theory/melodies/pop_rock.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/pop_rock.vibe#L33) |

### `stdlib/theory/melodies/tv_themes.vibe`

| Exact signature | Source |
|---|---|
| `addams_family_theme()` | [theory/melodies/tv_themes.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L43) |
| `brady_bunch_theme()` | [theory/melodies/tv_themes.vibe:68](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L68) |
| `cheers_theme()` | [theory/melodies/tv_themes.vibe:63](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L63) |
| `doctor_who_theme()` | [theory/melodies/tv_themes.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L18) |
| `flintstones_theme()` | [theory/melodies/tv_themes.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L23) |
| `i_love_lucy_theme()` | [theory/melodies/tv_themes.vibe:58](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L58) |
| `inspector_gadget_theme()` | [theory/melodies/tv_themes.vibe:78](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L78) |
| `mission_impossible_theme()` | [theory/melodies/tv_themes.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L33) |
| `pink_panther_theme()` | [theory/melodies/tv_themes.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L28) |
| `scooby_doo_theme()` | [theory/melodies/tv_themes.vibe:73](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L73) |
| `sesame_street_theme()` | [theory/melodies/tv_themes.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L48) |
| `simpsons_theme()` | [theory/melodies/tv_themes.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L8) |
| `star_trek_theme()` | [theory/melodies/tv_themes.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L13) |
| `twilight_zone_theme()` | [theory/melodies/tv_themes.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L53) |
| `x_files_theme()` | [theory/melodies/tv_themes.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/tv_themes.vibe#L38) |

### `stdlib/theory/melodies/video_games.vibe`

| Exact signature | Source |
|---|---|
| `castlevania_vampire_killer()` | [theory/melodies/video_games.vibe:63](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L63) |
| `chrono_trigger_theme()` | [theory/melodies/video_games.vibe:103](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L103) |
| `contra_jungle()` | [theory/melodies/video_games.vibe:73](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L73) |
| `donkey_kong_theme()` | [theory/melodies/video_games.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L48) |
| `duck_hunt_theme()` | [theory/melodies/video_games.vibe:93](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L93) |
| `ff_victory_fanfare()` | [theory/melodies/video_games.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L38) |
| `frogger_theme()` | [theory/melodies/video_games.vibe:78](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L78) |
| `galaga_theme()` | [theory/melodies/video_games.vibe:88](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L88) |
| `kirby_green_greens()` | [theory/melodies/video_games.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L53) |
| `mario_theme()` | [theory/melodies/video_games.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L13) |
| `megaman_theme()` | [theory/melodies/video_games.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L28) |
| `metroid_theme()` | [theory/melodies/video_games.vibe:68](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L68) |
| `pacman_theme()` | [theory/melodies/video_games.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L43) |
| `pokemon_theme()` | [theory/melodies/video_games.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L33) |
| `punch_out_theme()` | [theory/melodies/video_games.vibe:98](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L98) |
| `sonic_green_hill()` | [theory/melodies/video_games.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L23) |
| `space_invaders()` | [theory/melodies/video_games.vibe:83](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L83) |
| `street_fighter_ryu()` | [theory/melodies/video_games.vibe:58](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L58) |
| `tetris_theme()` | [theory/melodies/video_games.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L8) |
| `zelda_theme()` | [theory/melodies/video_games.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/video_games.vibe#L18) |

### `stdlib/theory/melodies/world_music.vibe`

| Exact signature | Source |
|---|---|
| `bella_ciao()` | [theory/melodies/world_music.vibe:28](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L28) |
| `frere_jacques()` | [theory/melodies/world_music.vibe:38](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L38) |
| `guantanamera()` | [theory/melodies/world_music.vibe:53](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L53) |
| `hatikvah()` | [theory/melodies/world_music.vibe:68](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L68) |
| `hava_nagila()` | [theory/melodies/world_music.vibe:8](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L8) |
| `irish_washerwoman()` | [theory/melodies/world_music.vibe:58](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L58) |
| `kalinka()` | [theory/melodies/world_music.vibe:23](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L23) |
| `la_cucaracha()` | [theory/melodies/world_music.vibe:13](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L13) |
| `malaguena()` | [theory/melodies/world_music.vibe:43](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L43) |
| `sakura_sakura()` | [theory/melodies/world_music.vibe:18](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L18) |
| `scotland_brave()` | [theory/melodies/world_music.vibe:63](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L63) |
| `siyahamba()` | [theory/melodies/world_music.vibe:48](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L48) |
| `waltzing_matilda()` | [theory/melodies/world_music.vibe:33](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L33) |
| `zorbas_dance()` | [theory/melodies/world_music.vibe:73](../../../crates/vibelang-std/stdlib/theory/melodies/world_music.vibe#L73) |

### `stdlib/theory/melody_gen.vibe`

| Exact signature | Source |
|---|---|
| `add_neighbor_tones(melody)` | [theory/melody_gen.vibe:158](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L158) |
| `add_neighbor_tones_ex(melody, lower)` | [theory/melody_gen.vibe:161](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L161) |
| `add_passing_tones(melody)` | [theory/melody_gen.vibe:131](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L131) |
| `arch_melody(root, scale_name, length)` | [theory/melody_gen.vibe:217](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L217) |
| `arch_melody_ex(root, scale_name, length, octave)` | [theory/melody_gen.vibe:220](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L220) |
| `arpeggiated_melody(progression)` | [theory/melody_gen.vibe:101](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L101) |
| `arpeggiated_melody_ex(progression, pattern)` | [theory/melody_gen.vibe:104](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L104) |
| `ascending_melody(root, scale_name, length)` | [theory/melody_gen.vibe:189](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L189) |
| `ascending_melody_ex(root, scale_name, length, octave)` | [theory/melody_gen.vibe:192](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L192) |
| `ascending_sequence(motif, steps)` | [theory/melody_gen.vibe:279](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L279) |
| `ascending_sequence_ex(motif, steps, semitone_interval)` | [theory/melody_gen.vibe:282](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L282) |
| `augmentation(motif)` | [theory/melody_gen.vibe:364](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L364) |
| `bebop_run(root)` | [theory/melody_gen.vibe:429](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L429) |
| `bebop_run_ex(root, octave, direction)` | [theory/melody_gen.vibe:432](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L432) |
| `call_and_response(call_motif)` | [theory/melody_gen.vibe:393](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L393) |
| `call_and_response_ex(call_motif, answer_motif)` | [theory/melody_gen.vibe:396](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L396) |
| `chord_tone_melody(progression)` | [theory/melody_gen.vibe:80](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L80) |
| `chord_tone_melody_ex(progression, notes_per_chord)` | [theory/melody_gen.vibe:83](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L83) |
| `descending_melody(root, scale_name, length)` | [theory/melody_gen.vibe:200](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L200) |
| `descending_melody_ex(root, scale_name, length, octave)` | [theory/melody_gen.vibe:203](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L203) |
| `descending_sequence(motif, steps)` | [theory/melody_gen.vibe:303](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L303) |
| `descending_sequence_ex(motif, steps, semitone_interval)` | [theory/melody_gen.vibe:306](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L306) |
| `diminution(motif)` | [theory/melody_gen.vibe:376](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L376) |
| `inversion(motif)` | [theory/melody_gen.vibe:342](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L342) |
| `limit_range(melody, lowest_note, highest_note)` | [theory/melody_gen.vibe:485](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L485) |
| `pad_with_rests(melody, target_length)` | [theory/melody_gen.vibe:454](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L454) |
| `random_walk_melody(root, scale_name, length)` | [theory/melody_gen.vibe:16](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L16) |
| `random_walk_melody_ex(root, scale_name, length, octave, max_jump)` | [theory/melody_gen.vibe:19](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L19) |
| `remove_consecutive_rests(melody)` | [theory/melody_gen.vibe:465](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L465) |
| `retrograde(motif)` | [theory/melody_gen.vibe:331](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L331) |
| `stepwise_melody(root, scale_name, length)` | [theory/melody_gen.vibe:51](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L51) |
| `stepwise_melody_ex(root, scale_name, length, octave)` | [theory/melody_gen.vibe:54](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L54) |
| `wave_melody(root, scale_name, length)` | [theory/melody_gen.vibe:247](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L247) |
| `wave_melody_ex(root, scale_name, length, octave)` | [theory/melody_gen.vibe:250](../../../crates/vibelang-std/stdlib/theory/melody_gen.vibe#L250) |

### `stdlib/theory/progressions.vibe`

| Exact signature | Source |
|---|---|
| `andalusian_cadence(key)` | [theory/progressions.vibe:316](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L316) |
| `andalusian_cadence_ex(key, octave)` | [theory/progressions.vibe:319](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L319) |
| `axis_progression(key)` | [theory/progressions.vibe:100](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L100) |
| `axis_progression_ex(key, octave)` | [theory/progressions.vibe:103](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L103) |
| `blues_12bar(key)` | [theory/progressions.vibe:187](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L187) |
| `blues_12bar_ex(key, octave)` | [theory/progressions.vibe:190](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L190) |
| `blues_minor(key)` | [theory/progressions.vibe:211](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L211) |
| `blues_minor_ex(key, octave)` | [theory/progressions.vibe:214](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L214) |
| `blues_quick_change(key)` | [theory/progressions.vibe:202](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L202) |
| `blues_quick_change_ex(key, octave)` | [theory/progressions.vibe:205](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L205) |
| `bossa_nova(key)` | [theory/progressions.vibe:366](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L366) |
| `bossa_nova_ex(key, octave)` | [theory/progressions.vibe:369](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L369) |
| `canon_progression(key)` | [theory/progressions.vibe:109](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L109) |
| `canon_progression_ex(key, octave)` | [theory/progressions.vibe:112](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L112) |
| `chord_progression(key, mode, degrees)` | [theory/progressions.vibe:19](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L19) |
| `chord_progression_ex(key, mode, degrees, octave, chord_type)` | [theory/progressions.vibe:22](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L22) |
| `chromatic_descent(key)` | [theory/progressions.vibe:388](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L388) |
| `chromatic_descent_ex(key, octave, steps)` | [theory/progressions.vibe:391](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L391) |
| `circle_of_fifths(key)` | [theory/progressions.vibe:408](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L408) |
| `circle_of_fifths_ex(key, octave)` | [theory/progressions.vibe:411](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L411) |
| `deceptive_cadence(key)` | [theory/progressions.vibe:298](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L298) |
| `deceptive_cadence_ex(key, octave)` | [theory/progressions.vibe:301](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L301) |
| `dorian_vamp(key)` | [theory/progressions.vibe:224](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L224) |
| `dorian_vamp_ex(key, octave)` | [theory/progressions.vibe:227](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L227) |
| `fifties_progression(key)` | [theory/progressions.vibe:91](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L91) |
| `fifties_progression_ex(key, octave)` | [theory/progressions.vibe:94](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L94) |
| `flatten_progression(progression)` | [theory/progressions.vibe:482](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L482) |
| `flatten_progression_ex(progression, notes_per_chord)` | [theory/progressions.vibe:485](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L485) |
| `half_cadence(key)` | [theory/progressions.vibe:307](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L307) |
| `half_cadence_ex(key, octave)` | [theory/progressions.vibe:310](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L310) |
| `house_progression(key)` | [theory/progressions.vibe:335](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L335) |
| `house_progression_ex(key, octave)` | [theory/progressions.vibe:338](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L338) |
| `jazz_ballad(key)` | [theory/progressions.vibe:149](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L149) |
| `jazz_ballad_ex(key, octave)` | [theory/progressions.vibe:152](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L152) |
| `jazz_ii_v_i(key)` | [theory/progressions.vibe:122](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L122) |
| `jazz_ii_v_i_ex(key, octave)` | [theory/progressions.vibe:125](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L125) |
| `jazz_iii_vi_ii_v(key)` | [theory/progressions.vibe:140](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L140) |
| `jazz_iii_vi_ii_v_ex(key, octave)` | [theory/progressions.vibe:143](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L143) |
| `jazz_minor_ii_v_i(key)` | [theory/progressions.vibe:174](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L174) |
| `jazz_minor_ii_v_i_ex(key, octave)` | [theory/progressions.vibe:177](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L177) |
| `jazz_turnaround(key)` | [theory/progressions.vibe:131](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L131) |
| `jazz_turnaround_ex(key, octave)` | [theory/progressions.vibe:134](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L134) |
| `lydian_progression(key)` | [theory/progressions.vibe:237](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L237) |
| `lydian_progression_ex(key, octave)` | [theory/progressions.vibe:240](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L240) |
| `mixolydian_groove(key)` | [theory/progressions.vibe:250](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L250) |
| `mixolydian_groove_ex(key, octave)` | [theory/progressions.vibe:253](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L253) |
| `montuno(key)` | [theory/progressions.vibe:375](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L375) |
| `montuno_ex(key, octave)` | [theory/progressions.vibe:378](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L378) |
| `perfect_cadence(key)` | [theory/progressions.vibe:280](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L280) |
| `perfect_cadence_ex(key, octave)` | [theory/progressions.vibe:283](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L283) |
| `phrygian_progression(key)` | [theory/progressions.vibe:263](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L263) |
| `phrygian_progression_ex(key, octave)` | [theory/progressions.vibe:266](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L266) |
| `plagal_cadence(key)` | [theory/progressions.vibe:289](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L289) |
| `plagal_cadence_ex(key, octave)` | [theory/progressions.vibe:292](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L292) |
| `pop_progression_1(key)` | [theory/progressions.vibe:64](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L64) |
| `pop_progression_1_ex(key, octave)` | [theory/progressions.vibe:67](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L67) |
| `pop_progression_2(key)` | [theory/progressions.vibe:82](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L82) |
| `pop_progression_2_ex(key, octave)` | [theory/progressions.vibe:85](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L85) |
| `progression(name, key)` | [theory/progressions.vibe:419](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L419) |
| `progression_ex(name, key, octave)` | [theory/progressions.vibe:423](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L423) |
| `progression_roots(progression)` | [theory/progressions.vibe:502](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L502) |
| `rhythm_changes_a(key)` | [theory/progressions.vibe:158](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L158) |
| `rhythm_changes_a_ex(key, octave)` | [theory/progressions.vibe:161](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L161) |
| `rock_progression_1(key)` | [theory/progressions.vibe:73](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L73) |
| `rock_progression_1_ex(key, octave)` | [theory/progressions.vibe:76](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L76) |
| `techno_vamp(key)` | [theory/progressions.vibe:353](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L353) |
| `techno_vamp_ex(key, octave)` | [theory/progressions.vibe:356](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L356) |
| `trance_progression(key)` | [theory/progressions.vibe:344](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L344) |
| `trance_progression_ex(key, octave)` | [theory/progressions.vibe:347](../../../crates/vibelang-std/stdlib/theory/progressions.vibe#L347) |

### `stdlib/theory/rhythm.vibe`

| Exact signature | Source |
|---|---|
| `accent_every_n(pattern, n)` | [theory/rhythm.vibe:369](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L369) |
| `add_accents(pattern, accent_positions)` | [theory/rhythm.vibe:352](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L352) |
| `add_space(pattern)` | [theory/rhythm.vibe:324](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L324) |
| `add_syncopation(pattern)` | [theory/rhythm.vibe:223](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L223) |
| `anticipate(pattern, beats_to_anticipate)` | [theory/rhythm.vibe:243](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L243) |
| `apply_rhythm(notes, rhythm)` | [theory/rhythm.vibe:462](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L462) |
| `backbeat()` | [theory/rhythm.vibe:47](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L47) |
| `backbeat_ex(bars)` | [theory/rhythm.vibe:50](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L50) |
| `bossa_nova_clave()` | [theory/rhythm.vibe:160](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L160) |
| `dnb_break()` | [theory/rhythm.vibe:184](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L184) |
| `double_time(pattern)` | [theory/rhythm.vibe:433](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L433) |
| `euclidean_pattern(name)` | [theory/rhythm.vibe:123](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L123) |
| `euclidean_rhythm(hits, steps)` | [theory/rhythm.vibe:92](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L92) |
| `four_on_floor()` | [theory/rhythm.vibe:27](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L27) |
| `four_on_floor_ex(bars)` | [theory/rhythm.vibe:30](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L30) |
| `funk_ghost_notes()` | [theory/rhythm.vibe:194](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L194) |
| `half_time(pattern)` | [theory/rhythm.vibe:445](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L445) |
| `half_time_shuffle()` | [theory/rhythm.vibe:214](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L214) |
| `house_kick()` | [theory/rhythm.vibe:169](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L169) |
| `insert_rests(pattern, rest_positions)` | [theory/rhythm.vibe:309](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L309) |
| `invert_pattern(pattern)` | [theory/rhythm.vibe:418](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L418) |
| `offbeat()` | [theory/rhythm.vibe:67](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L67) |
| `offbeat_ex(bars)` | [theory/rhythm.vibe:70](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L70) |
| `polyrhythm(pattern1, pattern2)` | [theory/rhythm.vibe:267](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L267) |
| `random_rhythm(steps)` | [theory/rhythm.vibe:487](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L487) |
| `random_rhythm_ex(steps, density)` | [theory/rhythm.vibe:490](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L490) |
| `reggae_one_drop()` | [theory/rhythm.vibe:189](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L189) |
| `reverse_pattern(pattern)` | [theory/rhythm.vibe:394](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L394) |
| `rhythm_pattern(hits, total_steps)` | [theory/rhythm.vibe:12](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L12) |
| `rotate_pattern(pattern, steps)` | [theory/rhythm.vibe:405](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L405) |
| `rumba_clave_2_3()` | [theory/rhythm.vibe:155](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L155) |
| `rumba_clave_3_2()` | [theory/rhythm.vibe:150](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L150) |
| `shuffle_pattern()` | [theory/rhythm.vibe:209](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L209) |
| `son_clave_2_3()` | [theory/rhythm.vibe:145](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L145) |
| `son_clave_3_2()` | [theory/rhythm.vibe:140](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L140) |
| `swing_8th()` | [theory/rhythm.vibe:204](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L204) |
| `techno_kick()` | [theory/rhythm.vibe:174](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L174) |
| `trap_hihat()` | [theory/rhythm.vibe:179](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe#L179) |

### `stdlib/theory/scales.vibe`

| Exact signature | Source |
|---|---|
| `aeolian_scale(root)` | [theory/scales.vibe:149](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L149) |
| `aeolian_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:153](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L153) |
| `arabic_scale(root)` | [theory/scales.vibe:373](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L373) |
| `arabic_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:377](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L377) |
| `blues_major_scale(root)` | [theory/scales.vibe:225](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L225) |
| `blues_major_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:229](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L229) |
| `blues_minor_scale(root)` | [theory/scales.vibe:233](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L233) |
| `blues_minor_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:237](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L237) |
| `blues_scale(root)` | [theory/scales.vibe:241](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L241) |
| `blues_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:245](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L245) |
| `byzantine_scale(root)` | [theory/scales.vibe:305](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L305) |
| `byzantine_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:309](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L309) |
| `chromatic_scale(root)` | [theory/scales.vibe:277](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L277) |
| `chromatic_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:281](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L281) |
| `diminished_scale(root)` | [theory/scales.vibe:261](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L261) |
| `diminished_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:265](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L265) |
| `diminished_whole_half_scale(root)` | [theory/scales.vibe:269](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L269) |
| `diminished_whole_half_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:273](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L273) |
| `dorian_scale(root)` | [theory/scales.vibe:117](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L117) |
| `dorian_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:121](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L121) |
| `double_harmonic_scale(root)` | [theory/scales.vibe:297](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L297) |
| `double_harmonic_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:301](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L301) |
| `harmonic_major_scale(root)` | [theory/scales.vibe:289](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L289) |
| `harmonic_major_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:293](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L293) |
| `harmonic_minor_scale(root)` | [theory/scales.vibe:185](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L185) |
| `harmonic_minor_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:189](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L189) |
| `hirajoshi_scale(root)` | [theory/scales.vibe:357](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L357) |
| `hirajoshi_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:361](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L361) |
| `hungarian_minor_scale(root)` | [theory/scales.vibe:321](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L321) |
| `hungarian_minor_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:325](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L325) |
| `ionian_scale(root)` | [theory/scales.vibe:109](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L109) |
| `ionian_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:113](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L113) |
| `japanese_scale(root)` | [theory/scales.vibe:349](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L349) |
| `japanese_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:353](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L353) |
| `locrian_scale(root)` | [theory/scales.vibe:165](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L165) |
| `locrian_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:169](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L169) |
| `lydian_scale(root)` | [theory/scales.vibe:133](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L133) |
| `lydian_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:137](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L137) |
| `major_pentatonic_scale(root)` | [theory/scales.vibe:205](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L205) |
| `major_pentatonic_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:209](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L209) |
| `major_scale(root)` | [theory/scales.vibe:101](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L101) |
| `major_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:105](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L105) |
| `melodic_minor_scale(root)` | [theory/scales.vibe:193](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L193) |
| `melodic_minor_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:197](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L197) |
| `minor_pentatonic_scale(root)` | [theory/scales.vibe:213](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L213) |
| `minor_pentatonic_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:217](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L217) |
| `minor_scale(root)` | [theory/scales.vibe:177](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L177) |
| `minor_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:181](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L181) |
| `mixolydian_scale(root)` | [theory/scales.vibe:141](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L141) |
| `mixolydian_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:145](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L145) |
| `natural_minor_scale(root)` | [theory/scales.vibe:157](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L157) |
| `natural_minor_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:161](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L161) |
| `neapolitan_major_scale(root)` | [theory/scales.vibe:329](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L329) |
| `neapolitan_major_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:333](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L333) |
| `neapolitan_minor_scale(root)` | [theory/scales.vibe:337](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L337) |
| `neapolitan_minor_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:341](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L341) |
| `pelog_scale(root)` | [theory/scales.vibe:365](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L365) |
| `pelog_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:369](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L369) |
| `phrygian_dominant_scale(root)` | [theory/scales.vibe:313](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L313) |
| `phrygian_dominant_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:317](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L317) |
| `phrygian_scale(root)` | [theory/scales.vibe:125](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L125) |
| `phrygian_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:129](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L129) |
| `scale(root, scale_name)` | [theory/scales.vibe:385](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L385) |
| `scale_degrees(root, scale_name, degrees)` | [theory/scales.vibe:434](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L434) |
| `scale_degrees_ex(root, scale_name, degrees, octave)` | [theory/scales.vibe:437](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L437) |
| `scale_ex(root, scale_name, octave, num_notes)` | [theory/scales.vibe:389](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L389) |
| `whole_tone_scale(root)` | [theory/scales.vibe:253](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L253) |
| `whole_tone_scale_ex(root, octave, num_notes)` | [theory/scales.vibe:257](../../../crates/vibelang-std/stdlib/theory/scales.vibe#L257) |

### `stdlib/theory/voice_leading.vibe`

| Exact signature | Source |
|---|---|
| `analyze_motion(note1_start, note1_end, note2_start, note2_end)` | [theory/voice_leading.vibe:213](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L213) |
| `assess_voice_leading(chord1, chord2)` | [theory/voice_leading.vibe:384](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L384) |
| `check_parallel_perfect(chord1, chord2)` | [theory/voice_leading.vibe:230](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L230) |
| `check_range(chord, ranges)` | [theory/voice_leading.vibe:286](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L286) |
| `find_common_tones(chord1, chord2)` | [theory/voice_leading.vibe:98](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L98) |
| `fix_voice_crossing(chord)` | [theory/voice_leading.vibe:280](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L280) |
| `has_voice_crossing(chord)` | [theory/voice_leading.vibe:265](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L265) |
| `satb_ranges()` | [theory/voice_leading.vibe:314](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L314) |
| `satb_voice_lead(chord1, chord2)` | [theory/voice_leading.vibe:328](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L328) |
| `voice_lead(chord1, chord2)` | [theory/voice_leading.vibe:15](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L15) |
| `voice_lead_progression(progression)` | [theory/voice_leading.vibe:76](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L76) |
| `voice_lead_with_common_tones(chord1, chord2)` | [theory/voice_leading.vibe:133](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe#L133) |

## Import-callable implementation helpers

The following 112 declarations are unsupported implementation details by naming
convention, not by language enforcement. Importing the containing module makes
them callable today. They may change without the compatibility guarantees of
the intended-supported table above; do not treat the leading underscore as an
access-control boundary.

| Import module | Exact import-callable signatures |
|---|---|
| [`stdlib/instruments/eurorack/erica_black_vco2.vibe`](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_black_vco2.vibe) | `_erica_black_vco2_fold(x)` |
| [`stdlib/instruments/eurorack/erica_vc_eg.vibe`](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_vc_eg.vibe) | `_erica_vc_eg_gate(x)`<br>`_erica_vc_eg_stage(base, cv, amt)`<br>`_erica_vc_eg_level(base, cv, amt)`<br>`_erica_vc_eg_rise(pos)` |
| [`stdlib/instruments/eurorack/erica_wavetable_vco.vibe`](../../../crates/vibelang-std/stdlib/instruments/eurorack/erica_wavetable_vco.vibe) | `_erica_wt_sub_shape(freq, shape)`<br>`_erica_wt_bank_cell(freq, bank, wave)` |
| [`stdlib/instruments/eurorack/frames.vibe`](../../../crates/vibelang-std/stdlib/instruments/eurorack/frames.vibe) | `_frames_mask(value, target)`<br>`_frames_select8(index, kf1, kf2, kf3, kf4, kf5, kf6, kf7, kf8)`<br>`_frames_ease(frac, easing)`<br>`_frames_interp(pos, easing, kf1, kf2, kf3, kf4, kf5, kf6, kf7, kf8)` |
| [`stdlib/instruments/eurorack/quadrax.vibe`](../../../crates/vibelang-std/stdlib/instruments/eurorack/quadrax.vibe) | `_quadrax_gate(x)`<br>`_quadrax_mode_weight(mode, target)`<br>`_quadrax_shape(pos, curve_exp)`<br>`_quadrax_ad_shape(phase, rise_frac, curve_exp)`<br>`_quadrax_channel(trig_param, rise_param, fall_param, shape_param, mode_param, level_param)` |
| [`stdlib/instruments/eurorack/stages.vibe`](../../../crates/vibelang-std/stdlib/instruments/eurorack/stages.vibe) | `_stages_gate(x)`<br>`_stages_mask(value, target)`<br>`_stages_curve(phase, shape)`<br>`_stages_segment_value(kind_raw, loop_raw, primary_raw, secondary_raw, gate_raw, trigger_raw, clock_raw, reset_raw, chain_trig, scale_raw)`<br>`_stages_segment_eor(kind_raw, loop_raw, primary_raw, gate_raw, trigger_raw, clock_raw, reset_raw, chain_trig)` |
| [`stdlib/instruments/eurorack/tetrapad.vibe`](../../../crates/vibelang-std/stdlib/instruments/eurorack/tetrapad.vibe) | `_tetrapad_mask(value, target)`<br>`_tetrapad_gate(x, threshold)`<br>`_tetrapad_scale_bit(scale, degree)`<br>`_tetrapad_quantize(semitone, scale)`<br>`_tetrapad_chord_interval(chord_mode, pad_index, voicing)`<br>`_tetrapad_pad(pressure_raw, position_raw, encoder_raw, mode_raw, slew_raw, voltage_raw, threshold_raw, pad_index, chord_mode_raw, keyboard_scale_raw)` |
| [`stdlib/instruments/spectral/spectraphon.vibe`](../../../crates/vibelang-std/stdlib/instruments/spectral/spectraphon.vibe) | `_spectraphon_write_mag(k, chain, f0_analyze, focus, mag_buf, active)`<br>`_spectraphon_mag_norm(mag_buf)` |
| [`stdlib/processors/delays/dld.vibe`](../../../crates/vibelang-std/stdlib/processors/delays/dld.vibe) | `_dld_gate(x)`<br>`_dld_time_multiplier(index)`<br>`_dld_feedback_path(tap, feedback)` |
| [`stdlib/processors/delays/rainmaker.vibe`](../../../crates/vibelang-std/stdlib/processors/delays/rainmaker.vibe) | `_rainmaker_filter(sig, cutoff, q, filter_type)`<br>`_rainmaker_tap(src, base_time, grid, tap_index, level, pan, cutoff, q, filter_type, pitch, detune, mute, reverse)`<br>`_rainmaker_comb_weight(taps, threshold)` |
| [`stdlib/processors/distortion/ufold.vibe`](../../../crates/vibelang-std/stdlib/processors/distortion/ufold.vibe) | `_ufold_stage(x)` |
| [`stdlib/processors/filters/smr8.vibe`](../../../crates/vibelang-std/stdlib/processors/filters/smr8.vibe) | `_smr8_wrap_index(idx, len)`<br>`_smr8_degree(idx, scale)`<br>`_smr8_freq(root, scale, rotate, spread, band_index, lock, transpose, fine, lag_t)`<br>`_smr8_band(src, freq, q, two_pass, level)` |
| [`stdlib/processors/fx/erica_black_hole_dsp.vibe`](../../../crates/vibelang-std/stdlib/processors/fx/erica_black_hole_dsp.vibe) | `_erica_bh_cv_weight(assign, target)`<br>`_erica_bh_soft(sig, drive)` |
| [`stdlib/processors/mixers/erica_black_output.vibe`](../../../crates/vibelang-std/stdlib/processors/mixers/erica_black_output.vibe) | `_erica_black_output_soft(sig, limit_on)` |
| [`stdlib/processors/mixers/erica_quad_vca2.vibe`](../../../crates/vibelang-std/stdlib/processors/mixers/erica_quad_vca2.vibe) | `_erica_quad_vca2_gain(cv, bias, level, curve)` |
| [`stdlib/theory/chords.vibe`](../../../crates/vibelang-std/stdlib/theory/chords.vibe) | `_major_triad_intervals()`<br>`_minor_triad_intervals()`<br>`_diminished_triad_intervals()`<br>`_augmented_triad_intervals()`<br>`_sus2_intervals()`<br>`_sus4_intervals()`<br>`_major7_intervals()`<br>`_minor7_intervals()`<br>`_dominant7_intervals()`<br>`_half_diminished7_intervals()`<br>`_diminished7_intervals()`<br>`_minor_major7_intervals()`<br>`_augmented_major7_intervals()`<br>`_major9_intervals()`<br>`_minor9_intervals()`<br>`_dominant9_intervals()`<br>`_major11_intervals()`<br>`_minor11_intervals()`<br>`_dominant11_intervals()`<br>`_major13_intervals()`<br>`_minor13_intervals()`<br>`_dominant13_intervals()`<br>`_dom7b9_intervals()`<br>`_dom7sharp9_intervals()`<br>`_dom7b5_intervals()`<br>`_dom7sharp5_intervals()`<br>`_power_chord_intervals()`<br>`_power_chord_octave_intervals()`<br>`_generate_chord(root, intervals)`<br>`_generate_chord_ex(root, intervals, octave)`<br>`_sort_notes_by_pitch(notes)` |
| [`stdlib/theory/harmony.vibe`](../../../crates/vibelang-std/stdlib/theory/harmony.vibe) | `_sort_intervals(intervals)`<br>`_intervals_to_string(intervals)` |
| [`stdlib/theory/rhythm.vibe`](../../../crates/vibelang-std/stdlib/theory/rhythm.vibe) | `_lcm(a, b)`<br>`_gcd(a, b)` |
| [`stdlib/theory/scales.vibe`](../../../crates/vibelang-std/stdlib/theory/scales.vibe) | `_major_intervals()`<br>`_dorian_intervals()`<br>`_phrygian_intervals()`<br>`_lydian_intervals()`<br>`_mixolydian_intervals()`<br>`_aeolian_intervals()`<br>`_locrian_intervals()`<br>`_harmonic_minor_intervals()`<br>`_melodic_minor_intervals()`<br>`_major_pentatonic_intervals()`<br>`_minor_pentatonic_intervals()`<br>`_blues_major_intervals()`<br>`_blues_minor_intervals()`<br>`_whole_tone_intervals()`<br>`_diminished_intervals()`<br>`_diminished_whole_half_intervals()`<br>`_chromatic_intervals()`<br>`_harmonic_major_intervals()`<br>`_double_harmonic_intervals()`<br>`_phrygian_dominant_intervals()`<br>`_hungarian_minor_intervals()`<br>`_neapolitan_major_intervals()`<br>`_neapolitan_minor_intervals()`<br>`_japanese_intervals()`<br>`_hirajoshi_intervals()`<br>`_pelog_intervals()`<br>`_arabic_intervals()`<br>`_generate_scale(root, intervals)`<br>`_generate_scale_ex(root, intervals, num_notes, start_octave)` |
| [`stdlib/theory/voice_leading.vibe`](../../../crates/vibelang-std/stdlib/theory/voice_leading.vibe) | `_total_voice_distance(chord1, chord2)`<br>`_contains_pitch_class(notes, pitch_class)`<br>`_find_closest_unused(target, chord, used)`<br>`_fix_range_violations(chord, ranges)` |

Import-only index files do not create DSP names themselves. Rust library
exports in `vibelang-std` are embedding/extraction infrastructure, not Rhai
globals. The source contains three duplicate DSP names and one duplicate
intended-supported function name; consumers should not infer global uniqueness
across independently imported modules.

## Regeneration strategy

The checked-in tables provide exhaustive source-level name/signature discovery
today, while supported-public status remains a convention. The target pipeline
should parse Rhai syntax (or require explicit export metadata), preserve module
path and duplicate definitions, extract typed DSP builder metadata, and fail CI
when the 829/890/707 inventories or this artifact drift. See the [API roadmap](../../roadmap/api-improvement-roadmap.md#publication-and-generation-order).
