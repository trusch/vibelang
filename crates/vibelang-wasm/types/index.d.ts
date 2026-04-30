/**
 * Hand-written TypeScript declarations for `vibelang-wasm`.
 *
 * The wasm-pack auto-generated declarations type the result of every async
 * Rust function as `any`. This file mirrors the wasm-bindgen surface exposed
 * from `crates/vibelang-wasm/src/lib.rs` with concrete types, so TypeScript
 * consumers (e.g. modular-hub) catch breaking changes at type-check time.
 *
 * Keep this file in sync with `src/lib.rs` whenever the public surface
 * changes — wasm-pack will not regenerate it.
 */

/* ----------------------------- result types ----------------------------- */

/**
 * Result returned by `VibelangRuntime#execute` and `VibelangEngine#execute`.
 *
 * Mirrors the Rust `ExecutionResult` struct. On parse/runtime failure the
 * runtime returns `success: false` with `error` populated; on success the
 * counts reflect the post-execution `ScriptState`.
 */
export interface VibelangResult {
  success: boolean;
  /** Populated when `success` is `false`. */
  error: string | null;
  groups: number;
  voices: number;
  patterns: number;
  melodies: number;
  /** Tempo in BPM. Defaults to 120 on failure. */
  tempo: number;
}

/**
 * Compiled synthdef payload returned by `getSystemSynthdefs` and
 * `VibelangEngine#getSynthdefs`. The `data` field is the raw SuperCollider
 * synthdef bytes — pass straight to the JS bridge's `loadSynthdef`.
 *
 * `Vec<u8>` is serialized by `serde-wasm-bindgen` as a plain `number[]` by
 * default; consumers should treat it as a byte array regardless.
 */
export interface VibelangCompiledSynthdef {
  name: string;
  data: Uint8Array | number[];
}

/**
 * Error shape thrown across the wasm boundary. `wasm_bindgen` rejects
 * promises with a `JsValue` that is constructed via `JsValue::from_str`,
 * so consumers see a string in most cases — but TS code should treat it
 * as `unknown` and check at the call site. This interface is provided
 * for the cases where the bridge wraps the rejection in an Error-like.
 */
export interface VibelangError {
  message: string;
  name?: string;
  stack?: string;
}

/* ----------------------------- bridge surface --------------------------- */

/**
 * The JS-side bridge that wasm calls into for SuperSonic communication.
 *
 * `crates/vibelang-wasm/src/lib.rs::load_synthdef_to_supersonic` looks up
 * `window.vibelangBridge` and invokes `loadSynthdef`. Embedders must define
 * `globalThis.vibelangBridge` before calling `runtime.execute(...)`.
 */
export interface VibelangBridge {
  /**
   * Send a compiled synthdef to SuperSonic. Called once per user synthdef
   * and once per built-in / effect synthdef on every `execute` invocation.
   *
   * The wasm side awaits this — return a Promise that resolves once the
   * synthdef has been registered. The resolved value is currently ignored.
   */
  loadSynthdef(name: string, data: Uint8Array): Promise<unknown>;
}

declare global {
  interface Window {
    /**
     * Optional global bridge that the wasm runtime calls into. Must be
     * assigned before `VibelangRuntime#execute` is invoked.
     */
    vibelangBridge?: VibelangBridge;
  }
}

/* ----------------------------- runtime class ---------------------------- */

/**
 * Native VibeLang runtime running in WASM.
 *
 * Lifecycle:
 *   1. `new VibelangRuntime()` — clears synthdef registries.
 *   2. `await runtime.init()` — wires up the WebScsynth backend; must be
 *      called once after SuperSonic is ready.
 *   3. `await runtime.execute(script)` — parses and applies a script.
 *   4. Drive `runtime.tick()` ~60Hz via requestAnimationFrame.
 *   5. `await runtime.start()` / `runtime.stop()` to control transport.
 */
export class VibelangRuntime {
  constructor();

  /**
   * Free the underlying wasm allocation. Auto-called via `using` /
   * `Symbol.dispose` if the consumer opts into explicit resource management.
   */
  free(): void;
  [Symbol.dispose](): void;

  /**
   * Wire up the WebScsynth backend. Idempotent — subsequent calls are no-ops.
   * Rejects with a string if the backend or builtins fail to load.
   */
  init(): Promise<void>;

  /**
   * Parse, deploy synthdefs, and apply state. Returns a `VibelangResult`
   * describing the post-execution state. Never rejects — parse/runtime
   * failures surface as `{ success: false, error }`.
   */
  execute(script: string): Promise<VibelangResult>;

  /**
   * Drive the scheduler. Call ~60 times per second (e.g. via
   * `requestAnimationFrame`).
   */
  tick(): Promise<void>;

  /** Start transport. Rejects with a string on backend send failure. */
  start(): Promise<void>;

  /** Stop transport. Rejects with a string on backend send failure. */
  stop(): Promise<void>;

  /** Stop transport and free all live synths. */
  stopAll(): Promise<void>;

  /** True once `init()` has resolved successfully. */
  isInitialized(): boolean;

  /**
   * All built-in system synthdefs (param routers, link audio, etc.) plus
   * the `system_link_audio` synthdef. The embedder is responsible for
   * passing each one to `VibelangBridge#loadSynthdef` before calling
   * `execute`.
   */
  static getSystemSynthdefs(): VibelangCompiledSynthdef[];

  /** MIDI note number for a note name (e.g. `"C4"` → 60). Returns `-1` on parse failure. */
  static parseNote(note: string): number;

  /** dB → linear amplitude (10 ^ (db / 20)). */
  static dbToAmp(db: number): number;

  /** Linear amplitude → dB (20 * log10(amp)). */
  static ampToDb(amp: number): number;
}

/* ----------------------------- legacy engine ---------------------------- */

/**
 * Legacy parse-only engine kept for backwards compatibility. Prefer
 * `VibelangRuntime` for new code. This will be removed once consumers
 * have migrated.
 */
export class VibelangEngine {
  constructor();
  free(): void;
  [Symbol.dispose](): void;

  execute(script: string): VibelangResult;
  getSynthdefs(): VibelangCompiledSynthdef[];
  static getSystemSynthdefs(): VibelangCompiledSynthdef[];
  clearSynthdefs(): void;

  static parseNote(note: string): number;
  static dbToAmp(db: number): number;
  static ampToDb(amp: number): number;
}

/* ----------------------------- module functions ------------------------- */

/** Install the panic hook. wasm-pack also calls this via `#[wasm_bindgen(start)]`. */
export function init_panic_hook(): void;

/** Log a string to the browser console. */
export function log(message: string): void;

/** The vibelang-wasm crate version (from `CARGO_PKG_VERSION`). */
export function version(): string;

/* ----------------------------- module init ------------------------------ */

export type InitInput =
  | RequestInfo
  | URL
  | Response
  | BufferSource
  | WebAssembly.Module;

export type SyncInitInput = BufferSource | WebAssembly.Module;

/** Output of synchronous module init — surface kept opaque on purpose. */
export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly [exportName: string]: unknown;
}

/** Synchronously instantiate the wasm module. */
export function initSync(
  module: { module: SyncInitInput } | SyncInitInput,
): InitOutput;

/**
 * Default export — fetches and instantiates the wasm module. Pass the URL
 * to your `vibelang_wasm_bg.wasm` file (or a `RequestInfo`/`Response`).
 */
export default function __wbg_init(
  module_or_path?:
    | { module_or_path: InitInput | Promise<InitInput> }
    | InitInput
    | Promise<InitInput>,
): Promise<InitOutput>;
