---
title: "Voice (Instrument-Instanz)"
id: voice-instrument-instanz
status: open
tags: [konzept, audio]
labels:
  kategorie: kern
  bereich: instrument
priority: high
created: 2026-03-11T08:16:00+01:00
updated: 2026-03-11T08:16:00+01:00
---

# Voice (Instrument-Instanz)

Eine **Voice** ist eine benannte Instanz eines Synthesizers. Sie verbindet einen Synthdef (Klangerzeuger) mit Lautstärke- und Polyphonie-Einstellungen.

## Erzeugung

```rhai
let kick = voice("kick")        // Name (eindeutig)
    .synth("kick_808")          // Synthdef zuweisen
    .gain(db(-6))               // Lautstärke in dB
    .poly(4);                   // Polyphonie (Stimmen gleichzeitig)
```

## Methoden

| Methode | Beschreibung |
|---------|-------------|
| `voice("name")` | Erzeugt eine neue Voice mit eindeutigem Namen |
| `.synth("synthdef_name")` | Weist einen Synthdef zu |
| `.gain(db(n))` | Setzt die Lautstärke (in Dezibel) |
| `.poly(n)` | Setzt die Polyphonie (1 = monophon) |
| `.set_param("key", value)` | Setzt einen Synthdef-Parameter |
| `.mute()` / `.unmute()` | Stummschalten / Lautschalten |

## Beziehungen

- Wird einem **Pattern** oder einer **Melody** über `.on(voice)` zugewiesen
- Nutzt einen **Synthdef** als Klangquelle
- Gehört optional zu einer **Group** (Mischgruppe)
- Empfängt **MIDI-Eingabe** über `midi.keyboard().to(voice)`

## Beispiel

```rhai
import "stdlib/bass/acid/acid_303_classic.vibe";

let bass = voice("bass")
    .synth("acid_303_classic")
    .gain(db(-10))
    .poly(1)                          // monophon
    .set_param("cutoff", 800.0);      // Filter-Cutoff setzen
```

## Vokabular

- **Voice** = Instrument-Instanz (nicht Synthdef!)
- **Synthdef** = Klangdefinition (wiederverwendbar)
- **poly(n)** = Polyphonie (Anzahl gleichzeitiger Noten)
- **gain** = Lautstärke (immer über `db()` Hilfsfunktion)
