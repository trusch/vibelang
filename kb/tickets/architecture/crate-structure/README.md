---
title: Crate Structure
id: crate-structure
status: open
tags:
- reference
labels:
  area: arch
  topic: crates
created: 2026-03-11T08:36:07.567489351+01:00
updated: 2026-03-11T08:36:07.567489351+01:00
---

# Crate Structure

VibeLang is a Rust workspace with these crates:

| Crate | Purpose |
|-------|---------|
| `vibelang-cli` | CLI tool (`vibe` command) |
| `vibelang-core` | Core logic, state management, audio graph |
| `vibelang-rhai` | Rhai API bindings (all .vibe functions) |
| `vibelang-dsp` | DSP processing |
| `vibelang-std` | Standard library (187 synthdefs) |
| `vibelang-sfz` | SFZ parser and sample management |
| `vibelang-lsp` | Language Server Protocol (editor integration) |
| `vibelang-keys` | Keyboard/input handling |
| `vibelang-http` | HTTP server (experimental) |
| `vibelang-wasm` | WebAssembly target |

## Key Source Paths

- `crates/vibelang-rhai/src/api/` — all API function implementations
- `crates/vibelang-std/stdlib/` — standard library .vibe files
- `crates/vibelang-core/src/` — state manager, handlers, runtime
