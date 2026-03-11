---
title: "UGen-Referenz (Unit Generators)"
id: ugen-referenz
status: open
tags: [referenz, synthese, audio]
labels:
  kategorie: referenz
  bereich: klang
priority: medium
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# UGen-Referenz (Unit Generators)

**UGens** (Unit Generators) sind die Bausteine für Custom Synthdefs. Sie erzeugen oder verarbeiten Audiosignale. Suffix `_ar` = Audiorate, `_kr` = Kontrollrate.

## Oszillatoren (Audiorate)

| UGen | Beschreibung | Parameter |
|------|-------------|-----------|
| `sin_osc_ar(freq)` | Sinuswelle | freq: Hz |
| `saw_ar(freq)` | Sägezahnwelle | freq: Hz |
| `pulse_ar(freq, width)` | Pulswelle | freq: Hz, width: 0.0-1.0 |
| `lf_tri_ar(freq)` | Dreieckswelle | freq: Hz |
| `white_noise_ar()` | Weißes Rauschen | — |
| `pink_noise_ar()` | Rosa Rauschen | — |

## Oszillatoren (Kontrollrate)

| UGen | Beschreibung | Parameter |
|------|-------------|-----------|
| `sin_osc_kr(freq)` | Sinus-LFO | freq: Hz |

## Filter

| UGen | Beschreibung | Parameter |
|------|-------------|-----------|
| `lpf_ar(input, freq)` | Tiefpassfilter | input: Signal, freq: Grenzfrequenz |
| `hpf_ar(input, freq)` | Hochpassfilter | input: Signal, freq: Grenzfrequenz |
| `rlpf_ar(input, freq, res)` | Resonanter Tiefpass | input, freq, res: Resonanz 0.0-1.0 |
| `rhpf_ar(input, freq, res)` | Resonanter Hochpass | input, freq, res: Resonanz 0.0-1.0 |

## Hüllkurven (Envelope)

```rhai
// ADSR (Attack-Decay-Sustain-Release)
let env = envelope()
    .adsr("10ms", "100ms", 0.7, "200ms")
    .gate(gate)
    .cleanup_on_finish()
    .build();

// ASR (Attack-Sustain-Release)
let env = envelope()
    .asr("15ms", 0.7, "100ms")
    .gate(gate)
    .cleanup_on_finish()
    .build();

// Perkussiv (nur Attack-Release, kein Sustain)
let env = envelope()
    .perc("1ms", "50ms")
    .gate(gate)
    .cleanup_on_finish()
    .build();
```

Zeitangaben als Strings: `"10ms"`, `"1s"`, `"500ms"`

## Signalverknüpfung

UGens werden mit arithmetischen Operatoren kombiniert:

```rhai
let osc = saw_ar(freq) + saw_ar(freq * 1.01);   // Addieren (Layering)
let mixed = osc * 0.5;                            // Skalieren (Lautstärke)
let filtered = rlpf_ar(osc, cutoff, 0.3);         // Filtern
let output = filtered * env * amp;                 // Hüllkurve anwenden
```

## Typisches Synthdef-Muster

```rhai
define_synthdef("beispiel")
    .param("freq", 440.0)
    .param("amp", 0.5)
    .param("gate", 1.0)
    .body(|freq, amp, gate| {
        let env = envelope().adsr("5ms", "100ms", 0.7, "200ms")
            .gate(gate).cleanup_on_finish().build();
        let osc = saw_ar(freq);
        let filtered = rlpf_ar(osc, 1200.0, 0.3);
        filtered * env * amp
    });
```

## Vokabular

- **UGen** = Unit Generator (Signalbaustein)
- **Audiorate (_ar)** = Berechnung pro Audio-Sample (44100/s)
- **Kontrollrate (_kr)** = Berechnung pro Kontrollblock (langsamer, für LFOs)
- **LFO** = Low Frequency Oscillator (langsamer Oszillator für Modulation)
- **Resonanz** = Verstärkung an der Grenzfrequenz eines Filters
