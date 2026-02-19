//! MIDI message encoder with running status optimization.
//!
//! This module provides efficient MIDI message encoding for output,
//! with optional running status optimization to reduce bandwidth.

use super::events::MidiMessage;
use super::parser::status;

// ============================================================================
// MIDI Encoder
// ============================================================================

/// MIDI message encoder with running status support.
///
/// Running status allows omitting the status byte when sending
/// consecutive messages of the same type, reducing bandwidth.
pub struct MidiEncoder {
    /// Last status byte sent (for running status).
    last_status: Option<u8>,
    /// Whether to use running status optimization.
    use_running_status: bool,
}

impl MidiEncoder {
    /// Create a new encoder with running status enabled.
    pub fn new() -> Self {
        Self {
            last_status: None,
            use_running_status: true,
        }
    }

    /// Create an encoder without running status.
    pub fn without_running_status() -> Self {
        Self {
            last_status: None,
            use_running_status: false,
        }
    }

    /// Enable or disable running status.
    pub fn set_running_status(&mut self, enable: bool) {
        self.use_running_status = enable;
        if !enable {
            self.last_status = None;
        }
    }

    /// Reset encoder state (clears running status).
    pub fn reset(&mut self) {
        self.last_status = None;
    }

    /// Encode a MIDI message to bytes.
    pub fn encode(&mut self, msg: &MidiMessage) -> Vec<u8> {
        match msg {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => self.encode_channel_msg(
                status::NOTE_ON | channel.get(),
                *note,
                Some(velocity.to_midi1()),
            ),

            MidiMessage::NoteOff {
                channel,
                note,
                velocity,
            } => self.encode_channel_msg(
                status::NOTE_OFF | channel.get(),
                *note,
                Some(velocity.to_midi1()),
            ),

            MidiMessage::PolyAftertouch {
                channel,
                note,
                pressure,
            } => self.encode_channel_msg(
                status::POLY_AFTERTOUCH | channel.get(),
                *note,
                Some(*pressure),
            ),

            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => self.encode_channel_msg(
                status::CONTROL_CHANGE | channel.get(),
                *controller,
                Some(*value),
            ),

            MidiMessage::ProgramChange { channel, program } => {
                self.encode_channel_msg(status::PROGRAM_CHANGE | channel.get(), *program, None)
            }

            MidiMessage::ChannelAftertouch { channel, pressure } => {
                self.encode_channel_msg(status::CHANNEL_AFTERTOUCH | channel.get(), *pressure, None)
            }

            MidiMessage::PitchBend { channel, value } => {
                // Convert from -8192..+8191 to 0..16383
                let pb = (*value + 8192) as u16;
                let lsb = (pb & 0x7F) as u8;
                let msb = ((pb >> 7) & 0x7F) as u8;
                self.encode_channel_msg_raw(status::PITCH_BEND | channel.get(), &[lsb, msb])
            }

            MidiMessage::SysEx(data) => self.encode_sysex(data),

            MidiMessage::TimeCode { msg_type, value } => {
                let byte = ((*msg_type & 0x07) << 4) | (*value & 0x0F);
                self.clear_running_status();
                vec![status::TIME_CODE, byte]
            }

            MidiMessage::SongPosition { position } => {
                let lsb = (*position & 0x7F) as u8;
                let msb = ((*position >> 7) & 0x7F) as u8;
                self.clear_running_status();
                vec![status::SONG_POSITION, lsb, msb]
            }

            MidiMessage::SongSelect { song } => {
                self.clear_running_status();
                vec![status::SONG_SELECT, *song & 0x7F]
            }

            MidiMessage::TuneRequest => {
                self.clear_running_status();
                vec![status::TUNE_REQUEST]
            }

            MidiMessage::Clock => vec![status::CLOCK],
            MidiMessage::Start => vec![status::START],
            MidiMessage::Continue => vec![status::CONTINUE],
            MidiMessage::Stop => vec![status::STOP],
            MidiMessage::ActiveSensing => vec![status::ACTIVE_SENSING],
            MidiMessage::Reset => vec![status::RESET],

            // MIDI 2.0 messages - downscale to MIDI 1.0 for standard output
            MidiMessage::Midi2NoteOn {
                group_channel,
                note,
                velocity,
                ..
            } => self.encode_channel_msg(
                status::NOTE_ON | group_channel.channel(),
                *note,
                Some(velocity.to_midi1()),
            ),

            MidiMessage::Midi2NoteOff {
                group_channel,
                note,
                velocity,
                ..
            } => self.encode_channel_msg(
                status::NOTE_OFF | group_channel.channel(),
                *note,
                Some(velocity.to_midi1()),
            ),

            MidiMessage::Midi2ControlChange {
                group_channel,
                controller,
                value,
            } => self.encode_channel_msg(
                status::CONTROL_CHANGE | group_channel.channel(),
                *controller,
                Some(value.to_7bit()),
            ),

            MidiMessage::Midi2PitchBend {
                group_channel,
                value,
            } => {
                // Convert 32-bit MIDI 2.0 pitch bend to 14-bit MIDI 1.0
                // MIDI 2.0: 0 = -full, 0x80000000 = center, 0xFFFFFFFF = +full
                // MIDI 1.0: 0 = -full, 8192 = center, 16383 = +full
                let pb14 = (*value >> 18) as u16; // Scale down from 32-bit to 14-bit
                let lsb = (pb14 & 0x7F) as u8;
                let msb = ((pb14 >> 7) & 0x7F) as u8;
                self.encode_channel_msg_raw(status::PITCH_BEND | group_channel.channel(), &[lsb, msb])
            }

            MidiMessage::Midi2PerNotePitchBend {
                group_channel,
                note,
                value,
            } => {
                // Per-note pitch bend has no MIDI 1.0 equivalent
                // Best effort: send as channel pitch bend (loses per-note granularity)
                let pb14 = (*value >> 18) as u16;
                let lsb = (pb14 & 0x7F) as u8;
                let msb = ((pb14 >> 7) & 0x7F) as u8;
                // Note: we're losing the note-specific information
                let _ = note;
                self.encode_channel_msg_raw(status::PITCH_BEND | group_channel.channel(), &[lsb, msb])
            }

            MidiMessage::Midi2PerNoteController {
                group_channel,
                note,
                controller,
                value,
            } => {
                // Per-note CC has no MIDI 1.0 equivalent
                // Best effort: send as regular CC (loses per-note granularity)
                let _ = note;
                self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    *controller,
                    Some(value.to_7bit()),
                )
            }

            MidiMessage::Midi2PolyPressure {
                group_channel,
                note,
                pressure,
            } => {
                // Downscale 32-bit poly pressure to 7-bit
                self.encode_channel_msg(
                    status::POLY_AFTERTOUCH | group_channel.channel(),
                    *note,
                    Some(pressure.to_7bit()),
                )
            }

            MidiMessage::Midi2ChannelPressure {
                group_channel,
                pressure,
            } => {
                // Downscale 32-bit channel pressure to 7-bit
                self.encode_channel_msg(
                    status::CHANNEL_AFTERTOUCH | group_channel.channel(),
                    pressure.to_7bit(),
                    None,
                )
            }

            MidiMessage::Midi2ProgramChange {
                group_channel,
                program,
                bank_valid,
                bank_msb,
                bank_lsb,
            } => {
                // Send bank select CCs if valid, then program change
                let mut result = Vec::new();
                if *bank_valid {
                    result.extend(self.encode_channel_msg(
                        status::CONTROL_CHANGE | group_channel.channel(),
                        0, // CC 0 = Bank Select MSB
                        Some(*bank_msb),
                    ));
                    result.extend(self.encode_channel_msg(
                        status::CONTROL_CHANGE | group_channel.channel(),
                        32, // CC 32 = Bank Select LSB
                        Some(*bank_lsb),
                    ));
                }
                result.extend(self.encode_channel_msg(
                    status::PROGRAM_CHANGE | group_channel.channel(),
                    *program,
                    None,
                ));
                result
            }

            MidiMessage::Midi2PerNoteManagement { .. } => {
                // No MIDI 1.0 equivalent - return empty
                vec![]
            }

            MidiMessage::Midi2RegisteredPerNoteController {
                group_channel,
                note,
                index,
                value,
            } => {
                // No direct MIDI 1.0 equivalent - best effort: send as CC
                let _ = note; // Per-note granularity lost
                self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    *index,
                    Some(value.to_7bit()),
                )
            }

            MidiMessage::Midi2AssignablePerNoteController {
                group_channel,
                note,
                index,
                value,
            } => {
                // No direct MIDI 1.0 equivalent - best effort: send as CC
                let _ = note;
                self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    *index,
                    Some(value.to_7bit()),
                )
            }

            MidiMessage::Midi2RegisteredController {
                group_channel,
                bank,
                index,
                value,
            } => {
                // Convert to MIDI 1.0 RPN sequence
                let mut result = Vec::new();
                // RPN MSB (CC 101)
                result.extend(self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    101,
                    Some(*bank),
                ));
                // RPN LSB (CC 100)
                result.extend(self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    100,
                    Some(*index),
                ));
                // Data Entry MSB (CC 6)
                let value14 = value.to_14bit();
                result.extend(self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    6,
                    Some((value14 >> 7) as u8),
                ));
                // Data Entry LSB (CC 38)
                result.extend(self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    38,
                    Some((value14 & 0x7F) as u8),
                ));
                result
            }

            MidiMessage::Midi2AssignableController {
                group_channel,
                bank,
                index,
                value,
            } => {
                // Convert to MIDI 1.0 NRPN sequence
                let mut result = Vec::new();
                // NRPN MSB (CC 99)
                result.extend(self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    99,
                    Some(*bank),
                ));
                // NRPN LSB (CC 98)
                result.extend(self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    98,
                    Some(*index),
                ));
                // Data Entry MSB (CC 6)
                let value14 = value.to_14bit();
                result.extend(self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    6,
                    Some((value14 >> 7) as u8),
                ));
                // Data Entry LSB (CC 38)
                result.extend(self.encode_channel_msg(
                    status::CONTROL_CHANGE | group_channel.channel(),
                    38,
                    Some((value14 & 0x7F) as u8),
                ));
                result
            }

            MidiMessage::Midi2RelativeRegisteredController { .. }
            | MidiMessage::Midi2RelativeAssignableController { .. } => {
                // Relative controllers have no direct MIDI 1.0 equivalent
                vec![]
            }
        }
    }

    /// Encode a channel message with 1 or 2 data bytes.
    fn encode_channel_msg(&mut self, status: u8, data1: u8, data2: Option<u8>) -> Vec<u8> {
        let use_running = self.use_running_status && self.last_status == Some(status);

        if !use_running {
            self.last_status = Some(status);
        }

        match (use_running, data2) {
            (true, Some(d2)) => vec![data1 & 0x7F, d2 & 0x7F],
            (true, None) => vec![data1 & 0x7F],
            (false, Some(d2)) => vec![status, data1 & 0x7F, d2 & 0x7F],
            (false, None) => vec![status, data1 & 0x7F],
        }
    }

    /// Encode a channel message with raw data bytes.
    fn encode_channel_msg_raw(&mut self, status: u8, data: &[u8]) -> Vec<u8> {
        let use_running = self.use_running_status && self.last_status == Some(status);

        if !use_running {
            self.last_status = Some(status);
        }

        if use_running {
            data.iter().map(|b| b & 0x7F).collect()
        } else {
            let mut result = vec![status];
            result.extend(data.iter().map(|b| b & 0x7F));
            result
        }
    }

    /// Encode a SysEx message.
    pub fn encode_sysex(&mut self, data: &[u8]) -> Vec<u8> {
        self.clear_running_status();

        let mut result = Vec::with_capacity(data.len() + 2);
        result.push(status::SYSEX_START);
        result.extend(data.iter().map(|b| b & 0x7F)); // Ensure all data bytes have MSB clear
        result.push(status::SYSEX_END);
        result
    }

    /// Clear running status (called before system messages).
    fn clear_running_status(&mut self) {
        self.last_status = None;
    }
}

impl Default for MidiEncoder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Encode a note on message (no running status).
pub fn encode_note_on(channel: u8, note: u8, velocity: u8) -> [u8; 3] {
    [
        status::NOTE_ON | (channel & 0x0F),
        note & 0x7F,
        velocity & 0x7F,
    ]
}

/// Encode a note off message (no running status).
pub fn encode_note_off(channel: u8, note: u8, velocity: u8) -> [u8; 3] {
    [
        status::NOTE_OFF | (channel & 0x0F),
        note & 0x7F,
        velocity & 0x7F,
    ]
}

/// Encode a control change message (no running status).
pub fn encode_cc(channel: u8, controller: u8, value: u8) -> [u8; 3] {
    [
        status::CONTROL_CHANGE | (channel & 0x0F),
        controller & 0x7F,
        value & 0x7F,
    ]
}

/// Encode a program change message (no running status).
pub fn encode_program_change(channel: u8, program: u8) -> [u8; 2] {
    [status::PROGRAM_CHANGE | (channel & 0x0F), program & 0x7F]
}

/// Encode a pitch bend message (no running status).
pub fn encode_pitch_bend(channel: u8, value: i16) -> [u8; 3] {
    let pb = (value + 8192) as u16;
    let lsb = (pb & 0x7F) as u8;
    let msb = ((pb >> 7) & 0x7F) as u8;
    [status::PITCH_BEND | (channel & 0x0F), lsb, msb]
}

/// Encode a channel aftertouch message (no running status).
pub fn encode_channel_aftertouch(channel: u8, pressure: u8) -> [u8; 2] {
    [
        status::CHANNEL_AFTERTOUCH | (channel & 0x0F),
        pressure & 0x7F,
    ]
}

/// Encode a poly aftertouch message (no running status).
pub fn encode_poly_aftertouch(channel: u8, note: u8, pressure: u8) -> [u8; 3] {
    [
        status::POLY_AFTERTOUCH | (channel & 0x0F),
        note & 0x7F,
        pressure & 0x7F,
    ]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::midi::{Channel, Velocity};

    use super::*;

    #[test]
    fn test_encode_note_on() {
        let mut encoder = MidiEncoder::without_running_status();
        let msg = MidiMessage::NoteOn {
            channel: Channel::new(0),
            note: 60,
            velocity: Velocity::from_midi1(100),
        };
        let bytes = encoder.encode(&msg);
        assert_eq!(bytes, vec![0x90, 60, 100]);
    }

    #[test]
    fn test_encode_with_running_status() {
        let mut encoder = MidiEncoder::new();

        // First note - includes status
        let msg1 = MidiMessage::NoteOn {
            channel: Channel::new(0),
            note: 60,
            velocity: Velocity::from_midi1(100),
        };
        let bytes1 = encoder.encode(&msg1);
        assert_eq!(bytes1, vec![0x90, 60, 100]);

        // Second note - running status, no status byte
        let msg2 = MidiMessage::NoteOn {
            channel: Channel::new(0),
            note: 64,
            velocity: Velocity::from_midi1(80),
        };
        let bytes2 = encoder.encode(&msg2);
        assert_eq!(bytes2, vec![64, 80]); // No status byte!

        // Different message type - new status
        let msg3 = MidiMessage::NoteOff {
            channel: Channel::new(0),
            note: 60,
            velocity: Velocity::ZERO,
        };
        let bytes3 = encoder.encode(&msg3);
        assert_eq!(bytes3, vec![0x80, 60, 0]);
    }

    #[test]
    fn test_encode_pitch_bend() {
        let mut encoder = MidiEncoder::without_running_status();

        // Center (0)
        let msg = MidiMessage::PitchBend {
            channel: Channel::new(0),
            value: 0,
        };
        let bytes = encoder.encode(&msg);
        assert_eq!(bytes, vec![0xE0, 0x00, 0x40]); // 8192 = 0x2000 -> LSB=0, MSB=64

        // Max positive
        let msg = MidiMessage::PitchBend {
            channel: Channel::new(0),
            value: 8191,
        };
        let bytes = encoder.encode(&msg);
        assert_eq!(bytes, vec![0xE0, 0x7F, 0x7F]);
    }

    #[test]
    fn test_encode_sysex() {
        let mut encoder = MidiEncoder::new();
        let sysex = MidiMessage::SysEx(vec![0x7E, 0x00, 0x06, 0x01]);
        let bytes = encoder.encode(&sysex);
        assert_eq!(bytes, vec![0xF0, 0x7E, 0x00, 0x06, 0x01, 0xF7]);
    }

    #[test]
    fn test_encode_realtime() {
        let mut encoder = MidiEncoder::new();

        assert_eq!(encoder.encode(&MidiMessage::Clock), vec![0xF8]);
        assert_eq!(encoder.encode(&MidiMessage::Start), vec![0xFA]);
        assert_eq!(encoder.encode(&MidiMessage::Continue), vec![0xFB]);
        assert_eq!(encoder.encode(&MidiMessage::Stop), vec![0xFC]);
    }

    #[test]
    fn test_encode_program_change() {
        let msg = encode_program_change(5, 42);
        assert_eq!(msg, [0xC5, 42]);
    }

    #[test]
    fn test_encode_aftertouch() {
        let channel = encode_channel_aftertouch(2, 80);
        assert_eq!(channel, [0xD2, 80]);

        let poly = encode_poly_aftertouch(4, 60, 90);
        assert_eq!(poly, [0xA4, 60, 90]);
    }
}
