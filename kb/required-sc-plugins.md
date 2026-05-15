# Required SuperCollider Plugins

Date: 2026-05-15

This file tracks UGen manifest entries that are intentionally kept as literal
server UGens but require optional SuperCollider plugin packages.

| Plugin package | Manifest entries | Required by bundled stdlib | Notes |
|---|---|---|---|
| `mi-UGens` | `MiBraids`, `MiClouds`, `MiElements`, `MiGrids`, `MiMu`, `MiOmi`, `MiPlaits`, `MiRings`, `MiRipples`, `MiTides`, `MiVerb`, `MiWarps` | `MiClouds`, `MiPlaits`, `MiRings`, `MiTides` | Mutable Instruments ports. VibeLang does not provide native fallbacks; install the `mi-UGens` SuperCollider extension to use these synthdefs. |

The manifest marks these entries with `requires_plugin: "mi-UGens"` so startup
preflight can report the optional dependency instead of treating the missing
server UGen as an unexplained SynthDef rejection.
