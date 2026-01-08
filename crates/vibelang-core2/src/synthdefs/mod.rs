//! Built-in SynthDef generation.
//!
//! This module provides pre-built SynthDefs that are essential for
//! the runtime to function. Uses vibelang-dsp for comprehensive synthdef
//! generation including sample playback, SFZ instruments, MIDI output,
//! recording, and audio routing with metering.
//!
//! ## Built-in SynthDefs
//!
//! ### Routing
//! - `system_link_audio` - Routes group bus to output with metering
//!
//! ### Sample Playback
//! - `sample_voice_mono` - PlayBuf-based mono sample voice
//! - `sample_voice_stereo` - PlayBuf-based stereo sample voice
//! - `warp_voice_mono` - Warp1-based time-stretch mono voice
//! - `warp_voice_stereo` - Warp1-based time-stretch stereo voice
//!
//! ### SFZ Instruments
//! - `sfz_voice` - Gate-controlled SFZ sample voice
//! - `sfz_voice_mono` - Mono variant
//! - `sfz_voice_stereo` - Stereo variant
//!
//! ### MIDI Output
//! - `vibelang_midi_note_on` - Trigger MIDI note on
//! - `vibelang_midi_note_off` - Trigger MIDI note off
//! - `vibelang_midi_cc` - Send MIDI CC
//! - `vibelang_midi_pitch_bend` - Send pitch bend
//! - `vibelang_midi_clock` - MIDI clock
//! - `vibelang_midi_start/stop/continue` - Transport
//!
//! ### Recording
//! - `system_record_mono` - Record mono bus to buffer
//! - `system_record_stereo` - Record stereo bus to buffer
//!
//! ### Legacy (simple testing)
//! - `simple_sine` - Basic sine oscillator for testing

mod encoder;

use encoder::{GraphBuilder, Input, Rate};
use vibelang_dsp::system_synthdefs;

/// Generate all built-in synthdef bytes.
///
/// Returns a vector of (name, bytes) pairs for all essential synthdefs.
pub fn generate_builtins() -> Vec<(String, Vec<u8>)> {
    let mut all_defs = Vec::new();

    // Add the routing synthdef (system_link_audio with metering)
    // This is the only one that returns Result, so we unwrap here
    if let Ok(bytes) = system_synthdefs::create_system_link_audio_bytes() {
        all_defs.push(("system_link_audio".to_string(), bytes));
    } else {
        // Fallback to simple routing without metering if DSP version fails
        tracing::warn!("Failed to create system_link_audio from vibelang-dsp, using fallback");
        all_defs.push(("system_link_audio".to_string(), generate_simple_link_audio()));
    }

    // Add all system synthdefs from vibelang-dsp
    // This includes: MIDI, recording, sample voices, SFZ voices
    all_defs.extend(system_synthdefs::create_all_system_synthdefs());

    // Keep simple_sine for basic testing
    all_defs.push(("simple_sine".to_string(), generate_simple_sine()));

    all_defs
}

/// Simple fallback link audio synthdef (no metering).
fn generate_simple_link_audio() -> Vec<u8> {
    let mut builder = GraphBuilder::new("system_link_audio");

    builder.add_param("inbus", 0.0);
    builder.add_param("outbus", 0.0);
    builder.add_param("amp", 1.0);

    builder.create_control_ugen();

    // In.ar(inbus, 2)
    let in_node = builder.add_node(
        "In",
        Rate::Audio,
        vec![Input::param(0)],
        2,
        0,
    );

    // left * amp
    let left_scaled = builder.add_node(
        "BinaryOpUGen",
        Rate::Audio,
        vec![Input::node(in_node, 0), Input::param(2)],
        1,
        2,
    );

    // right * amp
    let right_scaled = builder.add_node(
        "BinaryOpUGen",
        Rate::Audio,
        vec![Input::node(in_node, 1), Input::param(2)],
        1,
        2,
    );

    // Out.ar(outbus, sig)
    builder.add_node(
        "Out",
        Rate::Audio,
        vec![
            Input::param(1),
            Input::node(left_scaled, 0),
            Input::node(right_scaled, 0),
        ],
        0,
        0,
    );

    builder.encode()
}

/// Generate simple_sine synthdef.
///
/// A basic sine oscillator with frequency, amplitude, and gate.
/// Parameters: freq, amp, gate, out
fn generate_simple_sine() -> Vec<u8> {
    let mut builder = GraphBuilder::new("simple_sine");

    // Parameters
    builder.add_param("freq", 440.0);
    builder.add_param("amp", 0.5);
    builder.add_param("gate", 1.0);
    builder.add_param("out", 0.0);

    builder.create_control_ugen();

    // Add constants we need
    let const_0 = builder.add_constant(0.0);
    let const_1 = builder.add_constant(1.0);
    let const_2 = builder.add_constant(2.0);
    let const_0_01 = builder.add_constant(0.01);
    let const_0_1 = builder.add_constant(0.1);

    // SinOsc.ar(freq, 0)
    let sin_node = builder.add_node(
        "SinOsc",
        Rate::Audio,
        vec![
            Input::param(0),               // freq
            Input::Constant { index: const_0 }, // phase
        ],
        1,
        0,
    );

    // EnvGen.kr - ASR envelope
    // EnvGen needs: gate, levelScale, levelBias, timeScale, doneAction, envelope...
    // For ASR envelope array: [0, 2, -99, -99, 1, 0.01, 5, 0, 0, 0.1, 5, 0]
    // But the array encoding is complex. Let's use Linen instead.
    //
    // Linen.kr(gate, attackTime, susLevel, releaseTime, doneAction)
    let linen_node = builder.add_node(
        "Linen",
        Rate::Control,
        vec![
            Input::param(2),                    // gate
            Input::Constant { index: const_0_01 }, // attack time
            Input::Constant { index: const_1 },    // sustain level
            Input::Constant { index: const_0_1 },  // release time
            Input::Constant { index: const_2 },    // doneAction: 2 (free)
        ],
        1,
        0,
    );

    // sig = sin * amp * env
    // Using BinaryOpUGen for multiplication (op 2)
    let mul1_node = builder.add_node(
        "BinaryOpUGen",
        Rate::Audio,
        vec![
            Input::node(sin_node, 0),
            Input::param(1), // amp
        ],
        1,
        2, // multiply
    );

    let mul2_node = builder.add_node(
        "BinaryOpUGen",
        Rate::Audio,
        vec![
            Input::node(mul1_node, 0),
            Input::node(linen_node, 0),
        ],
        1,
        2, // multiply
    );

    // Pan2.ar(sig, 0)
    let pan_node = builder.add_node(
        "Pan2",
        Rate::Audio,
        vec![
            Input::node(mul2_node, 0),
            Input::Constant { index: const_0 }, // center
            Input::Constant { index: const_1 }, // level
        ],
        2,
        0,
    );

    // Out.ar(out, sig)
    builder.add_node(
        "Out",
        Rate::Audio,
        vec![
            Input::param(3),         // out
            Input::node(pan_node, 0), // left
            Input::node(pan_node, 1), // right
        ],
        0,
        0,
    );

    builder.encode()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_builtins() {
        let builtins = generate_builtins();

        // Should have many synthdefs now from vibelang-dsp
        // At minimum: system_link_audio, sample voices, SFZ voices, simple_sine
        assert!(builtins.len() >= 5, "Expected at least 5 synthdefs, got {}", builtins.len());

        for (name, bytes) in &builtins {
            // All synthdefs should start with "SCgf" header
            assert_eq!(&bytes[0..4], b"SCgf", "Invalid header for {}", name);
            // Version should be 2
            assert_eq!(
                &bytes[4..8],
                &[0, 0, 0, 2],
                "Invalid version for {}",
                name
            );
        }
    }

    #[test]
    fn test_has_essential_synthdefs() {
        let builtins = generate_builtins();
        let names: Vec<&str> = builtins.iter().map(|(n, _)| n.as_str()).collect();

        // Check for essential synthdefs
        assert!(names.contains(&"system_link_audio"), "Missing system_link_audio");
        assert!(names.contains(&"simple_sine"), "Missing simple_sine");

        // Check for sample voices from vibelang-dsp
        assert!(
            names.iter().any(|n| n.contains("sample_voice")),
            "Missing sample voice synthdefs"
        );
    }

    #[test]
    fn test_simple_sine() {
        let bytes = generate_simple_sine();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..4], b"SCgf");
    }

    #[test]
    fn test_system_link_audio_from_dsp() {
        let bytes = system_synthdefs::create_system_link_audio_bytes().unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..4], b"SCgf");
    }
}
