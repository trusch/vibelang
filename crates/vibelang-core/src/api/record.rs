//! Recording API for VibeLang.
//!
//! Provides group-aware audio recording with quantized start, count-in,
//! metronome support, and file saving.

use crate::api::context::{self, SourceLocation};
use crate::api::sample::SampleHandle;
use crate::state::StateMessage;
use crate::timing::TimeSignature;
use rhai::{Engine, NativeCallContext};

use super::{require_handle, send_message};

// =============================================================================
// Record Handle
// =============================================================================

/// A RecordHandle represents a pending or active audio recording.
///
/// Use builder methods to configure the recording, then call `.apply()`
/// to start recording and get a sample handle.
#[derive(Clone, Debug)]
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
    /// Explicit start bar (overrides quantization).
    start_at_bar: Option<f64>,
    /// Number of channels (1 or 2).
    num_channels: i32,
    /// Source location for debugging.
    source_location: SourceLocation,
}

impl RecordHandle {
    /// Create a new recording handle.
    pub fn new(id: String) -> Self {
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
            start_at_bar: None,
            num_channels: 2,
            source_location: SourceLocation::default(),
        }
    }

    /// Create a new recording handle with source location.
    pub fn with_source_location(ctx: &NativeCallContext, id: String) -> Self {
        let pos = ctx.position();
        let mut handle = Self::new(id);
        handle.source_location = SourceLocation::new(
            context::get_current_script_file(),
            if pos.line().is_some() { Some(pos.line().unwrap() as u32) } else { None },
            if pos.position().is_some() { Some(pos.position().unwrap() as u32) } else { None },
        );
        handle
    }

    // === Length Configuration ===

    /// Set recording length in bars.
    pub fn bars(mut self, num_bars: f64) -> Self {
        self.length_bars = Some(num_bars);
        self.length_beats = None;
        self.length_seconds = None;
        self
    }

    /// Set recording length in beats.
    pub fn beats(mut self, num_beats: f64) -> Self {
        self.length_bars = None;
        self.length_beats = Some(num_beats);
        self.length_seconds = None;
        self
    }

    /// Set recording length in seconds (fixed time, ignores tempo).
    pub fn seconds(mut self, secs: f64) -> Self {
        self.length_bars = None;
        self.length_beats = None;
        self.length_seconds = Some(secs);
        self
    }

    // === Group Configuration ===

    /// Record from a specific group instead of the current group.
    pub fn from_group(mut self, group_path: String) -> Self {
        self.group_path = group_path;
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

    /// Start at a specific bar (overrides default quantization).
    pub fn start_at_bar(mut self, bar: f64) -> Self {
        self.start_at_bar = Some(bar);
        self.start_immediately = false;
        self
    }

    /// Set the number of channels (1 = mono, 2 = stereo).
    pub fn channels(mut self, num: i64) -> Self {
        self.num_channels = num.clamp(1, 2) as i32;
        self
    }

    // === Apply ===

    /// Start the recording and return a sample handle.
    ///
    /// The sample handle can be used immediately, though the audio
    /// won't be available until the recording completes.
    ///
    /// If a recording with the same ID is already in progress or completed,
    /// returns the existing sample handle without starting a new recording.
    pub fn apply(self) -> SampleHandle {
        let handle = require_handle();

        // Check if a sample with this ID already exists (recording completed)
        let existing_sample = handle.with_state(|state| {
            state.samples.get(&self.id).map(|info| {
                (info.buffer_id, info.num_channels)
            })
        });

        if let Some((buffer_id, num_channels)) = existing_sample {
            log::info!(
                "[RECORD] Recording '{}' already completed, reusing existing sample",
                self.id
            );
            return SampleHandle::new_pending(
                self.id.clone(),
                self.file_path.unwrap_or_default(),
                buffer_id,
                num_channels,
            );
        }

        // Check if a recording with this ID is already in progress
        #[cfg(feature = "native")]
        {
            let existing_recording = handle.with_state(|state| {
                state.audio_recordings.get(&self.id).map(|rec| {
                    (rec.buffer_id, rec.num_channels)
                })
            });

            if let Some((buffer_id, num_channels)) = existing_recording {
                log::info!(
                    "[RECORD] Recording '{}' already in progress, reusing existing handle",
                    self.id
                );
                return SampleHandle::new_pending(
                    self.id.clone(),
                    self.file_path.unwrap_or_default(),
                    buffer_id,
                    num_channels,
                );
            }
        }

        // Get tempo, time signature, and current beat from state
        let (tempo, time_sig, current_beat) = handle.with_state(|state| {
            (state.tempo, state.time_signature, state.current_beat)
        });

        // Calculate length in beats
        let length_beats = if let Some(bars) = self.length_bars {
            Some(bars * time_sig.beats_per_bar())
        } else if let Some(beats) = self.length_beats {
            Some(beats)
        } else {
            None
        };

        // Calculate start beat with quantization
        let start_beat = if self.start_immediately {
            current_beat
        } else if let Some(bar) = self.start_at_bar {
            (bar - 1.0) * time_sig.beats_per_bar() // bars are 1-indexed
        } else if let Some(beats) = length_beats {
            // Default: quantize to next multiple of recording length
            let length_bars = beats / time_sig.beats_per_bar();
            calculate_quantized_start_beat(current_beat, length_bars, &time_sig)
        } else if let Some(secs) = self.length_seconds {
            // For seconds-based, just start at next bar
            let current_bar = current_beat / time_sig.beats_per_bar();
            (current_bar.floor() + 1.0) * time_sig.beats_per_bar()
        } else {
            // No length specified, default to 4 bars starting at next bar
            let current_bar = current_beat / time_sig.beats_per_bar();
            (current_bar.floor() + 1.0) * time_sig.beats_per_bar()
        };

        // Calculate count-in in beats
        let count_in_beats = self.count_in_bars * time_sig.beats_per_bar();

        // Allocate a buffer ID
        let buffer_id = handle.with_state_mut(|state| state.allocate_buffer_id());

        // Send the StartRecording message
        send_message(
            &handle,
            StateMessage::StartRecording {
                id: self.id.clone(),
                group_path: self.group_path.clone(),
                buffer_id,
                length_beats,
                length_seconds: self.length_seconds,
                start_beat,
                count_in_beats,
                metronome: self.metronome,
                file_path: self.file_path.clone(),
                num_channels: self.num_channels,
                source_location: self.source_location.clone(),
            },
            &format!("record.apply({})", self.id),
        );

        log::info!(
            "[RECORD] Started recording '{}' from group '{}', {} channels, start beat: {:.2}",
            self.id,
            self.group_path,
            self.num_channels,
            start_beat
        );

        // Create and return a SampleHandle
        // The sample will be usable once the recording completes
        SampleHandle::new_pending(
            self.id.clone(),
            self.file_path.unwrap_or_default(),
            buffer_id,
            self.num_channels,
        )
    }
}

// =============================================================================
// Quantization Logic
// =============================================================================

/// Calculate the next beat where recording should start, quantized to bar boundaries.
///
/// For a recording of N bars, this finds the next bar B where B % N == 0.
/// This ensures recordings always start on "even" boundaries relative to their length.
///
/// Examples:
/// - Recording 2 bars, current bar is 3 → start at bar 4 (4 % 2 == 0)
/// - Recording 4 bars, current bar is 5 → start at bar 8 (8 % 4 == 0)
/// - Recording 1 bar, current bar is 2.5 → start at bar 3
fn calculate_quantized_start_beat(
    current_beat: f64,
    length_bars: f64,
    time_sig: &TimeSignature,
) -> f64 {
    let beats_per_bar = time_sig.beats_per_bar();
    let current_bar = current_beat / beats_per_bar;

    // Always move to at least the next bar
    let next_bar = current_bar.floor() + 1.0;
    let length_bars_int = length_bars.ceil().max(1.0) as i64;

    // Find next bar that's a multiple of length_bars
    let quantized_bar = if length_bars_int > 0 {
        let next_bar_int = next_bar as i64;
        let remainder = next_bar_int % length_bars_int;
        if remainder == 0 {
            next_bar
        } else {
            next_bar + (length_bars_int - remainder) as f64
        }
    } else {
        next_bar
    };

    quantized_bar * beats_per_bar
}

// =============================================================================
// Rhai API Functions
// =============================================================================

/// Create a new recording handle.
///
/// Usage: `record("take1").bars(4).apply()`
fn record(ctx: NativeCallContext, id: String) -> RecordHandle {
    RecordHandle::with_source_location(&ctx, id)
}

/// Stop a recording by ID.
fn stop_recording(id: String) {
    let handle = require_handle();
    send_message(
        &handle,
        StateMessage::StopRecording { id: id.clone() },
        &format!("stop_recording({})", id),
    );
}

/// Cancel a pending or active recording by ID.
fn cancel_recording(id: String) {
    let handle = require_handle();
    send_message(
        &handle,
        StateMessage::CancelRecording { id: id.clone() },
        &format!("cancel_recording({})", id),
    );
}

// =============================================================================
// Rhai Registration
// =============================================================================

/// Register the recording API with a Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register RecordHandle type
    engine.register_type::<RecordHandle>();

    // Constructor
    engine.register_fn("record", record);

    // Length configuration (builder methods)
    engine.register_fn("bars", RecordHandle::bars);
    engine.register_fn("beats", RecordHandle::beats);
    engine.register_fn("seconds", RecordHandle::seconds);

    // Group configuration
    engine.register_fn("from_group", RecordHandle::from_group);

    // Recording options (builder methods)
    engine.register_fn("count_in", RecordHandle::count_in);
    engine.register_fn("metronome", RecordHandle::metronome);
    engine.register_fn("to_file", RecordHandle::to_file);
    engine.register_fn("immediate", RecordHandle::immediate);
    engine.register_fn("start_at_bar", RecordHandle::start_at_bar);
    engine.register_fn("channels", RecordHandle::channels);

    // Apply
    engine.register_fn("apply", RecordHandle::apply);

    // Control functions
    engine.register_fn("stop_recording", stop_recording);
    engine.register_fn("cancel_recording", cancel_recording);
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::TimeSignature;

    #[test]
    fn test_quantized_start_basic() {
        let time_sig = TimeSignature::new(4, 4); // 4 beats per bar

        // Current beat 10 (bar 2.5), recording 2 bars → start at bar 4 (beat 16)
        let start = calculate_quantized_start_beat(10.0, 2.0, &time_sig);
        assert_eq!(start, 16.0); // Bar 4 = beat 16

        // Current beat 20 (bar 5), recording 4 bars → start at bar 8 (beat 32)
        let start = calculate_quantized_start_beat(20.0, 4.0, &time_sig);
        assert_eq!(start, 32.0); // Bar 8 = beat 32
    }

    #[test]
    fn test_quantized_start_already_on_boundary() {
        let time_sig = TimeSignature::new(4, 4);

        // Current beat 16 (bar 4 exactly), recording 2 bars → start at bar 6 (beat 24)
        // because we always go to the NEXT bar first
        let start = calculate_quantized_start_beat(16.0, 2.0, &time_sig);
        assert_eq!(start, 24.0); // Bar 6 = beat 24
    }

    #[test]
    fn test_quantized_start_one_bar() {
        let time_sig = TimeSignature::new(4, 4);

        // Recording 1 bar always starts at next bar
        let start = calculate_quantized_start_beat(10.0, 1.0, &time_sig);
        assert_eq!(start, 12.0); // Bar 3 = beat 12
    }
}
