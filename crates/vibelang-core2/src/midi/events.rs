//! MIDI event types with full MIDI 1.0 support and MIDI 2.0 future-proofing.
//!
//! This module provides:
//! - `TimestampedMidiEvent` - MIDI events with preserved timestamps for jitter compensation
//! - `MidiMessage` - Full MIDI 1.0 message types including SysEx, aftertouch, etc.
//! - `Velocity` / `ControlValue` - Value types that can hold both MIDI 1.0 and 2.0 resolution

use crate::types::ids::MidiDeviceId;
use std::time::Instant;

// ============================================================================
// MIDI 2.0-Ready Value Types
// ============================================================================

/// Velocity value that can hold MIDI 1.0 (7-bit) or MIDI 2.0 (16-bit) resolution.
///
/// Internally stored as 16-bit for maximum precision. MIDI 1.0 values are
/// scaled up to fill the 16-bit range for consistent handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Velocity(u16);

impl Velocity {
    /// Zero velocity (note off).
    pub const ZERO: Self = Self(0);

    /// Maximum velocity.
    pub const MAX: Self = Self(0xFFFF);

    /// Create from MIDI 1.0 7-bit velocity (0-127).
    /// Scales to 16-bit range: v << 9 | v << 2 | v >> 5 for proper scaling.
    #[inline]
    pub fn from_midi1(v: u8) -> Self {
        // Scale 7-bit to 16-bit with proper interpolation
        // This ensures 127 maps to 65535, not 65024
        let v = v as u16;
        Self((v << 9) | (v << 2) | (v >> 5))
    }

    /// Create from MIDI 2.0 16-bit velocity.
    #[inline]
    pub fn from_midi2(v: u16) -> Self {
        Self(v)
    }

    /// Create from raw 7-bit value without scaling (for compatibility).
    #[inline]
    pub fn from_raw7(v: u8) -> Self {
        Self(v as u16)
    }

    /// Convert to MIDI 1.0 7-bit velocity (0-127).
    #[inline]
    pub fn to_midi1(&self) -> u8 {
        (self.0 >> 9) as u8
    }

    /// Get the raw 16-bit value.
    #[inline]
    pub fn to_midi2(&self) -> u16 {
        self.0
    }

    /// Convert to normalized float (0.0 to 1.0).
    #[inline]
    pub fn as_f32(&self) -> f32 {
        self.0 as f32 / 65535.0
    }

    /// Create from normalized float (0.0 to 1.0).
    #[inline]
    pub fn from_f32(v: f32) -> Self {
        Self((v.clamp(0.0, 1.0) * 65535.0) as u16)
    }

    /// Check if this is a note-off velocity (zero).
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl Default for Velocity {
    fn default() -> Self {
        Self::from_midi1(100) // Reasonable default velocity
    }
}

impl From<u8> for Velocity {
    fn from(v: u8) -> Self {
        Self::from_midi1(v)
    }
}

/// Controller value that can hold 7-bit, 14-bit, or 32-bit (MIDI 2.0) resolution.
///
/// Internally stored as 32-bit for maximum precision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ControlValue(u32);

impl ControlValue {
    /// Zero value.
    pub const ZERO: Self = Self(0);

    /// Maximum value.
    pub const MAX: Self = Self(0xFFFF_FFFF);

    /// Create from MIDI 1.0 7-bit value (0-127).
    #[inline]
    pub fn from_7bit(v: u8) -> Self {
        // Scale to 32-bit
        let v = v as u32;
        Self((v << 25) | (v << 18) | (v << 11) | (v << 4) | (v >> 3))
    }

    /// Create from MIDI 1.0 14-bit value (0-16383, from MSB+LSB CC pair).
    #[inline]
    pub fn from_14bit(v: u16) -> Self {
        let v = v as u32;
        Self((v << 18) | (v << 4) | (v >> 10))
    }

    /// Create from MIDI 2.0 32-bit value.
    #[inline]
    pub fn from_32bit(v: u32) -> Self {
        Self(v)
    }

    /// Convert to MIDI 1.0 7-bit value (0-127).
    #[inline]
    pub fn to_7bit(&self) -> u8 {
        (self.0 >> 25) as u8
    }

    /// Convert to MIDI 1.0 14-bit value (0-16383).
    #[inline]
    pub fn to_14bit(&self) -> u16 {
        (self.0 >> 18) as u16
    }

    /// Get the raw 32-bit value.
    #[inline]
    pub fn to_32bit(&self) -> u32 {
        self.0
    }

    /// Convert to normalized float (0.0 to 1.0).
    #[inline]
    pub fn as_f32(&self) -> f32 {
        self.0 as f32 / u32::MAX as f32
    }

    /// Create from normalized float (0.0 to 1.0).
    #[inline]
    pub fn from_f32(v: f32) -> Self {
        Self((v.clamp(0.0, 1.0) * u32::MAX as f32) as u32)
    }
}

impl Default for ControlValue {
    fn default() -> Self {
        Self::ZERO
    }
}

impl From<u8> for ControlValue {
    fn from(v: u8) -> Self {
        Self::from_7bit(v)
    }
}

// ============================================================================
// MIDI Channel (4-bit)
// ============================================================================

/// MIDI channel (0-15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Channel(u8);

impl Channel {
    /// Create a new channel (0-15). Panics if out of range.
    #[inline]
    pub fn new(ch: u8) -> Self {
        assert!(ch < 16, "MIDI channel must be 0-15");
        Self(ch)
    }

    /// Create a new channel, clamping to valid range.
    #[inline]
    pub fn new_clamped(ch: u8) -> Self {
        Self(ch & 0x0F)
    }

    /// Get the channel number (0-15).
    #[inline]
    pub fn get(&self) -> u8 {
        self.0
    }

    /// Get the channel number as 1-based (1-16) for display.
    #[inline]
    pub fn display(&self) -> u8 {
        self.0 + 1
    }
}

impl From<u8> for Channel {
    fn from(ch: u8) -> Self {
        Self::new_clamped(ch)
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self(0)
    }
}

// ============================================================================
// MIDI Messages
// ============================================================================

/// Full MIDI 1.0 message types with MIDI 2.0-ready value types.
///
/// This enum covers all standard MIDI 1.0 message types:
/// - Channel Voice Messages (notes, controllers, etc.)
/// - System Common Messages (SysEx, song position, etc.)
/// - System Realtime Messages (clock, transport, etc.)
#[derive(Clone, Debug, PartialEq)]
pub enum MidiMessage {
    // ========================================================================
    // Channel Voice Messages
    // ========================================================================
    /// Note On event.
    NoteOn {
        channel: Channel,
        note: u8,
        velocity: Velocity,
    },

    /// Note Off event.
    NoteOff {
        channel: Channel,
        note: u8,
        velocity: Velocity,
    },

    /// Polyphonic (per-note) Aftertouch / Key Pressure.
    PolyAftertouch {
        channel: Channel,
        note: u8,
        pressure: u8,
    },

    /// Control Change (CC).
    ControlChange {
        channel: Channel,
        controller: u8,
        value: u8,
    },

    /// Program Change.
    ProgramChange { channel: Channel, program: u8 },

    /// Channel Aftertouch / Channel Pressure.
    ChannelAftertouch { channel: Channel, pressure: u8 },

    /// Pitch Bend (-8192 to +8191, 0 = center).
    PitchBend { channel: Channel, value: i16 },

    // ========================================================================
    // System Common Messages
    // ========================================================================
    /// System Exclusive (SysEx) message.
    /// Contains the data bytes (without F0 start and F7 end markers).
    SysEx(Vec<u8>),

    /// MIDI Time Code Quarter Frame.
    TimeCode {
        /// Message type (0-7).
        msg_type: u8,
        /// Value (0-15).
        value: u8,
    },

    /// Song Position Pointer (in MIDI beats, 1 beat = 6 MIDI clocks).
    SongPosition {
        /// Position in MIDI beats (0-16383).
        position: u16,
    },

    /// Song Select.
    SongSelect {
        /// Song number (0-127).
        song: u8,
    },

    /// Tune Request - triggers oscillator tuning on analog synths.
    TuneRequest,

    // ========================================================================
    // System Realtime Messages
    // ========================================================================
    /// MIDI Clock pulse (24 pulses per quarter note).
    Clock,

    /// Start playback from the beginning.
    Start,

    /// Continue playback from current position.
    Continue,

    /// Stop playback.
    Stop,

    /// Active Sensing (sent every 300ms to indicate connection is alive).
    ActiveSensing,

    /// System Reset.
    Reset,
}

impl MidiMessage {
    /// Get the MIDI channel for channel messages, None for system messages.
    pub fn channel(&self) -> Option<Channel> {
        match self {
            MidiMessage::NoteOn { channel, .. }
            | MidiMessage::NoteOff { channel, .. }
            | MidiMessage::PolyAftertouch { channel, .. }
            | MidiMessage::ControlChange { channel, .. }
            | MidiMessage::ProgramChange { channel, .. }
            | MidiMessage::ChannelAftertouch { channel, .. }
            | MidiMessage::PitchBend { channel, .. } => Some(*channel),
            _ => None,
        }
    }

    /// Check if this is a note message (Note On or Note Off).
    pub fn is_note(&self) -> bool {
        matches!(self, MidiMessage::NoteOn { .. } | MidiMessage::NoteOff { .. })
    }

    /// Check if this is a system realtime message.
    pub fn is_realtime(&self) -> bool {
        matches!(
            self,
            MidiMessage::Clock
                | MidiMessage::Start
                | MidiMessage::Continue
                | MidiMessage::Stop
                | MidiMessage::ActiveSensing
                | MidiMessage::Reset
        )
    }

    /// Check if this is a system common message.
    pub fn is_system_common(&self) -> bool {
        matches!(
            self,
            MidiMessage::SysEx(_)
                | MidiMessage::TimeCode { .. }
                | MidiMessage::SongPosition { .. }
                | MidiMessage::SongSelect { .. }
                | MidiMessage::TuneRequest
        )
    }

    /// Get the note number for note messages.
    pub fn note(&self) -> Option<u8> {
        match self {
            MidiMessage::NoteOn { note, .. }
            | MidiMessage::NoteOff { note, .. }
            | MidiMessage::PolyAftertouch { note, .. } => Some(*note),
            _ => None,
        }
    }

    /// Get the velocity for note messages.
    pub fn velocity(&self) -> Option<Velocity> {
        match self {
            MidiMessage::NoteOn { velocity, .. } | MidiMessage::NoteOff { velocity, .. } => {
                Some(*velocity)
            }
            _ => None,
        }
    }
}

// ============================================================================
// Timestamped Event
// ============================================================================

/// A MIDI event with timestamp information for accurate timing.
///
/// This struct preserves both the original midir timestamp and the
/// system instant when the event was received, enabling:
/// - Accurate timing reconstruction
/// - Jitter compensation
/// - Latency measurement
#[derive(Clone, Debug)]
pub struct TimestampedMidiEvent {
    /// Microseconds from midir's reference point (platform-specific origin).
    /// This is the most accurate timing information from the MIDI driver.
    pub timestamp_us: u64,

    /// Monotonic instant when the event was received by our callback.
    /// Used for jitter compensation and latency measurement.
    pub received_at: Instant,

    /// The MIDI device that sent this event.
    pub device_id: MidiDeviceId,

    /// The MIDI message.
    pub message: MidiMessage,
}

impl TimestampedMidiEvent {
    /// Create a new timestamped event.
    pub fn new(
        timestamp_us: u64,
        received_at: Instant,
        device_id: MidiDeviceId,
        message: MidiMessage,
    ) -> Self {
        Self {
            timestamp_us,
            received_at,
            device_id,
            message,
        }
    }

    /// Create an event with the current instant as received_at.
    pub fn now(timestamp_us: u64, device_id: MidiDeviceId, message: MidiMessage) -> Self {
        Self {
            timestamp_us,
            received_at: Instant::now(),
            device_id,
            message,
        }
    }

    /// Get the time elapsed since this event was received.
    pub fn age(&self) -> std::time::Duration {
        self.received_at.elapsed()
    }
}

// ============================================================================
// MIDI 2.0 Preparation (Feature-Gated)
// ============================================================================

/// Per-note controller for MIDI 2.0.
/// MIDI 2.0 allows controllers to be applied to individual notes.
#[cfg(feature = "midi2")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerNoteController {
    pub channel: Channel,
    pub note: u8,
    pub controller: u8,
    pub value: ControlValue,
}

/// Per-note pitch bend for MIDI 2.0.
#[cfg(feature = "midi2")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerNotePitchBend {
    pub channel: Channel,
    pub note: u8,
    /// 32-bit pitch bend value (centered at 0x80000000).
    pub value: u32,
}

/// Per-note management message for MIDI 2.0.
#[cfg(feature = "midi2")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerNoteManagement {
    pub channel: Channel,
    pub note: u8,
    /// Flags: bit 0 = detach, bit 1 = reset controllers.
    pub flags: u8,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_velocity_midi1_roundtrip() {
        for v in 0..=127u8 {
            let vel = Velocity::from_midi1(v);
            assert_eq!(vel.to_midi1(), v, "Velocity {} should roundtrip", v);
        }
    }

    #[test]
    fn test_velocity_scaling() {
        // Zero should stay zero
        assert_eq!(Velocity::from_midi1(0).to_midi2(), 0);

        // Max should be max
        let max = Velocity::from_midi1(127);
        assert!(max.to_midi2() >= 65000, "127 should scale to near max");

        // Float conversion
        let mid = Velocity::from_midi1(64);
        let f = mid.as_f32();
        assert!(f > 0.4 && f < 0.6, "64 should be around 0.5");
    }

    #[test]
    fn test_control_value_roundtrip() {
        for v in 0..=127u8 {
            let cv = ControlValue::from_7bit(v);
            assert_eq!(cv.to_7bit(), v, "ControlValue {} should roundtrip", v);
        }
    }

    #[test]
    fn test_channel() {
        let ch = Channel::new(5);
        assert_eq!(ch.get(), 5);
        assert_eq!(ch.display(), 6);

        let ch_clamped = Channel::new_clamped(20);
        assert_eq!(ch_clamped.get(), 4); // 20 & 0x0F = 4
    }

    #[test]
    fn test_message_channel() {
        let msg = MidiMessage::NoteOn {
            channel: Channel::new(3),
            note: 60,
            velocity: Velocity::from_midi1(100),
        };
        assert_eq!(msg.channel().unwrap().get(), 3);
        assert!(msg.is_note());
        assert!(!msg.is_realtime());

        let clock = MidiMessage::Clock;
        assert!(clock.channel().is_none());
        assert!(clock.is_realtime());
    }

    #[test]
    fn test_timestamped_event() {
        let event = TimestampedMidiEvent::now(
            12345,
            MidiDeviceId::new(0),
            MidiMessage::NoteOn {
                channel: Channel::new(0),
                note: 60,
                velocity: Velocity::from_midi1(100),
            },
        );

        assert_eq!(event.timestamp_us, 12345);
        assert!(event.age().as_micros() < 1000); // Should be very recent
    }
}
