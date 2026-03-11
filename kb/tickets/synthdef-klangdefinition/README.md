---
title: "Synthdef (Klangdefinition)"
id: synthdef-klangdefinition
status: open
tags: [konzept, audio, synthese]
labels:
  kategorie: kern
  bereich: klang
priority: high
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Synthdef (Klangdefinition)

Ein **Synthdef** (Synthesizer-Definition) beschreibt _wie_ ein Klang erzeugt wird. Synthdefs sind wiederverwendbare Klangbaupläne, die von Voices instanziiert werden.

## Quellen

1. **Standard-Bibliothek** — importiert aus `stdlib/`
2. **Custom Synthdef** — selbst definiert mit `define_synthdef()`
3. **SFZ-Instrumente** — Sample-basiert via `.sfz` Dateien

## Import aus Standard-Bibliothek

```rhai
import "stdlib/drums/kicks/kick_808.vibe";     // Registriert Synthdef "kick_808"
import "stdlib/bass/acid/acid_303_classic.vibe"; // Registriert Synthdef "acid_303_classic"
```

## Custom Synthdef

```rhai
define_synthdef("my_bass")
    .param("freq", 110.0)          // Parameter mit Standardwert
    .param("amp", 0.5)
    .param("gate", 1.0)            // Gate für Hüllkurve
    .param("cutoff", 800.0)        // Eigener Parameter
    .body(|freq, amp, gate, cutoff| {
        let env = envelope()
            .adsr("10ms", "100ms", 0.7, "200ms")
            .gate(gate)
            .cleanup_on_finish()
            .build();

        let osc = saw_ar(freq) + saw_ar(freq * 1.01);  // Zwei verstimmte Sägezähne
        let filtered = rlpf_ar(osc, cutoff, 0.3);       // Resonanter Tiefpass
        filtered * env * amp
    });
```

## Standard-Parameter

Jeder Synthdef sollte diese Parameter definieren:

| Parameter | Typ | Beschreibung |
|-----------|-----|-------------|
| `freq` | float | Grundfrequenz in Hz |
| `amp` | float | Amplitude (0.0 - 1.0) |
| `gate` | float | Gate-Signal für Hüllkurve (1.0 = an, 0.0 = aus) |

Zusätzliche Parameter (z.B. `cutoff`, `resonance`) sind frei wählbar.

## Hüllkurven-Typen (Envelope)

```rhai
envelope().adsr("10ms", "100ms", 0.7, "200ms")  // Attack-Decay-Sustain-Release
envelope().asr("15ms", 0.7, "100ms")             // Attack-Sustain-Release
envelope().perc("1ms", "50ms")                   // Perkussiv (Attack-Release)
envelope().attack("5ms").release("200ms")         // Einfach Attack-Release
```

Wichtig: `.gate(gate).cleanup_on_finish().build()` am Ende der Hüllkurve!

## Beziehungen

- Wird von **Voice** über `.synth("name")` referenziert
- Nutzt **UGens** (Oszillatoren, Filter) für Klangerzeugung
- Wird bei `import` einer `.vibe`-Datei automatisch registriert

## Vokabular

- **Synthdef** = Klangdefinition / Klangbauplan
- **UGen** = Unit Generator (Oszillator, Filter, etc.)
- **Hüllkurve (Envelope)** = Lautstärkeverlauf über Zeit
- **Gate** = Steuersignal (Note an/aus)
- **ADSR** = Attack, Decay, Sustain, Release (Hüllkurven-Phasen)
