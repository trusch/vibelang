//! Recording API for Rhai scripts.
//!
//! Provides audio recording capabilities with quantized start, count-in,
//! metronome support, and file saving.
//!
//! ## Example
//!
//! ```rhai
//! // Record 4 bars from the drums group with count-in
//! let take1 = record("take1")
//!     .from_group("drums")
//!     .bars(4)
//!     .count_in(2)
//!     .metronome(true)
//!     .to_file("recordings/drums_take1.wav")
//!     .apply();
//!
//! // The sample handle can be used immediately
//! // (audio arrives when recording completes)
//! let drums_voice = voice("playback").on(take1).apply();
//! ```

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::path::PathBuf;
use vibelang_core::traits::RecordingConfig as CoreRecordingConfig;

use super::sample::SampleHandle;
use crate::context;

// Suppress unused imports warning for rhai derive macro requirements
#[allow(unused_imports)]
use rhai::INT;

/// A RecordHandle represents a pending or active audio recording.
///
/// Use builder methods to configure the recording, then call `.apply()`
/// to start recording and get a sample handle.
#[derive(Clone, Debug, CustomType)]
pub struct RecordHandle {
    /// Unique identifier for this recording.
    pub id: String,
    /// Group path to record from.
    group_path: String,
    /// Length in bars (tempo-aware).
    length_bars: Option<f64>,
    /// Length in beats (tempo-aware).
    length_beats: Option<f64>,
    /// Length in seconds (fixed time).
    length_seconds: Option<f64>,
    /// Count-in bars before recording starts.
    count_in_bars: f64,
    /// Play metronome during count-in/recording.
    metronome: bool,
    /// File path to save recording.
    file_path: Option<String>,
    /// Start immediately (no quantization).
    start_immediately: bool,
    /// Number of channels (1 or 2).
    num_channels: i64,
}

impl RecordHandle {
    /// Create a new recording handle.
    pub fn new(_ctx: NativeCallContext, id: String) -> Self {
        Self {
            id,
            group_path: context::current_group_path(),
            length_bars: None,
            length_beats: None,
            length_seconds: None,
            count_in_bars: 0.0,
            metronome: false,
            file_path: None,
            start_immediately: false,
            num_channels: 2,
        }
    }

    // === Getters ===

    /// Get the recording ID.
    pub fn get_id(&mut self) -> String {
        self.id.clone()
    }

    /// Get the group path.
    pub fn get_group_path(&mut self) -> String {
        self.group_path.clone()
    }

    // === Length Configuration ===

    /// Set recording length in bars.
    pub fn bars(mut self, num_bars: f64) -> Self {
        self.length_bars = Some(num_bars);
        self.length_beats = None;
        self.length_seconds = None;
        self
    }

    /// Set recording length in bars (integer variant).
    pub fn bars_int(self, num_bars: i64) -> Self {
        self.bars(num_bars as f64)
    }

    /// Set recording length in beats.
    pub fn beats(mut self, num_beats: f64) -> Self {
        self.length_bars = None;
        self.length_beats = Some(num_beats);
        self.length_seconds = None;
        self
    }

    /// Set recording length in beats (integer variant).
    pub fn beats_int(self, num_beats: i64) -> Self {
        self.beats(num_beats as f64)
    }

    /// Set recording length in seconds (fixed time, ignores tempo).
    pub fn seconds(mut self, secs: f64) -> Self {
        self.length_bars = None;
        self.length_beats = None;
        self.length_seconds = Some(secs);
        self
    }

    /// Set recording length in seconds (integer variant).
    pub fn seconds_int(self, secs: i64) -> Self {
        self.seconds(secs as f64)
    }

    // === Group Configuration ===

    /// Record from a specific group instead of the current group.
    pub fn from_group(mut self, group_path: String) -> Self {
        self.group_path = context::resolve_group_reference(&group_path).unwrap_or(group_path);
        self
    }

    // === Recording Options ===

    /// Add count-in bars before recording starts.
    pub fn count_in(mut self, bars: f64) -> Self {
        self.count_in_bars = bars;
        self
    }

    /// Enable/disable metronome during count-in and recording.
    pub fn metronome(mut self, enabled: bool) -> Self {
        self.metronome = enabled;
        self
    }

    /// Save recording to a file.
    pub fn to_file(mut self, path: String) -> Self {
        self.file_path = Some(path);
        self
    }

    /// Start immediately without quantization.
    pub fn immediate(mut self) -> Self {
        self.start_immediately = true;
        self
    }

    /// Set the number of channels (1 = mono, 2 = stereo).
    pub fn channels(mut self, num: i64) -> Self {
        self.num_channels = num.clamp(1, 2);
        self
    }

    // === Apply ===

    /// Start the recording and return a sample handle.
    ///
    /// The sample handle can be used immediately, though the audio
    /// won't be available until the recording completes.
    pub fn apply(self) -> SampleHandle {
        let recording_id = context::get_or_create_recording_id(&self.id);
        let group_id = context::get_or_create_group_id(&self.group_path);

        // Get tempo and time signature for beat calculations
        let (_tempo, time_sig) = (context::get_tempo(), context::get_time_signature());
        let beats_per_bar = time_sig.0 as f64 * (4.0 / time_sig.1 as f64);

        // Calculate length in beats
        let length_beats = self
            .length_bars
            .map(|bars| bars * beats_per_bar)
            .or(self.length_beats);

        // Calculate count-in in beats
        let count_in_beats = self.count_in_bars * beats_per_bar;

        // Save file path string for the sample handle (before moving into closure)
        let file_path_str = self.file_path.clone().unwrap_or_default();

        // Resolve file path
        let file_path = self.file_path.map(|p| {
            if let Some(current_file) = context::get_current_file() {
                if let Some(parent) = current_file.parent() {
                    let resolved = parent.join(&p);
                    return resolved;
                }
            }
            PathBuf::from(p)
        });

        // Create the recording config
        let config = CoreRecordingConfig {
            group: group_id,
            length_beats,
            length_seconds: self.length_seconds,
            start_beat: None, // Will be calculated by the runtime
            count_in_beats,
            metronome: self.metronome,
            file_path,
            num_channels: self.num_channels as u8,
        };

        // Store in script state for the runtime to pick up
        context::with_state(|state| {
            state.recordings.insert(recording_id, config);
        });

        // Calculate approximate buffer ID for the sample handle
        // The actual buffer will be allocated by the runtime
        let buffer_id = recording_id.raw() as i32;

        SampleHandle::new_pending(self.id, file_path_str, buffer_id, self.num_channels as i32)
    }
}

/// Create a new recording handle.
///
/// Usage: `record("take1").bars(4).apply()`
pub fn record(ctx: NativeCallContext, id: String) -> RecordHandle {
    RecordHandle::new(ctx, id)
}

/// Stop a recording by ID.
pub fn stop_recording(id: String) {
    let _recording_id = context::get_or_create_recording_id(&id);
    // The actual stop message would be sent via the runtime
    // For now, this just logs the intent
    log::info!("[RECORD] Stop requested for recording '{}'", id);
}

/// Cancel a pending or active recording by ID.
pub fn cancel_recording(id: String) {
    let _recording_id = context::get_or_create_recording_id(&id);
    // The actual cancel message would be sent via the runtime
    log::info!("[RECORD] Cancel requested for recording '{}'", id);
}

/// Register the recording API with a Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register RecordHandle type
    engine.build_type::<RecordHandle>();

    // Constructor
    engine.register_fn("record", record);

    // Getters
    engine.register_fn("id", RecordHandle::get_id);
    engine.register_get("id", RecordHandle::get_id);
    engine.register_fn("group_path", RecordHandle::get_group_path);
    engine.register_get("group_path", RecordHandle::get_group_path);

    // Length configuration (builder methods)
    engine.register_fn("bars", RecordHandle::bars);
    engine.register_fn("bars", RecordHandle::bars_int);
    engine.register_fn("beats", RecordHandle::beats);
    engine.register_fn("beats", RecordHandle::beats_int);
    engine.register_fn("seconds", RecordHandle::seconds);
    engine.register_fn("seconds", RecordHandle::seconds_int);

    // Group configuration
    engine.register_fn("from_group", RecordHandle::from_group);

    // Recording options (builder methods)
    engine.register_fn("count_in", RecordHandle::count_in);
    engine.register_fn("metronome", RecordHandle::metronome);
    engine.register_fn("to_file", RecordHandle::to_file);
    engine.register_fn("immediate", RecordHandle::immediate);
    engine.register_fn("channels", RecordHandle::channels);

    // Apply
    engine.register_fn("apply", RecordHandle::apply);

    // Control functions
    engine.register_fn("stop_recording", stop_recording);
    engine.register_fn("cancel_recording", cancel_recording);
}
