// SFZ voice synthdef with gate-controlled envelope
// PlayBuf -> EnvGen (ASR) -> amp -> Out

use vibelang_dsp::{GraphBuilderInner, GraphIR, Input, Rate, encode_synthdef};
use anyhow::Result;

/// Create SFZ voice synthdef with gate-controlled ASR envelope
///
/// The envelope responds to gate changes:
/// - gate=1: Attack phase (0 -> sustain), then holds at sustain
/// - gate=0: Release phase (sustain -> 0), then synth is freed via doneAction=2
///
/// Parameters:
/// - out: output bus (0)
/// - bufnum: buffer number (1)
/// - rate: playback rate (2)
/// - amp: amplitude (3)
/// - gate: envelope gate, 1=on, 0=release (4)
/// - attack: envelope attack time in seconds (5)
/// - decay: envelope decay time (6) - unused but kept for compatibility
/// - sustain: envelope sustain level 0-1 (7)
/// - release: envelope release time (8)
/// - loop: loop mode 0=no, 1=yes (9)
/// - startPos: start position in frames (10)
pub fn create_sfz_voice_synthdef_bufrd(num_channels: u32) -> Result<GraphIR> {
    let name = if num_channels == 1 {
        "sfz_voice_mono"
    } else {
        "sfz_voice_stereo"
    };

    log::info!(
        "Creating SFZ synthdef '{}' with {} channels and gate-controlled envelope",
        name,
        num_channels
    );

    let mut builder = GraphBuilderInner::new();

    // Parameters - order matters for control output indices
    builder.add_param("out".to_string(), vec![0.0], None); // 0
    builder.add_param("bufnum".to_string(), vec![0.0], None); // 1
    builder.add_param("rate".to_string(), vec![1.0], None); // 2
    builder.add_param("amp".to_string(), vec![1.0], None); // 3
    builder.add_param("gate".to_string(), vec![1.0], None); // 4 - envelope gate
    builder.add_param("attack".to_string(), vec![0.001], None); // 5 - attack time
    builder.add_param("decay".to_string(), vec![0.0], None); // 6 - decay (unused)
    builder.add_param("sustain".to_string(), vec![1.0], None); // 7 - sustain level
    builder.add_param("release".to_string(), vec![0.01], None); // 8 - release time
    builder.add_param("loop".to_string(), vec![0.0], None); // 9
    builder.add_param("startPos".to_string(), vec![0.0], None); // 10

    // Create control UGen (must be first node after params are added)
    builder.create_control_ugen();

    // Helper to get control output by param index
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

    builder.add_constant(trigger_const);
    builder.add_constant(done_action_free);
    builder.add_constant(zero);
    builder.add_constant(one);
    builder.add_constant(neg_one);
    builder.add_constant(shape_linear);

    // Additional constants for envelope
    let two = 2.0f32;
    builder.add_constant(two);

    // PlayBuf.ar(numChannels, bufnum, rate, trigger, startPos, loop, doneAction)
    let playbuf_node = builder.add_node(
        "PlayBuf".to_string(),
        Rate::Audio,
        vec![
            param(1),                       // bufnum
            param(2),                       // rate
            Input::Constant(trigger_const), // trigger
            param(10),                      // startPos
            param(9),                       // loop
            Input::Constant(zero),          // doneAction=0 (envelope handles freeing)
        ],
        num_channels,
        num_channels as i16,
    );

    // EnvGen with ASR envelope
    //
    // SuperCollider Env format for EnvGen:
    // EnvGen.ar(gate, levelScale, levelBias, timeScale, doneAction,
    //           initLevel, numStages, releaseNode, loopNode,
    //           [endLevel0, time0, shape0, curve0, endLevel1, time1, shape1, curve1, ...])
    //
    // ASR envelope (2 stages):
    // - Stage 0: Attack (0 -> sustain) - plays immediately
    // - Stage 1: Release (sustain -> 0) - plays when gate goes to 0
    //
    // releaseNode = 1 means stage 1 is the release (triggered when gate=0)
    // With releaseNode=1, the envelope will:
    // 1. Play stage 0 (attack) when gate > 0
    // 2. Hold at the end level of stage 0 (sustain level)
    // 3. When gate goes to 0, jump to stage 1 (release) and play it
    // 4. When stage 1 completes, doneAction fires

    let num_stages = 2.0f32;
    let release_node = 1.0f32; // Stage 1 is release
    let loop_node = -1.0f32;

    builder.add_constant(num_stages);
    builder.add_constant(release_node);

    let env_node = builder.add_node(
        "EnvGen".to_string(),
        Rate::Audio,
        vec![
            param(4),                          // gate
            Input::Constant(one),              // levelScale = 1
            Input::Constant(zero),             // levelBias = 0
            Input::Constant(one),              // timeScale = 1
            Input::Constant(done_action_free), // doneAction = 2 (free synth)
            // Envelope specification:
            Input::Constant(zero),             // initLevel = 0
            Input::Constant(num_stages),       // numStages = 2
            Input::Constant(release_node),     // releaseNode = 1
            Input::Constant(loop_node),        // loopNode = -1 (no loop)
            // Stage 0: Attack (0 -> sustain level)
            param(7),                          // endLevel = sustain param
            param(5),                          // time = attack param
            Input::Constant(shape_linear),     // shape = linear (1)
            Input::Constant(zero),             // curve = 0
            // Stage 1: Release (sustain -> 0)
            Input::Constant(zero),             // endLevel = 0
            param(8),                          // time = release param
            Input::Constant(shape_linear),     // shape = linear (1)
            Input::Constant(zero),             // curve = 0
        ],
        1, // 1 output
        0, // special_index
    );

    // Multiply each channel by envelope and amp
    let mut out_inputs = vec![param(0)]; // out bus
    for ch in 0..num_channels {
        let playbuf_ch = Input::Node {
            node_id: playbuf_node.0,
            output_index: ch,
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

    // Out
    builder.add_node("Out".to_string(), Rate::Audio, out_inputs, 0, 0);

    let ir = GraphIR::from_builder(name.to_string(), builder);
    Ok(ir)
}

/// Create and encode all SFZ synthdefs.
/// Returns a vector of (name, encoded_bytes) pairs.
pub fn create_sfz_synthdefs() -> Vec<(String, Vec<u8>)> {
    let mut defs = Vec::new();

    // Create mono version
    if let Ok(mono) = create_sfz_voice_synthdef_bufrd(1) {
        if let Ok(bytes) = encode_synthdef(&mono) {
            defs.push(("sfz_voice_mono".to_string(), bytes));
        }
    }

    // Create stereo version
    if let Ok(stereo) = create_sfz_voice_synthdef_bufrd(2) {
        if let Ok(bytes) = encode_synthdef(&stereo) {
            defs.push(("sfz_voice_stereo".to_string(), bytes));
        }
    }

    // Also create generic "sfz_voice" as stereo by default
    if let Ok(mut stereo2) = create_sfz_voice_synthdef_bufrd(2) {
        stereo2.name = "sfz_voice".to_string();
        if let Ok(bytes) = encode_synthdef(&stereo2) {
            defs.push(("sfz_voice".to_string(), bytes));
        }
    }

    defs
}
