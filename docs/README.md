# VibeLang documentation

This is the source-backed reference for the user-facing VibeLang language and
tools. It describes what the current registration, dispatch, and protocol code
actually exposes, including known no-ops and feature gates. VibeLang is alpha;
when prose and source disagree, follow the source pointers on each page.

## Choose the right surface

| Surface | Use it for | Start here |
|---|---|---|
| Rhai / `.vibe` authoring API | Songs, live coding, custom synthdefs, MIDI mappings | [Authoring reference](reference/README.md) |
| Standard library | Importable instruments, effects, processors, and theory helpers | [Generated stdlib index](reference/generated/stdlib.md) |
| Command-line and startup configuration | Running, rendering, LSP, hardware profiles, `vibe-keys` | [CLI and configuration](interfaces/cli-and-config.md) |
| REST and WebSocket | External controllers, editor integrations, monitoring | [HTTP and WebSocket](interfaces/http-and-websocket.md) |
| Browser / WASM | Embedding VibeLang in a web application | [WASM](interfaces/wasm.md) |
| LSP and editors | IDE capabilities, commands, settings, keymaps | [LSP and editors](interfaces/lsp-and-editors.md) |
| Rust crates | Embedding or contributing in Rust | [docs.rs](https://docs.rs/vibelang-core) and crate source; this is not the `.vibe` API |

The `.vibe` type system, HTTP DTOs, WebSocket messages, WASM classes, editor
commands, and Rust crate APIs are separate contracts. Similar names do not make
them interchangeable.

## Authoring reference

- [Reference index and availability](reference/README.md)
- [Execution, identity, hot reload, and state lifecycle](reference/runtime-model.md)
- [Globals, helpers, transport, and assertions](reference/globals.md)
- [Runtime objects and routing](reference/runtime-objects.md)
- [DSP graph, envelopes, synthdefs, and effects](reference/dsp.md)
- [Generated UGen index](reference/generated/ugens.md)
- [Generated standard-library index](reference/generated/stdlib.md)
- [MIDI authoring API](reference/midi.md)
- [Optional filesystem, process, and network extensions](reference/extensions.md)

## Tool and protocol reference

- [CLI and configuration](interfaces/cli-and-config.md)
- [HTTP and WebSocket](interfaces/http-and-websocket.md)
- [WASM](interfaces/wasm.md)
- [LSP, VS Code, and Emacs](interfaces/lsp-and-editors.md)

## Design and maintenance

- [API improvement roadmap](roadmap/api-improvement-roadmap.md)

The generated UGen and standard-library pages deliberately trade handwritten
prose for exhaustive, source-linked discovery. Their generation contract and
the plan for making all other reference data share one manifest are documented
in the [roadmap](roadmap/api-improvement-roadmap.md#publication-and-generation-order).

## Availability at a glance

| Capability | Native default build | Feature-minimal native | WASM |
|---|---:|---:|---:|
| Core globals and runtime objects | Yes | Yes | Yes |
| DSP graph and generated UGens | Yes | Yes | Yes, subject to browser backend/plugin availability |
| SFZ and native recording | Yes | Yes | No |
| MIDI authoring | With `midi` feature | No | No hardware MIDI |
| Filesystem / exec / network extensions | Compiled and locally enabled by CLI defaults | Feature-dependent | No native extensions |
| HTTP API / WebSocket | With `api` feature; CLI enables by default | No | Browser consumer only |
| LSP command | With `lsp` feature | No | No |

Local `vibe run` enables each compiled extension unless disabled. Code sent to
`POST /eval` remains extension-free unless the CLI was started with
`--api-allow-extensions`. See [Extensions](reference/extensions.md) before
exposing an API server.

## Reference conventions

- `Int`, `Float`, `String`, `Bool`, `Array`, `Map`, `Dynamic`, and `FnPtr` are
  Rhai values. The locked runtime is Rhai 1.23.6.
- A signature joined with `|` represents exact overloads, not coercion unless
  the entry says otherwise.
- “Chain” means the call returns the receiver or a new handle suitable for the
  next fluent call. “Unit” means there is no useful return value.
- “Snapshot” means the declaration is written into the script state consumed
  by reload reconciliation. It does not mean the audio runtime has already
  completed the change.
- Source links point to registration and implementation roots. Generated pages
  additionally link every catalogued manifest or `.vibe` source file.
