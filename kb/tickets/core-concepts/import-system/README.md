---
title: Import System
id: import-system
status: open
tags:
- concept
labels:
  area: core
  topic: import
created: 2026-03-11T08:35:45.429757230+01:00
updated: 2026-03-11T08:35:45.429757230+01:00
---

# Import System

VibeLang uses `import` to load synthdefs and libraries. Imported `.vibe` files register their synthdefs automatically.

## Syntax

```rhai
import "stdlib/drums/kicks/kick_808.vibe";     // standard library
import "my_sounds/custom_bass.vibe";            // relative path
```

## Path Resolution

- `stdlib/...` → standard library (bundled)
- Relative paths → relative to the current `.vibe` script
- `-I <path>` CLI flag → additional search path

## What Happens on Import

1. `.vibe` file is loaded and executed
2. Synthdefs defined inside are registered in the global registry
3. The synthdef name can then be used in `voice().synth("name")`
