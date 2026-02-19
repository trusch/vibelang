//! MIDI trigger synthdefs for SuperCollider-managed MIDI output.
//!
//! These synthdefs use SendReply to trigger MIDI events at sample-accurate times.
//! When scsynth creates these synths, they immediately fire a SendReply message
//! and free themselves. The Rust side listens for these OSC messages and sends
//! the actual MIDI bytes to the device.
//!
//! This approach ensures perfect synchronization between MIDI and audio events,
//! as both are scheduled through the same OSC bundle timing mechanism.

use crate::{encode_synthdef, GraphBuilderInner, GraphIR, Input, Rate};

/// SendReply command IDs for MIDI messages.
/// These are used as the cmdName in SendReply to distinguish message types.
pub const MIDI_REPLY_NOTE_ON: &str = "/midi/note_on";
pub const MIDI_REPLY_NOTE_OFF: &str = "/midi/note_off";
pub const MIDI_REPLY_CC: &str = "/midi/cc";
pub const MIDI_REPLY_PITCH_BEND: &str = "/midi/pitch_bend";
pub const MIDI_REPLY_CLOCK: &str = "/midi/clock";
pub const MIDI_REPLY_START: &str = "/midi/start";
pub const MIDI_REPLY_STOP: &str = "/midi/stop";
pub const MIDI_REPLY_CONTINUE: &str = "/midi/continue";

/// Create and encode all MIDI trigger synthdefs.
/// Returns a vector of (name, encoded_bytes) pairs.
pub fn create_midi_synthdefs() -> Vec<(String, Vec<u8>)> {
    let mut defs = Vec::new();

    // Note messages
    if let Some((name, bytes)) = generate_midi_note_on_synthdef() {
        defs.push((name, bytes));
    }
    if let Some((name, bytes)) = generate_midi_note_off_synthdef() {
        defs.push((name, bytes));
    }

    // Control messages
    if let Some((name, bytes)) = generate_midi_cc_synthdef() {
        defs.push((name, bytes));
    }
    if let Some((name, bytes)) = generate_midi_pitch_bend_synthdef() {
        defs.push((name, bytes));
    }

    // Clock (persistent synth)
    if let Some((name, bytes)) = generate_midi_clock_synthdef() {
        defs.push((name, bytes));
    }

    // Transport messages
    if let Some((name, bytes)) = generate_midi_start_synthdef() {
        defs.push((name, bytes));
    }
    if let Some((name, bytes)) = generate_midi_stop_synthdef() {
        defs.push((name, bytes));
    }
    if let Some((name, bytes)) = generate_midi_continue_synthdef() {
        defs.push((name, bytes));
    }

    defs
}

/// Generate the vibelang_midi_note_on synthdef.
///
/// This synthdef fires a SendReply with note-on data and immediately frees itself.
///
/// Parameters:
/// - device_id: MIDI output device ID (0)
/// - channel: MIDI channel 0-15 (1)
/// - note: MIDI note number 0-127 (2)
/// - velocity: Note velocity 0-127 (3)
///
/// SendReply format: [device_id, channel, note, velocity]
fn generate_midi_note_on_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_note_on";
    let mut builder = GraphBuilderInner::new();

    // Parameters - we'll pack these into a single value on the Rust side
    // The synthdef receives a single "packed" value that contains all MIDI data
    builder.add_param("packed_data".to_string(), vec![0.0], None); // 0: (device << 18) | (ch << 14) | (note << 7) | vel

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    // Constants
    let zero = 0.0f32;

    builder.add_constant(zero);
    // Add trigger ID constant - single trigger with packed value
    builder.add_constant(100.0f32); // packed data trigger

    // Impulse.kr(0) - fires once at synth creation
    let impulse = builder.add_node(
        "Impulse".to_string(),
        Rate::Control,
        vec![Input::Constant(zero)], // freq=0 means fire once
        1,
        0,
    );

    // Single SendTrig with packed data (ID 100)
    // The packed value is pre-computed on the Rust side: (device << 18) | (ch << 14) | (note << 7) | vel
    builder.add_node(
        "SendTrig".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: impulse.0,
                output_index: 0,
            },
            Input::Constant(100.0), // ID for note_on packed data
            param(0),               // packed_data value
        ],
        0,
        0,
    );

    // FreeSelf.kr(trig) - free the synth after trigger fires
    builder.add_node(
        "FreeSelf".to_string(),
        Rate::Control,
        vec![Input::Node {
            node_id: impulse.0,
            output_index: 0,
        }],
        0,
        0,
    );

    // Need some audio output even if silent (required by some SC configs)
    // DC.ar(0) outputs silence
    let dc = builder.add_node(
        "DC".to_string(),
        Rate::Audio,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    // Out.ar(0, DC.ar(0)) - silent output
    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            Input::Constant(zero),
            Input::Node {
                node_id: dc.0,
                output_index: 0,
            },
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[MIDI_SYNTHDEF] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[MIDI_SYNTHDEF] Failed to encode '{}': {}", name, e);
            None
        }
    }
}

/// Generate the vibelang_midi_note_off synthdef.
///
/// Parameters:
/// - packed_data: Packed MIDI data (device << 14) | (ch << 7) | note
///
/// SendTrig IDs: 110 = packed note_off data
fn generate_midi_note_off_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_note_off";
    let mut builder = GraphBuilderInner::new();

    // Single packed parameter: (device << 14) | (ch << 7) | note
    builder.add_param("packed_data".to_string(), vec![0.0], None);

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    let zero = 0.0f32;
    builder.add_constant(zero);
    // Add trigger ID constant - single trigger with packed value
    builder.add_constant(110.0f32); // packed data trigger

    let impulse = builder.add_node(
        "Impulse".to_string(),
        Rate::Control,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    // Single SendTrig with packed data (ID 110)
    // The packed value is pre-computed on the Rust side: (device << 14) | (ch << 7) | note
    builder.add_node(
        "SendTrig".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: impulse.0,
                output_index: 0,
            },
            Input::Constant(110.0), // ID for note_off packed data
            param(0),               // packed_data value
        ],
        0,
        0,
    );

    builder.add_node(
        "FreeSelf".to_string(),
        Rate::Control,
        vec![Input::Node {
            node_id: impulse.0,
            output_index: 0,
        }],
        0,
        0,
    );

    let dc = builder.add_node(
        "DC".to_string(),
        Rate::Audio,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            Input::Constant(zero),
            Input::Node {
                node_id: dc.0,
                output_index: 0,
            },
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[MIDI_SYNTHDEF] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[MIDI_SYNTHDEF] Failed to encode '{}': {}", name, e);
            None
        }
    }
}

/// Generate the vibelang_midi_cc synthdef.
///
/// Parameters:
/// - packed_data: Packed MIDI data (device << 18) | (ch << 14) | (cc_num << 7) | value
///
/// SendTrig ID: 120 = packed CC data
fn generate_midi_cc_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_cc";
    let mut builder = GraphBuilderInner::new();

    // Single packed parameter: (device << 18) | (ch << 14) | (cc_num << 7) | value
    builder.add_param("packed_data".to_string(), vec![0.0], None);

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    let zero = 0.0f32;
    builder.add_constant(zero);
    builder.add_constant(120.0f32); // CC trigger ID constant

    let impulse = builder.add_node(
        "Impulse".to_string(),
        Rate::Control,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    // Single SendTrig with packed data (ID 120)
    builder.add_node(
        "SendTrig".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: impulse.0,
                output_index: 0,
            },
            Input::Constant(120.0), // ID for CC packed data
            param(0),               // packed_data value
        ],
        0,
        0,
    );

    builder.add_node(
        "FreeSelf".to_string(),
        Rate::Control,
        vec![Input::Node {
            node_id: impulse.0,
            output_index: 0,
        }],
        0,
        0,
    );

    let dc = builder.add_node(
        "DC".to_string(),
        Rate::Audio,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            Input::Constant(zero),
            Input::Node {
                node_id: dc.0,
                output_index: 0,
            },
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[MIDI_SYNTHDEF] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[MIDI_SYNTHDEF] Failed to encode '{}': {}", name, e);
            None
        }
    }
}

/// Generate the vibelang_midi_pitch_bend synthdef.
///
/// Parameters:
/// - packed_data: Packed MIDI data (device << 18) | (ch << 14) | value
///
/// SendTrig ID: 130 = packed pitch bend data
fn generate_midi_pitch_bend_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_pitch_bend";
    let mut builder = GraphBuilderInner::new();

    // Single packed parameter: (device << 18) | (ch << 14) | value
    builder.add_param("packed_data".to_string(), vec![0.0], None);

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    let zero = 0.0f32;
    builder.add_constant(zero);
    builder.add_constant(130.0f32); // Pitch bend trigger ID constant

    let impulse = builder.add_node(
        "Impulse".to_string(),
        Rate::Control,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    // Single SendTrig with packed data (ID 130)
    builder.add_node(
        "SendTrig".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: impulse.0,
                output_index: 0,
            },
            Input::Constant(130.0), // ID for pitch bend packed data
            param(0),               // packed_data value
        ],
        0,
        0,
    );

    builder.add_node(
        "FreeSelf".to_string(),
        Rate::Control,
        vec![Input::Node {
            node_id: impulse.0,
            output_index: 0,
        }],
        0,
        0,
    );

    let dc = builder.add_node(
        "DC".to_string(),
        Rate::Audio,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            Input::Constant(zero),
            Input::Node {
                node_id: dc.0,
                output_index: 0,
            },
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[MIDI_SYNTHDEF] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[MIDI_SYNTHDEF] Failed to encode '{}': {}", name, e);
            None
        }
    }
}

/// Generate the vibelang_midi_clock synthdef.
///
/// This is a PERSISTENT synth that continuously sends MIDI clock pulses.
/// It does NOT free itself - use n_free to stop it.
///
/// Parameters:
/// - device_id: MIDI output device ID (0)
/// - freq: Clock frequency in Hz = BPM/60*24 for 24 PPQN (1)
///
/// SendTrig ID: 140 = clock pulse (value = device_id)
fn generate_midi_clock_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_clock";
    let mut builder = GraphBuilderInner::new();

    builder.add_param("device_id".to_string(), vec![0.0], None);
    builder.add_param("freq".to_string(), vec![48.0], None); // 120 BPM * 24 / 60 = 48 Hz

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    let zero = 0.0f32;
    builder.add_constant(zero);
    // Add trigger ID constant
    builder.add_constant(140.0f32); // clock trigger

    // Impulse.kr(freq) - fires at clock rate
    let impulse = builder.add_node(
        "Impulse".to_string(),
        Rate::Control,
        vec![param(1)], // freq parameter
        1,
        0,
    );

    // SendTrig for clock (ID 140, value = device_id)
    builder.add_node(
        "SendTrig".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: impulse.0,
                output_index: 0,
            },
            Input::Constant(140.0),
            param(0), // device_id
        ],
        0,
        0,
    );

    // NO FreeSelf - this synth runs continuously

    let dc = builder.add_node(
        "DC".to_string(),
        Rate::Audio,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            Input::Constant(zero),
            Input::Node {
                node_id: dc.0,
                output_index: 0,
            },
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[MIDI_SYNTHDEF] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[MIDI_SYNTHDEF] Failed to encode '{}': {}", name, e);
            None
        }
    }
}

/// Generate the vibelang_midi_start synthdef.
///
/// Parameters:
/// - device_id: MIDI output device ID (0)
///
/// SendTrig ID: 150 = start (value = device_id)
fn generate_midi_start_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_start";
    let mut builder = GraphBuilderInner::new();

    builder.add_param("device_id".to_string(), vec![0.0], None);

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    let zero = 0.0f32;
    builder.add_constant(zero);
    // Add trigger ID constant
    builder.add_constant(150.0f32); // start trigger

    let impulse = builder.add_node(
        "Impulse".to_string(),
        Rate::Control,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "SendTrig".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: impulse.0,
                output_index: 0,
            },
            Input::Constant(150.0),
            param(0),
        ],
        0,
        0,
    );

    builder.add_node(
        "FreeSelf".to_string(),
        Rate::Control,
        vec![Input::Node {
            node_id: impulse.0,
            output_index: 0,
        }],
        0,
        0,
    );

    let dc = builder.add_node(
        "DC".to_string(),
        Rate::Audio,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            Input::Constant(zero),
            Input::Node {
                node_id: dc.0,
                output_index: 0,
            },
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[MIDI_SYNTHDEF] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[MIDI_SYNTHDEF] Failed to encode '{}': {}", name, e);
            None
        }
    }
}

/// Generate the vibelang_midi_stop synthdef.
///
/// Parameters:
/// - device_id: MIDI output device ID (0)
///
/// SendTrig ID: 151 = stop (value = device_id)
fn generate_midi_stop_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_stop";
    let mut builder = GraphBuilderInner::new();

    builder.add_param("device_id".to_string(), vec![0.0], None);

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    let zero = 0.0f32;
    builder.add_constant(zero);
    // Add trigger ID constant
    builder.add_constant(151.0f32); // stop trigger

    let impulse = builder.add_node(
        "Impulse".to_string(),
        Rate::Control,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "SendTrig".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: impulse.0,
                output_index: 0,
            },
            Input::Constant(151.0),
            param(0),
        ],
        0,
        0,
    );

    builder.add_node(
        "FreeSelf".to_string(),
        Rate::Control,
        vec![Input::Node {
            node_id: impulse.0,
            output_index: 0,
        }],
        0,
        0,
    );

    let dc = builder.add_node(
        "DC".to_string(),
        Rate::Audio,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            Input::Constant(zero),
            Input::Node {
                node_id: dc.0,
                output_index: 0,
            },
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[MIDI_SYNTHDEF] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[MIDI_SYNTHDEF] Failed to encode '{}': {}", name, e);
            None
        }
    }
}

/// Generate the vibelang_midi_continue synthdef.
///
/// Parameters:
/// - device_id: MIDI output device ID (0)
///
/// SendTrig ID: 152 = continue (value = device_id)
fn generate_midi_continue_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_continue";
    let mut builder = GraphBuilderInner::new();

    builder.add_param("device_id".to_string(), vec![0.0], None);

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    let zero = 0.0f32;
    builder.add_constant(zero);
    // Add trigger ID constant
    builder.add_constant(152.0f32); // continue trigger

    let impulse = builder.add_node(
        "Impulse".to_string(),
        Rate::Control,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "SendTrig".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: impulse.0,
                output_index: 0,
            },
            Input::Constant(152.0),
            param(0),
        ],
        0,
        0,
    );

    builder.add_node(
        "FreeSelf".to_string(),
        Rate::Control,
        vec![Input::Node {
            node_id: impulse.0,
            output_index: 0,
        }],
        0,
        0,
    );

    let dc = builder.add_node(
        "DC".to_string(),
        Rate::Audio,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            Input::Constant(zero),
            Input::Node {
                node_id: dc.0,
                output_index: 0,
            },
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[MIDI_SYNTHDEF] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[MIDI_SYNTHDEF] Failed to encode '{}': {}", name, e);
            None
        }
    }
}

/// SendTrig IDs for MIDI message types.
///
/// All MIDI messages use single triggers with packed values.
/// Data is packed into f32 values (24-bit precision).
pub mod trigger_ids {
    // Note On: ID 100
    // Format: (device << 18) | (channel << 14) | (note << 7) | velocity
    pub const NOTE_ON: i32 = 100;

    // Note Off: ID 110
    // Format: (device << 14) | (channel << 7) | note
    pub const NOTE_OFF: i32 = 110;

    // CC: ID 120
    // Format: (device << 18) | (channel << 14) | (cc_num << 7) | value
    pub const CC: i32 = 120;

    // Pitch Bend: ID 130
    // Format: (device << 18) | (channel << 14) | value
    pub const PITCH_BEND: i32 = 130;

    // Clock: ID 140 (value = device_id)
    pub const CLOCK: i32 = 140;

    // Transport: IDs 150-152 (value = device_id)
    pub const START: i32 = 150;
    pub const STOP: i32 = 151;
    pub const CONTINUE: i32 = 152;

    /// Get clock trigger ID for a specific device.
    ///
    /// Each device gets its own trigger ID in the range CLOCK..(CLOCK+256).
    pub fn clock_id(device: u8) -> i32 {
        CLOCK + device as i32
    }

    /// Get the message type from a trigger ID.
    pub fn message_type(id: i32) -> Option<super::MidiTriggerType> {
        use super::MidiTriggerType;
        match id {
            NOTE_ON => Some(MidiTriggerType::NoteOn),
            NOTE_OFF => Some(MidiTriggerType::NoteOff),
            CC => Some(MidiTriggerType::CC),
            PITCH_BEND => Some(MidiTriggerType::PitchBend),
            // Transport messages must be checked before clock range
            START => Some(MidiTriggerType::Start),
            STOP => Some(MidiTriggerType::Stop),
            CONTINUE => Some(MidiTriggerType::Continue),
            // Clock range: CLOCK (140) to START-1 (149) - up to 10 devices
            // The synthdef uses fixed ID 140 with device_id as the value
            CLOCK..=149 => Some(MidiTriggerType::Clock),
            _ => None,
        }
    }
}

/// Types of MIDI trigger messages.
///
/// All messages use packed format with a single SendTrig.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiTriggerType {
    /// Note on: (device << 18) | (channel << 14) | (note << 7) | velocity
    NoteOn,
    /// Note off: (device << 14) | (channel << 7) | note
    NoteOff,
    /// CC: (device << 18) | (channel << 14) | (cc_num << 7) | value
    CC,
    /// Pitch bend: (device << 18) | (channel << 14) | value
    PitchBend,
    /// Clock pulse (value = device_id)
    Clock,
    /// Transport start (value = device_id)
    Start,
    /// Transport stop (value = device_id)
    Stop,
    /// Transport continue (value = device_id)
    Continue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_midi_synthdefs() {
        let defs = create_midi_synthdefs();

        let names: Vec<&str> = defs.iter().map(|(n, _)| n.as_str()).collect();
        eprintln!("Created synthdefs ({}):", names.len());
        for name in &names {
            eprintln!("  - {}", name);
        }

        // Check all expected synthdefs exist
        let expected = [
            "vibelang_midi_note_on",
            "vibelang_midi_note_off",
            "vibelang_midi_cc",
            "vibelang_midi_pitch_bend",
            "vibelang_midi_clock",
            "vibelang_midi_start",
            "vibelang_midi_stop",
            "vibelang_midi_continue",
        ];

        for expected_name in expected {
            assert!(
                names.contains(&expected_name),
                "Missing synthdef: {}",
                expected_name
            );
        }

        assert_eq!(defs.len(), 8, "Should create 8 MIDI synthdefs");
    }

    #[test]
    fn test_trigger_id_exact_matches() {
        use trigger_ids::*;

        // Each trigger ID is a specific value, not a range
        assert_eq!(message_type(NOTE_ON), Some(MidiTriggerType::NoteOn)); // 100
        assert_eq!(message_type(NOTE_OFF), Some(MidiTriggerType::NoteOff)); // 110
        assert_eq!(message_type(CC), Some(MidiTriggerType::CC)); // 120
        assert_eq!(message_type(PITCH_BEND), Some(MidiTriggerType::PitchBend)); // 130
        assert_eq!(message_type(CLOCK), Some(MidiTriggerType::Clock)); // 140
        assert_eq!(message_type(START), Some(MidiTriggerType::Start)); // 150
        assert_eq!(message_type(STOP), Some(MidiTriggerType::Stop)); // 151
        assert_eq!(message_type(CONTINUE), Some(MidiTriggerType::Continue)); // 152

        // Unknown IDs should return None
        assert_eq!(message_type(99), None);
        assert_eq!(message_type(101), None);
        assert_eq!(message_type(200), None);
    }
}
