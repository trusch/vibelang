//! Rhai API modules.
//!
//! Each module provides builders and functions for a specific feature area.

pub mod assert;
pub mod buffer;
pub mod global;
pub mod group;
pub mod helpers;
pub mod melody;
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
}

// Native-only modules (require file I/O)
#[cfg(not(target_arch = "wasm32"))]
pub mod recording;
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

    // Register script-allocated buffer API
    buffer::register(engine);

    // Register SFZ API (native only - requires file I/O)
    #[cfg(not(target_arch = "wasm32"))]
    sfz::register(engine);

    // Register Recording API (native only - requires file I/O)
    #[cfg(not(target_arch = "wasm32"))]
    recording::register(engine);

    // Register MIDI API (feature-gated)
    #[cfg(feature = "midi")]
    midi::register(engine);
}

/// Install the vibe-api 2 authoring-family surface into a Rhai engine.
///
/// Families install in the deterministic M08 order: Group, Voice, Pattern,
/// Melody, then Sequence. The sequence module also owns the Fade, Effect,
/// SynthDef, and EffectDef families and layers them over the DSP surface it
/// installs first, so it must stay last of the M08 block.
///
/// The M09 families follow in the deterministic order Sample, Buffer, SFZ,
/// Record, Route, then MIDI. SFZ and Record require file I/O and are native
/// only; MIDI follows the crate's `midi` feature exactly like the v1
/// registration root.
pub(crate) fn install_v2_api(engine: &mut Engine) {
    group::install_v2(engine);
    voice::install_v2(engine);
    pattern::install_v2(engine);
    melody::install_v2(engine);
    sequence::install_v2(engine);

    sample::install_v2(engine);
    buffer::install_v2(engine);
    #[cfg(not(target_arch = "wasm32"))]
    sfz::install_v2(engine);
    #[cfg(not(target_arch = "wasm32"))]
    recording::install_v2(engine);
    route::install_v2(engine);
    #[cfg(feature = "midi")]
    midi::install_v2(engine);
}
