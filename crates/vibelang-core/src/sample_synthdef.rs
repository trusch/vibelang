//! Sample voice synthdef generation.
//!
//! Provides PlayBuf-based sample playback and Warp1-based time-stretching synthdefs.

use vibelang_dsp::{encode_synthdef, GraphBuilderInner, GraphIR, Input, Rate};

/// Create and encode all sample voice synthdefs.
/// Returns a vector of (name, encoded_bytes) pairs.
pub fn create_sample_synthdefs() -> Vec<(String, Vec<u8>)> {
    let mut defs = Vec::new();

    // PlayBuf-based sample voices
    if let Some((name, bytes)) = generate_sample_voice_synthdef("sample_voice_mono", 1) {
        defs.push((name, bytes));
    }
    if let Some((name, bytes)) = generate_sample_voice_synthdef("sample_voice_stereo", 2) {
        defs.push((name, bytes));
    }

    // Warp1-based time-stretch voices
    if let Some((name, bytes)) = generate_warp_voice_synthdef("warp_voice_mono", 1) {
        defs.push((name, bytes));
    }
    if let Some((name, bytes)) = generate_warp_voice_synthdef("warp_voice_stereo", 2) {
        defs.push((name, bytes));
    }

    defs
}

/// Generate a PlayBuf-based sample voice synthdef.
///
/// This synthdef automatically releases when the buffer finishes playing (for non-looping samples).
/// Uses Done.kr to detect buffer completion and automatically closes the gate.
///
/// Parameters:
/// - out: output bus (0)
/// - bufnum: buffer number (1)
/// - rate: playback rate (2)
/// - amp: amplitude (3)
/// - gate: envelope gate (4)
/// - attack: envelope attack time (5)
/// - sustain: envelope sustain level (6)
/// - release: envelope release time (7)
/// - loop: loop mode 0/1 (8)
/// - startPos: start position in frames (9)
/// - endPos: end position in frames, -1 = full sample (10)
fn generate_sample_voice_synthdef(name: &str, num_channels: i32) -> Option<(String, Vec<u8>)> {
    let mut builder = GraphBuilderInner::new();

    // Parameters - order matters for control output indices
    builder.add_param("out".to_string(), vec![0.0], None);        // 0
    builder.add_param("bufnum".to_string(), vec![0.0], None);     // 1
    builder.add_param("rate".to_string(), vec![1.0], None);       // 2
    builder.add_param("amp".to_string(), vec![1.0], None);        // 3
    builder.add_param("gate".to_string(), vec![1.0], None);       // 4
    builder.add_param("attack".to_string(), vec![0.001], None);   // 5
    builder.add_param("sustain".to_string(), vec![1.0], None);    // 6
    builder.add_param("release".to_string(), vec![0.01], None);   // 7
    builder.add_param("loop".to_string(), vec![0.0], None);       // 8
    builder.add_param("startPos".to_string(), vec![0.0], None);   // 9
    builder.add_param("endPos".to_string(), vec![-1.0], None);    // 10

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    // Constants
    let trigger_const = 1.0f32;
    let done_action_free = 2.0f32;
    let zero = 0.0f32;
    let one = 1.0f32;
    let neg_one = -1.0f32;
    let shape_linear = 1.0f32;
    let num_stages = 2.0f32;
    let release_node = 1.0f32;

    builder.add_constant(trigger_const);
    builder.add_constant(done_action_free);
    builder.add_constant(zero);
    builder.add_constant(one);
    builder.add_constant(neg_one);
    builder.add_constant(shape_linear);
    builder.add_constant(num_stages);
    builder.add_constant(release_node);

    // PlayBuf.ar(numChannels, bufnum, rate, trigger, startPos, loop, doneAction)
    let playbuf_node = builder.add_node(
        "PlayBuf".to_string(),
        Rate::Audio,
        vec![
            param(1),                       // bufnum
            param(2),                       // rate
            Input::Constant(trigger_const), // trigger
            param(9),                       // startPos
            param(8),                       // loop
            Input::Constant(zero),          // doneAction=0 (envelope handles freeing)
        ],
        num_channels as u32,
        num_channels as i16,
    );

    // Done.kr detects when PlayBuf has finished playing
    // Outputs 1.0 when the source UGen (PlayBuf) is done
    let done_node = builder.add_node(
        "Done".to_string(),
        Rate::Control,
        vec![Input::Node {
            node_id: playbuf_node.0,
            output_index: 0,
        }],
        1,
        0,
    );

    // For non-looping samples: close gate when PlayBuf finishes
    // effectiveGate = gate * (1 - ((1 - loop) * done))
    // When loop=0: effectiveGate = gate * (1 - done) → gate becomes 0 when done=1
    // When loop=1: effectiveGate = gate * (1 - 0) = gate → unaffected

    // (1 - loop)
    let one_minus_loop = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![Input::Constant(one), param(8)],
        1,
        1, // subtraction
    );

    // (1 - loop) * done
    let done_factor = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![
            Input::Node {
                node_id: one_minus_loop.0,
                output_index: 0,
            },
            Input::Node {
                node_id: done_node.0,
                output_index: 0,
            },
        ],
        1,
        2, // multiplication
    );

    // 1 - (done_factor)
    let gate_modifier = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![
            Input::Constant(one),
            Input::Node {
                node_id: done_factor.0,
                output_index: 0,
            },
        ],
        1,
        1, // subtraction
    );

    // gate * gate_modifier = effectiveGate
    let effective_gate = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![
            param(4), // original gate
            Input::Node {
                node_id: gate_modifier.0,
                output_index: 0,
            },
        ],
        1,
        2, // multiplication
    );

    // EnvGen with ASR envelope, using the effective gate
    let env_node = builder.add_node(
        "EnvGen".to_string(),
        Rate::Audio,
        vec![
            Input::Node {
                node_id: effective_gate.0,
                output_index: 0,
            }, // effectiveGate instead of raw gate
            Input::Constant(one),              // levelScale = 1
            Input::Constant(zero),             // levelBias = 0
            Input::Constant(one),              // timeScale = 1
            Input::Constant(done_action_free), // doneAction = 2 (free synth)
            Input::Constant(zero),             // initLevel = 0
            Input::Constant(num_stages),       // numStages = 2
            Input::Constant(release_node),     // releaseNode = 1
            Input::Constant(neg_one),          // loopNode = -1 (no loop)
            param(6),                          // stage0 endLevel = sustain param
            param(5),                          // stage0 time = attack param
            Input::Constant(shape_linear),     // shape = linear (1)
            Input::Constant(zero),             // curve = 0
            Input::Constant(zero),             // stage1 endLevel = 0
            param(7),                          // stage1 time = release param
            Input::Constant(shape_linear),     // shape = linear (1)
            Input::Constant(zero),             // curve = 0
        ],
        1,
        0,
    );

    // Multiply each channel by envelope and amp
    let mut out_inputs = vec![param(0)]; // out bus
    for ch in 0..num_channels {
        let playbuf_ch = Input::Node {
            node_id: playbuf_node.0,
            output_index: ch as u32,
        };

        // PlayBuf * envelope
        let env_mul_node = builder.add_node(
            "BinaryOpUGen".to_string(),
            Rate::Audio,
            vec![
                playbuf_ch,
                Input::Node {
                    node_id: env_node.0,
                    output_index: 0,
                },
            ],
            1,
            2, // multiplication
        );

        // (PlayBuf * envelope) * amp
        let amp_mul_node = builder.add_node(
            "BinaryOpUGen".to_string(),
            Rate::Audio,
            vec![
                Input::Node {
                    node_id: env_mul_node.0,
                    output_index: 0,
                },
                param(3), // amp
            ],
            1,
            2, // multiplication
        );

        out_inputs.push(Input::Node {
            node_id: amp_mul_node.0,
            output_index: 0,
        });
    }

    builder.add_node("Out".to_string(), Rate::Audio, out_inputs, 0, 0);

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::info!("[SAMPLE] Generated {} synthdef ({} bytes)", name, bytes.len());
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[SAMPLE] Failed to encode SynthDef '{}': {}", name, e);
            None
        }
    }
}

/// Generate a Warp1-based synthdef for time-stretching sample playback.
///
/// Warp1 allows independent control of playback speed and pitch.
/// This synthdef automatically releases when the Line UGen finishes (reaching endPos).
///
/// Parameters:
/// - out: output bus (0)
/// - bufnum: buffer number (1)
/// - speed: playback speed multiplier (2)
/// - pitch: pitch shift multiplier (3)
/// - amp: amplitude (4)
/// - gate: envelope gate (5)
/// - attack: envelope attack time (6)
/// - sustain: envelope sustain level (7)
/// - release: envelope release time (8)
/// - startPos: normalized start position 0-1 (9)
/// - endPos: normalized end position 0-1 (10)
/// - windowSize: granular window size in seconds (11)
/// - overlaps: number of overlapping grains (12)
fn generate_warp_voice_synthdef(name: &str, num_channels: i32) -> Option<(String, Vec<u8>)> {
    let mut builder = GraphBuilderInner::new();

    // Parameters
    builder.add_param("out".to_string(), vec![0.0], None);           // 0
    builder.add_param("bufnum".to_string(), vec![0.0], None);        // 1
    builder.add_param("speed".to_string(), vec![1.0], None);         // 2 - time stretch
    builder.add_param("pitch".to_string(), vec![1.0], None);         // 3 - pitch shift
    builder.add_param("amp".to_string(), vec![1.0], None);           // 4
    builder.add_param("gate".to_string(), vec![1.0], None);          // 5
    builder.add_param("attack".to_string(), vec![0.01], None);       // 6
    builder.add_param("sustain".to_string(), vec![1.0], None);       // 7
    builder.add_param("release".to_string(), vec![0.1], None);       // 8
    builder.add_param("startPos".to_string(), vec![0.0], None);      // 9 - normalized 0-1
    builder.add_param("endPos".to_string(), vec![1.0], None);        // 10 - normalized 0-1
    builder.add_param("windowSize".to_string(), vec![0.1], None);    // 11
    builder.add_param("overlaps".to_string(), vec![8.0], None);      // 12

    builder.create_control_ugen();

    let param = |idx: u32| Input::Node {
        node_id: 0,
        output_index: idx,
    };

    // Constants
    let zero = 0.0f32;
    let one = 1.0f32;
    let neg_one = -1.0f32;
    let done_action_free = 2.0f32;
    let shape_linear = 1.0f32;
    let num_stages = 2.0f32;
    let release_node = 1.0f32;
    let interp = 4.0f32; // cubic interpolation

    builder.add_constant(zero);
    builder.add_constant(one);
    builder.add_constant(neg_one);
    builder.add_constant(done_action_free);
    builder.add_constant(shape_linear);
    builder.add_constant(num_stages);
    builder.add_constant(release_node);
    builder.add_constant(interp);

    // BufDur to get buffer duration
    let buf_dur = builder.add_node(
        "BufDur".to_string(),
        Rate::Control,
        vec![param(1)], // bufnum
        1,
        0,
    );

    // Calculate playback duration based on speed
    // duration = (endPos - startPos) * bufDur / speed
    let pos_range = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![param(10), param(9)], // endPos - startPos
        1,
        1, // subtraction
    );

    let scaled_dur = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![
            Input::Node { node_id: pos_range.0, output_index: 0 },
            Input::Node { node_id: buf_dur.0, output_index: 0 },
        ],
        1,
        2, // multiplication
    );

    let actual_dur = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![
            Input::Node { node_id: scaled_dur.0, output_index: 0 },
            param(2), // speed
        ],
        1,
        4, // division
    );

    // Line UGen for pointer position (moves from startPos to endPos over duration)
    let pointer = builder.add_node(
        "Line".to_string(),
        Rate::Control,
        vec![
            param(9),  // start = startPos
            param(10), // end = endPos
            Input::Node { node_id: actual_dur.0, output_index: 0 }, // dur
            Input::Constant(zero), // doneAction=0 (envelope handles freeing)
        ],
        1,
        0,
    );

    // Done.kr detects when Line has finished
    let done_node = builder.add_node(
        "Done".to_string(),
        Rate::Control,
        vec![Input::Node {
            node_id: pointer.0,
            output_index: 0,
        }],
        1,
        0,
    );

    // Auto-close gate when Line finishes: effectiveGate = gate * (1 - done)
    let one_minus_done = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![
            Input::Constant(one),
            Input::Node {
                node_id: done_node.0,
                output_index: 0,
            },
        ],
        1,
        1, // subtraction
    );

    let effective_gate = builder.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![
            param(5), // original gate
            Input::Node {
                node_id: one_minus_done.0,
                output_index: 0,
            },
        ],
        1,
        2, // multiplication
    );

    // Warp1.ar(numChannels, bufnum, pointer, freqScale, windowSize, envbufnum, overlaps, windowRandRatio, interp)
    let warp_node = builder.add_node(
        "Warp1".to_string(),
        Rate::Audio,
        vec![
            param(1),  // bufnum
            Input::Node { node_id: pointer.0, output_index: 0 }, // pointer
            param(3),  // freqScale (pitch)
            param(11), // windowSize
            Input::Constant(neg_one), // envbufnum = -1 (built-in Hann window)
            param(12), // overlaps
            Input::Constant(zero),    // windowRandRatio = 0
            Input::Constant(interp),  // interp = 4 (cubic)
        ],
        num_channels as u32,
        0,
    );

    // ASR envelope with auto-closing effective gate
    let env_node = builder.add_node(
        "EnvGen".to_string(),
        Rate::Audio,
        vec![
            Input::Node {
                node_id: effective_gate.0,
                output_index: 0,
            }, // effectiveGate instead of raw gate
            Input::Constant(one),              // levelScale
            Input::Constant(zero),             // levelBias
            Input::Constant(one),              // timeScale
            Input::Constant(done_action_free), // doneAction = 2
            Input::Constant(zero),             // initLevel = 0
            Input::Constant(num_stages),       // numStages = 2
            Input::Constant(release_node),     // releaseNode = 1
            Input::Constant(neg_one),          // loopNode = -1
            param(7),                          // stage0 endLevel = sustain
            param(6),                          // stage0 time = attack
            Input::Constant(shape_linear),     // shape
            Input::Constant(zero),             // curve
            Input::Constant(zero),             // stage1 endLevel = 0
            param(8),                          // stage1 time = release
            Input::Constant(shape_linear),     // shape
            Input::Constant(zero),             // curve
        ],
        1,
        0,
    );

    // Multiply each channel by envelope and amp
    let mut out_inputs = vec![param(0)];
    for ch in 0..num_channels {
        let warp_ch = Input::Node {
            node_id: warp_node.0,
            output_index: ch as u32,
        };

        // Warp1 * envelope
        let env_mul_node = builder.add_node(
            "BinaryOpUGen".to_string(),
            Rate::Audio,
            vec![
                warp_ch,
                Input::Node { node_id: env_node.0, output_index: 0 },
            ],
            1,
            2, // multiplication
        );

        // (Warp1 * envelope) * amp
        let amp_mul_node = builder.add_node(
            "BinaryOpUGen".to_string(),
            Rate::Audio,
            vec![
                Input::Node { node_id: env_mul_node.0, output_index: 0 },
                param(4), // amp
            ],
            1,
            2, // multiplication
        );

        out_inputs.push(Input::Node {
            node_id: amp_mul_node.0,
            output_index: 0,
        });
    }

    builder.add_node("Out".to_string(), Rate::Audio, out_inputs, 0, 0);

    let ir = GraphIR::from_builder(name.to_string(), builder);
    match encode_synthdef(&ir) {
        Ok(bytes) => {
            log::info!("[SAMPLE] Generated {} synthdef ({} bytes)", name, bytes.len());
            Some((name.to_string(), bytes))
        }
        Err(e) => {
            log::error!("[SAMPLE] Failed to encode Warp SynthDef '{}': {}", name, e);
            None
        }
    }
}
