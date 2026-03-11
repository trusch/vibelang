---
title: "MIDI-Integration"
id: midi-integration
status: open
tags: [konzept, midi, experimentell]
labels:
  kategorie: kern
  bereich: midi
priority: medium
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# MIDI-Integration

VibeLang unterstützt **MIDI-Eingabe** und **MIDI-Ausgabe** (experimentell). MIDI-Controller können Voices in Echtzeit spielen.

## MIDI-Eingabe

```rhai
let midi = midi_open("vibe");           // MIDI-Port öffnen
midi_monitor(true);                      // MIDI-Nachrichten in Konsole anzeigen
midi.keyboard().to(voice_object);        // Keyboard-Eingabe an Voice routen
```

## Status

- MIDI-Eingabe: experimentell
- MIDI-Ausgabe: geplant/in Entwicklung
- MIDI 2.0: in Entwicklung (siehe midi2-to-cv Projekt)

## Feature-Flag

MIDI ist hinter einem Cargo Feature-Flag:

```bash
cargo install vibelang-cli --features midi
```

## Vokabular

- **MIDI** = Musical Instrument Digital Interface
- **MIDI-Port** = Virtueller oder physischer MIDI-Anschluss
- **Controller** = Hardware-Eingabegerät (Keyboard, Pad-Controller)
- **NOTE_ON / NOTE_OFF** = MIDI-Nachrichten für Notenstart/-ende
