//! Universal MIDI Packet (UMP) parser for MIDI 2.0.
//!
//! This module provides parsing of UMP packets into VibeLang's MidiMessage types.
//! UMP is the native packet format for MIDI 2.0 and can also carry MIDI 1.0 messages.
//!
//! ## UMP Message Types
//!
//! | Type | Size | Description |
//! |------|------|-------------|
//! | 0x0  | 32-bit | Utility (timestamps, etc.) |
//! | 0x1  | 32-bit | System Real Time / Common |
//! | 0x2  | 32-bit | MIDI 1.0 Channel Voice |
//! | 0x3  | 64-bit | Data (SysEx) |
//! | 0x4  | 64-bit | MIDI 2.0 Channel Voice |
//! | 0x5  | 128-bit | Data (large SysEx) |
//!
//! This parser focuses on types 0x2 (MIDI 1.0) and 0x4 (MIDI 2.0 Channel Voice).

use super::events::{Channel, ControlValue, Group, GroupChannel, MidiMessage, Velocity};

/// UMP message type (4 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UmpMessageType {
    /// Utility messages (timestamps, etc.)
    Utility = 0x0,
    /// System Real Time and System Common
    SystemRealtime = 0x1,
    /// MIDI 1.0 Channel Voice (legacy)
    Midi1ChannelVoice = 0x2,
    /// Data messages (64-bit SysEx)
    Data64 = 0x3,
    /// MIDI 2.0 Channel Voice
    Midi2ChannelVoice = 0x4,
    /// Data messages (128-bit SysEx)
    Data128 = 0x5,
    /// Reserved
    Reserved6 = 0x6,
    Reserved7 = 0x7,
    Reserved8 = 0x8,
    Reserved9 = 0x9,
    ReservedA = 0xA,
    ReservedB = 0xB,
    ReservedC = 0xC,
    /// Flex Data
    FlexData = 0xD,
    ReservedE = 0xE,
    /// UMP Stream
    UmpStream = 0xF,
}

impl TryFrom<u8> for UmpMessageType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(Self::Utility),
            0x1 => Ok(Self::SystemRealtime),
            0x2 => Ok(Self::Midi1ChannelVoice),
            0x3 => Ok(Self::Data64),
            0x4 => Ok(Self::Midi2ChannelVoice),
            0x5 => Ok(Self::Data128),
            0x6 => Ok(Self::Reserved6),
            0x7 => Ok(Self::Reserved7),
            0x8 => Ok(Self::Reserved8),
            0x9 => Ok(Self::Reserved9),
            0xA => Ok(Self::ReservedA),
            0xB => Ok(Self::ReservedB),
            0xC => Ok(Self::ReservedC),
            0xD => Ok(Self::FlexData),
            0xE => Ok(Self::ReservedE),
            0xF => Ok(Self::UmpStream),
            _ => Err(()),
        }
    }
}

/// MIDI 1.0 Channel Voice status (4 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Midi1Status {
    NoteOff = 0x8,
    NoteOn = 0x9,
    PolyPressure = 0xA,
    ControlChange = 0xB,
    ProgramChange = 0xC,
    ChannelPressure = 0xD,
    PitchBend = 0xE,
}

impl TryFrom<u8> for Midi1Status {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x8 => Ok(Self::NoteOff),
            0x9 => Ok(Self::NoteOn),
            0xA => Ok(Self::PolyPressure),
            0xB => Ok(Self::ControlChange),
            0xC => Ok(Self::ProgramChange),
            0xD => Ok(Self::ChannelPressure),
            0xE => Ok(Self::PitchBend),
            _ => Err(()),
        }
    }
}

/// MIDI 2.0 Channel Voice status (4 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Midi2Status {
    RegisteredPerNoteController = 0x0,
    AssignablePerNoteController = 0x1,
    RegisteredController = 0x2,
    AssignableController = 0x3,
    RelativeRegisteredController = 0x4,
    RelativeAssignableController = 0x5,
    PerNotePitchBend = 0x6,
    // 0x7 reserved
    NoteOff = 0x8,
    NoteOn = 0x9,
    PolyPressure = 0xA,
    ControlChange = 0xB,
    ProgramChange = 0xC,
    ChannelPressure = 0xD,
    PitchBend = 0xE,
    PerNoteManagement = 0xF,
}

impl TryFrom<u8> for Midi2Status {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(Self::RegisteredPerNoteController),
            0x1 => Ok(Self::AssignablePerNoteController),
            0x2 => Ok(Self::RegisteredController),
            0x3 => Ok(Self::AssignableController),
            0x4 => Ok(Self::RelativeRegisteredController),
            0x5 => Ok(Self::RelativeAssignableController),
            0x6 => Ok(Self::PerNotePitchBend),
            0x8 => Ok(Self::NoteOff),
            0x9 => Ok(Self::NoteOn),
            0xA => Ok(Self::PolyPressure),
            0xB => Ok(Self::ControlChange),
            0xC => Ok(Self::ProgramChange),
            0xD => Ok(Self::ChannelPressure),
            0xE => Ok(Self::PitchBend),
            0xF => Ok(Self::PerNoteManagement),
            _ => Err(()),
        }
    }
}

/// UMP parser for MIDI 2.0 packets.
///
/// Parses Universal MIDI Packets into VibeLang's MidiMessage types.
/// Supports both MIDI 1.0 (type 0x2) and MIDI 2.0 Channel Voice (type 0x4) messages.
#[derive(Clone, Debug, Default)]
pub struct UmpParser;

impl UmpParser {
    /// Create a new UMP parser.
    pub fn new() -> Self {
        Self
    }

    /// Parse a UMP packet into a MidiMessage.
    ///
    /// # Arguments
    /// * `data` - Slice of 32-bit words comprising the UMP packet
    ///
    /// # Returns
    /// * `Some(MidiMessage)` if parsing succeeds
    /// * `None` if the packet is malformed or unsupported
    pub fn parse(&self, data: &[u32]) -> Option<MidiMessage> {
        if data.is_empty() {
            return None;
        }

        let first_word = data[0];
        let msg_type = ((first_word >> 28) & 0xF) as u8;
        let group = ((first_word >> 24) & 0xF) as u8;

        let msg_type = UmpMessageType::try_from(msg_type).ok()?;

        match msg_type {
            UmpMessageType::Midi1ChannelVoice => self.parse_midi1_voice(first_word, group),
            UmpMessageType::Midi2ChannelVoice => {
                if data.len() < 2 {
                    return None;
                }
                self.parse_midi2_voice(first_word, data[1], group)
            }
            UmpMessageType::SystemRealtime => self.parse_system_realtime(first_word),
            _ => None, // Other types not yet supported
        }
    }

    /// Parse the expected size of a UMP packet based on its message type.
    ///
    /// # Arguments
    /// * `first_word` - First 32-bit word of the packet
    ///
    /// # Returns
    /// Number of 32-bit words in the packet (1, 2, 3, or 4).
    pub fn packet_size(first_word: u32) -> usize {
        let msg_type = ((first_word >> 28) & 0xF) as u8;
        match msg_type {
            0x0 | 0x1 | 0x2 => 1, // Utility, System, MIDI 1.0 CV
            0x3 | 0x4 => 2,       // Data 64, MIDI 2.0 CV
            0x5 | 0xD => 4,       // Data 128, Flex Data
            _ => 1,               // Unknown/reserved
        }
    }

    /// Parse MIDI 1.0 Channel Voice message (UMP type 0x2).
    fn parse_midi1_voice(&self, word: u32, group: u8) -> Option<MidiMessage> {
        let status = ((word >> 20) & 0xF) as u8;
        let channel = ((word >> 16) & 0xF) as u8;
        let data1 = ((word >> 8) & 0x7F) as u8;
        let data2 = (word & 0x7F) as u8;

        let channel = Channel::new_clamped(channel);
        let _group = Group::new_clamped(group);

        let status = Midi1Status::try_from(status).ok()?;

        Some(match status {
            Midi1Status::NoteOff => MidiMessage::NoteOff {
                channel,
                note: data1,
                velocity: Velocity::from_midi1(data2),
            },
            Midi1Status::NoteOn => {
                if data2 == 0 {
                    // Velocity 0 = Note Off
                    MidiMessage::NoteOff {
                        channel,
                        note: data1,
                        velocity: Velocity::ZERO,
                    }
                } else {
                    MidiMessage::NoteOn {
                        channel,
                        note: data1,
                        velocity: Velocity::from_midi1(data2),
                    }
                }
            }
            Midi1Status::PolyPressure => MidiMessage::PolyAftertouch {
                channel,
                note: data1,
                pressure: data2,
            },
            Midi1Status::ControlChange => MidiMessage::ControlChange {
                channel,
                controller: data1,
                value: data2,
            },
            Midi1Status::ProgramChange => MidiMessage::ProgramChange {
                channel,
                program: data1,
            },
            Midi1Status::ChannelPressure => MidiMessage::ChannelAftertouch {
                channel,
                pressure: data1,
            },
            Midi1Status::PitchBend => {
                let value = ((data2 as i16) << 7 | data1 as i16) - 8192;
                MidiMessage::PitchBend { channel, value }
            }
        })
    }

    /// Parse MIDI 2.0 Channel Voice message (UMP type 0x4).
    fn parse_midi2_voice(&self, word1: u32, word2: u32, group: u8) -> Option<MidiMessage> {
        let status = ((word1 >> 20) & 0xF) as u8;
        let channel = ((word1 >> 16) & 0xF) as u8;
        let index = ((word1 >> 8) & 0xFF) as u8; // Note number or controller index
        let byte4 = (word1 & 0xFF) as u8;        // Attribute type or bank

        let group_channel = GroupChannel::new(group, channel);

        let status = Midi2Status::try_from(status).ok()?;

        Some(match status {
            Midi2Status::NoteOff => {
                let velocity = (word2 >> 16) as u16;
                let attribute_value = (word2 & 0xFFFF) as u16;
                MidiMessage::Midi2NoteOff {
                    group_channel,
                    note: index,
                    velocity: Velocity::from_midi2(velocity),
                    attribute_type: byte4,
                    attribute_value,
                }
            }
            Midi2Status::NoteOn => {
                let velocity = (word2 >> 16) as u16;
                let attribute_value = (word2 & 0xFFFF) as u16;
                MidiMessage::Midi2NoteOn {
                    group_channel,
                    note: index,
                    velocity: Velocity::from_midi2(velocity),
                    attribute_type: byte4,
                    attribute_value,
                }
            }
            Midi2Status::PolyPressure => MidiMessage::Midi2PolyPressure {
                group_channel,
                note: index,
                pressure: ControlValue::from_32bit(word2),
            },
            Midi2Status::ControlChange => MidiMessage::Midi2ControlChange {
                group_channel,
                controller: index,
                value: ControlValue::from_32bit(word2),
            },
            Midi2Status::ProgramChange => {
                // byte4 contains option flags
                let bank_valid = (byte4 & 0x01) != 0;
                let bank_msb = ((word2 >> 8) & 0x7F) as u8;
                let bank_lsb = (word2 & 0x7F) as u8;
                let program = ((word2 >> 24) & 0x7F) as u8;
                MidiMessage::Midi2ProgramChange {
                    group_channel,
                    program,
                    bank_valid,
                    bank_msb,
                    bank_lsb,
                }
            }
            Midi2Status::ChannelPressure => MidiMessage::Midi2ChannelPressure {
                group_channel,
                pressure: ControlValue::from_32bit(word2),
            },
            Midi2Status::PitchBend => MidiMessage::Midi2PitchBend {
                group_channel,
                value: word2,
            },
            Midi2Status::PerNotePitchBend => MidiMessage::Midi2PerNotePitchBend {
                group_channel,
                note: index,
                value: word2,
            },
            Midi2Status::RegisteredPerNoteController => {
                MidiMessage::Midi2RegisteredPerNoteController {
                    group_channel,
                    note: index,
                    index: byte4,
                    value: ControlValue::from_32bit(word2),
                }
            }
            Midi2Status::AssignablePerNoteController => {
                MidiMessage::Midi2AssignablePerNoteController {
                    group_channel,
                    note: index,
                    index: byte4,
                    value: ControlValue::from_32bit(word2),
                }
            }
            Midi2Status::RegisteredController => MidiMessage::Midi2RegisteredController {
                group_channel,
                bank: index,
                index: byte4,
                value: ControlValue::from_32bit(word2),
            },
            Midi2Status::AssignableController => MidiMessage::Midi2AssignableController {
                group_channel,
                bank: index,
                index: byte4,
                value: ControlValue::from_32bit(word2),
            },
            Midi2Status::RelativeRegisteredController => {
                MidiMessage::Midi2RelativeRegisteredController {
                    group_channel,
                    bank: index,
                    index: byte4,
                    delta: word2 as i32,
                }
            }
            Midi2Status::RelativeAssignableController => {
                MidiMessage::Midi2RelativeAssignableController {
                    group_channel,
                    bank: index,
                    index: byte4,
                    delta: word2 as i32,
                }
            }
            Midi2Status::PerNoteManagement => MidiMessage::Midi2PerNoteManagement {
                group_channel,
                note: index,
                detach: (byte4 & 0x01) != 0,
                reset_controllers: (byte4 & 0x02) != 0,
            },
        })
    }

    /// Parse System Realtime message (UMP type 0x1).
    fn parse_system_realtime(&self, word: u32) -> Option<MidiMessage> {
        let status = ((word >> 16) & 0xFF) as u8;

        Some(match status {
            0xF8 => MidiMessage::Clock,
            0xFA => MidiMessage::Start,
            0xFB => MidiMessage::Continue,
            0xFC => MidiMessage::Stop,
            0xFE => MidiMessage::ActiveSensing,
            0xFF => MidiMessage::Reset,
            _ => return None,
        })
    }
}

/// Encode a MidiMessage into UMP format.
///
/// Returns the UMP words needed to represent the message.
pub fn encode_ump(msg: &MidiMessage) -> Vec<u32> {
    match msg {
        // MIDI 1.0 messages -> UMP type 0x2
        MidiMessage::NoteOn {
            channel,
            note,
            velocity,
        } => {
            let word = 0x20000000
                | (0x9 << 20)
                | ((channel.get() as u32) << 16)
                | ((*note as u32) << 8)
                | (velocity.to_midi1() as u32);
            vec![word]
        }
        MidiMessage::NoteOff {
            channel,
            note,
            velocity,
        } => {
            let word = 0x20000000
                | (0x8 << 20)
                | ((channel.get() as u32) << 16)
                | ((*note as u32) << 8)
                | (velocity.to_midi1() as u32);
            vec![word]
        }
        MidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => {
            let word = 0x20000000
                | (0xB << 20)
                | ((channel.get() as u32) << 16)
                | ((*controller as u32) << 8)
                | (*value as u32);
            vec![word]
        }
        MidiMessage::PitchBend { channel, value } => {
            let unsigned = (*value + 8192) as u16;
            let lsb = unsigned & 0x7F;
            let msb = (unsigned >> 7) & 0x7F;
            let word = 0x20000000
                | (0xE << 20)
                | ((channel.get() as u32) << 16)
                | ((lsb as u32) << 8)
                | (msb as u32);
            vec![word]
        }

        // MIDI 2.0 messages -> UMP type 0x4
        MidiMessage::Midi2NoteOn {
            group_channel,
            note,
            velocity,
            attribute_type,
            attribute_value,
        } => {
            let word1 = 0x40000000
                | ((group_channel.group() as u32) << 24)
                | (0x9 << 20)
                | ((group_channel.channel() as u32) << 16)
                | ((*note as u32) << 8)
                | (*attribute_type as u32);
            let word2 = ((velocity.to_midi2() as u32) << 16) | (*attribute_value as u32);
            vec![word1, word2]
        }
        MidiMessage::Midi2NoteOff {
            group_channel,
            note,
            velocity,
            attribute_type,
            attribute_value,
        } => {
            let word1 = 0x40000000
                | ((group_channel.group() as u32) << 24)
                | (0x8 << 20)
                | ((group_channel.channel() as u32) << 16)
                | ((*note as u32) << 8)
                | (*attribute_type as u32);
            let word2 = ((velocity.to_midi2() as u32) << 16) | (*attribute_value as u32);
            vec![word1, word2]
        }
        MidiMessage::Midi2ControlChange {
            group_channel,
            controller,
            value,
        } => {
            let word1 = 0x40000000
                | ((group_channel.group() as u32) << 24)
                | (0xB << 20)
                | ((group_channel.channel() as u32) << 16)
                | ((*controller as u32) << 8);
            let word2 = value.to_32bit();
            vec![word1, word2]
        }
        MidiMessage::Midi2PitchBend {
            group_channel,
            value,
        } => {
            let word1 = 0x40000000
                | ((group_channel.group() as u32) << 24)
                | (0xE << 20)
                | ((group_channel.channel() as u32) << 16);
            vec![word1, *value]
        }
        MidiMessage::Midi2PerNotePitchBend {
            group_channel,
            note,
            value,
        } => {
            let word1 = 0x40000000
                | ((group_channel.group() as u32) << 24)
                | (0x6 << 20)
                | ((group_channel.channel() as u32) << 16)
                | ((*note as u32) << 8);
            vec![word1, *value]
        }

        // System Realtime -> UMP type 0x1
        MidiMessage::Clock => vec![0x10F80000],
        MidiMessage::Start => vec![0x10FA0000],
        MidiMessage::Continue => vec![0x10FB0000],
        MidiMessage::Stop => vec![0x10FC0000],
        MidiMessage::ActiveSensing => vec![0x10FE0000],
        MidiMessage::Reset => vec![0x10FF0000],

        // Other messages not yet supported for encoding
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_midi1_note_on() {
        let parser = UmpParser::new();
        // UMP type 0x2, group 0, Note On channel 0, note 60, velocity 100
        let ump = [0x20903C64u32];
        let msg = parser.parse(&ump).unwrap();

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
    fn test_parse_midi1_note_off_via_zero_velocity() {
        let parser = UmpParser::new();
        // Note On with velocity 0 = Note Off
        let ump = [0x20903C00u32];
        let msg = parser.parse(&ump).unwrap();

        assert!(matches!(msg, MidiMessage::NoteOff { .. }));
    }

    #[test]
    fn test_parse_midi2_note_on() {
        let parser = UmpParser::new();
        // UMP type 0x4, group 2, Note On channel 5, note 60, velocity 0x8000 (50%)
        let word1 = 0x42953C00u32; // Type 4, group 2, status 9 (note on), channel 5, note 60
        let word2 = 0x80000000u32; // Velocity 0x8000, no attribute
        let msg = parser.parse(&[word1, word2]).unwrap();

        match msg {
            MidiMessage::Midi2NoteOn {
                group_channel,
                note,
                velocity,
                ..
            } => {
                assert_eq!(group_channel.group(), 2);
                assert_eq!(group_channel.channel(), 5);
                assert_eq!(note, 60);
                assert!((velocity.as_f32() - 0.5).abs() < 0.01);
            }
            _ => panic!("Expected Midi2NoteOn"),
        }
    }

    #[test]
    fn test_parse_midi2_per_note_pitch_bend() {
        let parser = UmpParser::new();
        // UMP type 0x4, status 6 (per-note PB), note 60, value = center + 0.5
        let word1 = 0x40603C00u32; // Type 4, group 0, status 6, channel 0, note 60
        let word2 = 0xC0000000u32; // +0.5 of range
        let msg = parser.parse(&[word1, word2]).unwrap();

        match msg {
            MidiMessage::Midi2PerNotePitchBend {
                note,
                value,
                group_channel,
            } => {
                assert_eq!(note, 60);
                assert_eq!(group_channel.group(), 0);
                let semitones = GroupChannel::pitch_bend_to_semitones(value, 48);
                assert!((semitones - 24.0).abs() < 0.5);
            }
            _ => panic!("Expected Midi2PerNotePitchBend"),
        }
    }

    #[test]
    fn test_parse_system_realtime() {
        let parser = UmpParser::new();

        assert!(matches!(
            parser.parse(&[0x10F80000]).unwrap(),
            MidiMessage::Clock
        ));
        assert!(matches!(
            parser.parse(&[0x10FA0000]).unwrap(),
            MidiMessage::Start
        ));
        assert!(matches!(
            parser.parse(&[0x10FC0000]).unwrap(),
            MidiMessage::Stop
        ));
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let parser = UmpParser::new();

        let original = MidiMessage::Midi2NoteOn {
            group_channel: GroupChannel::new(3, 7),
            note: 72,
            velocity: Velocity::from_midi2(0xAAAA),
            attribute_type: 0,
            attribute_value: 0,
        };

        let encoded = encode_ump(&original);
        assert_eq!(encoded.len(), 2);

        let decoded = parser.parse(&encoded).unwrap();
        match decoded {
            MidiMessage::Midi2NoteOn {
                group_channel,
                note,
                velocity,
                ..
            } => {
                assert_eq!(group_channel.group(), 3);
                assert_eq!(group_channel.channel(), 7);
                assert_eq!(note, 72);
                assert_eq!(velocity.to_midi2(), 0xAAAA);
            }
            _ => panic!("Roundtrip failed"),
        }
    }

    #[test]
    fn test_packet_size() {
        // MIDI 1.0 CV = 1 word
        assert_eq!(UmpParser::packet_size(0x20000000), 1);
        // MIDI 2.0 CV = 2 words
        assert_eq!(UmpParser::packet_size(0x40000000), 2);
        // System = 1 word
        assert_eq!(UmpParser::packet_size(0x10000000), 1);
    }
}
