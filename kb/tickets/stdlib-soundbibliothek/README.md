---
title: "Standard-Bibliothek (Soundbibliothek)"
id: stdlib-soundbibliothek
status: open
tags: [referenz, audio, bibliothek]
labels:
  kategorie: referenz
  bereich: klang
priority: high
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Standard-Bibliothek (Soundbibliothek)

VibeLang enthält **187 Synthdefs** als `.vibe`-Dateien in der Standard-Bibliothek (`stdlib/`). Alle Klänge sind editierbar und transparent.

## Übersicht

| Kategorie | Anzahl | Beispiele |
|-----------|--------|-----------|
| Drums | 62 | kick_808, snare_909, hihat_808_closed, clap, clave |
| Bass | 47 | sub_deep, acid_303_classic, reese_classic, wobble_classic, fm_bass |
| Leads | 28 | lead_saw, lead_supersaw, pluck_bright, stab_brass |
| Pads | 20 | pad_warm, pad_shimmer, pad_evolving |
| FX | 20 | riser_white_noise, impact_hard, sweep_filter_up, subdrop_classic |
| Textures | 10 | wind, rain, drone_dark, granular |

## Drums (62)

- **Kicks** (12): kick_808, kick_909, kick_techno_deep, kick_techno_hard, kick_dnb, kick_trap, kick_acoustic, kick_sub, kick_pitched, kick_fm, kick_distorted, kick_soft
- **Snares** (12): snare_808, snare_909, snare_acoustic, snare_clap, snare_piccolo, snare_rimshot, snare_tight, snare_loose, snare_layered, snare_reverb, snare_filtered, snare_lofi
- **Hihats** (10): hihat_808_closed/open, hihat_909_closed/open, hihat_metallic, hihat_short, hihat_long, hihat_filtered, hihat_dusty, hihat_splash
- **Claps** (10): clap_808, clap_short, clap_reverb, clap_layered, clap_crowd, clap_tight, clap_loose, clap_filtered, clap_lofi, handclap
- **Percussion** (10): tom_high/mid/low, rim, cowbell, clave, woodblock, shaker, tambourine, snap
- **Cymbals** (8): crash_bright/dark, ride, splash, china, crash_reverse, crash_swell, bell_ping

## Bass (47)

- **Sub** (10): sub_pure, sub_triangle, sub_filtered, sub_octave, sub_modulated, sub_warm, sub_deep, sub_mono, sub_stereo, sub_harmonic
- **Acid** (10): acid_303_classic, acid_squelchy, acid_distorted, acid_bubbly, acid_minimal, acid_aggressive, acid_modulated, acid_detuned, acid_filtered_square, acid_resonant_sweep
- **Pluck** (10): pluck_short, pluck_long, pluck_bright, pluck_dark, pluck_resonant, pluck_muted, pluck_funky, pluck_elastic, pluck_bell, pluck_percussive
- **Reese** (6): reese_classic, reese_deep, reese_aggressive, reese_smooth, reese_evolving, reese_distorted
- **Wobble** (6): wobble_classic, wobble_aggressive, wobble_smooth, wobble_squelch, wobble_deep, wobble_fm
- **FM** (5): fm_bass_classic, fm_bass_deep, fm_bass_metallic, fm_bass_evolving, fm_bass_aggressive

## Import-Pfade

```rhai
import "stdlib/drums/kicks/kick_808.vibe";
import "stdlib/bass/acid/acid_303_classic.vibe";
import "stdlib/leads/synth/lead_saw.vibe";
import "stdlib/pads/ambient/pad_warm.vibe";
import "stdlib/effects/reverbs/reverb.vibe";
```

## Genre-Eignung

- **Hip-Hop/Trap**: 808/909 Drums, Trap-Kick, Sub-Bass, Lofi-Sounds
- **Techno/House**: 909 Drums, Acid Bass, Synth-Leads, Cymbals
- **Drum & Bass**: DnB-Kick, Reese Bass, schnelle Hihats
- **Dubstep/Bass**: Sub-Kicks, Wobble Bass, Sub Drops, Impacts
- **Ambient**: Pads, Textures, Drones, weiche Sounds
- **Trance**: Supersaw-Leads, Stabs, Pitch-Risers

## Vokabular

- **stdlib** = Standard-Bibliothek (Verzeichnis mit .vibe-Dateien)
- **Synthdef** = Klangdefinition (wird durch Import registriert)
- **.vibe-Datei** = Quelldatei mit Synthdef-Code (editierbar)
