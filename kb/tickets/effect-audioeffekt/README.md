---
title: "Effect (Audioeffekt)"
id: effect-audioeffekt
status: open
tags: [konzept, audio, effekt]
labels:
  kategorie: kern
  bereich: effekt
priority: high
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Effect (Audioeffekt)

**Effects** (Effekte) verarbeiten Audio auf einem Gruppen-Bus. Sie werden innerhalb einer Group mit `fx()` erstellt und nutzen Synthdefs aus der Standard-Bibliothek.

## Erzeugung

```rhai
fx("reverb_name")
    .synth("reverb")              // Effekt-Synthdef
    .param("room", 0.5)           // Parameter setzen
    .param("mix", 0.3)            // Wet/Dry-Mischung
    .apply();                     // Auf den Gruppen-Bus anwenden
```

## Verfügbare Effekt-Kategorien

### Reverb (Hall)
```rhai
import "stdlib/effects/reverbs/reverb.vibe";
import "stdlib/effects/reverbs/hall_reverb.vibe";
import "stdlib/effects/reverbs/plate_reverb.vibe";
import "stdlib/effects/reverbs/room_reverb.vibe";
import "stdlib/effects/reverbs/shimmer_reverb.vibe";
```
Parameter: `room` (Raumgröße), `mix` (Wet/Dry), `decay` (Nachhall)

### Delay (Verzögerung)
```rhai
import "stdlib/effects/delays/delay.vibe";
import "stdlib/effects/delays/ping_pong_delay.vibe";
import "stdlib/effects/delays/dub_delay.vibe";
```
Parameter: `time` (Verzögerungszeit in Sekunden), `feedback` (Rückkopplung), `mix`

### Filter
```rhai
import "stdlib/effects/filters/lowpass.vibe";
import "stdlib/effects/filters/moog_filter.vibe";
```
Parameter: `cutoff` (Grenzfrequenz in Hz), `resonance` (Resonanz)

### Dynamik
```rhai
import "stdlib/effects/dynamics/compressor.vibe";
import "stdlib/effects/dynamics/limiter.vibe";
```
Parameter: `threshold` (Schwelle, via `db()`), `ratio` (Verhältnis)

### Modulation
```rhai
import "stdlib/effects/modulation/chorus.vibe";
import "stdlib/effects/modulation/phaser.vibe";
```
Parameter: `rate` (LFO-Geschwindigkeit), `depth` (Tiefe), `mix`

### Verzerrung
```rhai
import "stdlib/effects/distortion/distortion.vibe";
import "stdlib/effects/distortion/overdrive.vibe";
import "stdlib/effects/distortion/bitcrush.vibe";
```
Parameter: `drive` (Verzerrungsgrad), `mix`, `bits` (bei Bitcrush)

## Beziehungen

- Wird innerhalb einer **Group** auf den Gruppen-Bus angewendet
- Nutzt **Synthdefs** als Effekt-Algorithmus
- Kann von **Fade** über `.on_effect("name")` automatisiert werden

## Vokabular

- **Effect / fx** = Audioeffekt auf dem Gruppen-Bus
- **Mix** = Wet/Dry-Mischung (0.0=trocken, 1.0=nur Effekt)
- **Reverb** = Hall
- **Delay** = Verzögerung / Echo
- **Cutoff** = Grenzfrequenz (Filter)
- **Feedback** = Rückkopplung
