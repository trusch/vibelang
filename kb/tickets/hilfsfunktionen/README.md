---
title: "Hilfsfunktionen (Helper Functions)"
id: hilfsfunktionen
status: open
tags: [referenz, api]
labels:
  kategorie: referenz
  bereich: api
priority: medium
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Hilfsfunktionen (Helper Functions)

VibeLang bietet zahlreiche Hilfsfunktionen für Musik, Mathematik und Konvertierung.

## Audio & Musik

| Funktion | Beschreibung | Beispiel |
|----------|-------------|---------|
| `db(n)` | Dezibel in linearen Gain umrechnen | `db(-6)` → 0.5 |
| `note("C4")` | Notenname in MIDI-Nummer | `note("C4")` → 60 |
| `chord("Cm")` | Akkord als MIDI-Array | `chord("Cm7")` |
| `scale("C", "minor")` | Skala als MIDI-Array | `scale("C", "dorian")` |
| `scale_degree("C", "minor", 3)` | Einzelner Skalenton | |
| `bars(n)` | Takte in Beats umrechnen | `bars(4)` → 16.0 (in 4/4) |
| `midi_to_freq(note)` | MIDI-Nummer in Hz | `midi_to_freq(69)` → 440.0 |
| `freq_to_midi(freq)` | Hz in MIDI-Nummer | `freq_to_midi(440.0)` → 69 |

## Globale Einstellungen

| Funktion | Beschreibung |
|----------|-------------|
| `set_tempo(bpm)` | Tempo setzen (BPM) |
| `get_tempo()` | Aktuelles Tempo abfragen |
| `set_time_signature(num, denom)` | Taktart setzen (z.B. 4, 4) |
| `set_quantization(beats)` | Quantisierung setzen |
| `get_quantization()` | Quantisierung abfragen |
| `get_current_bar()` | Aktuellen Takt abfragen |

## Mathematik & Zufall

| Funktion | Beschreibung |
|----------|-------------|
| `random()` | Zufallszahl 0.0 - 1.0 |
| `random_range(min, max)` | Zufallszahl im Bereich |
| `random_int(max)` | Zufällige Ganzzahl 0 bis max-1 |
| `random_choice(array)` | Zufälliges Element aus Array |
| `random_seed(n)` | Seed für Reproduzierbarkeit |
| `clamp(value, min, max)` | Wert begrenzen |
| `lerp(a, b, t)` | Lineare Interpolation |
| `map_range(v, in_min, in_max, out_min, out_max)` | Wert von einem Bereich in einen anderen abbilden |
| `smoothstep(edge0, edge1, x)` | Glatte Interpolation |
| `wrap(value, max)` | Wert umbrechen (Modulo) |
| `quantize(value, step)` | Wert auf Schrittraster runden |

## Array-Funktionen

| Funktion | Beschreibung |
|----------|-------------|
| `array_zip(a, b)` | Zwei Arrays verschränken |
| `array_shuffle(arr)` | Array zufällig mischen |
| `array_rotate(arr, n)` | Array rotieren |
| `array_reverse(arr)` | Array umkehren |
| `array_flatten(arr)` | Verschachteltes Array flach machen |
| `array_repeat(arr, n)` | Array n-mal wiederholen |
| `array_take(arr, n)` | Erste n Elemente |
| `array_skip(arr, n)` | Erste n Elemente überspringen |

## Konvertierung

| Funktion | Beschreibung |
|----------|-------------|
| `to_int(float)` | Float zu Integer |
| `to_float(int)` | Integer zu Float |
| `to_string(value)` | Wert zu String |
| `timestamp()` | Aktuelle Zeit (Sekunden) |
| `timestamp_ms()` | Aktuelle Zeit (Millisekunden) |

## Vokabular

- **db()** = Dezibel-Konvertierung (immer für Lautstärke verwenden)
- **bars()** = Takte-zu-Beats-Konvertierung
- **BPM** = Beats Per Minute (Schläge pro Minute)
- **MIDI-Nummer** = Ganzzahl-Repräsentation einer Note (60 = C4)
