---
title: "Pattern (Step-Sequencer)"
id: pattern-step-sequencer
status: open
tags: [konzept, audio, sequencing]
labels:
  kategorie: kern
  bereich: rhythmus
priority: high
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Pattern (Step-Sequencer)

Ein **Pattern** ist ein Step-Sequencer, der eine Voice rhythmisch triggert. Patterns definieren _wann_ ein Klang gespielt wird, nicht _welcher_ Klang.

## Erzeugung

```rhai
pattern("kick_pattern")
    .on(kick_voice)                        // Voice zuweisen
    .step("x... x... x..x ....")           // Step-Notation
    .start();                              // Sofort starten und loopen
```

## Methoden

| Methode | Beschreibung |
|---------|-------------|
| `pattern("name")` | Erzeugt ein neues Pattern mit eindeutigem Namen |
| `.on(voice)` | Weist eine Voice zu (welches Instrument getriggert wird) |
| `.step("notation")` | Definiert den Rhythmus in Step-Notation |
| `.euclid(hits, steps)` | Euklidischer Rhythmus statt Step-Notation |
| `.start()` | Startet das Pattern sofort (Loop) |

## Step-Notation

| Zeichen | Bedeutung |
|---------|-----------|
| `x` | Trigger (volle Velocity) |
| `.` | Pause (Stille) |
| `1`-`9` | Trigger mit Velocity-Stufe (1=leise, 9=laut) |
| Leerzeichen | Optionale Lesbarkeit (wird ignoriert) |

## Pattern-Länge

Die Anzahl der Tokens bestimmt die Auflösung:

- 16 Tokens = Sechzehntelnoten (1 Takt in 4/4)
- 8 Tokens = Achtelnoten (1 Takt in 4/4)
- 4 Tokens = Viertelnoten (1 Takt in 4/4)
- 32 Tokens = Zweiunddreißigstelnoten (1 Takt in 4/4)

## Euklidische Rhythmen

```rhai
pattern("afro").on(perc).euclid(5, 8).start();   // 5 Schläge auf 8 Steps
pattern("clave").on(clave).euclid(5, 16).start(); // 5 Schläge auf 16 Steps
```

## Verwendung in Sequences

Patterns ohne `.start()` können in einer **Sequence** zeitlich platziert werden:

```rhai
let kick_basic = pattern("kick_basic").on(kick).step("x... x... x... x...");
// kein .start() → wird von Sequence gesteuert
```

## Vokabular

- **Pattern** = Step-Sequencer (rhythmische Trigger)
- **Step-Notation** = Zeichenkette aus x, ., 1-9
- **Velocity** = Anschlagstärke (1-9 oder x=voll)
- **Euclidean** = Algorithmus zur gleichmäßigen Verteilung von Schlägen
