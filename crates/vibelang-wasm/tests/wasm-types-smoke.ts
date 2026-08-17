/**
 * Type-level smoke test for `crates/vibelang-wasm/types/index.d.ts`.
 *
 * Imports every callable exported symbol and uses each method/field at least
 * once so that `tsc --noEmit` will fail loudly if the surface drifts from
 * `src/lib.rs`. Module start hooks are intentionally not callable exports.
 * There is no runtime: the file exists purely to type-check.
 */

import init, {
  initSync,
  log,
  version,
  VibelangRuntime,
  VibelangEngine,
  type VibelangResult,
  type VibelangCompiledSynthdef,
  type VibelangError,
  type VibelangBridge,
  type InitInput,
  type SyncInitInput,
  type InitOutput,
} from "../types";

declare const wasmUrl: URL;
declare const wasmBytes: BufferSource;

async function smoke(): Promise<void> {
  // free functions
  log("hello");
  const v: string = version();
  void v;

  // module init
  const out: InitOutput = await init(wasmUrl);
  void out.memory;
  const out2: InitOutput = initSync(wasmBytes);
  void out2;

  // typed input aliases — make sure the unions accept what the docs claim
  const _initInput: InitInput = wasmUrl;
  const _syncInput: SyncInitInput = wasmBytes;
  void _initInput;
  void _syncInput;

  // bridge wiring on the global window
  const bridge: VibelangBridge = {
    async loadSynthdef(name, data) {
      const _name: string = name;
      const _data: Uint8Array = data;
      void _name;
      void _data;
      return undefined;
    },
  };
  globalThis.window.vibelangBridge = bridge;

  // VibelangRuntime
  const runtime = new VibelangRuntime();
  await runtime.init();

  const result: VibelangResult = await runtime.execute("set_tempo(128);");
  if (!result.success) {
    const err: string | null = result.error;
    void err;
  }
  const counts: number = result.groups + result.voices + result.patterns + result.melodies;
  const tempo: number = result.tempo;
  void counts;
  void tempo;

  await runtime.tick();
  await runtime.start();
  await runtime.stop();
  await runtime.stopAll();

  const ready: boolean = runtime.isInitialized();
  void ready;

  const sds: VibelangCompiledSynthdef[] = VibelangRuntime.getSystemSynthdefs();
  for (const sd of sds) {
    const _name: string = sd.name;
    const _data: Uint8Array | number[] = sd.data;
    void _name;
    void _data;
  }

  const midi: number = VibelangRuntime.parseNote("C4");
  const amp: number = VibelangRuntime.dbToAmp(-6);
  const db: number = VibelangRuntime.ampToDb(0.5);
  void midi;
  void amp;
  void db;

  runtime.free();

  // VibelangEngine (legacy)
  const engine = new VibelangEngine();
  const legacyResult: VibelangResult = engine.execute("set_tempo(120);");
  void legacyResult;
  const legacySds: VibelangCompiledSynthdef[] = engine.getSynthdefs();
  void legacySds;
  const legacySys: VibelangCompiledSynthdef[] = VibelangEngine.getSystemSynthdefs();
  void legacySys;
  engine.clearSynthdefs();
  const _l1: number = VibelangEngine.parseNote("D#3");
  const _l2: number = VibelangEngine.dbToAmp(0);
  const _l3: number = VibelangEngine.ampToDb(1);
  void _l1;
  void _l2;
  void _l3;
  engine.free();

  // VibelangError shape — usually surfaced as a thrown string, but consumers
  // that wrap it in an Error-like should be assignable.
  const wrapped: VibelangError = { message: "boom", name: "VibelangError" };
  void wrapped;
}

void smoke;
