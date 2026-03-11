---
title: "Sequence (Arrangement)"
id: sequence-arrangement
status: open
tags: [konzept, audio, sequencing]
labels:
  kategorie: kern
  bereich: arrangement
priority: high
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Sequence (Arrangement)

Eine **Sequence** ordnet Patterns, Melodies und Fades zeitlich an — vergleichbar mit der Arrangement-Ansicht einer DAW. Sequences ermöglichen Song-Strukturen mit Intro, Drop, Breakdown etc.

## Erzeugung

```rhai
sequence("drum_arrangement")
    .loop_bars(32)                              // Gesamtlänge in Takten (Loop)
    .clip(0..bars(8), hihat_basic)              // Intro: nur Hihats
    .clip(bars(8)..bars(16), kick_basic)        // Build: Kick dazu
    .clip(bars(16)..bars(24), kick_busy)        // Drop: volle Drums
    .clip(bars(16)..bars(24), snare_basic)
    .start();
```

## Methoden

| Methode | Beschreibung |
|---------|-------------|
| `sequence("name")` | Erzeugt eine neue Sequence |
| `.loop_bars(n)` | Setzt Gesamtlänge in Takten (Loop-Punkt) |
| `.clip(range, element)` | Platziert Pattern/Melody/Fade in einem Zeitbereich |
| `.start()` | Startet die Sequence |

## Clip-Bereiche

Bereiche werden mit der `bars()` Hilfsfunktion und dem Range-Operator `..` definiert:

```rhai
0..bars(8)              // Takt 0 bis 8 (in Beats umgerechnet)
bars(8)..bars(16)       // Takt 8 bis 16
bars(16)..bars(32)      // Takt 16 bis 32
```

## Wichtig: .start() vs. Sequence

- Pattern/Melody mit `.start()` → spielt sofort und loopt unendlich
- Pattern/Melody ohne `.start()` → wird von Sequence gesteuert
- Nie beides gleichzeitig verwenden!

## Typische Song-Struktur

```rhai
sequence("arrangement")
    .loop_bars(64)
    .clip(0..bars(8), intro_elements)           // Intro
    .clip(bars(8)..bars(16), build_elements)    // Build
    .clip(bars(16)..bars(32), full_elements)    // Drop
    .clip(bars(32)..bars(40), breakdown)        // Breakdown
    .clip(bars(40)..bars(48), build2)           // Build 2
    .clip(bars(48)..bars(60), drop2)            // Drop 2
    .clip(bars(60)..bars(64), outro)            // Outro
    .start();
```

## Beziehungen

- Steuert **Patterns**, **Melodies** und **Fades** zeitlich
- Nutzt `bars()` **Hilfsfunktion** für Zeitangaben
- Mehrere Clips können sich zeitlich überlappen (Layering)

## Vokabular

- **Sequence** = Zeitliche Anordnung (Arrangement)
- **Clip** = Platzierter Inhalt in einem Zeitbereich
- **bars(n)** = Hilfsfunktion: wandelt Takte in Beats um
- **loop_bars** = Gesamtlänge des Loops in Takten
