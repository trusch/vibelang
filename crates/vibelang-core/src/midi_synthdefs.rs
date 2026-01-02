//! MIDI trigger synthdefs for SuperCollider-managed MIDI output.
//!
//! These synthdefs use SendReply to trigger MIDI events at sample-accurate times.
//! When scsynth creates these synths, they immediately fire a SendReply message
//! and free themselves. The Rust side listens for these OSC messages and sends
//! the actual MIDI bytes to the device.
//!
//! This approach ensures perfect synchronization between MIDI and audio events,
//! as both are scheduled through the same OSC bundle timing mechanism.

use vibelang_dsp::{encode_synthdef, GraphBuilderInner, GraphIR, Input, Rate};

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
    builder.add_param("packed_data".to_string(), vec![0.0], None); // 0: (device << 21) | (ch << 14) | (note << 7) | vel

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
    // The packed value is pre-computed on the Rust side: (device << 21) | (ch << 14) | (note << 7) | vel
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
/// - device_id: MIDI output device ID (0)
/// - channel: MIDI channel 0-15 (1)
/// - cc_num: CC number 0-127 (2)
/// - value: CC value 0-127 (3)
///
/// SendTrig IDs: 120 = device_id, 121 = channel, 122 = cc_num, 123 = value
fn generate_midi_cc_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_cc";
    let mut builder = GraphBuilderInner::new();

    builder.add_param("device_id".to_string(), vec![0.0], None);
    builder.add_param("channel".to_string(), vec![0.0], None);
    builder.add_param("cc_num".to_string(), vec![1.0], None);
    builder.add_param("value".to_string(), vec![0.0], None);

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    let zero = 0.0f32;
    builder.add_constant(zero);
    // Add trigger ID constants
    builder.add_constant(120.0f32); // device_id trigger
    builder.add_constant(121.0f32); // channel trigger
    builder.add_constant(122.0f32); // cc_num trigger
    builder.add_constant(123.0f32); // value trigger

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
            Input::Constant(120.0),
            param(0),
        ],
        0,
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
            Input::Constant(121.0),
            param(1),
        ],
        0,
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
            Input::Constant(122.0),
            param(2),
        ],
        0,
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
            Input::Constant(123.0),
            param(3),
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
/// - device_id: MIDI output device ID (0)
/// - channel: MIDI channel 0-15 (1)
/// - value: 14-bit pitch bend value 0-16383, center=8192 (2)
///
/// SendTrig IDs: 130 = device_id, 131 = channel, 132 = value
fn generate_midi_pitch_bend_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "vibelang_midi_pitch_bend";
    let mut builder = GraphBuilderInner::new();

    builder.add_param("device_id".to_string(), vec![0.0], None);
    builder.add_param("channel".to_string(), vec![0.0], None);
    builder.add_param("value".to_string(), vec![8192.0], None); // Center

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    let zero = 0.0f32;
    builder.add_constant(zero);
    // Add trigger ID constants
    builder.add_constant(130.0f32); // device_id trigger
    builder.add_constant(131.0f32); // channel trigger
    builder.add_constant(132.0f32); // value trigger

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
            Input::Constant(130.0),
            param(0),
        ],
        0,
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
            Input::Constant(131.0),
            param(1),
        ],
        0,
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
            Input::Constant(132.0),
            param(2),
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

/// SendTrig ID ranges for MIDI message types.
///
/// These constants define the trigger ID scheme used to identify MIDI messages
/// when scsynth sends /tr OSC messages.
///
/// The new approach uses single triggers with packed values to avoid accumulator issues.
pub mod trigger_ids {
    // Note On: ID 100 - single trigger with packed data
    // Format: (device << 21) | (channel << 14) | (note << 7) | velocity
    pub const NOTE_ON_PACKED: i32 = 100;

    // Note Off: ID 110 - single trigger with packed data
    // Format: (device << 14) | (channel << 7) | note
    pub const NOTE_OFF_PACKED: i32 = 110;

    // Legacy constants for backwards compatibility (unused)
    pub const NOTE_ON_DEVICE_ID: i32 = 100;
    pub const NOTE_ON_CHANNEL: i32 = 101;
    pub const NOTE_ON_NOTE: i32 = 102;
    pub const NOTE_ON_VELOCITY: i32 = 103;
    pub const NOTE_OFF_DEVICE_ID: i32 = 110;
    pub const NOTE_OFF_CHANNEL: i32 = 111;
    pub const NOTE_OFF_NOTE: i32 = 112;

    // CC: IDs 120-123
    pub const CC_DEVICE_ID: i32 = 120;
    pub const CC_CHANNEL: i32 = 121;
    pub const CC_NUM: i32 = 122;
    pub const CC_VALUE: i32 = 123;

    // Pitch Bend: IDs 130-132
    pub const PITCH_BEND_DEVICE_ID: i32 = 130;
    pub const PITCH_BEND_CHANNEL: i32 = 131;
    pub const PITCH_BEND_VALUE: i32 = 132;

    // Clock: ID 140
    pub const CLOCK: i32 = 140;

    // Transport: IDs 150-152
    pub const START: i32 = 150;
    pub const STOP: i32 = 151;
    pub const CONTINUE: i32 = 152;

    /// Get the message type from a trigger ID.
    pub fn message_type(id: i32) -> Option<super::MidiTriggerType> {
        use super::MidiTriggerType;
        match id {
            // Single packed triggers
            NOTE_ON_PACKED => Some(MidiTriggerType::NoteOnPacked),
            NOTE_OFF_PACKED => Some(MidiTriggerType::NoteOffPacked),
            // Legacy multi-trigger (not used by new synthdefs but kept for compat)
            101..=103 => Some(MidiTriggerType::NoteOn),
            111..=112 => Some(MidiTriggerType::NoteOff),
            120..=123 => Some(MidiTriggerType::CC),
            130..=132 => Some(MidiTriggerType::PitchBend),
            140 => Some(MidiTriggerType::Clock),
            150 => Some(MidiTriggerType::Start),
            151 => Some(MidiTriggerType::Stop),
            152 => Some(MidiTriggerType::Continue),
            _ => None,
        }
    }
}

/// Types of MIDI trigger messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiTriggerType {
    /// New single-trigger note on with packed data
    NoteOnPacked,
    /// New single-trigger note off with packed data
    NoteOffPacked,
    /// Legacy multi-trigger note on (unused by new synthdefs)
    NoteOn,
    /// Legacy multi-trigger note off (unused by new synthdefs)
    NoteOff,
    CC,
    PitchBend,
    Clock,
    Start,
    Stop,
    Continue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_midi_synthdefs() {
        let defs = create_midi_synthdefs();
        assert_eq!(defs.len(), 8, "Should create 8 MIDI synthdefs");

        let names: Vec<&str> = defs.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"vibelang_midi_note_on"));
        assert!(names.contains(&"vibelang_midi_note_off"));
        assert!(names.contains(&"vibelang_midi_cc"));
        assert!(names.contains(&"vibelang_midi_pitch_bend"));
        assert!(names.contains(&"vibelang_midi_clock"));
        assert!(names.contains(&"vibelang_midi_start"));
        assert!(names.contains(&"vibelang_midi_stop"));
        assert!(names.contains(&"vibelang_midi_continue"));
    }

    #[test]
    fn test_trigger_id_ranges() {
        use trigger_ids::*;

        assert_eq!(message_type(100), Some(MidiTriggerType::NoteOn));
        assert_eq!(message_type(103), Some(MidiTriggerType::NoteOn));
        assert_eq!(message_type(110), Some(MidiTriggerType::NoteOff));
        assert_eq!(message_type(140), Some(MidiTriggerType::Clock));
        assert_eq!(message_type(150), Some(MidiTriggerType::Start));
        assert_eq!(message_type(99), None);
    }
}
