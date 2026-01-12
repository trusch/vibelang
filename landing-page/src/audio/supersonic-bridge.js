/**
 * SuperSonic Bridge - JavaScript integration between VibeLang WASM and SuperSonic
 *
 * This module provides the vibelangBridge global object that allows VibeLang
 * to communicate with SuperSonic (scsynth WASM) via OSC messages.
 */

import { SuperSonic } from 'supersonic-scsynth';

// SuperSonic instance
let supersonic = null;
let isReady = false;

// Buffer storage for tracking loaded buffers
const buffers = new Map();

/**
 * Initialize the SuperSonic audio engine.
 * @returns {Promise<void>}
 */
export async function initSupersonic() {
  if (supersonic && isReady) {
    console.log('SuperSonic already initialized');
    return;
  }

  try {
    // Create SuperSonic instance configured to load assets from CDN
    // The main npm package is just the client API - WASM/workers are in supersonic-scsynth-core
    supersonic = new SuperSonic({
      debug: false,  // Disable debug logging for better performance
      wasmBaseURL: 'https://unpkg.com/supersonic-scsynth-core@0.25.5/wasm/',
      workerBaseURL: 'https://unpkg.com/supersonic-scsynth-core@0.25.5/workers/',
      // Audio quality settings
      scsynthOptions: {
        numBuffers: 4096,           // Increase max audio buffers (default: 1024)
        numAudioBusChannels: 256,   // More audio buses (default: 128)
        realTimeMemorySize: 16384,  // More RT memory in KB (default: 8192)
        numWireBufs: 128,           // More wire buffers (default: 64)
      }
    });

    // Initialize the audio engine
    await supersonic.init();

    isReady = true;
    console.log('SuperSonic initialized with optimized settings');
  } catch (error) {
    console.error('Failed to initialize SuperSonic:', error);
    throw error;
  }
}

/**
 * Resume audio context (needed after user gesture).
 */
export async function resumeAudio() {
  if (supersonic) {
    // Debug: log what properties supersonic has
    console.log('SuperSonic object keys:', Object.keys(supersonic));
    console.log('SuperSonic.audioContext:', supersonic.audioContext);
    console.log('SuperSonic._audioContext:', supersonic._audioContext);
    console.log('SuperSonic.context:', supersonic.context);

    // Try different ways to access audio context
    const ctx = supersonic.audioContext || supersonic._audioContext || supersonic.context;

    if (ctx && ctx.state === 'suspended') {
      await ctx.resume();
      console.log('Audio context resumed, state:', ctx.state);
    } else if (ctx) {
      console.log('Audio context state:', ctx.state);
    } else {
      console.log('No audio context found on supersonic object');
      // Try to resume via SuperSonic's own method if it has one
      if (typeof supersonic.resume === 'function') {
        console.log('Trying supersonic.resume()...');
        await supersonic.resume();
      }
    }
  } else {
    console.log('resumeAudio called but supersonic is null');
  }
}

// Debug counter for OSC messages
let oscMessageCount = 0;

/**
 * Send an OSC message to SuperSonic.
 * @param {string} address - OSC address (e.g., '/s_new')
 * @param {Array} args - OSC arguments
 * @returns {Promise<void>}
 */
function sendOsc(address, args) {
  return new Promise((resolve, reject) => {
    if (!isReady || !supersonic) {
      reject(new Error('SuperSonic not initialized'));
      return;
    }

    try {
      // Log first few OSC messages for debugging
      if (oscMessageCount < 20) {
        console.log(`[OSC] ${address}`, args);
        oscMessageCount++;
      }
      // SuperSonic.send takes address followed by args spread
      supersonic.send(address, ...args);
      resolve();
    } catch (error) {
      console.error(`[OSC Error] ${address}:`, error);
      reject(error);
    }
  });
}

/**
 * Load a synthdef into SuperSonic.
 * @param {string} name - Synthdef name
 * @param {Uint8Array} data - Compiled synthdef bytes
 * @returns {Promise<void>}
 */
async function loadSynthdef(name, data) {
  if (!isReady || !supersonic) {
    throw new Error('SuperSonic not initialized');
  }

  try {
    // SuperSonic expects synthdef data via /d_recv OSC message
    // The data is the compiled synthdef bytes
    supersonic.send('/d_recv', data);
    console.log(`Loaded synthdef: ${name}`);
  } catch (error) {
    console.error(`Failed to load synthdef ${name}:`, error);
    throw error;
  }
}

/**
 * Load an audio buffer from a URL.
 * @param {number} bufferId - Buffer ID
 * @param {string} url - URL to audio file
 * @returns {Promise<{frames: number, channels: number, sampleRate: number}>}
 */
async function loadBuffer(bufferId, url) {
  if (!isReady || !supersonic) {
    throw new Error('SuperSonic not initialized');
  }

  try {
    // Use SuperSonic's buffer allocation with file read
    // /b_allocRead bufnum, path, startFrame, numFrames
    supersonic.send('/b_allocRead', bufferId, url, 0, 0);

    // Track the buffer
    buffers.set(bufferId, { url, loaded: true });

    // Return placeholder info - actual info comes async
    return {
      frames: 0,  // Will be filled by scsynth
      channels: 2,
      sampleRate: 48000
    };
  } catch (error) {
    console.error(`Failed to load buffer ${bufferId} from ${url}:`, error);
    throw error;
  }
}

/**
 * Allocate an empty buffer for recording.
 * @param {number} bufferId - Buffer ID
 * @param {number} frames - Number of frames
 * @param {number} channels - Number of channels
 * @returns {Promise<{frames: number, channels: number, sampleRate: number}>}
 */
async function allocBuffer(bufferId, frames, channels) {
  if (!isReady || !supersonic) {
    throw new Error('SuperSonic not initialized');
  }

  const sampleRate = 48000;  // SuperSonic default

  // /b_alloc bufnum, numFrames, numChannels
  supersonic.send('/b_alloc', bufferId, frames, channels);

  buffers.set(bufferId, { frames, channels, sampleRate });

  return { frames, channels, sampleRate };
}

/**
 * Free a buffer.
 * @param {number} bufferId - Buffer ID
 * @returns {Promise<void>}
 */
async function freeBuffer(bufferId) {
  if (!supersonic) return;

  supersonic.send('/b_free', bufferId);
  buffers.delete(bufferId);
}

/**
 * Panic - send note-off to all running synths.
 * This sends gate=0 to all nodes to release their envelopes.
 * @returns {Promise<void>}
 */
async function panic() {
  if (!supersonic || !isReady) {
    console.log('panic() called but SuperSonic not ready');
    return;
  }

  try {
    // Send gate=0 to all nodes (node ID -1 is broadcast to all)
    // This releases all envelopes without freeing the groups
    console.log('[Panic] Sending note-off to all synths');
    supersonic.send('/n_set', -1, 'gate', 0);
  } catch (error) {
    console.error('[Panic] Error sending note-off:', error);
  }
}

/**
 * Get current audio time in seconds.
 * @returns {number}
 */
function getCurrentTime() {
  return 0;
}

/**
 * Get the sample rate.
 * @returns {number}
 */
function getSampleRate() {
  return 48000;  // SuperSonic default
}

/**
 * Check if SuperSonic is ready.
 * @returns {boolean}
 */
function isSupersonicReady() {
  return isReady;
}

/**
 * The vibelangBridge global object - called by VibeLang code
 *
 * This provides the JavaScript functions that control audio.
 */
window.vibelangBridge = {
  /**
   * Send an OSC message to SuperSonic.
   * @param {string} address - OSC address
   * @param {any} args - Arguments array
   * @returns {Promise<void>}
   */
  sendOsc: async (address, args) => {
    const argsArray = Array.isArray(args) ? args : [];
    return sendOsc(address, argsArray);
  },

  /**
   * Load a synthdef.
   * @param {string} name - Synthdef name
   * @param {Uint8Array} data - Compiled bytes
   * @returns {Promise<void>}
   */
  loadSynthdef: loadSynthdef,

  /**
   * Load a buffer from URL.
   * @param {number} bufferId
   * @param {string} url
   * @returns {Promise<{frames: number, channels: number, sampleRate: number}>}
   */
  loadBuffer: loadBuffer,

  /**
   * Allocate an empty buffer.
   * @param {number} bufferId
   * @param {number} frames
   * @param {number} channels
   * @returns {Promise<{frames: number, channels: number, sampleRate: number}>}
   */
  allocBuffer: allocBuffer,

  /**
   * Free a buffer.
   * @param {number} bufferId
   * @returns {Promise<void>}
   */
  freeBuffer: freeBuffer,

  /**
   * Panic - free all running synths.
   * @returns {Promise<void>}
   */
  panic: panic,

  /**
   * Get current audio time.
   * @returns {number}
   */
  getCurrentTime: getCurrentTime,

  /**
   * Get sample rate.
   * @returns {number}
   */
  getSampleRate: getSampleRate,

  /**
   * Check if ready.
   * @returns {boolean}
   */
  isReady: isSupersonicReady
};

// Export functions for direct use
export {
  sendOsc,
  loadSynthdef,
  loadBuffer,
  allocBuffer,
  freeBuffer,
  panic,
  getCurrentTime,
  getSampleRate,
  isSupersonicReady
};
