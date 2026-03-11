---
title: Data Flow
id: data-flow
status: open
tags:
- reference
labels:
  area: arch
  topic: dataflow
created: 2026-03-11T08:36:07.625323255+01:00
updated: 2026-03-11T08:36:07.625323255+01:00
---

# Data Flow

## Pipeline

```
.vibe file
  → Rhai parser (vibelang-rhai)
  → Messages (Voice/Pattern/Melody/Sequence definitions)
  → State Manager (vibelang-core)
  → Reconciliation (diff current vs desired state)
  → SuperCollider OSC commands
  → Audio output via JACK
```

## Reconciliation

On each reload:
1. Script executes → produces desired state
2. State manager compares desired vs current
3. Only differences are sent to SuperCollider
4. Enables ~1ms hot reload
