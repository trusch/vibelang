//! Sample API for Rhai scripts.
//!
//! Samples are audio files loaded for playback.
//!
//! ## Example
//! ```rhai
//! let kick = sample("kick", "samples/kick.wav")
//!     .attack(0.001)
//!     .release(0.1)
//!     .amp(0.8);
//!
//! let loop_sample = sample("loop", "samples/loop.wav")
//!     .warp(true)
//!     .speed(0.5)
//!     .pitch(1.2)
//!     .loop_mode(true);
//! ```

use rhai::{CustomType, Engine, TypeBuilder};
use std::path::PathBuf;
use vibelang_core::traits::SampleConfig;

use crate::context;

/// Handle to a loaded sample.
#[derive(Debug, Clone, CustomType)]
pub struct SampleHandle {
    /// Sample ID name.
    pub id: String,
    /// Path to the audio file.
    pub path: PathBuf,
    /// Buffer ID (assigned by runtime).
    pub buffer_id: u32,
}

impl SampleHandle {
    /// Create a new sample handle.
    pub fn new(id: String, path: PathBuf) -> Self {
        Self {
            id,
            path,
            buffer_id: 0,
        }
    }

    /// Create a pending sample handle for recordings.
    ///
    /// The buffer isn't ready yet, but will be available once the recording completes.
    pub fn new_pending(id: String, path: String, buffer_id: i32, _channels: i32) -> Self {
        Self {
            id,
            path: PathBuf::from(path),
            buffer_id: buffer_id as u32,
        }
    }

    /// Get the sample ID.
    pub fn get_id(&mut self) -> String {
        self.id.clone()
    }

    /// Get the path.
    pub fn get_path(&mut self) -> String {
        self.path.to_string_lossy().to_string()
    }

    // === Envelope methods ===

    /// Set envelope attack time in seconds.
    pub fn attack(self, seconds: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.attack = seconds;
                }
            });
        }
        self
    }

    /// Set envelope sustain level (0.0 - 1.0).
    pub fn sustain(self, level: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.sustain = level.clamp(0.0, 1.0);
                }
            });
        }
        self
    }

    /// Set envelope release time in seconds.
    pub fn release(self, seconds: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.release = seconds;
                }
            });
        }
        self
    }

    // === Playback methods ===

    /// Set playback amplitude (0.0 - 1.0+).
    pub fn amp(self, value: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.amp = value;
                }
            });
        }
        self
    }

    /// Set playback rate multiplier (1.0 = normal speed).
    pub fn rate(self, value: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.rate = value;
                }
            });
        }
        self
    }

    /// Set loop mode (true = loop, false = one-shot).
    pub fn loop_mode(self, enabled: bool) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.loop_mode = enabled;
                }
            });
        }
        self
    }

    /// Set start offset in seconds.
    pub fn offset(self, seconds: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.offset = seconds;
                }
            });
        }
        self
    }

    /// Set playback length in seconds.
    pub fn length(self, seconds: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.length = Some(seconds);
                }
            });
        }
        self
    }

    // === Warp/time-stretch methods ===

    /// Enable or disable warp (time-stretch) mode.
    pub fn warp(self, enabled: bool) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.warp = enabled;
                }
            });
        }
        self
    }

    /// Set playback speed for warp mode (1.0 = normal, 0.5 = half speed, 2.0 = double).
    pub fn speed(self, value: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.speed = value;
                    config.warp = true; // Auto-enable warp when speed is set
                }
            });
        }
        self
    }

    /// Set pitch multiplier for warp mode (1.0 = original, 2.0 = octave up, 0.5 = octave down).
    pub fn pitch(self, value: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.pitch = value;
                    config.warp = true; // Auto-enable warp when pitch is set
                }
            });
        }
        self
    }

    /// Shift pitch by semitones (-12 = octave down, 12 = octave up).
    pub fn semitones(self, semitones: f64) -> Self {
        let pitch = (2.0_f64).powf(semitones / 12.0);
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.pitch = pitch;
                    config.warp = true; // Auto-enable warp when pitch is set
                }
            });
        }
        self
    }

    /// Set target BPM for auto-speed calculation.
    pub fn warp_to_bpm(self, target_bpm: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.target_bpm = Some(target_bpm);
                    config.warp = true;
                }
            });
        }
        self
    }

    /// Set granular window size for warp mode (default: 0.1 seconds).
    pub fn window_size(self, seconds: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.window_size = seconds.clamp(0.01, 1.0);
                }
            });
        }
        self
    }

    /// Set number of overlapping grains for warp mode (default: 8).
    pub fn overlaps(self, count: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.overlaps = count.clamp(1.0, 32.0);
                }
            });
        }
        self
    }

    // === Slicing ===

    /// Create a slice of the sample from start to end (in seconds).
    pub fn slice(self, start_seconds: f64, end_seconds: f64) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.offset = start_seconds;
                    config.length = Some(end_seconds - start_seconds);
                }
            });
        }
        self
    }
}

/// Load a sample from a file or URL.
///
/// On native platforms, the path is resolved relative to the current script file.
/// On WASM, the path is used as-is (typically a URL or identifier).
pub fn sample(id: String, path: String) -> SampleHandle {
    let sample_id = context::get_or_create_sample_id(&id);

    // Resolve path relative to current script file (native only)
    #[cfg(not(target_arch = "wasm32"))]
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

    // On WASM, just use the path as-is (could be URL or identifier)
    #[cfg(target_arch = "wasm32")]
    let resolved_path = PathBuf::from(&path);

    let config = SampleConfig::new(resolved_path.clone());

    context::with_state(|state| {
        state.samples.insert(sample_id, config);
    });

    SampleHandle::new(id, resolved_path)
}

/// Register sample API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    engine.build_type::<SampleHandle>();

    // Constructor
    engine.register_fn("sample", sample);

    // ID and path getters
    engine.register_fn("id", SampleHandle::get_id);
    engine.register_get("id", SampleHandle::get_id);
    engine.register_fn("path", SampleHandle::get_path);
    engine.register_get("path", SampleHandle::get_path);

    // Envelope methods
    engine.register_fn("attack", SampleHandle::attack);
    engine.register_fn("sustain", SampleHandle::sustain);
    engine.register_fn("release", SampleHandle::release);

    // Playback methods
    engine.register_fn("amp", SampleHandle::amp);
    engine.register_fn("rate", SampleHandle::rate);
    engine.register_fn("loop_mode", SampleHandle::loop_mode);
    engine.register_fn("offset", SampleHandle::offset);
    engine.register_fn("length", SampleHandle::length);

    // Warp/time-stretch methods
    engine.register_fn("warp", SampleHandle::warp);
    engine.register_fn("speed", SampleHandle::speed);
    engine.register_fn("pitch", SampleHandle::pitch);
    engine.register_fn("semitones", SampleHandle::semitones);
    engine.register_fn("warp_to_bpm", SampleHandle::warp_to_bpm);
    engine.register_fn("window_size", SampleHandle::window_size);
    engine.register_fn("overlaps", SampleHandle::overlaps);

    // Slicing
    engine.register_fn("slice", SampleHandle::slice);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a SampleHandle for testing.
    fn test_sample(id: &str, path: &str) -> SampleHandle {
        SampleHandle {
            id: id.to_string(),
            path: PathBuf::from(path),
            buffer_id: 0,
        }
    }

    // ==================== Constructor Tests ====================

    #[test]
    fn test_sample_handle_new() {
        let sample = SampleHandle::new("kick".to_string(), PathBuf::from("samples/kick.wav"));
        assert_eq!(sample.id, "kick");
        assert_eq!(sample.path, PathBuf::from("samples/kick.wav"));
        assert_eq!(sample.buffer_id, 0);
    }

    #[test]
    fn test_sample_handle_new_pending() {
        let sample =
            SampleHandle::new_pending("recording".to_string(), "output/rec.wav".to_string(), 42, 2);
        assert_eq!(sample.id, "recording");
        assert_eq!(sample.path, PathBuf::from("output/rec.wav"));
        assert_eq!(sample.buffer_id, 42);
    }

    // ==================== Getter Tests ====================

    #[test]
    fn test_sample_get_id() {
        let mut sample = test_sample("my_sample", "path/to/sample.wav");
        assert_eq!(sample.get_id(), "my_sample");
    }

    #[test]
    fn test_sample_get_path() {
        let mut sample = test_sample("test", "samples/drums/kick.wav");
        assert_eq!(sample.get_path(), "samples/drums/kick.wav");
    }

    #[test]
    fn test_sample_get_path_with_spaces() {
        let mut sample = test_sample("test", "samples/my sounds/kick 01.wav");
        assert_eq!(sample.get_path(), "samples/my sounds/kick 01.wav");
    }

    // ==================== Chainability Tests ====================

    #[test]
    fn test_sample_methods_return_self() {
        // Test that builder methods return Self for chaining
        // (We can't test the actual state changes without context,
        // but we can verify the chaining works)
        let sample = test_sample("test", "test.wav");

        // All these should compile and return SampleHandle
        let _ = sample.clone().attack(0.01);
        let _ = sample.clone().sustain(0.8);
        let _ = sample.clone().release(0.1);
        let _ = sample.clone().amp(0.5);
        let _ = sample.clone().rate(1.5);
        let _ = sample.clone().loop_mode(true);
        let _ = sample.clone().offset(0.5);
        let _ = sample.clone().length(2.0);
        let _ = sample.clone().warp(true);
        let _ = sample.clone().speed(0.5);
        let _ = sample.clone().pitch(1.2);
        let _ = sample.clone().semitones(7.0);
        let _ = sample.clone().warp_to_bpm(120.0);
        let _ = sample.clone().window_size(0.1);
        let _ = sample.clone().overlaps(8.0);
        let _ = sample.clone().slice(1.0, 2.0);
    }

    #[test]
    fn test_sample_method_chaining() {
        let sample = test_sample("loop", "samples/loop.wav")
            .attack(0.001)
            .release(0.1)
            .amp(0.8)
            .warp(true)
            .speed(0.5);

        // After chaining, sample should still be valid
        assert_eq!(sample.id, "loop");
    }

    // ==================== SampleConfig Tests ====================

    #[test]
    fn test_sample_config_default_values() {
        let config = SampleConfig::new(PathBuf::from("test.wav"));

        // Verify default values
        assert_eq!(config.attack, 0.001);
        assert_eq!(config.sustain, 1.0);
        assert_eq!(config.release, 0.01);
        assert_eq!(config.amp, 1.0);
        assert_eq!(config.rate, 1.0);
        assert!(!config.loop_mode);
        assert_eq!(config.offset, 0.0);
        assert!(config.length.is_none());
        assert!(!config.warp);
        assert_eq!(config.speed, 1.0);
        assert_eq!(config.pitch, 1.0);
        assert!(config.target_bpm.is_none());
        assert_eq!(config.window_size, 0.1);
        assert_eq!(config.overlaps, 8.0);
    }
}
