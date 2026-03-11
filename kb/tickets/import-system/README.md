---
title: "Import-System"
id: import-system
status: open
tags: [konzept, sprache]
labels:
  kategorie: kern
  bereich: sprache
priority: medium
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Import-System

VibeLang nutzt `import` um Synthdefs und Bibliotheken zu laden. Importierte `.vibe`-Dateien registrieren ihre Synthdefs automatisch.

## Syntax

```rhai
import "stdlib/drums/kicks/kick_808.vibe";          // Standard-Bibliothek
import "stdlib/effects/reverbs/reverb.vibe";         // Effekt-Synthdef
import "my_sounds/custom_bass.vibe";                  // Eigene Datei (relativ)
```

## Pfad-Auflösung

- `stdlib/...` → Standard-Bibliothek (mitgeliefert)
- Relative Pfade → relativ zum aktuellen `.vibe`-Skript
- `-I <pfad>` CLI-Flag → zusätzlicher Suchpfad

## Was passiert beim Import?

1. Die `.vibe`-Datei wird geladen und ausgeführt
2. Darin definierte Synthdefs werden im globalen Registry registriert
3. Der Synthdef-Name kann dann in `voice().synth("name")` verwendet werden

## CLI-Suchpfade

```bash
vibe my_song.vibe -I ../shared_sounds -I ~/my_library
```

## Vokabular

- **import** = Lädt eine .vibe-Datei und registriert deren Synthdefs
- **stdlib** = Standard-Bibliothek (mitgelieferter Klangvorrat)
- **Registry** = Globales Verzeichnis aller registrierten Synthdefs
