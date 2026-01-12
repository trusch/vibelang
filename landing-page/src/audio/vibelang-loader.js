/**
 * VibeLang WASM Loader
 *
 * This module handles loading and initializing the VibeLang WASM module
 * and provides a high-level API for the Playground component.
 *
 * Uses the native Rust runtime and scheduler compiled to WASM.
 * The tick() method drives the scheduler via requestAnimationFrame.
 */

// Module references
let vibelangWasm = null;
let vibelangRuntime = null;
let isInitialized = false;
let audioEnabled = false;
let isPlaying = false;
let animationFrameId = null;

// Try to import SuperSonic bridge (may not be available)
let supersonicBridge = null;

/**
 * Initialize VibeLang and optionally SuperSonic.
 * @param {Object} options - Configuration options
 * @param {boolean} options.enableAudio - Whether to enable audio (requires SuperSonic)
 * @returns {Promise<{audioEnabled: boolean}>}
 */
export async function initVibelang(options = {}) {
  if (isInitialized) {
    console.log('VibeLang already initialized');
    return { audioEnabled };
  }

  const { enableAudio = true } = options;

  try {
    // Import the VibeLang WASM module
    const wasmModule = await import('vibelang-wasm');

    // Initialize the WASM module (loads the .wasm file)
    await wasmModule.default();

    vibelangWasm = wasmModule;

    // Try to initialize SuperSonic if requested
    if (enableAudio) {
      try {
        supersonicBridge = await import('./supersonic-bridge');
        // SuperSonic npm package loads assets from CDN automatically
        await supersonicBridge.initSupersonic();

        // Load system synthdefs (needed before runtime init)
        const systemSynthdefs = vibelangWasm.VibelangRuntime.getSystemSynthdefs();
        for (const synthdef of systemSynthdefs) {
          await window.vibelangBridge.loadSynthdef(synthdef.name, new Uint8Array(synthdef.data));
        }

        // Create and initialize the native runtime
        vibelangRuntime = new vibelangWasm.VibelangRuntime();
        await vibelangRuntime.init();

        audioEnabled = true;
        console.log('VibeLang initialized with native runtime and audio support');
      } catch (audioError) {
        console.warn('SuperSonic not available, running in parse-only mode:', audioError.message);
        audioEnabled = false;
        // Create runtime anyway for parsing
        vibelangRuntime = new vibelangWasm.VibelangRuntime();
      }
    } else {
      // Create runtime for parsing only
      vibelangRuntime = new vibelangWasm.VibelangRuntime();
    }

    isInitialized = true;
    console.log('VibeLang initialized' + (audioEnabled ? ' with audio' : ' (parse-only mode)'));
    return { audioEnabled };
  } catch (error) {
    console.error('Failed to initialize VibeLang:', error);
    throw error;
  }
}

/**
 * The tick loop that drives the native scheduler.
 * Called via requestAnimationFrame when playing.
 */
let tickCount = 0;
async function tickLoop() {
  if (!isPlaying || !vibelangRuntime) {
    console.log('[VibeLang] Tick loop stopped - isPlaying:', isPlaying, 'runtime:', !!vibelangRuntime);
    animationFrameId = null;
    return;
  }

  // Log first few ticks
  if (tickCount < 5) {
    console.log('[VibeLang] tick', tickCount);
    tickCount++;
  }

  // Tick the native runtime
  await vibelangRuntime.tick();

  // Schedule next tick
  animationFrameId = requestAnimationFrame(tickLoop);
}

/**
 * Compile/validate a VibeLang script without executing.
 * Useful for syntax checking before running.
 *
 * @param {string} script - The VibeLang script to compile
 * @returns {Promise<CompileResult>}
 */
export async function compileScript(script) {
  if (!isInitialized) {
    throw new Error('VibeLang not initialized. Call initVibelang() first.');
  }

  // Use the runtime's execute method but don't start transport
  // The execute method returns parse results even without audio
  const result = await vibelangRuntime.execute(script);

  return {
    success: result.success,
    error: result.error || null,
    stats: result.success ? {
      groups: result.groups,
      voices: result.voices,
      patterns: result.patterns,
      melodies: result.melodies,
      tempo: result.tempo
    } : null
  };
}

/**
 * Execute a VibeLang script.
 *
 * This follows the native CLI's seamless reload pattern:
 * - Transport starts once and continues running
 * - Script execution only sends ReloadMessage::Apply
 * - The diff mechanism handles updating entities seamlessly
 *
 * @param {string} script - The VibeLang script to execute
 * @returns {Promise<ExecutionResult>}
 */
export async function executeScript(script) {
  if (!isInitialized) {
    throw new Error('VibeLang not initialized. Call initVibelang() first.');
  }

  // Resume audio context if audio is enabled (required for browser autoplay policy)
  if (audioEnabled && supersonicBridge) {
    await supersonicBridge.resumeAudio();
  }

  // Execute the script via native runtime (sends ReloadMessage::Apply internally)
  // This is seamless - the diff mechanism handles entity updates without stopping playback
  const result = await vibelangRuntime.execute(script);

  if (!result.success) {
    return {
      success: false,
      error: result.error,
      stats: null,
      audioEnabled
    };
  }

  // If audio is enabled and not yet playing, start transport (first run only)
  if (audioEnabled && !isPlaying) {
    console.log('[VibeLang] Starting transport (first run)...');
    await vibelangRuntime.start();
    isPlaying = true;
    tickCount = 0;
    // Start the tick loop
    if (!animationFrameId) {
      console.log('[VibeLang] Starting tick loop');
      animationFrameId = requestAnimationFrame(tickLoop);
    }
  }

  return {
    success: true,
    error: null,
    stats: {
      groups: result.groups,
      voices: result.voices,
      patterns: result.patterns,
      melodies: result.melodies,
      tempo: result.tempo
    },
    audioEnabled
  };
}

/**
 * Stop all audio.
 */
export async function stopAll() {
  console.log('[VibeLang] stopAll() called');
  isPlaying = false;

  // Cancel animation frame
  if (animationFrameId) {
    cancelAnimationFrame(animationFrameId);
    animationFrameId = null;
  }

  // Stop the native runtime (transport only - reload handles entity cleanup)
  if (vibelangRuntime) {
    console.log('[VibeLang] Stopping runtime...');
    await vibelangRuntime.stopAll();
  }

  // Panic - free all running synths to stop any lingering notes
  if (supersonicBridge) {
    console.log('[VibeLang] Calling panic()...');
    await supersonicBridge.panic();
    console.log('[VibeLang] panic() completed');
  } else {
    console.log('[VibeLang] supersonicBridge is null, cannot panic');
  }
}

/**
 * Fully reset the VibeLang runtime.
 * This stops all audio, destroys the current runtime, and allows re-initialization.
 * Use this when switching between tutorials to ensure a clean slate.
 */
export async function resetRuntime() {
  console.log('[VibeLang] resetRuntime() called - performing full reset');

  // First stop everything
  await stopAll();

  // Free all nodes in scsynth to ensure clean state
  if (supersonicBridge && supersonicBridge.isSupersonicReady && supersonicBridge.isSupersonicReady()) {
    try {
      // Send /g_freeAll to free all nodes in the default group (group 1)
      // This is more thorough than panic() which only sets gate=0
      console.log('[VibeLang] Freeing all synths in scsynth...');
      await window.vibelangBridge.sendOsc('/g_freeAll', [1]);
    } catch (e) {
      console.warn('[VibeLang] Could not free all nodes:', e);
    }
  }

  // Destroy the runtime instance
  if (vibelangRuntime) {
    console.log('[VibeLang] Destroying runtime instance...');
    vibelangRuntime = null;
  }

  // Reset initialization flag so next init creates fresh runtime
  isInitialized = false;
  audioEnabled = false;
  tickCount = 0;

  console.log('[VibeLang] Runtime reset complete - ready for re-initialization');
}

/**
 * Parse a note name to MIDI number.
 * @param {string} note - Note name (e.g., "C4", "A#3")
 * @returns {number} MIDI note number, or -1 if invalid
 */
export function parseNote(note) {
  if (!vibelangWasm) {
    return -1;
  }
  return vibelangWasm.VibelangRuntime.parseNote(note);
}

/**
 * Convert decibels to linear amplitude.
 * @param {number} db - Decibel value
 * @returns {number} Linear amplitude
 */
export function dbToAmp(db) {
  if (!vibelangWasm) {
    return Math.pow(10, db / 20);
  }
  return vibelangWasm.VibelangRuntime.dbToAmp(db);
}

/**
 * Convert linear amplitude to decibels.
 * @param {number} amp - Linear amplitude
 * @returns {number} Decibel value
 */
export function ampToDb(amp) {
  if (!vibelangWasm) {
    return 20 * Math.log10(amp);
  }
  return vibelangWasm.VibelangRuntime.ampToDb(amp);
}

/**
 * Get VibeLang version.
 * @returns {string}
 */
export function getVersion() {
  if (!vibelangWasm) {
    return 'unknown';
  }
  return vibelangWasm.version();
}

/**
 * Check if VibeLang is initialized and ready.
 * @returns {boolean}
 */
export function isReady() {
  return isInitialized;
}

/**
 * Check if audio playback is available.
 * @returns {boolean}
 */
export function isAudioEnabled() {
  return audioEnabled;
}

/**
 * Check if currently playing.
 * @returns {boolean}
 */
export function isPlaybackActive() {
  return isPlaying;
}

// Export types for TypeScript users
/**
 * @typedef {Object} ExecutionResult
 * @property {boolean} success - Whether execution succeeded
 * @property {string|null} error - Error message if failed
 * @property {ExecutionStats|null} stats - Execution statistics if succeeded
 */

/**
 * @typedef {Object} ExecutionStats
 * @property {number} groups - Number of groups created
 * @property {number} voices - Number of voices created
 * @property {number} patterns - Number of patterns created
 * @property {number} melodies - Number of melodies created
 * @property {number} tempo - Tempo in BPM
 */
