---
title: Recording
id: recording
status: open
tags:
- concept
labels:
  area: workflow
  topic: recording
created: 2026-03-11T08:36:07.920062204+01:00
updated: 2026-03-11T08:36:07.920062204+01:00
---

# Recording

VibeLang can record audio output to files.

## Format

- WAV (default)
- Other formats via external tools (ffmpeg)

## Implementation

Recording API is in `vibelang-rhai/src/api/recording.rs` (native only, not WASM).
