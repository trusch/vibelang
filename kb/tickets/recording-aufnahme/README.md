---
title: "Recording (Aufnahme)"
id: recording-aufnahme
status: open
tags: [konzept, workflow, audio]
labels:
  kategorie: kern
  bereich: workflow
priority: low
created: 2026-03-11T08:20:00+01:00
updated: 2026-03-11T08:20:00+01:00
---

# Recording (Aufnahme)

VibeLang kann Audio-Ausgabe in Dateien aufnehmen.

## Status

Recording-API ist in `vibelang-rhai/src/api/recording.rs` implementiert (nur native, nicht WASM).

## Ausgabeformate

- WAV (Standard)
- Weitere Formate über externe Tools (ffmpeg)

## Vokabular

- **Recording** = Aufnahme der Audio-Ausgabe in Datei
- **WAV** = Unkomprimiertes Audioformat
