//! SFZ API for Rhai scripts.
//!
//! SFZ instruments are sample-based instruments with key/velocity layers.
//!
//! ## Example
//! ```rhai
//! // Load an SFZ instrument
//! let piano = load_sfz("piano", "instruments/piano.sfz");
//!
//! // Create a voice that uses the SFZ instrument
//! let piano_voice = voice("piano_voice")
//!     .on_sfz(piano)
//!     .gain(db(-6))
//!     .apply();
//!
//! // Use in a melody
//! melody("melody")
//!     .on(piano_voice)
//!     .notes("C4 E4 G4 C5")
//!     .start();
//! ```

use rhai::{CustomType, Engine, TypeBuilder};
use std::path::PathBuf;
use vibelang_core::traits::SfzConfig;

use crate::context;

/// Handle to a loaded SFZ instrument.
#[derive(Debug, Clone, CustomType)]
pub struct SfzHandle {
    /// SFZ instrument ID name.
    pub id: String,
    /// Path to the SFZ file.
    pub path: PathBuf,
}

impl SfzHandle {
    /// Create a new SFZ handle.
    pub fn new(id: String, path: PathBuf) -> Self {
        Self { id, path }
    }

    /// Get the SFZ ID name.
    pub fn get_id(&mut self) -> String {
        self.id.clone()
    }

    /// Get the path.
    pub fn get_path(&mut self) -> String {
        self.path.to_string_lossy().to_string()
    }
}

/// Load an SFZ instrument from a file.
///
/// # Arguments
///
/// * `id` - Unique identifier for this instrument
/// * `path` - Path to the .sfz file (relative to script or absolute)
///
/// # Returns
///
/// An SfzHandle that can be assigned to voices using `.on_sfz()`.
pub fn load_sfz(id: String, path: String) -> SfzHandle {
    let sfz_id = context::get_or_create_sfz_id(&id);

    // Resolve path relative to current script file
    let resolved_path = if let Some(current_file) = context::get_current_file() {
        if let Some(parent) = current_file.parent() {
            let p = parent.join(&path);
            if p.exists() {
                p
            } else {
                PathBuf::from(&path)
            }
        } else {
            PathBuf::from(&path)
        }
    } else {
        PathBuf::from(&path)
    };

    let config = SfzConfig::new(resolved_path.clone());

    context::with_state(|state| {
        state.sfz_instruments.insert(sfz_id, config);
    });

    SfzHandle::new(id, resolved_path)
}

/// Register SFZ API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    engine.build_type::<SfzHandle>();

    // Constructor
    engine.register_fn("load_sfz", load_sfz);

    // ID and path getters
    engine.register_fn("id", SfzHandle::get_id);
    engine.register_get("id", SfzHandle::get_id);
    engine.register_fn("path", SfzHandle::get_path);
    engine.register_get("path", SfzHandle::get_path);
}
