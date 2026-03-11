---
title: "CLI-Befehle (vibe)"
id: cli-befehle
status: open
tags: [referenz, cli]
labels:
  kategorie: referenz
  bereich: workflow
priority: medium
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# CLI-Befehle (vibe)

Das `vibe` Kommandozeilen-Tool ist der Einstiegspunkt für VibeLang.

## Installation

```bash
cargo install vibelang-cli
```

## Befehle

```bash
vibe my_song.vibe                  # Song abspielen (mit Hot-Reload)
vibe run my_song.vibe -w           # Explizit mit Watch-Modus
vibe my_song.vibe -I ../lib        # Zusätzlicher Import-Suchpfad
vibe --help                         # Hilfe anzeigen
```

## Voraussetzungen

- **SuperCollider** installiert und gestartet
- **JACK Audio** aktiv (Linux/Mac)
- Rust Toolchain (für Installation via cargo)

## Debugging

```bash
RUST_LOG=debug vibe my_song.vibe    # Ausführliche Logs
RUST_LOG=info vibe my_song.vibe     # Info-Level Logs
```

## Beispiele abspielen

```bash
cd examples/sfz
../../target/release/vibelang run -w -I ../.. example.vibe
```

## Vokabular

- **vibe** = CLI-Befehl (Kurzform von vibelang-cli)
- **Watch-Modus (-w)** = Datei auf Änderungen überwachen
- **Import-Pfad (-I)** = Zusätzliches Verzeichnis für Import-Suche
- **RUST_LOG** = Umgebungsvariable für Log-Level
