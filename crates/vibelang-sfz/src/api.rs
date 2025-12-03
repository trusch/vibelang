//! Rhai API for SFZ instruments.
//!
//! This module provides the API functions that can be registered with a Rhai engine
//! to enable SFZ instrument support in VibeLang scripts.

use rhai::Engine;

use crate::types::SfzInstrument;

/// A handle to a loaded SFZ instrument for use in scripts.
///
/// This is a thin wrapper around the instrument ID that can be passed
/// to voices to use the SFZ instrument for playback.
#[derive(Clone, Debug)]
pub struct SfzInstrumentHandle {
    /// The instrument ID.
    pub id: String,
    /// The path to the SFZ file.
    pub path: String,
    /// Number of regions in the instrument.
    pub num_regions: usize,
}

impl SfzInstrumentHandle {
    /// Create a new handle from an instrument ID and info.
    pub fn new(id: String, path: String, num_regions: usize) -> Self {
        Self {
            id,
            path,
            num_regions,
        }
    }

    /// Create a handle from a loaded instrument.
    pub fn from_instrument(id: &str, instrument: &SfzInstrument) -> Self {
        Self {
            id: id.to_string(),
            path: instrument.source_file.to_string_lossy().to_string(),
            num_regions: instrument.regions.len(),
        }
    }

    /// Get the instrument ID.
    pub fn get_id(&mut self) -> String {
        self.id.clone()
    }

    /// Get the SFZ file path.
    pub fn get_path(&mut self) -> String {
        self.path.clone()
    }

    /// Get the number of regions.
    pub fn get_num_regions(&mut self) -> i64 {
        self.num_regions as i64
    }

    /// Get a human-readable info string.
    pub fn info(&mut self) -> String {
        format!(
            "SFZ '{}': {} regions from {}",
            self.id, self.num_regions, self.path
        )
    }
}

/// Register the SFZ types with a Rhai engine.
///
/// This registers the `SfzInstrumentHandle` type and its methods.
/// The actual `load_sfz` function must be registered separately by
/// the integration code (vibelang-core) since it needs access to
/// the runtime.
pub fn register_sfz_types(engine: &mut Engine) {
    // Register the handle type
    engine.register_type_with_name::<SfzInstrumentHandle>("SfzInstrument");

    // Register getter methods
    engine.register_get("id", SfzInstrumentHandle::get_id);
    engine.register_get("path", SfzInstrumentHandle::get_path);
    engine.register_get("num_regions", SfzInstrumentHandle::get_num_regions);

    // Register info method
    engine.register_fn("info", SfzInstrumentHandle::info);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sfz_instrument_handle() {
        let mut handle = SfzInstrumentHandle::new(
            "test_bass".to_string(),
            "/path/to/bass.sfz".to_string(),
            42,
        );

        assert_eq!(handle.get_id(), "test_bass");
        assert_eq!(handle.get_path(), "/path/to/bass.sfz");
        assert_eq!(handle.get_num_regions(), 42);
        assert!(handle.info().contains("test_bass"));
        assert!(handle.info().contains("42"));
    }
}
