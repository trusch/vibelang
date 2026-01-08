//! Rhai API modules.
//!
//! Each module provides builders and functions for a specific feature area.

pub mod assert;
pub mod global;
pub mod group;
pub mod helpers;
pub mod voice;
pub mod pattern;
pub mod melody;
pub mod sequence;
pub mod sample;
pub mod sfz;
pub mod recording;

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

    // Register pattern API
    pattern::register(engine);

    // Register melody API
    melody::register(engine);

    // Register sequence/fade/fx API
    sequence::register(engine);

    // Register sample API
    sample::register(engine);

    // Register SFZ API
    sfz::register(engine);

    // Register Recording API
    recording::register(engine);

    // Register MIDI API (feature-gated)
    #[cfg(feature = "midi")]
    midi::register(engine);
}
