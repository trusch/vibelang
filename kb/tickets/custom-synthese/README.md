---
title: "Custom Synthese (Eigene Klänge)"
id: custom-synthese
status: open
tags: [konzept, synthese, audio]
labels:
  kategorie: anleitung
  bereich: klang
priority: medium
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Custom Synthese (Eigene Klänge)

VibeLang erlaubt es, eigene Klänge mit `define_synthdef()` zu erstellen. Custom Synthdefs verwenden UGens (Oszillatoren, Filter, Hüllkurven) als Bausteine.

## Grundstruktur

```rhai
define_synthdef("name")
    .param("freq", 440.0)       // Pflicht: Frequenz
    .param("amp", 0.5)          // Pflicht: Amplitude
    .param("gate", 1.0)         // Pflicht: Gate für Hüllkurve
    .param("cutoff", 1200.0)    // Optional: eigene Parameter
    .body(|freq, amp, gate, cutoff| {
        // Hüllkurve
        let env = envelope().adsr("5ms", "100ms", 0.7, "200ms")
            .gate(gate).cleanup_on_finish().build();

        // Klangerzeugung
        let osc = saw_ar(freq);

        // Klangformung
        let filtered = rlpf_ar(osc, cutoff, 0.3);

        // Ausgabe
        filtered * env * amp
    });
```

## Rezepte

### Fetter Bass (Detuned Saws)
```rhai
define_synthdef("fat_bass")
    .param("freq", 55.0).param("amp", 0.5).param("gate", 1.0)
    .body(|freq, amp, gate| {
        let env = envelope().adsr("5ms", "80ms", 0.8, "150ms")
            .gate(gate).cleanup_on_finish().build();
        let sub = sin_osc_ar(freq) * 0.6;
        let mid = saw_ar(freq * 2.0) * 0.3;
        let top = pulse_ar(freq * 4.0, 0.3) * 0.1;
        rlpf_ar(sub + mid + top, 600.0, 0.25) * env * amp
    });
```

### Lead mit Vibrato
```rhai
define_synthdef("vibrato_lead")
    .param("freq", 440.0).param("amp", 0.3).param("gate", 1.0)
    .body(|freq, amp, gate| {
        let env = envelope().adsr("50ms", "200ms", 0.8, "500ms")
            .gate(gate).cleanup_on_finish().build();
        let vibrato = sin_osc_kr(5.0) * 10.0;       // 5 Hz LFO, ±10 Hz
        let osc = saw_ar(freq + vibrato);
        rlpf_ar(osc, 2000.0, 0.3) * env * amp
    });
```

### Perkussiver Pluck
```rhai
define_synthdef("pluck")
    .param("freq", 440.0).param("amp", 0.4).param("gate", 1.0)
    .body(|freq, amp, gate| {
        let env = envelope().perc("1ms", "400ms")
            .gate(gate).cleanup_on_finish().build();
        let filter_env = envelope().perc("1ms", "150ms").gate(gate).build();
        let osc = saw_ar(freq) + saw_ar(freq * 1.005);
        let cutoff = 300.0 + (filter_env * 4000.0);
        rlpf_ar(osc, cutoff, 0.2) * env * amp * 0.5
    });
```

## Tipps

- Immer `freq`, `amp`, `gate` als Parameter definieren
- `.cleanup_on_finish()` sorgt dafür, dass der Synth nach Hüllkurven-Ende freigegeben wird
- Detuning: zweiten Oszillator leicht verstimmen (z.B. `freq * 1.01`)
- Sub-Oktave: `sin_osc_ar(freq * 0.5)` für tiefe Fülle
- Filter-Hüllkurve: separate Envelope für zeitabhängiges Cutoff

## Vokabular

- **Custom Synthdef** = Selbst definierter Klangbauplan
- **Detuning** = Leichte Verstimmung für Breite/Fülle
- **Filter-Envelope** = Hüllkurve die den Filter-Cutoff moduliert
- **Sub-Oktave** = Oszillator eine Oktave tiefer für Bass-Fundament
