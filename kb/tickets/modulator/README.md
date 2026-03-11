---
title: "Modulator (LFO & Automation)"
id: modulator
status: open
tags: [konzept, audio, automation]
labels:
  kategorie: kern
  bereich: automation
priority: medium
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Modulator (LFO & Automation)

**Modulatoren** erzeugen sich wiederholende Steuersignale (LFOs) die Parameter von Voices, Groups oder Effects kontinuierlich verändern. Im Gegensatz zu Fades (einmalige Rampe) sind Modulatoren zyklisch.

## API

Die Modulator-API ist in `vibelang-rhai/src/api/modulator.rs` implementiert.

## Typische Anwendungen

- **Vibrato**: LFO auf Voice-Frequenz
- **Tremolo**: LFO auf Voice-Amplitude
- **Filter-Wobble**: LFO auf Cutoff-Frequenz
- **Auto-Pan**: LFO auf Stereo-Position

## In Custom Synthdefs

LFOs werden direkt als UGen eingebaut:

```rhai
define_synthdef("wobble_bass")
    .param("freq", 55.0).param("amp", 0.5).param("gate", 1.0)
    .body(|freq, amp, gate| {
        let env = envelope().adsr("5ms", "100ms", 0.8, "200ms")
            .gate(gate).cleanup_on_finish().build();
        let lfo = sin_osc_kr(2.0);                    // 2 Hz LFO
        let cutoff = 800.0 + (lfo * 600.0);            // Cutoff: 200-1400 Hz
        let osc = saw_ar(freq);
        rlpf_ar(osc, cutoff, 0.3) * env * amp
    });
```

## Abgrenzung

| Konzept | Verhalten | Verwendung |
|---------|-----------|------------|
| **Fade** | Einmalige Rampe (A→B) | Build-ups, Übergänge |
| **Modulator** | Zyklisch (LFO) | Vibrato, Wobble, Tremolo |
| **Automation in Sequence** | Zeitgesteuert | Song-Arrangement |

## Vokabular

- **Modulator** = Zyklischer Steuersignal-Generator
- **LFO** = Low Frequency Oscillator (langsamer Oszillator, typisch 0.1-20 Hz)
- **Vibrato** = Frequenzmodulation durch LFO
- **Tremolo** = Amplitudenmodulation durch LFO
- **Wobble** = Filter-Cutoff-Modulation durch LFO (typisch für Dubstep)
