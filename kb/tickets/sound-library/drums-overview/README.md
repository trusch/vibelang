---
title: Drums Overview
id: drums-overview
status: open
tags:
- reference
labels:
  area: stdlib
  topic: drums
created: 2026-03-11T08:36:07.143382811+01:00
updated: 2026-03-11T08:36:07.143382811+01:00
---

# Drums Overview

62 drum synthdefs organized by type.

## Kicks (12)

`stdlib/drums/kicks/` — kick_808, kick_909, kick_techno_deep, kick_techno_hard, kick_dnb, kick_trap, kick_acoustic, kick_sub, kick_pitched, kick_fm, kick_distorted, kick_soft

## Snares (12)

`stdlib/drums/snares/` — snare_808, snare_909, snare_acoustic, snare_clap, snare_piccolo, snare_rimshot, snare_tight, snare_loose, snare_layered, snare_reverb, snare_filtered, snare_lofi

## Hi-hats (10)

`stdlib/drums/hihats/` — hihat_808_closed, hihat_808_open, hihat_909_closed, hihat_909_open, hihat_metallic, hihat_short, hihat_long, hihat_filtered, hihat_dusty, hihat_splash

## Claps (10)

`stdlib/drums/claps/` — clap_808, clap_short, clap_reverb, clap_layered, clap_crowd, clap_tight, clap_loose, clap_filtered, clap_lofi, handclap

## Percussion (10)

`stdlib/drums/percussion/` — tom_high, tom_mid, tom_low, rim, cowbell, clave, woodblock, shaker, tambourine, snap

## Cymbals (8)

`stdlib/drums/cymbals/` — crash_bright, crash_dark, ride, splash, china, crash_reverse, crash_swell, bell_ping

## Import Example

```rhai
import "stdlib/drums/kicks/kick_808.vibe";
let kick = voice("kick").synth("kick_808").gain(db(-6));
```
