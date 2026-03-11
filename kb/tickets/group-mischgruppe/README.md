---
title: "Group (Mischgruppe)"
id: group-mischgruppe
status: open
tags: [konzept, audio, mixing]
labels:
  kategorie: kern
  bereich: mixing
priority: high
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Group (Mischgruppe)

Eine **Group** fasst mehrere Voices, Patterns und Melodies zu einer logischen Einheit zusammen — vergleichbar mit einem Bus/Submix in einer DAW. Groups ermöglichen gemeinsame Effekte und Lautstärkekontrolle.

## Erzeugung

```rhai
define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));

    pattern("kick").on(kick).step("x... x... x... x...").start();
    pattern("snare").on(snare).step(".... x... .... x...").start();

    // Effekte auf die gesamte Gruppe anwenden
    fx("drum_reverb").synth("reverb").param("room", 0.2).param("mix", 0.15).apply();
});
```

## Group-Steuerung

```rhai
let drums = get_group("Drums");
drums.gain(db(-3));           // Gruppen-Lautstärke ändern
drums.mute();                 // Gesamte Gruppe stummschalten
drums.unmute();               // Stummschaltung aufheben
drums.solo(true);             // Gruppe solo (alle anderen stumm)
```

## Audio-Routing

- Bus 0: Haupt-Audioausgang (JACK/Hardware)
- Busse 16+: Gruppen-Busse für Submixing
- `system_link_audio` Synthdef routet Gruppen-Busse zum Haupt-Ausgang
- Link-Synths werden automatisch nach Script-Ausführung erstellt (via `FinalizeGroups`)

## Beziehungen

- Enthält **Voices**, **Patterns**, **Melodies**
- Kann **Effects** über `fx()` auf den Gruppen-Bus anwenden
- Wird von **Fade** über `.on_group("name")` automatisiert
- Kann von **Sequence** gesteuert werden

## Vokabular

- **Group** = Mischgruppe / Submix-Bus
- **Bus** = Audio-Kanal für Routing
- **Solo** = Nur diese Gruppe hörbar
- **Mute** = Stummschalten
