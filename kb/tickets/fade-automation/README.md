---
title: "Fade (Automation)"
id: fade-automation
status: open
tags: [konzept, audio, automation]
labels:
  kategorie: kern
  bereich: automation
priority: medium
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Fade (Automation)

Ein **Fade** automatisiert Parameter über Zeit — z.B. Filter-Sweeps, Lautstärke-Rampen oder Effekt-Übergänge.

## Erzeugung

```rhai
let filter_sweep = fade("filter_sweep")
    .on_voice("bass")              // Ziel: Voice-Parameter
    .param("cutoff")               // Welcher Parameter
    .from(400.0)                   // Startwert
    .to(4000.0)                    // Endwert
    .over_bars(8)                  // Dauer in Takten
    .apply();
```

## Zieltypen

```rhai
// Voice-Parameter automatisieren
fade("name").on_voice("voice_name").param("cutoff")...

// Group-Parameter automatisieren
fade("name").on_group("Drums").param("amp")...

// Effect-Parameter automatisieren
fade("name").on_effect("reverb_name").param("mix")...
```

## Methoden

| Methode | Beschreibung |
|---------|-------------|
| `fade("name")` | Erzeugt einen neuen Fade |
| `.on_voice("name")` | Ziel: Voice-Parameter |
| `.on_group("name")` | Ziel: Group-Parameter |
| `.on_effect("name")` | Ziel: Effect-Parameter |
| `.param("key")` | Welcher Parameter automatisiert wird |
| `.from(value)` | Startwert |
| `.to(value)` | Endwert |
| `.over_bars(n)` | Dauer in Takten |
| `.apply()` | Fade aktivieren |

## Verwendung in Sequences

```rhai
let reverb_swell = fade("reverb_swell")
    .on_effect("lead_reverb")
    .param("mix")
    .from(0.0)
    .to(0.5)
    .over_bars(4)
    .apply();

sequence("lead_seq")
    .loop_bars(4)
    .clip(0..bars(4), lead_melody)
    .clip(0..bars(4), reverb_swell)    // Fade gleichzeitig abspielen
    .start();
```

## Vokabular

- **Fade** = Automatisierter Parameter-Übergang
- **Sweep** = Kontinuierliche Parameteränderung (z.B. Filter-Sweep)
- **Automation** = Zeitgesteuerte Parameteränderung
