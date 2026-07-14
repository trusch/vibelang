# Browser / WASM API

This is the JavaScript embedding contract exported by `vibelang-wasm`, not a
Rhai type. Authoritative source:
[`crates/vibelang-wasm/src/lib.rs`](../../crates/vibelang-wasm/src/lib.rs) and
the checked-in [`types/index.d.ts`](../../crates/vibelang-wasm/types/index.d.ts).

## Module exports

| Exact export | Return / purpose |
|---|---|
| default async initializer | Load/instantiate wasm-bindgen module |
| `initSync(...)` | Synchronous initializer |
| `init_panic_hook()` | Start hook installing browser panic diagnostics |
| `log(message: string)` | Browser console log |
| `version()` | Package version String |

## Result types

```ts
interface VibelangResult {
  success: boolean;
  error: string | null;
  groups: number;
  voices: number;
  patterns: number;
  melodies: number;
  tempo: number;
}

interface VibelangCompiledSynthdef {
  name: string;
  data: Uint8Array;
}
```

A script failure is returned as `VibelangResult`, not a rejected Promise.
Failure counts are zero and tempo is 120.

## VibelangRuntime

| Exact JavaScript member | Behavior |
|---|---|
| `new VibelangRuntime()` | Clears process-global user synthdef/effect registries |
| `async init(): Promise<void>` | Idempotently initializes WebScsynthBackend and loads builtins |
| `async execute(script: string): Promise<VibelangResult>` | Clears registries, executes Rhai, loads user synthdefs/effects through bridge when initialized, queues reload |
| `tick(): void` | Drives one runtime tick; embedder must call regularly |
| `async start(): Promise<void>`; `async stop()` | Queue transport without requiring initialization |
| `async stopAll(): Promise<void>` | Currently only calls stop; it does not free all synth nodes despite declaration comments |
| `isInitialized(): boolean` | Backend initialization flag |
| `static getSystemSynthdefs(): VibelangCompiledSynthdef[]` | System synthdefs including `system_link_audio` |
| `static parseNote(name: string): number` | MIDI note or -1 invalid |
| `static dbToAmp(db: number): number`; `ampToDb(amp: number)` | Conversion helpers |
| `free()`; `[Symbol.dispose]()` | wasm-bindgen resource release |

`execute` returns a successful result even if an individual synthdef bridge
load or state application fails; those failures only warn. It clears user
registries on every execution.

## Legacy VibelangEngine

`VibelangEngine` provides constructor, synchronous `execute(script)`,
`getSynthdefs()` (user plus effects), `clearSynthdefs()`, and the same static
`getSystemSynthdefs`, `parseNote`, `dbToAmp`, and `ampToDb` helpers. It also has
wasm-bindgen `free`/`Symbol.dispose`.

## Browser bridge

Embedders may supply:

```ts
window.vibelangBridge = {
  async loadSynthdef(name: string, bytes: Uint8Array): Promise<void> {
    // Load into the browser synthesis backend.
  }
};
```

Rust probes `globalThis.vibelangBridge` but the extern call targets
`window.vibelangBridge`; environments where those do not alias can observe a
silent no-op. The handwritten TypeScript declaration also says builtins/effects
load on every execute, while Rust loads builtins during `init` and execute walks
user/effect registries. Treat Rust behavior above as current.

## `.vibe` availability inside WASM

Core Rhai/DSP APIs and in-memory stdlib imports are installed. Native SFZ,
native recording, hardware MIDI, native timestamps, and native fs/exec/net
extensions are excluded by target/host constraints. Generated UGen functions
can still require synthesis plugins the browser backend does not provide; see
the per-entry availability in the [UGen index](../reference/generated/ugens.md).

The landing-page React application is one consumer, not an additional stable
programmatic API.
