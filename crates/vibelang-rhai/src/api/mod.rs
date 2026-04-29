//! Rhai API modules.
//!
//! Each module provides builders and functions for a specific feature area.

pub mod assert;
pub mod global;
pub mod group;
pub mod helpers;
pub mod melody;
pub mod modulator;
pub mod pattern;
pub mod route;
pub mod sample;
pub mod sequence;
pub mod voice;

/// Clear all object registries.
///
/// This should be called before each script execution to start with a clean slate.
pub fn clear_all_registries() {
    pattern::clear_registry();
    melody::clear_registry();
    sequence::clear_registry();
    #[cfg(not(target_arch = "wasm32"))]
    reel::clear_registry();
}

// Native-only modules (require file I/O)
#[cfg(not(target_arch = "wasm32"))]
pub mod recording;
#[cfg(not(target_arch = "wasm32"))]
pub mod reel;
#[cfg(not(target_arch = "wasm32"))]
pub mod sfz;

#[cfg(feature = "midi")]
pub mod midi;

use rhai::Engine;

/// Register all VibeLang API functions with a Rhai engine.
pub fn register_api(engine: &mut Engine) {
    // Register helper functions first (db, note, bars)
    helpers::register(engine);

    // Register global functions (set_tempo, etc.)
    global::register(engine);

    // Register assertion functions (for testing)
    assert::register(engine);

    // Register group API
    group::register(engine);

    // Register voice API
    voice::register(engine);

    // Register route API (RouteHandle terminal verbs)
    route::register(engine);

    // Register pattern API
    pattern::register(engine);

    // Register melody API
    melody::register(engine);

    // Register sequence/fade/fx API
    sequence::register(engine);

    // Register sample API
    sample::register(engine);

    // Register modulator API
    modulator::register(engine);

    // Register SFZ API (native only - requires file I/O)
    #[cfg(not(target_arch = "wasm32"))]
    sfz::register(engine);

    // Register Reel API (native only - requires file I/O for cue chunks)
    #[cfg(not(target_arch = "wasm32"))]
    reel::register(engine);

    // Register Recording API (native only - requires file I/O)
    #[cfg(not(target_arch = "wasm32"))]
    recording::register(engine);

    // Register MIDI API (feature-gated)
    #[cfg(feature = "midi")]
    midi::register(engine);
}
