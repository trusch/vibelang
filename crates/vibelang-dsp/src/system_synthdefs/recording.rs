//! Recording synthdefs for VibeLang.
//!
//! Provides synthdefs for audio recording and metronome click generation.

use crate::{encode_synthdef, GraphBuilderInner, GraphIR, Input, Rate};

/// Create and encode all recording-related synthdefs.
/// Returns a vector of (name, encoded_bytes) pairs.
pub fn create_recording_synthdefs() -> Vec<(String, Vec<u8>)> {
    let mut defs = Vec::new();

    // Recording synthdefs
    if let Some((name, bytes)) = generate_record_synthdef("system_record_mono", 1) {
        defs.push((name, bytes));
    }
    if let Some((name, bytes)) = generate_record_synthdef("system_record_stereo", 2) {
        defs.push((name, bytes));
    }

    // Metronome click
    if let Some((name, bytes)) = generate_metronome_synthdef() {
        defs.push((name, bytes));
    }

    defs
}

/// Generate a recording synthdef that records from a bus to a buffer.
///
/// Uses RecordBuf UGen to write audio from an input bus to a buffer.
/// The synth runs until the gate is closed or the buffer is full.
///
/// Parameters:
/// - bufnum: buffer number to record into (0)
/// - in_bus: audio bus to record from (1)
/// - run: 1 = recording, 0 = paused (2)
/// - gate: envelope gate, set to 0 to stop and free (3)
fn generate_record_synthdef(name: &str, num_channels: i32) -> Option<(String, Vec<u8>)> {
    let mut builder = GraphBuilderInner::new();

    // Parameters
    builder.add_param("bufnum".to_string(), vec![0.0], None); // 0
    builder.add_param("in_bus".to_string(), vec![0.0], None); // 1
    builder.add_param("run".to_string(), vec![1.0], None); // 2 - 1=recording, 0=paused
    builder.add_param("gate".to_string(), vec![1.0], None); // 3 - close to stop

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    // Constants
    let zero = 0.0f32;
    let one = 1.0f32;
    let done_action_free = 2.0f32;
    let point_one = 0.1f32;
    let num_channels_f = num_channels as f32;

    builder.add_constant(zero);
    builder.add_constant(one);
    builder.add_constant(done_action_free);
    builder.add_constant(point_one);
    builder.add_constant(num_channels_f);

    // In.ar(bus) - read from the group's audio bus
    // Note: Number of channels is determined by num_outputs, NOT by an input parameter
    log::debug!(
        "[RECORDING] {} creating In.ar with bus param at index 1, {} channels",
        name,
        num_channels
    );
    let in_node = builder.add_node(
        "In".to_string(),
        Rate::Audio,
        vec![
            param(1), // in_bus - ONLY input (channel count from output count)
        ],
        num_channels as u32, // output count determines number of channels to read
        0,
    );

    // RecordBuf UGen input order (from SC source - inputArray comes LAST!):
    // bufnum, offset, recLevel, preLevel, run, loop, trigger, doneAction, ...inputArray
    let mut recordbuf_inputs = vec![
        // Fixed parameters FIRST
        param(0),              // bufnum
        Input::Constant(zero), // offset = 0
        Input::Constant(one),  // recLevel = 1
        Input::Constant(zero), // preLevel = 0
        param(2),              // run
        Input::Constant(zero), // loop = 0 (no loop)
        Input::Constant(one),  // trigger = 1
        Input::Constant(zero), // doneAction = 0 (envelope handles freeing)
    ];

    // Input channels LAST (this is what SC's multiNew does: ++ inputArray.asArray)
    for ch in 0..num_channels {
        recordbuf_inputs.push(Input::Node {
            node_id: in_node.0,
            output_index: ch as u32,
        });
    }

    builder.add_node(
        "RecordBuf".to_string(),
        Rate::Audio,
        recordbuf_inputs,
        1, // RecordBuf outputs the current record position
        0,
    );

    // Simple envelope that stays open until gate closes
    // Linen.kr(gate, attackTime, susLevel, releaseTime, doneAction)
    let env_node = builder.add_node(
        "Linen".to_string(),
        Rate::Control,
        vec![
            param(3),                          // gate
            Input::Constant(zero),             // attackTime = 0 (instant open)
            Input::Constant(one),              // susLevel = 1
            Input::Constant(point_one),        // releaseTime = 0.1s
            Input::Constant(done_action_free), // doneAction = 2 (free synth)
        ],
        1,
        0,
    );

    // Output silence - recording synths shouldn't output audible signal
    // DC.ar(0) * envelope ensures synth is properly gated and freed
    let dc = builder.add_node(
        "DC".to_string(),
        Rate::Audio,
        vec![Input::Constant(zero)],
        1,
        0,
    );

    // Multiply DC by envelope to ensure it releases properly
    let out_sig = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Audio,
        vec![
            Input::Node {
                node_id: dc.0,
                output_index: 0,
            },
            Input::Node {
                node_id: env_node.0,
                output_index: 0,
            },
        ],
        1,
        2, // multiplication
    );

    // Output to bus 0 (silent)
    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            Input::Constant(zero), // out bus 0
            Input::Node {
                node_id: out_sig.0,
                output_index: 0,
            },
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);

    // Debug: log the synthdef structure
    log::debug!("[RECORDING] {} synthdef structure:", name);
    log::debug!(
        "[RECORDING]   Parameters: {:?}",
        ir.params.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    log::debug!("[RECORDING]   Constants: {:?}", ir.constants);
    for (i, node) in ir.nodes.iter().enumerate() {
        log::debug!(
            "[RECORDING]   Node {}: {} (rate={:?}, inputs={}, outputs={})",
            i,
            node.name,
            node.rate,
            node.inputs.len(),
            node.num_outputs
        );
    }

    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[RECORDING] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[RECORDING] Failed to encode SynthDef '{}': {}", name, e);
            None
        }
    }
}

/// Generate a metronome click synthdef.
///
/// A simple click sound using SinOsc with a percussive envelope.
/// The synth frees itself after playing the click.
///
/// Parameters:
/// - out: output bus (0)
/// - amp: click amplitude (1)
/// - accent: 0 = normal click (880 Hz), 1 = accented beat 1 (1320 Hz) (2)
fn generate_metronome_synthdef() -> Option<(String, Vec<u8>)> {
    let name = "system_metronome";
    let mut builder = GraphBuilderInner::new();

    // Parameters
    builder.add_param("out".to_string(), vec![0.0], None); // 0
    builder.add_param("amp".to_string(), vec![0.5], None); // 1
    builder.add_param("accent".to_string(), vec![0.0], None); // 2 - 0=normal, 1=beat 1

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    // Constants
    let zero = 0.0f32;
    let one = 1.0f32;
    let done_action_free = 2.0f32;
    let attack = 0.001f32;
    let release = 0.1f32;
    let freq_normal = 880.0f32;
    let freq_accent = 1320.0f32;

    builder.add_constant(zero);
    builder.add_constant(one);
    builder.add_constant(done_action_free);
    builder.add_constant(attack);
    builder.add_constant(release);
    builder.add_constant(freq_normal);
    builder.add_constant(freq_accent);

    // Select frequency based on accent parameter
    // freq = accent > 0.5 ? freq_accent : freq_normal
    // Using Select.kr(index, [val0, val1, ...])
    // We need to convert accent to an index (0 or 1)
    // For simplicity, use a linear interpolation: freq = (1-accent)*880 + accent*1320

    // (1 - accent)
    let one_minus_accent = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![Input::Constant(one), param(2)],
        1,
        1, // subtraction
    );

    // (1 - accent) * 880
    let freq_part1 = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: one_minus_accent.0,
                output_index: 0,
            },
            Input::Constant(freq_normal),
        ],
        1,
        2, // multiplication
    );

    // accent * 1320
    let freq_part2 = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![param(2), Input::Constant(freq_accent)],
        1,
        2, // multiplication
    );

    // freq = freq_part1 + freq_part2
    let freq = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: freq_part1.0,
                output_index: 0,
            },
            Input::Node {
                node_id: freq_part2.0,
                output_index: 0,
            },
        ],
        1,
        0, // addition
    );

    // SinOsc.ar(freq)
    let osc = builder.add_node(
        "SinOsc".to_string(),
        Rate::Audio,
        vec![
            Input::Node {
                node_id: freq.0,
                output_index: 0,
            },
            Input::Constant(zero), // phase = 0
        ],
        1,
        0,
    );

    // Percussive envelope using Linen or a simple EnvGen
    // We'll use Linen with very short attack and moderate release
    // Linen.kr(gate, attack, sustain, release, doneAction)
    // But Linen needs a gate that stays high - instead use Line for a one-shot

    // Line.kr(start, end, dur, doneAction)
    // Goes from 1 to 0 over the duration, then frees
    let env = builder.add_node(
        "Line".to_string(),
        Rate::Control,
        vec![
            Input::Constant(one),              // start = 1
            Input::Constant(zero),             // end = 0
            Input::Constant(release),          // dur = 0.1s
            Input::Constant(done_action_free), // doneAction = 2
        ],
        1,
        0,
    );

    // osc * env
    let osc_env = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Audio,
        vec![
            Input::Node {
                node_id: osc.0,
                output_index: 0,
            },
            Input::Node {
                node_id: env.0,
                output_index: 0,
            },
        ],
        1,
        2, // multiplication
    );

    // osc * env * amp
    let final_sig = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Audio,
        vec![
            Input::Node {
                node_id: osc_env.0,
                output_index: 0,
            },
            param(1), // amp
        ],
        1,
        2, // multiplication
    );

    // Output to both channels (stereo)
    builder.add_node(
        "Out".to_string(),
        Rate::Audio,
        vec![
            param(0), // out bus
            Input::Node {
                node_id: final_sig.0,
                output_index: 0,
            },
            Input::Node {
                node_id: final_sig.0,
                output_index: 0,
            }, // duplicate for stereo
        ],
        0,
        0,
    );

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::debug!(
                "[RECORDING] Generated {} synthdef ({} bytes)",
                name,
                bytes.len()
            );
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[RECORDING] Failed to encode SynthDef '{}': {}", name, e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_recording_synthdefs() {
        let defs = create_recording_synthdefs();
        assert_eq!(defs.len(), 3);

        let names: Vec<_> = defs.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"system_record_mono"));
        assert!(names.contains(&"system_record_stereo"));
        assert!(names.contains(&"system_metronome"));
    }
}
