//! MIDI event types with full MIDI 1.0 support and MIDI 2.0 future-proofing.
//!
//! This module provides:
//! - `TimestampedMidiEvent` - MIDI events with preserved timestamps for latency measurement
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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

// ============================================================================
// MIDI 2.0 Group (4-bit)
// ============================================================================

/// MIDI 2.0 Group (0-15).
///
/// MIDI 2.0 adds the concept of Groups, allowing up to 16 groups of 16 channels each,
/// for a total of 256 addressable channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Group(u8);

impl Group {
    /// Create a new group (0-15). Panics if out of range.
    #[inline]
    pub fn new(g: u8) -> Self {
        assert!(g < 16, "MIDI 2.0 group must be 0-15");
        Self(g)
    }

    /// Create a new group, clamping to valid range.
    #[inline]
    pub fn new_clamped(g: u8) -> Self {
        Self(g & 0x0F)
    }

    /// Get the group number (0-15).
    #[inline]
    pub fn get(&self) -> u8 {
        self.0
    }
}

impl From<u8> for Group {
    fn from(g: u8) -> Self {
        Self::new_clamped(g)
    }
}

// ============================================================================
// MIDI 2.0 Group + Channel
// ============================================================================

/// Combined Group and Channel for MIDI 2.0 addressing.
///
/// Efficiently stores both group (4-bit) and channel (4-bit) in a single byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GroupChannel(u8);

impl GroupChannel {
    /// Create a new group-channel pair.
    #[inline]
    pub fn new(group: u8, channel: u8) -> Self {
        Self(((group & 0x0F) << 4) | (channel & 0x0F))
    }

    /// Get the group (0-15).
    #[inline]
    pub fn group(&self) -> u8 {
        self.0 >> 4
    }

    /// Get the channel (0-15).
    #[inline]
    pub fn channel(&self) -> u8 {
        self.0 & 0x0F
    }

    /// Get as Group type.
    #[inline]
    pub fn as_group(&self) -> Group {
        Group(self.group())
    }

    /// Get as Channel type.
    #[inline]
    pub fn as_channel(&self) -> Channel {
        Channel(self.channel())
    }

    /// Get the flat index (0-255) for array indexing.
    #[inline]
    pub fn to_flat(&self) -> u8 {
        self.0
    }

    /// Alias for `to_flat()` - get flat index for HashMap keying.
    #[inline]
    pub fn flat_index(&self) -> u16 {
        self.0 as u16
    }

    /// Create from flat index (u8).
    #[inline]
    pub fn from_flat(flat: u8) -> Self {
        Self(flat)
    }

    /// Create from flat index (u16, for HashMap compatibility).
    #[inline]
    pub fn from_flat_u16(flat: u16) -> Self {
        Self(flat as u8)
    }

    /// Convert MIDI 2.0 pitch bend value to semitones.
    #[inline]
    pub fn pitch_bend_to_semitones(value: u32, range: u8) -> f32 {
        let center = 0x80000000u32;
        let offset = (value as i64) - (center as i64);
        (offset as f64 / center as f64 * range as f64) as f32
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

    // ========================================================================
    // MIDI 2.0 Channel Voice Messages
    // ========================================================================
    /// MIDI 2.0 Note On with 16-bit velocity and per-note attributes.
    Midi2NoteOn {
        group_channel: GroupChannel,
        note: u8,
        velocity: Velocity,
        attribute_type: u8,
        attribute_value: u16,
    },

    /// MIDI 2.0 Note Off with 16-bit velocity and per-note attributes.
    Midi2NoteOff {
        group_channel: GroupChannel,
        note: u8,
        velocity: Velocity,
        attribute_type: u8,
        attribute_value: u16,
    },

    /// MIDI 2.0 Control Change with 32-bit value.
    Midi2ControlChange {
        group_channel: GroupChannel,
        controller: u8,
        value: ControlValue,
    },

    /// MIDI 2.0 Pitch Bend with 32-bit value.
    Midi2PitchBend {
        group_channel: GroupChannel,
        value: u32,
    },

    /// MIDI 2.0 Per-Note Pitch Bend.
    Midi2PerNotePitchBend {
        group_channel: GroupChannel,
        note: u8,
        value: u32,
    },

    /// MIDI 2.0 Per-Note Controller.
    Midi2PerNoteController {
        group_channel: GroupChannel,
        note: u8,
        controller: u8,
        value: ControlValue,
    },

    /// MIDI 2.0 Polyphonic Aftertouch with 32-bit value.
    Midi2PolyPressure {
        group_channel: GroupChannel,
        note: u8,
        pressure: ControlValue,
    },

    /// MIDI 2.0 Channel Aftertouch with 32-bit value.
    Midi2ChannelPressure {
        group_channel: GroupChannel,
        pressure: ControlValue,
    },

    /// MIDI 2.0 Program Change with bank select.
    Midi2ProgramChange {
        group_channel: GroupChannel,
        program: u8,
        bank_valid: bool,
        bank_msb: u8,
        bank_lsb: u8,
    },

    /// MIDI 2.0 Per-Note Management message.
    Midi2PerNoteManagement {
        group_channel: GroupChannel,
        note: u8,
        detach: bool,
        reset_controllers: bool,
    },

    /// MIDI 2.0 Registered Per-Note Controller.
    Midi2RegisteredPerNoteController {
        group_channel: GroupChannel,
        note: u8,
        index: u8,
        value: ControlValue,
    },

    /// MIDI 2.0 Assignable Per-Note Controller.
    Midi2AssignablePerNoteController {
        group_channel: GroupChannel,
        note: u8,
        index: u8,
        value: ControlValue,
    },

    /// MIDI 2.0 Registered Controller (RPN).
    Midi2RegisteredController {
        group_channel: GroupChannel,
        bank: u8,
        index: u8,
        value: ControlValue,
    },

    /// MIDI 2.0 Assignable Controller (NRPN).
    Midi2AssignableController {
        group_channel: GroupChannel,
        bank: u8,
        index: u8,
        value: ControlValue,
    },

    /// MIDI 2.0 Relative Registered Controller.
    Midi2RelativeRegisteredController {
        group_channel: GroupChannel,
        bank: u8,
        index: u8,
        delta: i32,
    },

    /// MIDI 2.0 Relative Assignable Controller.
    Midi2RelativeAssignableController {
        group_channel: GroupChannel,
        bank: u8,
        index: u8,
        delta: i32,
    },
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

            // MIDI 2.0 messages use GroupChannel
            MidiMessage::Midi2NoteOn { group_channel, .. }
            | MidiMessage::Midi2NoteOff { group_channel, .. }
            | MidiMessage::Midi2ControlChange { group_channel, .. }
            | MidiMessage::Midi2PitchBend { group_channel, .. }
            | MidiMessage::Midi2PerNotePitchBend { group_channel, .. }
            | MidiMessage::Midi2PerNoteController { group_channel, .. }
            | MidiMessage::Midi2PolyPressure { group_channel, .. }
            | MidiMessage::Midi2ChannelPressure { group_channel, .. }
            | MidiMessage::Midi2ProgramChange { group_channel, .. }
            | MidiMessage::Midi2PerNoteManagement { group_channel, .. }
            | MidiMessage::Midi2RegisteredPerNoteController { group_channel, .. }
            | MidiMessage::Midi2AssignablePerNoteController { group_channel, .. }
            | MidiMessage::Midi2RegisteredController { group_channel, .. }
            | MidiMessage::Midi2AssignableController { group_channel, .. }
            | MidiMessage::Midi2RelativeRegisteredController { group_channel, .. }
            | MidiMessage::Midi2RelativeAssignableController { group_channel, .. } => {
                Some(group_channel.as_channel())
            }

            _ => None,
        }
    }

    /// Get the MIDI 2.0 group-channel pair, None for MIDI 1.0 or system messages.
    pub fn group_channel(&self) -> Option<GroupChannel> {
        match self {
            MidiMessage::Midi2NoteOn { group_channel, .. }
            | MidiMessage::Midi2NoteOff { group_channel, .. }
            | MidiMessage::Midi2ControlChange { group_channel, .. }
            | MidiMessage::Midi2PitchBend { group_channel, .. }
            | MidiMessage::Midi2PerNotePitchBend { group_channel, .. }
            | MidiMessage::Midi2PerNoteController { group_channel, .. }
            | MidiMessage::Midi2PolyPressure { group_channel, .. }
            | MidiMessage::Midi2ChannelPressure { group_channel, .. }
            | MidiMessage::Midi2ProgramChange { group_channel, .. }
            | MidiMessage::Midi2PerNoteManagement { group_channel, .. }
            | MidiMessage::Midi2RegisteredPerNoteController { group_channel, .. }
            | MidiMessage::Midi2AssignablePerNoteController { group_channel, .. }
            | MidiMessage::Midi2RegisteredController { group_channel, .. }
            | MidiMessage::Midi2AssignableController { group_channel, .. }
            | MidiMessage::Midi2RelativeRegisteredController { group_channel, .. }
            | MidiMessage::Midi2RelativeAssignableController { group_channel, .. } => {
                Some(*group_channel)
            }
            _ => None,
        }
    }

    /// Check if this is a MIDI 2.0 message.
    pub fn is_midi2(&self) -> bool {
        matches!(
            self,
            MidiMessage::Midi2NoteOn { .. }
                | MidiMessage::Midi2NoteOff { .. }
                | MidiMessage::Midi2ControlChange { .. }
                | MidiMessage::Midi2PitchBend { .. }
                | MidiMessage::Midi2PerNotePitchBend { .. }
                | MidiMessage::Midi2PerNoteController { .. }
                | MidiMessage::Midi2PolyPressure { .. }
                | MidiMessage::Midi2ChannelPressure { .. }
                | MidiMessage::Midi2ProgramChange { .. }
                | MidiMessage::Midi2PerNoteManagement { .. }
                | MidiMessage::Midi2RegisteredPerNoteController { .. }
                | MidiMessage::Midi2AssignablePerNoteController { .. }
                | MidiMessage::Midi2RegisteredController { .. }
                | MidiMessage::Midi2AssignableController { .. }
                | MidiMessage::Midi2RelativeRegisteredController { .. }
                | MidiMessage::Midi2RelativeAssignableController { .. }
        )
    }

    /// Check if this is a note message (Note On or Note Off).
    pub fn is_note(&self) -> bool {
        matches!(
            self,
            MidiMessage::NoteOn { .. }
                | MidiMessage::NoteOff { .. }
                | MidiMessage::Midi2NoteOn { .. }
                | MidiMessage::Midi2NoteOff { .. }
        )
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
    /// Used for latency measurement and beat-accurate capture.
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
// Reserved for future MIDI 2.0 support - feature not yet defined in Cargo.toml
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerNoteController {
    pub channel: Channel,
    pub note: u8,
    pub controller: u8,
    pub value: ControlValue,
}

/// Per-note pitch bend for MIDI 2.0.
// Reserved for future MIDI 2.0 support - feature not yet defined in Cargo.toml
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerNotePitchBend {
    pub channel: Channel,
    pub note: u8,
    /// 32-bit pitch bend value (centered at 0x80000000).
    pub value: u32,
}

/// Per-note management message for MIDI 2.0.
// Reserved for future MIDI 2.0 support - feature not yet defined in Cargo.toml
#[allow(dead_code)]
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
