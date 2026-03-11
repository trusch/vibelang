---
title: Hot Reload
id: hot-reload
status: open
tags:
- concept
labels:
  area: workflow
  topic: hotreload
created: 2026-03-11T08:36:07.799476867+01:00
updated: 2026-03-11T08:36:07.799476867+01:00
---

# Hot Reload

VibeLang supports **~1ms hot reload**: changes to `.vibe` files are heard instantly without interrupting audio.

## How It Works

1. Start `vibe my_song.vibe`
2. Edit the `.vibe` file in your editor
3. Save → changes are heard immediately
4. Errors don't kill the audio (last valid state continues)

## State Reconciliation

On reload, the audio graph is diffed:
- New voices/patterns are started
- Removed voices/patterns are stopped
- Changed parameters are updated
- Running patterns stay in sync
