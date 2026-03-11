---
title: "Melody (Notensequenz)"
id: melody-notensequenz
status: open
tags: [konzept, audio, sequencing]
labels:
  kategorie: kern
  bereich: melodie
priority: high
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Melody (Notensequenz)

Eine **Melody** definiert eine Abfolge von Noten mit Tonhöhe und Dauer. Im Gegensatz zu einem Pattern (das nur rhythmisch triggert) bestimmt eine Melody _welche_ Noten gespielt werden.

## Erzeugung

```rhai
melody("bassline")
    .on(bass_voice)                                        // Voice zuweisen
    .notes("C2 - - . | E2 - G2 . | A2 - - - | G2 . E2 .") // Noten-Notation
    .start();                                               // Sofort starten
```

## Methoden

| Methode | Beschreibung |
|---------|-------------|
| `melody("name")` | Erzeugt eine neue Melody mit eindeutigem Namen |
| `.on(voice)` | Weist eine Voice zu |
| `.notes("notation")` | Definiert die Notenfolge in Noten-Notation |
| `.start()` | Startet die Melody sofort (Loop) |

## Noten-Notation

| Zeichen | Bedeutung |
|---------|-----------|
| `C4`, `D#3`, `Bb2` | Notenname mit Oktave (# = Kreuz, b = Be) |
| `.` | Pause (Stille) |
| `-` | Halte die vorherige Note |
| `\|` | Taktstrich (visuell, wird ignoriert) |
| Leerzeichen | Trennung zwischen Tokens |

## Akkorde

Für Akkorde: mehrere Melodies auf einer polyphonen Voice stapeln:

```rhai
let pad = voice("pad").synth("pad_warm").gain(db(-16)).poly(8);

melody("chord_root").on(pad).notes("C3 - - - - - - -").start();
melody("chord_third").on(pad).notes("E3 - - - - - - -").start();
melody("chord_fifth").on(pad).notes("G3 - - - - - - -").start();
```

## Verwendung in Sequences

Melodies ohne `.start()` können in einer **Sequence** zeitlich platziert werden:

```rhai
let bass_main = melody("bass_main").on(bass)
    .notes("C2 . . . | C2 . E2 . | F2 . . . | G2 . F2 E2");
// kein .start() → wird von Sequence gesteuert
```

## Vokabular

- **Melody** = Notensequenz (Tonhöhe + Rhythmus)
- **Noten-Notation** = Zeichenkette aus Notennamen, -, . und |
- **Hold (-)** = Vorherige Note halten (verlängern)
- **Rest (.)** = Pause
