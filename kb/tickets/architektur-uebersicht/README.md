---
title: "Architektur-Übersicht"
id: architektur-uebersicht
status: open
tags: [referenz, architektur]
labels:
  kategorie: referenz
  bereich: architektur
priority: high
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Architektur-Übersicht

VibeLang besteht aus mehreren Rust-Crates, die zusammen eine Musikprogrammiersprache bilden.

## Tech Stack

- **Sprache**: Rust (CLI, Core, LSP)
- **Scripting**: Rhai (eingebettete Skriptsprache für .vibe-Dateien)
- **Audio-Engine**: SuperCollider (professionelle Klangerzeugung)
- **Audio-Treiber**: JACK Audio (Linux/Mac)
- **Paket**: `cargo install vibelang-cli` (crates.io)

## Crate-Struktur

| Crate | Beschreibung |
|-------|-------------|
| `vibelang-cli` | Kommandozeilen-Tool (`vibe` Befehl) |
| `vibelang-core` | Kern-Logik, State Management, Audio-Graph |
| `vibelang-rhai` | Rhai API-Bindungen (alle .vibe Funktionen) |
| `vibelang-dsp` | DSP-Verarbeitung |
| `vibelang-std` | Standard-Bibliothek (187 Synthdefs) |
| `vibelang-sfz` | SFZ-Parser und Sample-Management |
| `vibelang-lsp` | Language Server Protocol (Editor-Integration) |
| `vibelang-keys` | Tastatur/Eingabe-Verarbeitung |
| `vibelang-http` | HTTP-Server (experimentell) |
| `vibelang-wasm` | WebAssembly-Target |

## Datenfluss

```
.vibe Datei
    → Rhai Parser (vibelang-rhai)
    → Messages (Voice/Pattern/Melody/Sequence)
    → State Manager (vibelang-core)
    → Reconciliation (Diff mit aktuellem Zustand)
    → SuperCollider OSC-Befehle
    → Audio-Ausgabe via JACK
```

## State Management

Der State Manager verwaltet den aktuellen Audio-Graphen:

1. Script wird ausgeführt → erzeugt Messages
2. Messages beschreiben den gewünschten Zustand
3. Reconciliation vergleicht gewünscht vs. aktuell
4. Nur Unterschiede werden an SuperCollider gesendet
5. Ermöglicht ~1ms Hot-Reload

## Editor-Integration

- **VSCode Extension** — Syntax-Highlighting, LSP
- **Emacs Mode** — in Entwicklung

## Web-Präsenz

- Website: vibelang.org
- Docs: vibelang.org/#/docs
- GitHub: trusch/vibelang
- crates.io: vibelang-cli

## Vokabular

- **Rhai** = Eingebettete Skriptsprache (interpretiert .vibe-Dateien)
- **SuperCollider** = Audio-Engine (Open-Source Synthesizer)
- **JACK** = Audio-Treiber (niedrige Latenz, professionell)
- **OSC** = Open Sound Control (Protokoll für SuperCollider-Kommunikation)
- **Reconciliation** = Zustandsabgleich (nur Änderungen senden)
- **LSP** = Language Server Protocol (Editor-Unterstützung)
