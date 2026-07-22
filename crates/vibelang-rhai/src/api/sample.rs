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

    /// Get the underlying SC buffer ID.
    pub fn get_bufnum(&mut self) -> i64 {
        self.buffer_id as i64
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

    // === Trigger mode methods ===

    /// Set trigger mode to one-shot (ignore note-off).
    pub fn one_shot(self) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.trigger_mode = "one_shot".to_string();
                }
            });
        }
        self
    }

    /// Set trigger mode to gate (release on note-off, default).
    pub fn gate(self) -> Self {
        if let Some(sample_id) = context::get_sample_id(&self.id) {
            context::with_state(|state| {
                if let Some(config) = state.samples.get_mut(&sample_id) {
                    config.trigger_mode = "gate".to_string();
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

    let mut config = SampleConfig::new(resolved_path.clone());
    // Capture the file's mtime at script-eval time: the reload diff uses
    // it to detect an overwritten file at an unchanged path (re-recorded
    // sample) and reload the buffer.
    config.refresh_mtime();

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
    engine.register_fn("bufnum", SampleHandle::get_bufnum);
    engine.register_get("bufnum", SampleHandle::get_bufnum);
    engine.register_fn("buffer_id", SampleHandle::get_bufnum);
    engine.register_get("buffer_id", SampleHandle::get_bufnum);

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

    // Trigger mode
    engine.register_fn("one_shot", SampleHandle::one_shot);
    engine.register_fn("gate", SampleHandle::gate);

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

use rhai::{EvalAltResult, Position};
use vibelang_core::candidate::{
    AuthoringDeclaration, Cancellation, CandidateError, CanonicalF64, Composition,
    DeclarationOwner, DeclarationPayload, GroupScope, LifecycleAction, LifecycleMetadata,
    SampleAuthoring, SampleKind, SampleTriggerAuthoring, SampleWarpAuthoring, TerminalEffect,
};

use crate::foundation::{self, BuilderBase, FoundationError, Observation, RefBase};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SampleRef {
    base: RefBase,
}

impl SampleRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<SampleKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        let source = foundation::operation_source(&self.base, "remove")?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::Remove,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SampleWarpV2 {
    speed: f64,
    pitch: f64,
    target_bpm: Option<f64>,
    window_size: f64,
    overlaps: u8,
}

impl Default for SampleWarpV2 {
    fn default() -> Self {
        Self {
            speed: 1.0,
            pitch: 1.0,
            target_bpm: None,
            window_size: 0.1,
            overlaps: 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SampleBuilder {
    base: BuilderBase,
    source: String,
    attack: f64,
    sustain: f64,
    release: f64,
    amp: f64,
    rate: f64,
    loop_mode: bool,
    offset: f64,
    length: Option<f64>,
    trigger: SampleTriggerAuthoring,
    warp: Option<SampleWarpV2>,
}

impl SampleBuilder {
    pub fn new(base: BuilderBase, source: String) -> Result<Self, FoundationError> {
        if source.is_empty() || source.trim() != source {
            return Err(CandidateError::InvalidAuthoring(
                "Sample source must be a non-empty path without surrounding whitespace".into(),
            )
            .into());
        }
        Ok(Self {
            base,
            source,
            attack: 0.001,
            sustain: 1.0,
            release: 0.01,
            amp: 1.0,
            rate: 1.0,
            loop_mode: false,
            offset: 0.0,
            length: None,
            trigger: SampleTriggerAuthoring::Gate,
            warp: None,
        })
    }

    fn strict(value: f64, valid: bool, message: &str) -> Result<f64, FoundationError> {
        if !value.is_finite() || !valid {
            return Err(CandidateError::InvalidAuthoring(message.into()).into());
        }
        Ok(value)
    }

    fn warp_mut(&mut self) -> &mut SampleWarpV2 {
        self.warp.get_or_insert_with(SampleWarpV2::default)
    }

    pub fn attack(mut self, seconds: f64) -> Result<Self, FoundationError> {
        self.attack = Self::strict(
            seconds,
            seconds >= 0.0,
            "Sample attack must be finite and non-negative",
        )?;
        Ok(self)
    }

    pub fn sustain(mut self, level: f64) -> Result<Self, FoundationError> {
        self.sustain = Self::strict(
            level,
            (0.0..=1.0).contains(&level),
            "Sample sustain must be in 0.0..=1.0",
        )?;
        Ok(self)
    }

    pub fn release(mut self, seconds: f64) -> Result<Self, FoundationError> {
        self.release = Self::strict(
            seconds,
            seconds >= 0.0,
            "Sample release must be finite and non-negative",
        )?;
        Ok(self)
    }

    pub fn amp(mut self, value: f64) -> Result<Self, FoundationError> {
        self.amp = Self::strict(
            value,
            value >= 0.0,
            "Sample amp must be finite and non-negative",
        )?;
        Ok(self)
    }

    pub fn rate(mut self, value: f64) -> Result<Self, FoundationError> {
        self.rate = Self::strict(
            value,
            value > 0.0,
            "Sample rate must be finite and positive",
        )?;
        Ok(self)
    }

    #[must_use]
    pub fn loop_mode(mut self, enabled: bool) -> Self {
        self.loop_mode = enabled;
        self
    }

    pub fn offset(mut self, seconds: f64) -> Result<Self, FoundationError> {
        self.offset = Self::strict(
            seconds,
            seconds >= 0.0,
            "Sample offset must be finite and non-negative",
        )?;
        Ok(self)
    }

    pub fn length(mut self, seconds: f64) -> Result<Self, FoundationError> {
        self.length = Some(Self::strict(
            seconds,
            seconds > 0.0,
            "Sample length must be finite and positive",
        )?);
        Ok(self)
    }

    /// Effective forwarding alias for `.offset(start).length(end - start)`.
    pub fn slice(self, start_seconds: f64, end_seconds: f64) -> Result<Self, FoundationError> {
        if !start_seconds.is_finite() || !end_seconds.is_finite() || end_seconds <= start_seconds {
            return Err(CandidateError::InvalidAuthoring(
                "Sample slice needs finite bounds with end after start".into(),
            )
            .into());
        }
        self.offset(start_seconds)?
            .length(end_seconds - start_seconds)
    }

    #[must_use]
    pub fn warp(mut self, enabled: bool) -> Self {
        if enabled {
            self.warp_mut();
        } else {
            self.warp = None;
        }
        self
    }

    pub fn speed(mut self, value: f64) -> Result<Self, FoundationError> {
        let value = Self::strict(
            value,
            value > 0.0,
            "Sample speed must be finite and positive",
        )?;
        self.warp_mut().speed = value;
        Ok(self)
    }

    pub fn pitch(mut self, value: f64) -> Result<Self, FoundationError> {
        let value = Self::strict(
            value,
            value > 0.0,
            "Sample pitch must be finite and positive",
        )?;
        self.warp_mut().pitch = value;
        Ok(self)
    }

    /// Effective forwarding alias for `.pitch(2^(semitones / 12))`.
    pub fn semitones(self, semitones: f64) -> Result<Self, FoundationError> {
        if !semitones.is_finite() {
            return Err(
                CandidateError::InvalidAuthoring("Sample semitones must be finite".into()).into(),
            );
        }
        self.pitch((2.0_f64).powf(semitones / 12.0))
    }

    pub fn warp_to_bpm(mut self, target_bpm: f64) -> Result<Self, FoundationError> {
        let value = Self::strict(
            target_bpm,
            target_bpm > 0.0,
            "Sample warp BPM must be finite and positive",
        )?;
        self.warp_mut().target_bpm = Some(value);
        Ok(self)
    }

    pub fn window_size(mut self, seconds: f64) -> Result<Self, FoundationError> {
        let value = Self::strict(
            seconds,
            (0.01..=1.0).contains(&seconds),
            "Sample window size must be in 0.01..=1.0 seconds",
        )?;
        self.warp_mut().window_size = value;
        Ok(self)
    }

    pub fn overlaps(mut self, count: i64) -> Result<Self, FoundationError> {
        if !(1..=32).contains(&count) {
            return Err(CandidateError::InvalidAuthoring(
                "Sample overlaps must be in 1..=32".into(),
            )
            .into());
        }
        self.warp_mut().overlaps = count as u8;
        Ok(self)
    }

    #[must_use]
    pub fn one_shot(mut self) -> Self {
        self.trigger = SampleTriggerAuthoring::OneShot;
        self
    }

    #[must_use]
    pub fn gate(mut self) -> Self {
        self.trigger = SampleTriggerAuthoring::Gate;
        self
    }

    pub fn apply(self) -> Result<SampleRef, FoundationError> {
        let warp = self
            .warp
            .map(|warp| {
                Ok::<_, CandidateError>(SampleWarpAuthoring {
                    speed: CanonicalF64::new(warp.speed)?,
                    pitch: CanonicalF64::new(warp.pitch)?,
                    target_bpm: warp.target_bpm.map(CanonicalF64::new).transpose()?,
                    window_size: CanonicalF64::new(warp.window_size)?,
                    overlaps: warp.overlaps,
                })
            })
            .transpose()?;
        let declaration = SampleAuthoring {
            source: self.source.clone(),
            attack: CanonicalF64::new(self.attack)?,
            sustain: CanonicalF64::new(self.sustain)?,
            release: CanonicalF64::new(self.release)?,
            amp: CanonicalF64::new(self.amp)?,
            rate: CanonicalF64::new(self.rate)?,
            loop_mode: self.loop_mode,
            offset: CanonicalF64::new(self.offset)?,
            length: self.length.map(CanonicalF64::new).transpose()?,
            trigger: self.trigger,
            warp,
        };
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::Sample(declaration))?;
        let owner = DeclarationOwner::Structural(self.base.source().syntax_key().clone());
        let reference = self.base.apply(
            owner,
            LifecycleMetadata::register(Composition::Standalone),
            payload,
        )?;
        SampleRef::new(reference)
    }
}

pub(crate) fn sample_builder_v2(
    name: String,
    source: String,
) -> Result<SampleBuilder, Box<EvalAltResult>> {
    SampleBuilder::new(
        foundation::authoring_builder::<SampleKind>(&name, GroupScope::root())
            .map_err(|error| sample_v2_error(error, Position::NONE))?,
        source,
    )
    .map_err(|error| sample_v2_error(error, Position::NONE))
}

pub(crate) fn sample_ref_v2(name: String) -> Result<SampleRef, Box<EvalAltResult>> {
    SampleRef::new(
        foundation::authoring_ref::<SampleKind>(&name, GroupScope::root())
            .map_err(|error| sample_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| sample_v2_error(error, Position::NONE))
}

fn sample_v2_error(error: FoundationError, position: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        error.to_string().into(),
        position,
    ))
}

pub(crate) fn install_v2(engine: &mut Engine) {
    fn strict<T>(result: Result<T, FoundationError>) -> Result<T, Box<EvalAltResult>> {
        result.map_err(|error| sample_v2_error(error, Position::NONE))
    }

    engine
        .register_type_with_name::<SampleBuilder>("SampleBuilder")
        .register_type_with_name::<SampleRef>("SampleRef")
        .register_fn("sample", sample_builder_v2)
        .register_fn("sample_ref", sample_ref_v2)
        .register_fn("attack", |builder: SampleBuilder, seconds: f64| {
            strict(builder.attack(seconds))
        })
        .register_fn("sustain", |builder: SampleBuilder, level: f64| {
            strict(builder.sustain(level))
        })
        .register_fn("release", |builder: SampleBuilder, seconds: f64| {
            strict(builder.release(seconds))
        })
        .register_fn("amp", |builder: SampleBuilder, value: f64| {
            strict(builder.amp(value))
        })
        .register_fn("rate", |builder: SampleBuilder, value: f64| {
            strict(builder.rate(value))
        })
        .register_fn("loop_mode", SampleBuilder::loop_mode)
        .register_fn("offset", |builder: SampleBuilder, seconds: f64| {
            strict(builder.offset(seconds))
        })
        .register_fn("length", |builder: SampleBuilder, seconds: f64| {
            strict(builder.length(seconds))
        })
        .register_fn("slice", |builder: SampleBuilder, start: f64, end: f64| {
            strict(builder.slice(start, end))
        })
        .register_fn("warp", SampleBuilder::warp)
        .register_fn("speed", |builder: SampleBuilder, value: f64| {
            strict(builder.speed(value))
        })
        .register_fn("pitch", |builder: SampleBuilder, value: f64| {
            strict(builder.pitch(value))
        })
        .register_fn("semitones", |builder: SampleBuilder, semitones: f64| {
            strict(builder.semitones(semitones))
        })
        .register_fn("warp_to_bpm", |builder: SampleBuilder, bpm: f64| {
            strict(builder.warp_to_bpm(bpm))
        })
        .register_fn("window_size", |builder: SampleBuilder, seconds: f64| {
            strict(builder.window_size(seconds))
        })
        .register_fn("overlaps", |builder: SampleBuilder, count: i64| {
            strict(builder.overlaps(count))
        })
        .register_fn("one_shot", SampleBuilder::one_shot)
        .register_fn("gate", SampleBuilder::gate)
        .register_fn("apply", |builder: SampleBuilder| strict(builder.apply()))
        .register_fn("remove", |reference: SampleRef| strict(reference.remove()))
        .register_fn("status", |reference: SampleRef| strict(reference.status()));
}

#[cfg(test)]
mod v2_tests {
    use super::*;
    use vibelang_core::candidate::EntityKind;

    fn v2_identity() -> vibelang_core::candidate::EvaluationIdentity {
        vibelang_core::candidate::EvaluationIdentity::new(
            vibelang_core::candidate::LanguageContract::v2(
                vibelang_core::candidate::ContractDigest::from_bytes(b"sample-v2-test"),
            ),
            vibelang_core::candidate::EngineInstanceId::new(),
            vibelang_core::mutation::RuntimeEpoch::new(),
        )
    }

    fn builder(name: &str, source: &str) -> SampleBuilder {
        SampleBuilder::new(
            foundation::authoring_builder::<SampleKind>(name, GroupScope::root()).unwrap(),
            source.into(),
        )
        .unwrap()
    }

    #[test]
    fn v2_sample_configuration_is_pure_and_strict() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let configured = builder("kick", "samples/kick.wav")
            .attack(0.002)
            .unwrap()
            .sustain(0.8)
            .unwrap()
            .rate(1.5)
            .unwrap()
            .loop_mode(true);

        assert!(matches!(
            configured.clone().sustain(1.5),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            configured.clone().rate(0.0),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            configured.clone().window_size(0.005),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            configured.clone().overlaps(33),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            configured.clone().slice(2.0, 1.0),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            SampleBuilder::new(
                foundation::authoring_builder::<SampleKind>("bad", GroupScope::root()).unwrap(),
                " padded.wav".into(),
            ),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));

        let candidate = foundation::finish_evaluation().unwrap();
        assert!(
            candidate.declarations().is_empty(),
            "configuration alone must not declare anything"
        );
    }

    #[test]
    fn v2_sample_terminal_registers_once_and_returns_typed_ref() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let reference = builder("kick", "samples/kick.wav").apply().unwrap();
        assert_eq!(reference.base().kind(), EntityKind::Sample);
        assert!(matches!(
            reference.status(),
            Err(FoundationError::ObservationUnavailable)
        ));
        assert!(matches!(
            builder("kick", "samples/kick.wav").apply(),
            Err(FoundationError::Candidate(
                CandidateError::DuplicateDeclaration { .. }
            ))
        ));

        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.declarations().len(), 1);
        assert_eq!(
            candidate.declarations()[0].lifecycle().terminal_effect,
            TerminalEffect::Register
        );
    }

    #[test]
    fn v2_sample_forwarding_aliases_are_effective() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let via_alias = builder("kick", "samples/kick.wav").semitones(12.0).unwrap();
        let via_pitch = builder("kick", "samples/kick.wav").pitch(2.0).unwrap();
        assert_eq!(via_alias.warp, via_pitch.warp);

        let via_slice = builder("kick", "samples/kick.wav").slice(0.5, 2.0).unwrap();
        let explicit = builder("kick", "samples/kick.wav")
            .offset(0.5)
            .unwrap()
            .length(1.5)
            .unwrap();
        assert_eq!(via_slice.offset, explicit.offset);
        assert_eq!(via_slice.length, explicit.length);

        let toggled = builder("kick", "samples/kick.wav").one_shot().gate();
        assert_eq!(toggled.trigger, SampleTriggerAuthoring::Gate);
        assert!(builder("kick", "samples/kick.wav")
            .speed(0.5)
            .unwrap()
            .warp
            .is_some());
        foundation::abort_evaluation();
    }

    #[test]
    fn v2_sample_ref_remove_is_a_real_operation() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let reference = sample_ref_v2("kick".into()).unwrap();
        reference.clone().remove().unwrap();

        let candidate = foundation::finish_evaluation();
        assert!(
            candidate.is_err(),
            "an operation against an undeclared, uncataloged Sample must not resolve"
        );
    }

    #[test]
    fn v2_sample_rhai_surface_authors_from_script() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        crate::foundation::register(&mut engine);
        install_v2(&mut engine);
        let reference = engine
            .eval::<SampleRef>(
                r#"sample("kick", "samples/kick.wav")
                    .attack(0.002)
                    .slice(0.5, 2.0)
                    .semitones(12.0)
                    .one_shot()
                    .apply()"#,
            )
            .unwrap();
        assert_eq!(reference.base().kind(), EntityKind::Sample);
        assert!(engine
            .eval::<SampleBuilder>(r#"sample("bad", "samples/kick.wav").overlaps(33)"#)
            .is_err());

        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.declarations().len(), 1);
    }
}
