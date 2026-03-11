---
title: Audio Routing
id: audio-routing
status: open
tags:
- reference
labels:
  area: arch
  topic: routing
created: 2026-03-11T08:36:07.743996289+01:00
updated: 2026-03-11T08:36:07.743996289+01:00
---

# Audio Routing

## Bus System

- **Bus 0**: Main audio output (connected to JACK/hardware)
- **Buses 16+**: Group buses for submixing
- `system_link_audio` synthdef routes group buses to main output
- Link synths are created automatically via `FinalizeGroups` message after script execution

## Signal Flow

```
Voice → Group Bus (16+) → Effects on bus → Link Synth → Main Bus (0) → JACK Output
```

Groups that share a bus get mixed together before effects are applied.
