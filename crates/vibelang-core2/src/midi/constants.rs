//! MIDI constants and packed data decoding.
//!
//! This module re-exports trigger IDs from vibelang-dsp and provides
//! utilities for decoding packed MIDI data from SuperCollider SendTrig messages.

// Re-export trigger IDs and types from vibelang-dsp
pub use vibelang_dsp::system_synthdefs::trigger_ids;
pub use vibelang_dsp::system_synthdefs::MidiTriggerType;

/// Decoded MIDI data from a packed SendTrig value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiData {
    /// Note on: device, channel, note, velocity
    NoteOn {
        device: u8,
        channel: u8,
        note: u8,
        velocity: u8,
    },
    /// Note off: device, channel, note
    NoteOff { device: u8, channel: u8, note: u8 },
    /// Control change: device, channel, cc number, value
    ControlChange {
        device: u8,
        channel: u8,
        cc: u8,
        value: u8,
    },
    /// Pitch bend: device, channel, value (-8192 to +8191)
    PitchBend {
        device: u8,
        channel: u8,
        value: i16,
    },
    /// Clock pulse: device
    Clock { device: u8 },
    /// Transport start: device
    Start { device: u8 },
    /// Transport stop: device
    Stop { device: u8 },
    /// Transport continue: device
    Continue { device: u8 },
}

/// Decode packed MIDI data from a SendTrig value.
///
/// # Arguments
///
/// * `trigger_id` - The trigger ID from the /tr message
/// * `value` - The packed value from the /tr message
///
/// # Returns
///
/// `Some(MidiData)` if the trigger ID is a known MIDI type, `None` otherwise.
pub fn decode_packed_midi(trigger_id: i32, value: f32) -> Option<MidiData> {
    let msg_type = trigger_ids::message_type(trigger_id)?;

    Some(match msg_type {
        MidiTriggerType::NoteOn => {
            // Format: (device << 18) | (channel << 14) | (note << 7) | velocity
            let packed = value as u32;
            MidiData::NoteOn {
                device: ((packed >> 18) & 0x3F) as u8,
                channel: ((packed >> 14) & 0xF) as u8,
                note: ((packed >> 7) & 0x7F) as u8,
                velocity: (packed & 0x7F) as u8,
            }
        }
        MidiTriggerType::NoteOff => {
            // Format: (device << 14) | (channel << 7) | note
            let packed = value as u32;
            MidiData::NoteOff {
                device: ((packed >> 14) & 0x3FFF) as u8,
                channel: ((packed >> 7) & 0x7F) as u8,
                note: (packed & 0x7F) as u8,
            }
        }
        MidiTriggerType::CC => {
            // Format: (device << 18) | (channel << 14) | (cc_num << 7) | value
            let packed = value as u32;
            MidiData::ControlChange {
                device: ((packed >> 18) & 0x3F) as u8,
                channel: ((packed >> 14) & 0xF) as u8,
                cc: ((packed >> 7) & 0x7F) as u8,
                value: (packed & 0x7F) as u8,
            }
        }
        MidiTriggerType::PitchBend => {
            // Format: (device << 18) | (channel << 14) | value
            let packed = value as u32;
            let pb_value = (packed & 0x3FFF) as u16;
            // Convert from 0-16383 to -8192..+8191
            let pb_centered = (pb_value as i16) - 8192;
            MidiData::PitchBend {
                device: ((packed >> 18) & 0x3F) as u8,
                channel: ((packed >> 14) & 0xF) as u8,
                value: pb_centered,
            }
        }
        MidiTriggerType::Clock => MidiData::Clock {
            device: value as u8,
        },
        MidiTriggerType::Start => MidiData::Start {
            device: value as u8,
        },
        MidiTriggerType::Stop => MidiData::Stop {
            device: value as u8,
        },
        MidiTriggerType::Continue => MidiData::Continue {
            device: value as u8,
        },
    })
}

/// Pack MIDI note-on data into a SendTrig value.
///
/// # Arguments
///
/// * `device` - MIDI device ID (0-63)
/// * `channel` - MIDI channel (0-15)
/// * `note` - MIDI note number (0-127)
/// * `velocity` - Note velocity (0-127)
///
/// # Returns
///
/// Packed f32 value for use with `vibelang_midi_note_on` synthdef.
pub fn pack_note_on(device: u8, channel: u8, note: u8, velocity: u8) -> f32 {
    let packed = ((device as u32 & 0x3F) << 18)
        | ((channel as u32 & 0xF) << 14)
        | ((note as u32 & 0x7F) << 7)
        | (velocity as u32 & 0x7F);
    packed as f32
}

/// Pack MIDI note-off data into a SendTrig value.
///
/// # Arguments
///
/// * `device` - MIDI device ID (0-16383)
/// * `channel` - MIDI channel (0-127)
/// * `note` - MIDI note number (0-127)
///
/// # Returns
///
/// Packed f32 value for use with `vibelang_midi_note_off` synthdef.
pub fn pack_note_off(device: u8, channel: u8, note: u8) -> f32 {
    let packed =
        ((device as u32) << 14) | ((channel as u32 & 0x7F) << 7) | (note as u32 & 0x7F);
    packed as f32
}

/// Pack MIDI CC data into a SendTrig value.
///
/// # Arguments
///
/// * `device` - MIDI device ID (0-63)
/// * `channel` - MIDI channel (0-15)
/// * `cc` - CC number (0-127)
/// * `value` - CC value (0-127)
///
/// # Returns
///
/// Packed f32 value for use with `vibelang_midi_cc` synthdef.
pub fn pack_cc(device: u8, channel: u8, cc: u8, value: u8) -> f32 {
    let packed = ((device as u32 & 0x3F) << 18)
        | ((channel as u32 & 0xF) << 14)
        | ((cc as u32 & 0x7F) << 7)
        | (value as u32 & 0x7F);
    packed as f32
}

/// Pack MIDI pitch bend data into a SendTrig value.
///
/// # Arguments
///
/// * `device` - MIDI device ID (0-63)
/// * `channel` - MIDI channel (0-15)
/// * `value` - Pitch bend value (-8192 to +8191)
///
/// # Returns
///
/// Packed f32 value for use with `vibelang_midi_pitch_bend` synthdef.
pub fn pack_pitch_bend(device: u8, channel: u8, value: i16) -> f32 {
    // Convert from -8192..+8191 to 0..16383
    let pb_value = (value + 8192) as u32 & 0x3FFF;
    let packed = ((device as u32 & 0x3F) << 18) | ((channel as u32 & 0xF) << 14) | pb_value;
    packed as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_on_pack_decode() {
        let device = 1u8;
        let channel = 5u8;
        let note = 60u8;
        let velocity = 100u8;

        let packed = pack_note_on(device, channel, note, velocity);
        let decoded = decode_packed_midi(trigger_ids::NOTE_ON, packed).unwrap();

        match decoded {
            MidiData::NoteOn {
                device: d,
                channel: c,
                note: n,
                velocity: v,
            } => {
                assert_eq!(d, device);
                assert_eq!(c, channel);
                assert_eq!(n, note);
                assert_eq!(v, velocity);
            }
            _ => panic!("Expected NoteOn"),
        }
    }

    #[test]
    fn test_note_off_pack_decode() {
        let device = 2u8;
        let channel = 3u8;
        let note = 72u8;

        let packed = pack_note_off(device, channel, note);
        let decoded = decode_packed_midi(trigger_ids::NOTE_OFF, packed).unwrap();

        match decoded {
            MidiData::NoteOff {
                device: d,
                channel: c,
                note: n,
            } => {
                assert_eq!(d, device);
                assert_eq!(c, channel);
                assert_eq!(n, note);
            }
            _ => panic!("Expected NoteOff"),
        }
    }

    #[test]
    fn test_cc_pack_decode() {
        let device = 0u8;
        let channel = 1u8;
        let cc = 74u8;
        let value = 64u8;

        let packed = pack_cc(device, channel, cc, value);
        let decoded = decode_packed_midi(trigger_ids::CC, packed).unwrap();

        match decoded {
            MidiData::ControlChange {
                device: d,
                channel: c,
                cc: cc_num,
                value: v,
            } => {
                assert_eq!(d, device);
                assert_eq!(c, channel);
                assert_eq!(cc_num, cc);
                assert_eq!(v, value);
            }
            _ => panic!("Expected ControlChange"),
        }
    }

    #[test]
    fn test_pitch_bend_pack_decode() {
        let device = 1u8;
        let channel = 0u8;

        // Test center (0)
        let packed = pack_pitch_bend(device, channel, 0);
        let decoded = decode_packed_midi(trigger_ids::PITCH_BEND, packed).unwrap();
        match decoded {
            MidiData::PitchBend {
                device: d,
                channel: c,
                value: v,
            } => {
                assert_eq!(d, device);
                assert_eq!(c, channel);
                assert_eq!(v, 0);
            }
            _ => panic!("Expected PitchBend"),
        }

        // Test positive
        let packed = pack_pitch_bend(device, channel, 4096);
        let decoded = decode_packed_midi(trigger_ids::PITCH_BEND, packed).unwrap();
        match decoded {
            MidiData::PitchBend { value: v, .. } => {
                assert_eq!(v, 4096);
            }
            _ => panic!("Expected PitchBend"),
        }

        // Test negative
        let packed = pack_pitch_bend(device, channel, -4096);
        let decoded = decode_packed_midi(trigger_ids::PITCH_BEND, packed).unwrap();
        match decoded {
            MidiData::PitchBend { value: v, .. } => {
                assert_eq!(v, -4096);
            }
            _ => panic!("Expected PitchBend"),
        }
    }

    #[test]
    fn test_transport_decode() {
        assert!(matches!(
            decode_packed_midi(trigger_ids::CLOCK, 1.0),
            Some(MidiData::Clock { device: 1 })
        ));
        assert!(matches!(
            decode_packed_midi(trigger_ids::START, 2.0),
            Some(MidiData::Start { device: 2 })
        ));
        assert!(matches!(
            decode_packed_midi(trigger_ids::STOP, 3.0),
            Some(MidiData::Stop { device: 3 })
        ));
        assert!(matches!(
            decode_packed_midi(trigger_ids::CONTINUE, 0.0),
            Some(MidiData::Continue { device: 0 })
        ));
    }

    #[test]
    fn test_unknown_trigger_id() {
        assert!(decode_packed_midi(999, 0.0).is_none());
        assert!(decode_packed_midi(50, 0.0).is_none());
    }
}
