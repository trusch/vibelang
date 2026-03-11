---
title: SFZ Instrument
id: sfz-instrument
status: open
tags:
- concept
labels:
  area: core
  topic: sfz
created: 2026-03-11T08:35:45.494144695+01:00
updated: 2026-03-11T08:35:45.494144695+01:00
---

# SFZ Instrument

VibeLang supports **SFZ instruments** — sample-based sounds using the open SFZ format.

## Creation

```rhai
let piano = sfz_voice("piano", "path/to/piano.sfz").gain(db(-6));
```

## How It Works

1. SFZ file defines regions (note-to-sample mappings)
2. On note trigger, the matching sample is selected
3. Playback rate is calculated from `pitch_keycenter`

## Known Issues

**pitch_keycenter**: Samples need `pitch_keycenter` for correct pitch. Without it, the system assumes the sample was recorded at the played note.

**NOTE_OFF (critical)**: SFZ voices use NOTE_ON/NOTE_OFF (MIDI-style), but melodies don't automatically send NOTE_OFF. The envelope stays in sustain while PlayBuf has finished. **Workaround**: use `.note_off(note)` manually or use regular (non-SFZ) voices.

## Playback Rate

```
rate = midi_to_freq(target_note) / midi_to_freq(pitch_keycenter)
```
