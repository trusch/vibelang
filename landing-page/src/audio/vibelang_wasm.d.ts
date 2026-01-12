/* tslint:disable */
/* eslint-disable */
export function init_panic_hook(): void;
/**
 * Log a message to the browser console.
 */
export function log(message: string): void;
/**
 * Get the VibeLang version.
 */
export function version(): string;
/**
 * VibeLang scripting engine for WASM.
 *
 * This is the main entry point for using VibeLang in the browser.
 */
export class VibelangEngine {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new VibeLang engine.
   */
  constructor();
  /**
   * Execute a VibeLang script.
   *
   * Returns a JSON object with execution results.
   */
  execute(script: string): any;
  /**
   * Get playback data for the JavaScript scheduler.
   *
   * This returns all the data needed to play the script:
   * - tempo
   * - voices with their synthdef and parameters
   * - patterns with their steps
   * - melodies with their notes
   */
  getPlaybackData(): any;
  /**
   * Get compiled synthdefs from the current session.
   *
   * Returns an array of objects with `name` and `data` (array of bytes) fields.
   */
  getSynthdefs(): any;
  /**
   * Get all built-in system synthdefs.
   *
   * Returns an array of compiled synthdefs needed for basic operation.
   * These include audio routing, sample playback, and other core synthdefs.
   */
  static getSystemSynthdefs(): any;
  /**
   * Clear all user-defined synthdefs.
   *
   * Call this before executing a new script if you want a clean slate.
   */
  clearSynthdefs(): void;
  /**
   * Parse a note name to MIDI number.
   *
   * Examples: "C4" -> 60, "A#3" -> 58, "Bb5" -> 82
   */
  static parseNote(note: string): number;
  /**
   * Convert decibels to linear amplitude.
   */
  static dbToAmp(db: number): number;
  /**
   * Convert linear amplitude to decibels.
   */
  static ampToDb(amp: number): number;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_vibelangengine_free: (a: number, b: number) => void;
  readonly vibelangengine_new: () => number;
  readonly vibelangengine_execute: (a: number, b: number, c: number) => any;
  readonly vibelangengine_getPlaybackData: (a: number) => any;
  readonly vibelangengine_getSynthdefs: (a: number) => any;
  readonly vibelangengine_getSystemSynthdefs: () => any;
  readonly vibelangengine_clearSynthdefs: (a: number) => void;
  readonly vibelangengine_parseNote: (a: number, b: number) => number;
  readonly log: (a: number, b: number) => void;
  readonly version: () => [number, number];
  readonly vibelangengine_dbToAmp: (a: number) => number;
  readonly vibelangengine_ampToDb: (a: number) => number;
  readonly init_panic_hook: () => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
