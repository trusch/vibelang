//! VibeLang Core Constants
//!
//! This module contains documented constants used throughout the VibeLang core.
//! All magic numbers should be defined here with explanations of their purpose
//! and rationale for the chosen values.

// === Timing Constants ===

/// Beat comparison epsilon. Two beats within this tolerance are considered equal.
/// Value chosen to handle floating-point rounding in 16-bit fixed-point arithmetic.
/// With 16 fractional bits, the smallest representable value is ~1.5e-5, so 1e-6
/// provides a safety margin.
pub const BEAT_EPSILON: f64 = 1e-6;

/// Scheduler lookahead in milliseconds. Events are scheduled this far ahead
/// to ensure they're sent to SuperCollider before they need to play.
/// 250ms provides buffer for system scheduling jitter while keeping latency
/// acceptable for interactive use. Lower values reduce latency but risk
/// late events under system load.
pub const SCHEDULER_LOOKAHEAD_MS: u64 = 250;

/// Minimum time between scheduler runs in milliseconds.
/// This prevents the scheduler from spinning too fast and wasting CPU.
pub const SCHEDULER_MIN_INTERVAL_MS: u64 = 1;

// === Audio Constants ===

/// Default sample rate in Hz.
/// TODO: Should be queried from audio device at runtime.
/// 48kHz is the standard for professional audio and JACK default.
pub const DEFAULT_SAMPLE_RATE: f32 = 48000.0;

/// Reference frequency for A4 (concert pitch) in Hz.
/// Used for MIDI note to frequency conversions.
pub const A4_FREQUENCY: f64 = 440.0;

/// MIDI note number for A4.
pub const A4_MIDI_NOTE: u8 = 69;

// === Node ID Allocation ===
//
// SuperCollider uses node IDs to identify synths and groups.
// We partition the ID space to avoid collisions:
//
// 0-99:          Reserved for system nodes
// 100-999:       Groups (bus routing, mixing)
// 1000-1999:     Reserved for future use
// 2000+:         Synth voices (dynamically allocated)

/// Starting node ID for synth voices.
/// IDs below this are reserved for groups and system nodes.
pub const VOICE_NODE_ID_BASE: i32 = 2000;

/// Starting node ID for group nodes.
pub const GROUP_NODE_ID_BASE: i32 = 100;

/// Starting bus number for group audio buses.
/// Buses 0-15 are reserved for hardware I/O.
pub const GROUP_BUS_BASE: i32 = 16;

/// Maximum number of groups supported.
pub const MAX_GROUPS: usize = 64;

// === MIDI Constants ===

/// Default MIDI velocity when not specified.
pub const DEFAULT_MIDI_VELOCITY: u8 = 100;

/// Maximum MIDI note number.
pub const MIDI_NOTE_MAX: u8 = 127;

/// Maximum MIDI velocity value.
pub const MIDI_VELOCITY_MAX: u8 = 127;

/// Maximum MIDI CC value.
pub const MIDI_CC_MAX: u8 = 127;

/// Number of MIDI channels (1-16, represented as 0-15 internally).
pub const MIDI_CHANNEL_COUNT: u8 = 16;

// === Buffer Sizes ===

/// OSC receive buffer size in bytes.
/// 64KB is sufficient for most OSC bundles while staying within
/// typical UDP packet size limits.
pub const OSC_BUFFER_SIZE: usize = 65536;

/// Maximum length of a synthdef name.
/// SuperCollider has internal limits on name lengths.
pub const MAX_SYNTHDEF_NAME_LENGTH: usize = 255;

// === Pattern/Melody Constants ===

/// Default pattern length in beats when not specified.
pub const DEFAULT_PATTERN_LENGTH: f64 = 4.0;

/// Default quantization grid for patterns (in beats).
/// 0.25 = sixteenth notes at 4/4.
pub const DEFAULT_QUANTIZATION: f64 = 0.25;

/// Maximum number of events that can be scheduled per pattern loop.
/// This prevents runaway patterns from consuming all memory.
pub const MAX_EVENTS_PER_LOOP: usize = 10000;

// === Hot Reload ===

/// Debounce time for file change detection in milliseconds.
/// Multiple rapid file saves are coalesced into a single reload.
pub const RELOAD_DEBOUNCE_MS: u64 = 100;

/// Grace period after reload before cleaning up old nodes (milliseconds).
/// Allows audio to fade out gracefully.
pub const RELOAD_CLEANUP_DELAY_MS: u64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_epsilon_is_small() {
        assert!(BEAT_EPSILON < 1e-5);
        assert!(BEAT_EPSILON > 0.0);
    }

    #[test]
    fn test_a4_frequency() {
        assert_eq!(A4_FREQUENCY, 440.0);
        assert_eq!(A4_MIDI_NOTE, 69);
    }

    #[test]
    fn test_node_id_partitioning() {
        // Groups should come before voices
        assert!(GROUP_NODE_ID_BASE < VOICE_NODE_ID_BASE);
        // Voice IDs should start high enough to leave room for groups
        assert!(VOICE_NODE_ID_BASE >= GROUP_NODE_ID_BASE + MAX_GROUPS as i32);
    }
}
