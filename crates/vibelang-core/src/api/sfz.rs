//! SFZ instrument API for VibeLang scripts.
//!
//! This module provides the `load_sfz` function that loads an SFZ instrument
//! file and makes it available for use with voices.

use crate::api::{context, require_handle};
use crate::state::StateMessage;
use rhai::Engine;
use vibelang_sfz::SfzInstrumentHandle;

/// Load an SFZ instrument from a file.
///
/// # Arguments
///
/// * `id` - A unique identifier for this instrument
/// * `path` - Path to the SFZ file (can be relative to the script or cwd)
///
/// # Returns
///
/// An `SfzInstrumentHandle` that can be used with voices via `.on(instrument)`.
///
/// # Path Resolution
///
/// The path is resolved using the following order:
/// 1. If absolute and exists, use directly
/// 2. Relative to the current working directory
/// 3. Relative to the script directory
/// 4. Relative to import paths
///
/// # Example
///
/// ```rhai
/// let bass = load_sfz("bass", "samples/bass.sfz");
/// let bass_voice = voice("bass").on(bass);
/// ```
pub fn load_sfz(id: String, path: String) -> SfzInstrumentHandle {
    let handle = require_handle();

    // Resolve the path using the smart file resolver
    let sfz_path = match context::resolve_file_or_error(&path) {
        Ok(resolved) => {
            log::info!("Resolved SFZ path '{}' to '{}'", path, resolved.display());
            resolved
        }
        Err(err) => {
            log::error!("{}", err);
            return SfzInstrumentHandle::new(id, path, 0);
        }
    };

    // Send the load message to the runtime
    let _ = handle.send(StateMessage::LoadSfzInstrument {
        id: id.clone(),
        sfz_path: sfz_path.clone(),
    });

    // Wait for the instrument to be loaded (poll state)
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(10);

    loop {
        if start.elapsed() > timeout {
            log::error!("Timeout waiting for SFZ instrument '{}' to load", id);
            return SfzInstrumentHandle::new(id, path, 0);
        }

        // Check if the instrument is loaded
        let loaded = handle.with_state(|state| {
            state.sfz_instruments.get(&id).map(|inst| inst.num_regions())
        });

        if let Some(num_regions) = loaded {
            log::info!("SFZ instrument '{}' loaded with {} regions", id, num_regions);
            return SfzInstrumentHandle::new(id, path, num_regions);
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Register the SFZ API with a Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register the SfzInstrumentHandle type
    vibelang_sfz::register_sfz_types(engine);

    // Register the load_sfz function
    engine.register_fn("load_sfz", load_sfz);
}
