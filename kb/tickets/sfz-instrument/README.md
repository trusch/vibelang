---
title: "SFZ-Instrument (Sample-basiert)"
id: sfz-instrument
status: open
tags: [konzept, audio, sampling]
labels:
  kategorie: kern
  bereich: klang
priority: medium
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# SFZ-Instrument (Sample-basiert)

VibeLang unterstützt **SFZ-Instrumente** — sample-basierte Klänge, die aus aufgenommenen Audio-Dateien bestehen. SFZ ist ein offener Standard für Sampler.

## Erzeugung

```rhai
let piano = sfz_voice("piano", "path/to/piano.sfz").gain(db(-6));
```

## Funktionsweise

1. SFZ-Datei definiert Regionen (Zuordnung Note → Sample)
2. Beim Spielen einer Note wird das passende Sample ausgewählt
3. Playback-Rate wird basierend auf `pitch_keycenter` berechnet

## Bekannte Einschränkungen

### pitch_keycenter
- SFZ-Samples brauchen `pitch_keycenter` für korrekte Tonhöhe
- Ohne `pitch_keycenter` wird angenommen, dass das Sample auf der gespielten Note aufgenommen wurde
- Verbesserung geplant: Tonhöhe aus Dateinamen ableiten (z.B. "c5_p_rr4.wav" → 72)

### NOTE_OFF Problem (kritisch)
- SFZ-Voices nutzen NOTE_ON/NOTE_OFF (MIDI-Stil)
- Melodies senden aktuell kein automatisches NOTE_OFF
- Hüllkurve bleibt im Sustain hängen, PlayBuf ist bereits fertig
- **Workaround**: `.note_off(note)` manuell aufrufen oder normale Voices verwenden

## Playback-Rate Berechnung

```
target_freq = midi_to_freq(note)
sample_freq = midi_to_freq(pitch_keycenter)
rate = target_freq / sample_freq
```

## Vokabular

- **SFZ** = Offenes Sample-Format (Text-Datei die Samples referenziert)
- **Region** = Zuordnung eines Notenbereichs zu einem Sample
- **pitch_keycenter** = MIDI-Note auf der das Sample aufgenommen wurde
- **PlayBuf** = SuperCollider UGen der Samples abspielt
- **NOTE_ON / NOTE_OFF** = MIDI-Nachrichten zum Starten/Stoppen einer Note
