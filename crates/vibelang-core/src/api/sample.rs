//! Sample API for VibeLang.
//!
//! Provides sample loading, slicing, playback configuration, and BPM detection.
//! Supports time-stretching and pitch-shifting via the Warp1 UGen.

use crate::state::StateMessage;
use rhai::Engine;
use std::path::Path;

use super::context;
use super::require_handle;

// =============================================================================
// BPM Detection
// =============================================================================

/// Result of BPM analysis.
#[derive(Clone, Debug)]
pub struct BpmAnalysis {
    /// Detected BPM (0.0 if not detected)
    pub bpm: f64,
    /// Confidence level 0.0-1.0
    pub confidence: f64,
    /// Number of beats detected
    pub beat_count: usize,
}

/// Detect BPM from audio samples.
///
/// Uses aubio's tempo detection algorithm for accurate BPM estimation.
pub fn detect_bpm(samples: &[f32], sample_rate: u32) -> BpmAnalysis {
    use aubio_rs::{OnsetMode, Tempo};

    const BUF_SIZE: usize = 1024;
    const HOP_SIZE: usize = 512;

    if samples.len() < BUF_SIZE {
        return BpmAnalysis {
            bpm: 0.0,
            confidence: 0.0,
            beat_count: 0,
        };
    }

    // Tempo::new(onset_mode, buf_size, hop_size, sample_rate)
    let Ok(mut tempo) = Tempo::new(OnsetMode::default(), BUF_SIZE, HOP_SIZE, sample_rate) else {
        log::warn!("[BPM] Failed to create tempo detector");
        return BpmAnalysis {
            bpm: 0.0,
            confidence: 0.0,
            beat_count: 0,
        };
    };

    let mut bpm_values: Vec<f64> = Vec::new();
    let mut confidences: Vec<f64> = Vec::new();
    let mut beat_count = 0;

    // Process audio in chunks
    for chunk in samples.chunks(HOP_SIZE) {
        if chunk.len() < HOP_SIZE {
            break;
        }

        let Ok(bpm) = tempo.do_result(chunk) else {
            continue;
        };

        let confidence = tempo.get_confidence() as f64;

        // A beat was detected when bpm > 0
        if bpm > 0.0 {
            beat_count += 1;
            bpm_values.push(bpm as f64);
            confidences.push(confidence);
        }
    }

    if bpm_values.is_empty() {
        return BpmAnalysis {
            bpm: 0.0,
            confidence: 0.0,
            beat_count: 0,
        };
    }

    // Sort and get median for robustness
    bpm_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_bpm = bpm_values[bpm_values.len() / 2];

    // Average confidence
    let avg_confidence = confidences.iter().sum::<f64>() / confidences.len() as f64;

    BpmAnalysis {
        bpm: median_bpm,
        confidence: avg_confidence,
        beat_count,
    }
}

/// Detect BPM from a WAV file.
pub fn detect_bpm_from_file(path: &Path) -> BpmAnalysis {
    use std::fs::File;
    use std::io::BufReader;

    let Ok(file) = File::open(path) else {
        log::warn!("[BPM] Failed to open file: {:?}", path);
        return BpmAnalysis {
            bpm: 0.0,
            confidence: 0.0,
            beat_count: 0,
        };
    };

    let reader = BufReader::new(file);
    let Ok(wav_reader) = hound::WavReader::new(reader) else {
        log::warn!("[BPM] Failed to parse WAV file: {:?}", path);
        return BpmAnalysis {
            bpm: 0.0,
            confidence: 0.0,
            beat_count: 0,
        };
    };

    let spec = wav_reader.spec();
    let sample_rate = spec.sample_rate;
    let channels = spec.channels as usize;

    // Read all samples and convert to f32 mono
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            let raw_samples: Vec<f32> = wav_reader
                .into_samples::<f32>()
                .filter_map(|s| s.ok())
                .collect();
            mix_to_mono(&raw_samples, channels)
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_value = (1i64 << (bits - 1)) as f32;
            let raw_samples: Vec<f32> = wav_reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_value)
                .collect();
            mix_to_mono(&raw_samples, channels)
        }
    };

    detect_bpm(&samples, sample_rate)
}

/// Mix multi-channel audio to mono by averaging channels.
fn mix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }

    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

// =============================================================================
// Sample Handle
// =============================================================================

/// A SampleHandle represents a loaded audio sample with playback parameters.
#[derive(Clone, Debug)]
pub struct SampleHandle {
    /// Unique identifier for this sample
    pub id: String,
    /// Original file path
    pub path: String,
    /// Attack time in seconds (default: 0.001)
    pub attack: f64,
    /// Sustain level 0.0-1.0 (default: 1.0)
    pub sustain_level: f64,
    /// Release time in seconds (default: 0.01)
    pub release: f64,
    /// Start offset in seconds (default: 0.0)
    pub offset_seconds: f64,
    /// Playback length in seconds (None = full sample)
    pub length_seconds: Option<f64>,
    /// Playback rate multiplier (default: 1.0) - for PlayBuf mode
    pub rate: f64,
    /// Loop mode (default: false)
    pub loop_mode: bool,
    /// Amplitude (default: 1.0)
    pub amp: f64,
    /// Parent sample ID if this is a slice
    parent_id: Option<String>,
    /// Start frame for slices
    start_frame: Option<i32>,
    /// End frame for slices
    end_frame: Option<i32>,

    // === Time-stretch / pitch-shift parameters ===
    /// Use Warp1-based time stretching (default: false)
    pub warp_mode: bool,
    /// Playback speed for Warp mode (1.0 = normal, 0.5 = half speed, 2.0 = double)
    pub speed: f64,
    /// Pitch shift multiplier (1.0 = normal, 0.5 = octave down, 2.0 = octave up)
    pub pitch: f64,
    /// Detected BPM (0.0 if not analyzed)
    pub detected_bpm: f64,
    /// Target BPM for automatic time-stretch warping
    pub target_bpm: Option<f64>,
    /// Granular window size for Warp mode (default: 0.1 seconds)
    pub window_size: f64,
    /// Number of overlapping grains (default: 8)
    pub overlaps: f64,
}

impl SampleHandle {
    /// Create a new sample handle by loading a sample from a file.
    /// If the sample is already loaded with the same path, returns immediately without reloading.
    pub fn new(id: String, path: String) -> Self {
        let handle = require_handle();

        // Resolve the path on the Rhai thread where context (script_dir, import_paths) is available
        let resolved_path = context::resolve_file(&path).map(|p| p.to_string_lossy().to_string());

        if resolved_path.is_none() {
            log::error!(
                "[SAMPLE] Could not resolve path '{}' for sample '{}'. File not found.",
                path,
                id
            );
        }

        // Check if sample is already loaded with the same path
        let already_loaded = handle.with_state(|state| {
            state.samples.get(&id).map(|s| {
                // Compare resolved paths
                if let Some(ref rp) = resolved_path {
                    s.path == *rp
                } else {
                    s.path == path
                }
            }).unwrap_or(false)
        });

        if already_loaded {
            log::debug!("[SAMPLE] Sample '{}' already loaded, reusing", id);
        } else {
            log::info!("[SAMPLE] Loading sample '{}' from '{}'", id, path);

            // Send LoadSample message with pre-resolved path
            let _ = handle.send(StateMessage::LoadSample {
                id: id.clone(),
                path: path.clone(),
                resolved_path,
                analyze_bpm: false,
                warp_to_bpm: None,
            });

            // Wait for sample to load
            for attempt in 0..50 {
                std::thread::sleep(std::time::Duration::from_millis(100));

                let loaded = handle.with_state(|state| state.samples.contains_key(&id));

                if loaded {
                    log::info!("[SAMPLE] Sample '{}' loaded successfully", id);
                    break;
                }

                if attempt % 10 == 0 && attempt > 0 {
                    log::debug!(
                        "[SAMPLE] Still waiting for '{}' (attempt {}/50)...",
                        id,
                        attempt
                    );
                }
            }
        }

        Self {
            id,
            path,
            attack: 0.001,
            sustain_level: 1.0,
            release: 0.01,
            offset_seconds: 0.0,
            length_seconds: None,
            rate: 1.0,
            loop_mode: false,
            amp: 1.0,
            parent_id: None,
            start_frame: None,
            end_frame: None,
            // Time-stretch / pitch-shift defaults
            warp_mode: false,
            speed: 1.0,
            pitch: 1.0,
            detected_bpm: 0.0,
            target_bpm: None,
            window_size: 0.1,
            overlaps: 8.0,
        }
    }

    /// Create a sample handle for a slice (internal use).
    fn new_slice(
        id: String,
        parent_id: String,
        path: String,
        start_frame: i32,
        end_frame: i32,
        sample_rate: f32,
    ) -> Self {
        let duration = (end_frame - start_frame) as f64 / sample_rate as f64;
        Self {
            id,
            path,
            attack: 0.001,
            sustain_level: 1.0,
            release: 0.01,
            offset_seconds: 0.0,
            length_seconds: Some(duration),
            rate: 1.0,
            loop_mode: false,
            amp: 1.0,
            parent_id: Some(parent_id),
            start_frame: Some(start_frame),
            end_frame: Some(end_frame),
            // Time-stretch / pitch-shift defaults
            warp_mode: false,
            speed: 1.0,
            pitch: 1.0,
            detected_bpm: 0.0,
            target_bpm: None,
            window_size: 0.1,
            overlaps: 8.0,
        }
    }

    // === Envelope configuration ===

    /// Set the attack time in seconds.
    pub fn attack(mut self, seconds: f64) -> Self {
        self.attack = seconds.max(0.0001);
        self
    }

    /// Set the sustain level (0.0 to 1.0).
    pub fn sustain(mut self, level: f64) -> Self {
        self.sustain_level = level.clamp(0.0, 1.0);
        self
    }

    /// Set the release time in seconds.
    pub fn release(mut self, seconds: f64) -> Self {
        self.release = seconds.max(0.001);
        self
    }

    // === Playback configuration ===

    /// Set the start offset in seconds.
    pub fn offset(mut self, seconds: f64) -> Self {
        self.offset_seconds = seconds.max(0.0);
        self
    }

    /// Set the playback length in seconds.
    pub fn length(mut self, seconds: f64) -> Self {
        self.length_seconds = Some(seconds.max(0.001));
        self
    }

    /// Set the playback rate multiplier.
    pub fn set_rate(mut self, rate: f64) -> Self {
        self.rate = rate.max(0.01);
        self
    }

    /// Enable or disable loop mode.
    pub fn loop_mode_set(mut self, enabled: bool) -> Self {
        self.loop_mode = enabled;
        self
    }

    /// Set the amplitude (0.0 to 1.0+).
    pub fn set_amp(mut self, amp: f64) -> Self {
        self.amp = amp.max(0.0);
        self
    }

    // === Time-stretch / Pitch-shift configuration ===

    /// Enable warp mode for time-stretching with independent pitch control.
    ///
    /// In warp mode, `speed` controls playback speed and `pitch` controls pitch independently.
    /// This uses granular synthesis (Warp1 UGen) which sounds great for most audio
    /// but may not be ideal for very short samples or percussive transients.
    pub fn warp(mut self, enabled: bool) -> Self {
        self.warp_mode = enabled;
        self
    }

    /// Set playback speed for warp mode.
    ///
    /// - 1.0 = normal speed
    /// - 0.5 = half speed (slower, stretches time)
    /// - 2.0 = double speed (faster, compresses time)
    ///
    /// Automatically enables warp mode.
    pub fn set_speed(mut self, speed: f64) -> Self {
        self.speed = speed.clamp(0.1, 10.0);
        self.warp_mode = true;
        self
    }

    /// Set pitch shift multiplier for warp mode.
    ///
    /// - 1.0 = original pitch
    /// - 0.5 = one octave down
    /// - 2.0 = one octave up
    /// - 1.0595 = one semitone up (2^(1/12))
    ///
    /// Automatically enables warp mode.
    pub fn set_pitch(mut self, pitch: f64) -> Self {
        self.pitch = pitch.clamp(0.25, 4.0);
        self.warp_mode = true;
        self
    }

    /// Shift pitch by semitones.
    ///
    /// - 0 = original pitch
    /// - 12 = one octave up
    /// - -12 = one octave down
    ///
    /// Automatically enables warp mode.
    pub fn semitones(mut self, semitones: f64) -> Self {
        self.pitch = 2.0f64.powf(semitones / 12.0);
        self.warp_mode = true;
        self
    }

    /// Shift pitch by semitones (integer version).
    pub fn semitones_i64(self, semitones: i64) -> Self {
        self.semitones(semitones as f64)
    }

    /// Analyze the sample and detect its BPM.
    ///
    /// This reads the audio file and uses aubio's tempo detection.
    /// The detected BPM is stored and can be used with `warp_to_bpm()`.
    pub fn analyze_bpm(mut self) -> Self {
        // Resolve the file path using the same resolution logic as sample loading
        let resolved_path = match context::resolve_file(&self.path) {
            Some(p) => p,
            None => {
                log::warn!(
                    "[SAMPLE] Could not resolve path for BPM analysis: '{}'",
                    self.path
                );
                return self;
            }
        };

        let analysis = detect_bpm_from_file(&resolved_path);
        self.detected_bpm = analysis.bpm;
        if analysis.bpm > 0.0 {
            log::info!(
                "[SAMPLE] Detected BPM for '{}': {:.1} (confidence: {:.0}%, {} beats)",
                self.id,
                analysis.bpm,
                analysis.confidence * 100.0,
                analysis.beat_count
            );
        } else {
            log::warn!("[SAMPLE] Could not detect BPM for '{}'", self.id);
        }
        self
    }

    /// Get the detected BPM (0.0 if not analyzed).
    pub fn get_detected_bpm(&self) -> f64 {
        self.detected_bpm
    }

    /// Warp the sample to match a target BPM.
    ///
    /// If BPM was detected (via `analyze_bpm()`), this calculates the speed
    /// multiplier needed to match the target BPM and enables warp mode.
    ///
    /// Example: If sample is 140 BPM and target is 120 BPM:
    /// speed = 120/140 = 0.857 (slower playback)
    pub fn warp_to_bpm(mut self, target_bpm: f64) -> Self {
        self.target_bpm = Some(target_bpm);
        if self.detected_bpm > 0.0 && target_bpm > 0.0 {
            self.speed = target_bpm / self.detected_bpm;
            self.warp_mode = true;
            log::info!(
                "[SAMPLE] Warping '{}' from {:.1} BPM to {:.1} BPM (speed: {:.3})",
                self.id,
                self.detected_bpm,
                target_bpm,
                self.speed
            );
        } else {
            log::warn!(
                "[SAMPLE] Cannot warp '{}' - detected BPM is {:.1}. Run analyze_bpm() first.",
                self.id,
                self.detected_bpm
            );
        }
        self
    }

    /// Set the granular window size for warp mode (in seconds).
    ///
    /// Smaller windows (0.02-0.05) are better for percussive material.
    /// Larger windows (0.1-0.3) are better for sustained/melodic material.
    /// Default is 0.1 seconds.
    pub fn set_window_size(mut self, size: f64) -> Self {
        self.window_size = size.clamp(0.01, 1.0);
        self
    }

    /// Set the number of overlapping grains for warp mode.
    ///
    /// More overlaps (8-16) = smoother but more CPU.
    /// Fewer overlaps (2-4) = more efficient but may have artifacts.
    /// Default is 8.
    pub fn set_overlaps(mut self, overlaps: f64) -> Self {
        self.overlaps = overlaps.clamp(1.0, 32.0);
        self
    }

    // === Sample info getters ===

    /// Get the SuperCollider buffer ID for this sample.
    pub fn buffer_id(&self) -> i64 {
        let handle = require_handle();
        let sample_id = self.parent_id.as_ref().unwrap_or(&self.id);
        handle
            .with_state(|state| {
                state
                    .samples
                    .get(sample_id)
                    .map(|info| info.buffer_id as i64)
            })
            .unwrap_or(0)
    }

    /// Get the synthdef name for this sample.
    pub fn synthdef_name(&self) -> String {
        let handle = require_handle();
        let sample_id = self.parent_id.as_ref().unwrap_or(&self.id);
        handle
            .with_state(|state| {
                state
                    .samples
                    .get(sample_id)
                    .map(|info| info.synthdef_name.clone())
            })
            .unwrap_or_else(|| format!("__sample_{}", self.id))
    }

    /// Get the sample duration in seconds.
    pub fn duration(&self) -> f64 {
        let handle = require_handle();
        let sample_id = self.parent_id.as_ref().unwrap_or(&self.id);
        handle
            .with_state(|state| {
                state.samples.get(sample_id).map(|info| {
                    if let (Some(start), Some(end)) = (self.start_frame, self.end_frame) {
                        (end - start) as f64 / info.sample_rate as f64
                    } else {
                        info.num_frames as f64 / info.sample_rate as f64
                    }
                })
            })
            .unwrap_or(0.0)
    }

    /// Get the sample rate.
    pub fn sample_rate(&self) -> f64 {
        let handle = require_handle();
        let sample_id = self.parent_id.as_ref().unwrap_or(&self.id);
        handle
            .with_state(|state| {
                state
                    .samples
                    .get(sample_id)
                    .map(|info| info.sample_rate as f64)
            })
            .unwrap_or(44100.0)
    }

    /// Get the number of channels.
    pub fn num_channels(&self) -> i64 {
        let handle = require_handle();
        let sample_id = self.parent_id.as_ref().unwrap_or(&self.id);
        handle
            .with_state(|state| {
                state
                    .samples
                    .get(sample_id)
                    .map(|info| info.num_channels as i64)
            })
            .unwrap_or(2)
    }

    // === Slicing ===

    /// Create a slice of this sample from start to end (in seconds).
    pub fn slice_range(&self, start_seconds: f64, end_seconds: f64) -> Self {
        let handle = require_handle();
        let sample_id = self.parent_id.as_ref().unwrap_or(&self.id);

        // Get sample info from state
        let sample_info = handle.with_state(|state| {
            state.samples.get(sample_id).map(|info| {
                (
                    info.buffer_id,
                    info.num_frames,
                    info.num_channels,
                    info.sample_rate,
                    info.path.clone(),
                )
            })
        });

        let Some((_buffer_id, num_frames, _num_channels, sample_rate, path)) = sample_info else {
            log::error!("Sample '{}' not found", sample_id);
            return self.clone();
        };

        // Calculate frame positions
        let base_start_frame = self.start_frame.unwrap_or(0);
        let start_frame = base_start_frame + (start_seconds * sample_rate as f64) as i32;
        let end_frame = base_start_frame + (end_seconds * sample_rate as f64) as i32;

        // Clamp to valid range
        let max_frame = self.end_frame.unwrap_or(num_frames);
        let start_frame = start_frame.max(base_start_frame).min(max_frame);
        let end_frame = end_frame.max(start_frame).min(max_frame);

        // Create a unique ID for this slice
        let slice_id = format!(
            "{}_slice_{:.3}_{:.3}",
            sample_id, start_seconds, end_seconds
        );

        log::info!(
            "[SAMPLE] Created slice '{}' from {} (frames {}-{})",
            slice_id,
            sample_id,
            start_frame,
            end_frame
        );

        // Create the slice handle with inherited envelope settings
        let mut slice = SampleHandle::new_slice(
            slice_id,
            sample_id.to_string(),
            path,
            start_frame,
            end_frame,
            sample_rate,
        );

        slice.attack = self.attack;
        slice.sustain_level = self.sustain_level;
        slice.release = self.release;
        slice.rate = self.rate;
        slice.amp = self.amp;
        slice.loop_mode = self.loop_mode;
        // Copy warp settings
        slice.warp_mode = self.warp_mode;
        slice.speed = self.speed;
        slice.pitch = self.pitch;
        slice.detected_bpm = self.detected_bpm;
        slice.target_bpm = self.target_bpm;
        slice.window_size = self.window_size;
        slice.overlaps = self.overlaps;

        slice
    }

    // === Playback parameters ===

    /// Get the start position in frames for playback.
    pub fn get_start_frame(&self) -> i32 {
        if let Some(start) = self.start_frame {
            let sample_rate = self.sample_rate() as f32;
            start + (self.offset_seconds * sample_rate as f64) as i32
        } else {
            let sample_rate = self.sample_rate() as f32;
            (self.offset_seconds * sample_rate as f64) as i32
        }
    }

    /// Get the end position in frames for playback (-1 for full sample).
    pub fn get_end_frame(&self) -> i32 {
        if let Some(length) = self.length_seconds {
            let sample_rate = self.sample_rate() as f32;
            self.get_start_frame() + (length * sample_rate as f64) as i32
        } else if let Some(end) = self.end_frame {
            end
        } else {
            -1
        }
    }
}

// === Rhai getters ===

fn sample_buffer_id(sample: &mut SampleHandle) -> i64 {
    sample.buffer_id()
}

fn sample_synthdef_name(sample: &mut SampleHandle) -> String {
    sample.synthdef_name()
}

fn sample_duration(sample: &mut SampleHandle) -> f64 {
    sample.duration()
}

fn sample_sample_rate(sample: &mut SampleHandle) -> f64 {
    sample.sample_rate()
}

fn sample_num_channels(sample: &mut SampleHandle) -> i64 {
    sample.num_channels()
}

fn sample_detected_bpm(sample: &mut SampleHandle) -> f64 {
    sample.detected_bpm
}

fn sample_warp_mode(sample: &mut SampleHandle) -> bool {
    sample.warp_mode
}

fn sample_speed(sample: &mut SampleHandle) -> f64 {
    sample.speed
}

fn sample_pitch(sample: &mut SampleHandle) -> f64 {
    sample.pitch
}

fn sample_path(sample: &mut SampleHandle) -> String {
    sample.path.clone()
}

fn sample_window_size(sample: &mut SampleHandle) -> f64 {
    sample.window_size
}

fn sample_overlaps(sample: &mut SampleHandle) -> f64 {
    sample.overlaps
}

/// Load a sample from a file path.
pub fn sample(id: String, path: String) -> SampleHandle {
    SampleHandle::new(id, path)
}

/// Load a sample from a file path (alias).
pub fn load_sample(id: String, path: String) -> SampleHandle {
    SampleHandle::new(id, path)
}

/// Register sample API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register SampleHandle type
    engine.register_type::<SampleHandle>();

    // Constructors
    engine.register_fn("sample", sample);
    engine.register_fn("load_sample", load_sample);

    // Info getters
    engine.register_fn("buffer_id", sample_buffer_id);
    engine.register_fn("synthdef_name", sample_synthdef_name);
    engine.register_fn("duration", sample_duration);
    engine.register_get("duration", sample_duration);
    engine.register_fn("sample_rate", sample_sample_rate);
    engine.register_get("sample_rate", sample_sample_rate);
    engine.register_fn("num_channels", sample_num_channels);
    engine.register_get("num_channels", sample_num_channels);

    // Path getter
    engine.register_fn("path", sample_path);
    engine.register_get("path", sample_path);

    // BPM and warp getters
    engine.register_fn("detected_bpm", sample_detected_bpm);
    engine.register_get("detected_bpm", sample_detected_bpm);
    engine.register_fn("warp_mode", sample_warp_mode);
    engine.register_get("warp_mode", sample_warp_mode);
    engine.register_fn("speed", sample_speed);
    engine.register_get("speed", sample_speed);
    engine.register_fn("pitch", sample_pitch);
    engine.register_get("pitch", sample_pitch);
    engine.register_fn("window_size", sample_window_size);
    engine.register_get("window_size", sample_window_size);
    engine.register_fn("overlaps", sample_overlaps);
    engine.register_get("overlaps", sample_overlaps);

    // Envelope configuration (builder methods)
    engine.register_fn("attack", SampleHandle::attack);
    engine.register_fn("sustain", SampleHandle::sustain);
    engine.register_fn("release", SampleHandle::release);

    // Playback configuration (builder methods)
    engine.register_fn("offset", SampleHandle::offset);
    engine.register_fn("length", SampleHandle::length);
    engine.register_fn("rate", SampleHandle::set_rate);
    engine.register_fn("loop_mode", SampleHandle::loop_mode_set);
    engine.register_fn("amp", SampleHandle::set_amp);

    // Time-stretch / pitch-shift configuration (builder methods)
    engine.register_fn("warp", SampleHandle::warp);
    engine.register_fn("set_speed", SampleHandle::set_speed);
    engine.register_fn("set_pitch", SampleHandle::set_pitch);
    engine.register_fn("semitones", SampleHandle::semitones);
    engine.register_fn("semitones", SampleHandle::semitones_i64);
    engine.register_fn("analyze_bpm", SampleHandle::analyze_bpm);
    engine.register_fn("warp_to_bpm", SampleHandle::warp_to_bpm);
    engine.register_fn("window_size", SampleHandle::set_window_size);
    engine.register_fn("overlaps", SampleHandle::set_overlaps);

    // Slicing
    engine.register_fn("slice", SampleHandle::slice_range);
}
