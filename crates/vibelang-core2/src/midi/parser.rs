//! MIDI message parser with running status and SysEx support.
//!
//! This module provides a stateful parser that handles:
//! - Running status (omitted status bytes for consecutive messages of the same type)
//! - SysEx messages that may span multiple packets
//! - All MIDI 1.0 message types

use super::events::{Channel, MidiMessage, Velocity};

// ============================================================================
// MIDI Status Bytes
// ============================================================================

/// MIDI status byte constants.
pub mod status {
    // Channel Voice Messages (0x8n - 0xEn where n = channel)
    pub const NOTE_OFF: u8 = 0x80;
    pub const NOTE_ON: u8 = 0x90;
    pub const POLY_AFTERTOUCH: u8 = 0xA0;
    pub const CONTROL_CHANGE: u8 = 0xB0;
    pub const PROGRAM_CHANGE: u8 = 0xC0;
    pub const CHANNEL_AFTERTOUCH: u8 = 0xD0;
    pub const PITCH_BEND: u8 = 0xE0;

    // System Common Messages
    pub const SYSEX_START: u8 = 0xF0;
    pub const TIME_CODE: u8 = 0xF1;
    pub const SONG_POSITION: u8 = 0xF2;
    pub const SONG_SELECT: u8 = 0xF3;
    pub const TUNE_REQUEST: u8 = 0xF6;
    pub const SYSEX_END: u8 = 0xF7;

    // System Realtime Messages
    pub const CLOCK: u8 = 0xF8;
    pub const START: u8 = 0xFA;
    pub const CONTINUE: u8 = 0xFB;
    pub const STOP: u8 = 0xFC;
    pub const ACTIVE_SENSING: u8 = 0xFE;
    pub const RESET: u8 = 0xFF;

    /// Check if a byte is a status byte (MSB set).
    #[inline]
    pub fn is_status(byte: u8) -> bool {
        byte & 0x80 != 0
    }

    /// Check if a status byte is a realtime message.
    #[inline]
    pub fn is_realtime(status: u8) -> bool {
        status >= CLOCK
    }

    /// Get the number of data bytes expected for a status byte.
    /// Returns None for SysEx (variable length) and realtime (0 bytes handled separately).
    pub fn data_length(status: u8) -> Option<usize> {
        match status & 0xF0 {
            NOTE_OFF | NOTE_ON | POLY_AFTERTOUCH | CONTROL_CHANGE | PITCH_BEND => Some(2),
            PROGRAM_CHANGE | CHANNEL_AFTERTOUCH => Some(1),
            0xF0 => match status {
                SYSEX_START => None, // Variable length
                TIME_CODE | SONG_SELECT => Some(1),
                SONG_POSITION => Some(2),
                TUNE_REQUEST | SYSEX_END => Some(0),
                _ if status >= CLOCK => Some(0), // Realtime
                _ => None,
            },
            _ => None,
        }
    }
}

// ============================================================================
// MIDI Parser
// ============================================================================

/// Stateful MIDI message parser.
///
/// Handles:
/// - Running status (when status byte is omitted)
/// - SysEx messages spanning multiple packets
/// - All MIDI 1.0 message types
pub struct MidiParser {
    /// Current running status (for when status byte is omitted).
    running_status: Option<u8>,
    /// Buffer for accumulating SysEx data.
    sysex_buffer: Vec<u8>,
    /// Whether we're currently in the middle of a SysEx message.
    in_sysex: bool,
}

impl MidiParser {
    /// Create a new MIDI parser.
    pub fn new() -> Self {
        Self {
            running_status: None,
            sysex_buffer: Vec::with_capacity(256),
            in_sysex: false,
        }
    }

    /// Reset the parser state.
    pub fn reset(&mut self) {
        self.running_status = None;
        self.sysex_buffer.clear();
        self.in_sysex = false;
    }

    /// Parse MIDI bytes into messages.
    ///
    /// May return multiple messages if the input contains multiple complete messages.
    /// Returns an empty Vec if the data is incomplete (e.g., partial SysEx).
    pub fn parse(&mut self, data: &[u8]) -> Vec<MidiMessage> {
        let mut messages = Vec::new();
        let mut pos = 0;

        while pos < data.len() {
            let byte = data[pos];

            // Handle system realtime messages - they can appear anywhere, even mid-SysEx
            if status::is_realtime(byte) {
                if let Some(msg) = self.parse_realtime(byte) {
                    messages.push(msg);
                }
                pos += 1;
                continue;
            }

            // Handle SysEx accumulation
            if self.in_sysex {
                if byte == status::SYSEX_END {
                    // SysEx complete
                    let sysex_data = std::mem::take(&mut self.sysex_buffer);
                    messages.push(MidiMessage::SysEx(sysex_data));
                    self.in_sysex = false;
                    self.running_status = None; // SysEx clears running status
                    pos += 1;
                } else if status::is_status(byte) {
                    // Unexpected status byte - abort SysEx
                    tracing::warn!(
                        "[MIDI_PARSER] SysEx aborted by status byte 0x{:02X}",
                        byte
                    );
                    self.sysex_buffer.clear();
                    self.in_sysex = false;
                    // Don't advance pos - let the status byte be parsed
                } else {
                    // Data byte - accumulate
                    self.sysex_buffer.push(byte);
                    pos += 1;
                }
                continue;
            }

            // Check for SysEx start
            if byte == status::SYSEX_START {
                self.in_sysex = true;
                self.sysex_buffer.clear();
                self.running_status = None; // SysEx clears running status
                pos += 1;
                continue;
            }

            // Parse regular message
            let (msg, consumed) = self.parse_message(&data[pos..]);
            if let Some(m) = msg {
                messages.push(m);
            }
            if consumed == 0 {
                // Can't parse - need more data
                break;
            }
            pos += consumed;
        }

        messages
    }

    /// Parse a single MIDI message (non-SysEx, non-realtime).
    /// Returns the message and number of bytes consumed.
    fn parse_message(&mut self, data: &[u8]) -> (Option<MidiMessage>, usize) {
        if data.is_empty() {
            return (None, 0);
        }

        let first = data[0];

        // Determine status byte
        let (status, data_start) = if status::is_status(first) && first < status::SYSEX_START {
            // Explicit status byte (channel message)
            self.running_status = Some(first);
            (first, 1)
        } else if let Some(rs) = self.running_status {
            // Use running status
            (rs, 0)
        } else {
            // No status and no running status - can't parse
            tracing::debug!("[MIDI_PARSER] Data byte without running status: 0x{:02X}", first);
            return (None, 1); // Skip the byte
        };

        // Get expected data length
        let data_len = match status::data_length(status) {
            Some(len) => len,
            None => return (None, 1), // Unknown message type
        };

        // Check if we have enough data
        let available = data.len() - data_start;
        if available < data_len {
            return (None, 0); // Need more data
        }

        // Extract data bytes
        let msg_data = &data[data_start..data_start + data_len];
        let consumed = data_start + data_len;

        // Parse the message
        let channel = Channel::from(status & 0x0F);
        let msg = match status & 0xF0 {
            status::NOTE_OFF => Some(MidiMessage::NoteOff {
                channel,
                note: msg_data[0] & 0x7F,
                velocity: Velocity::from_midi1(msg_data[1] & 0x7F),
            }),
            status::NOTE_ON => {
                let velocity = msg_data[1] & 0x7F;
                if velocity == 0 {
                    // Note On with velocity 0 = Note Off
                    Some(MidiMessage::NoteOff {
                        channel,
                        note: msg_data[0] & 0x7F,
                        velocity: Velocity::ZERO,
                    })
                } else {
                    Some(MidiMessage::NoteOn {
                        channel,
                        note: msg_data[0] & 0x7F,
                        velocity: Velocity::from_midi1(velocity),
                    })
                }
            }
            status::POLY_AFTERTOUCH => Some(MidiMessage::PolyAftertouch {
                channel,
                note: msg_data[0] & 0x7F,
                pressure: msg_data[1] & 0x7F,
            }),
            status::CONTROL_CHANGE => Some(MidiMessage::ControlChange {
                channel,
                controller: msg_data[0] & 0x7F,
                value: msg_data[1] & 0x7F,
            }),
            status::PROGRAM_CHANGE => Some(MidiMessage::ProgramChange {
                channel,
                program: msg_data[0] & 0x7F,
            }),
            status::CHANNEL_AFTERTOUCH => Some(MidiMessage::ChannelAftertouch {
                channel,
                pressure: msg_data[0] & 0x7F,
            }),
            status::PITCH_BEND => {
                let lsb = (msg_data[0] & 0x7F) as i16;
                let msb = (msg_data[1] & 0x7F) as i16;
                let value = ((msb << 7) | lsb) - 8192;
                Some(MidiMessage::PitchBend { channel, value })
            }
            _ => None,
        };

        (msg, consumed)
    }

    /// Parse a system realtime message.
    fn parse_realtime(&self, status: u8) -> Option<MidiMessage> {
        match status {
            status::CLOCK => Some(MidiMessage::Clock),
            status::START => Some(MidiMessage::Start),
            status::CONTINUE => Some(MidiMessage::Continue),
            status::STOP => Some(MidiMessage::Stop),
            status::ACTIVE_SENSING => Some(MidiMessage::ActiveSensing),
            status::RESET => Some(MidiMessage::Reset),
            _ => None,
        }
    }

    /// Parse system common messages (non-SysEx).
    /// Called for F1-F6 status bytes.
    pub fn parse_system_common(&mut self, data: &[u8]) -> (Option<MidiMessage>, usize) {
        if data.is_empty() {
            return (None, 0);
        }

        let status = data[0];
        match status {
            status::TIME_CODE if data.len() >= 2 => {
                let byte = data[1] & 0x7F;
                let msg = MidiMessage::TimeCode {
                    msg_type: (byte >> 4) & 0x07,
                    value: byte & 0x0F,
                };
                (Some(msg), 2)
            }
            status::SONG_POSITION if data.len() >= 3 => {
                let lsb = (data[1] & 0x7F) as u16;
                let msb = (data[2] & 0x7F) as u16;
                let position = (msb << 7) | lsb;
                (Some(MidiMessage::SongPosition { position }), 3)
            }
            status::SONG_SELECT if data.len() >= 2 => {
                let song = data[1] & 0x7F;
                (Some(MidiMessage::SongSelect { song }), 2)
            }
            status::TUNE_REQUEST => (Some(MidiMessage::TuneRequest), 1),
            _ => (None, 1),
        }
    }
}

impl Default for MidiParser {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Parse a single complete MIDI message (no running status, no SysEx spanning).
/// This is a convenience function for simple cases where you have a complete message.
pub fn parse_midi_bytes(data: &[u8]) -> Option<MidiMessage> {
    if data.is_empty() {
        return None;
    }

    let status = data[0];

    // Handle system realtime first (single byte)
    if status::is_realtime(status) {
        return match status {
            status::CLOCK => Some(MidiMessage::Clock),
            status::START => Some(MidiMessage::Start),
            status::CONTINUE => Some(MidiMessage::Continue),
            status::STOP => Some(MidiMessage::Stop),
            status::ACTIVE_SENSING => Some(MidiMessage::ActiveSensing),
            status::RESET => Some(MidiMessage::Reset),
            _ => None,
        };
    }

    // Handle system common
    match status {
        status::SYSEX_START => {
            // Find SYSEX_END
            let end_pos = data.iter().position(|&b| b == status::SYSEX_END)?;
            let sysex_data = data[1..end_pos].to_vec();
            return Some(MidiMessage::SysEx(sysex_data));
        }
        status::TIME_CODE if data.len() >= 2 => {
            let byte = data[1] & 0x7F;
            return Some(MidiMessage::TimeCode {
                msg_type: (byte >> 4) & 0x07,
                value: byte & 0x0F,
            });
        }
        status::SONG_POSITION if data.len() >= 3 => {
            let lsb = (data[1] & 0x7F) as u16;
            let msb = (data[2] & 0x7F) as u16;
            return Some(MidiMessage::SongPosition {
                position: (msb << 7) | lsb,
            });
        }
        status::SONG_SELECT if data.len() >= 2 => {
            return Some(MidiMessage::SongSelect {
                song: data[1] & 0x7F,
            });
        }
        status::TUNE_REQUEST => {
            return Some(MidiMessage::TuneRequest);
        }
        _ => {}
    }

    // Channel messages
    let msg_type = status & 0xF0;
    let channel = Channel::from(status & 0x0F);

    match msg_type {
        status::NOTE_OFF if data.len() >= 3 => Some(MidiMessage::NoteOff {
            channel,
            note: data[1] & 0x7F,
            velocity: Velocity::from_midi1(data[2] & 0x7F),
        }),
        status::NOTE_ON if data.len() >= 3 => {
            let velocity = data[2] & 0x7F;
            if velocity == 0 {
                Some(MidiMessage::NoteOff {
                    channel,
                    note: data[1] & 0x7F,
                    velocity: Velocity::ZERO,
                })
            } else {
                Some(MidiMessage::NoteOn {
                    channel,
                    note: data[1] & 0x7F,
                    velocity: Velocity::from_midi1(velocity),
                })
            }
        }
        status::POLY_AFTERTOUCH if data.len() >= 3 => Some(MidiMessage::PolyAftertouch {
            channel,
            note: data[1] & 0x7F,
            pressure: data[2] & 0x7F,
        }),
        status::CONTROL_CHANGE if data.len() >= 3 => Some(MidiMessage::ControlChange {
            channel,
            controller: data[1] & 0x7F,
            value: data[2] & 0x7F,
        }),
        status::PROGRAM_CHANGE if data.len() >= 2 => Some(MidiMessage::ProgramChange {
            channel,
            program: data[1] & 0x7F,
        }),
        status::CHANNEL_AFTERTOUCH if data.len() >= 2 => Some(MidiMessage::ChannelAftertouch {
            channel,
            pressure: data[1] & 0x7F,
        }),
        status::PITCH_BEND if data.len() >= 3 => {
            let lsb = (data[1] & 0x7F) as i16;
            let msb = (data[2] & 0x7F) as i16;
            let value = ((msb << 7) | lsb) - 8192;
            Some(MidiMessage::PitchBend { channel, value })
        }
        _ => None,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_note_on() {
        let data = [0x90, 60, 100]; // Note On, channel 0, note 60, velocity 100
        let msg = parse_midi_bytes(&data).unwrap();

        match msg {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => {
                assert_eq!(channel.get(), 0);
                assert_eq!(note, 60);
                assert_eq!(velocity.to_midi1(), 100);
            }
            _ => panic!("Expected NoteOn"),
        }
    }

    #[test]
    fn test_note_on_velocity_zero_is_note_off() {
        let data = [0x90, 60, 0]; // Note On with velocity 0 = Note Off
        let msg = parse_midi_bytes(&data).unwrap();

        match msg {
            MidiMessage::NoteOff { channel, note, .. } => {
                assert_eq!(channel.get(), 0);
                assert_eq!(note, 60);
            }
            _ => panic!("Expected NoteOff"),
        }
    }

    #[test]
    fn test_parse_control_change() {
        let data = [0xB5, 74, 64]; // CC on channel 5, controller 74, value 64
        let msg = parse_midi_bytes(&data).unwrap();

        match msg {
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => {
                assert_eq!(channel.get(), 5);
                assert_eq!(controller, 74);
                assert_eq!(value, 64);
            }
            _ => panic!("Expected ControlChange"),
        }
    }

    #[test]
    fn test_parse_pitch_bend() {
        // Center position (8192 = 0x2000)
        let data = [0xE0, 0x00, 0x40]; // LSB=0, MSB=64
        let msg = parse_midi_bytes(&data).unwrap();

        match msg {
            MidiMessage::PitchBend { channel, value } => {
                assert_eq!(channel.get(), 0);
                assert_eq!(value, 0); // Center
            }
            _ => panic!("Expected PitchBend"),
        }

        // Max positive
        let data = [0xE0, 0x7F, 0x7F];
        let msg = parse_midi_bytes(&data).unwrap();
        match msg {
            MidiMessage::PitchBend { value, .. } => {
                assert_eq!(value, 8191); // Max positive
            }
            _ => panic!("Expected PitchBend"),
        }
    }

    #[test]
    fn test_parse_sysex() {
        let data = [0xF0, 0x7E, 0x00, 0x06, 0x01, 0xF7]; // Identity request
        let msg = parse_midi_bytes(&data).unwrap();

        match msg {
            MidiMessage::SysEx(sysex) => {
                assert_eq!(sysex, vec![0x7E, 0x00, 0x06, 0x01]);
            }
            _ => panic!("Expected SysEx"),
        }
    }

    #[test]
    fn test_parse_realtime() {
        assert!(matches!(
            parse_midi_bytes(&[0xF8]),
            Some(MidiMessage::Clock)
        ));
        assert!(matches!(
            parse_midi_bytes(&[0xFA]),
            Some(MidiMessage::Start)
        ));
        assert!(matches!(
            parse_midi_bytes(&[0xFB]),
            Some(MidiMessage::Continue)
        ));
        assert!(matches!(
            parse_midi_bytes(&[0xFC]),
            Some(MidiMessage::Stop)
        ));
    }

    #[test]
    fn test_parse_program_change() {
        let data = [0xC3, 42]; // Program Change, channel 3, program 42
        let msg = parse_midi_bytes(&data).unwrap();

        match msg {
            MidiMessage::ProgramChange { channel, program } => {
                assert_eq!(channel.get(), 3);
                assert_eq!(program, 42);
            }
            _ => panic!("Expected ProgramChange"),
        }
    }

    #[test]
    fn test_parse_aftertouch() {
        // Channel aftertouch
        let data = [0xD2, 80];
        let msg = parse_midi_bytes(&data).unwrap();
        match msg {
            MidiMessage::ChannelAftertouch { channel, pressure } => {
                assert_eq!(channel.get(), 2);
                assert_eq!(pressure, 80);
            }
            _ => panic!("Expected ChannelAftertouch"),
        }

        // Poly aftertouch
        let data = [0xA4, 60, 90];
        let msg = parse_midi_bytes(&data).unwrap();
        match msg {
            MidiMessage::PolyAftertouch {
                channel,
                note,
                pressure,
            } => {
                assert_eq!(channel.get(), 4);
                assert_eq!(note, 60);
                assert_eq!(pressure, 90);
            }
            _ => panic!("Expected PolyAftertouch"),
        }
    }

    #[test]
    fn test_stateful_parser_running_status() {
        let mut parser = MidiParser::new();

        // First message with status
        let msgs = parser.parse(&[0x90, 60, 100]);
        assert_eq!(msgs.len(), 1);
        assert!(matches!(msgs[0], MidiMessage::NoteOn { .. }));

        // Second message using running status
        let msgs = parser.parse(&[64, 80]); // Just note and velocity
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            MidiMessage::NoteOn { note, velocity, .. } => {
                assert_eq!(*note, 64);
                assert_eq!(velocity.to_midi1(), 80);
            }
            _ => panic!("Expected NoteOn"),
        }
    }

    #[test]
    fn test_stateful_parser_sysex_spanning() {
        let mut parser = MidiParser::new();

        // Start of SysEx
        let msgs = parser.parse(&[0xF0, 0x7E, 0x00]);
        assert_eq!(msgs.len(), 0); // Not complete yet

        // Middle
        let msgs = parser.parse(&[0x06, 0x01]);
        assert_eq!(msgs.len(), 0); // Still not complete

        // End
        let msgs = parser.parse(&[0xF7]);
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            MidiMessage::SysEx(data) => {
                assert_eq!(data, &[0x7E, 0x00, 0x06, 0x01]);
            }
            _ => panic!("Expected SysEx"),
        }
    }

    #[test]
    fn test_realtime_mid_sysex() {
        let mut parser = MidiParser::new();

        // SysEx with Clock in the middle
        let msgs = parser.parse(&[0xF0, 0x7E, 0xF8, 0x00, 0xF7]);
        assert_eq!(msgs.len(), 2);
        assert!(matches!(msgs[0], MidiMessage::Clock));
        assert!(matches!(msgs[1], MidiMessage::SysEx(_)));
    }
}
