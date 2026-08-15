//! MIDI types and message conversion.
//!
//! This module contains the internal MIDI message types used by the handler
//! and routing configuration types.

use crate::midi::{
    CcRouteBuilder, KeyboardRouteBuilder, MidiMessage as NewMidiMessage, NoteRouteBuilder,
};
use crate::traits::FadeTarget;
use crate::types::ids::MidiDeviceId;
use crate::types::VoiceId;
use std::collections::HashMap;

/// A parsed MIDI message (legacy format for internal routing).
#[derive(Clone, Debug)]
#[allow(dead_code)] // All fields intentionally kept for future routing features
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    ControlChange {
        channel: u8,
        cc: u8,
        value: u8,
    },
    PitchBend {
        channel: u8,
        value: i16,
    },
    /// MIDI Clock (0xF8) - 24 pulses per quarter note.
    Clock,
    /// MIDI Start (0xFA) - Start playback from beginning.
    Start,
    /// MIDI Stop (0xFC) - Stop playback.
    Stop,
    /// MIDI Continue (0xFB) - Continue playback from current position.
    Continue,
}

/// Convert new infrastructure MidiMessage to legacy MidiMessage.
///
/// Returns None for message types that the legacy system doesn't support.
pub fn convert_new_to_legacy_message(msg: &NewMidiMessage) -> Option<MidiMessage> {
    match msg {
        NewMidiMessage::NoteOn {
            channel,
            note,
            velocity,
        } => {
            let vel = velocity.to_midi1();
            if vel > 0 {
                Some(MidiMessage::NoteOn {
                    channel: channel.get(),
                    note: *note,
                    velocity: vel,
                })
            } else {
                Some(MidiMessage::NoteOff {
                    channel: channel.get(),
                    note: *note,
                })
            }
        }
        NewMidiMessage::NoteOff { channel, note, .. } => Some(MidiMessage::NoteOff {
            channel: channel.get(),
            note: *note,
        }),
        NewMidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => Some(MidiMessage::ControlChange {
            channel: channel.get(),
            cc: *controller,
            value: *value,
        }),
        NewMidiMessage::PitchBend { channel, value } => Some(MidiMessage::PitchBend {
            channel: channel.get(),
            value: *value,
        }),
        NewMidiMessage::Clock => Some(MidiMessage::Clock),
        NewMidiMessage::Start => Some(MidiMessage::Start),
        NewMidiMessage::Continue => Some(MidiMessage::Continue),
        NewMidiMessage::Stop => Some(MidiMessage::Stop),
        NewMidiMessage::Midi2NoteOn {
            group_channel,
            note,
            velocity,
            ..
        } => {
            let vel = velocity.to_midi1();
            if vel > 0 {
                Some(MidiMessage::NoteOn {
                    channel: group_channel.channel(),
                    note: *note,
                    velocity: vel,
                })
            } else {
                Some(MidiMessage::NoteOff {
                    channel: group_channel.channel(),
                    note: *note,
                })
            }
        }
        NewMidiMessage::Midi2NoteOff {
            group_channel,
            note,
            ..
        } => Some(MidiMessage::NoteOff {
            channel: group_channel.channel(),
            note: *note,
        }),
        NewMidiMessage::Midi2ControlChange {
            group_channel,
            controller,
            value,
        } => Some(MidiMessage::ControlChange {
            channel: group_channel.channel(),
            cc: *controller,
            value: value.to_7bit(),
        }),
        NewMidiMessage::Midi2PitchBend {
            group_channel,
            value,
        } => {
            let centered = ((*value as i64) - 0x8000_0000i64) / 0x4_0000;
            Some(MidiMessage::PitchBend {
                channel: group_channel.channel(),
                value: centered.clamp(i16::MIN as i64, i16::MAX as i64) as i16,
            })
        }
        // New message types not supported by legacy system
        NewMidiMessage::PolyAftertouch { .. }
        | NewMidiMessage::ProgramChange { .. }
        | NewMidiMessage::ChannelAftertouch { .. }
        | NewMidiMessage::SysEx(_)
        | NewMidiMessage::TimeCode { .. }
        | NewMidiMessage::SongPosition { .. }
        | NewMidiMessage::SongSelect { .. }
        | NewMidiMessage::TuneRequest
        | NewMidiMessage::ActiveSensing
        | NewMidiMessage::Reset => None,

        // Advanced MIDI 2.0 messages are handled separately via MIDI 2.0 routes.
        _ => None,
    }
}

/// Keyboard routing configuration.
#[derive(Clone, Debug)]
pub struct KeyboardRoute {
    pub device_id: MidiDeviceId,
    pub channel: Option<u8>, // None = all channels
    pub voice_id: VoiceId,
}

/// CC routing configuration.
#[derive(Clone, Debug)]
pub struct CcRoute {
    pub device_id: MidiDeviceId,
    pub cc: u8,
    pub target: FadeTarget,
    pub param: String,
    /// Min value when CC is 0.
    pub min_value: f32,
    /// Max value when CC is 127.
    pub max_value: f32,
}

/// MIDI 2.0 keyboard routing configuration.
/// Reserved for future MIDI 2.0 routing support.

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Midi2KeyboardRoute {
    pub device_id: MidiDeviceId,
    pub group: Option<u8>,
    pub channel: Option<u8>,
    pub note_min: u8,
    pub note_max: u8,
    pub transpose: i8,
    pub velocity_curve: String,
    pub voice_id: VoiceId,
}

/// MIDI 2.0 per-note route configuration.

#[derive(Clone, Debug)]
pub struct Midi2PerNoteRoute {
    pub device_id: MidiDeviceId,
    pub group: Option<u8>,
    pub channel: Option<u8>,
    pub controller_type: Midi2ControllerType,
    pub voice_id: VoiceId,
    pub param: String,
    pub min_value: f32,
    pub max_value: f32,
    pub curve: String,
}

/// MIDI 2.0 controller type for per-note routing.

#[derive(Clone, Debug)]
pub enum Midi2ControllerType {
    PitchBend { range: u8 },
    Pressure,
    Timbre,
    Controller(u8),
}

/// Internal routing state.
#[derive(Default)]
pub struct MidiRouting {
    pub keyboard_routes: Vec<KeyboardRoute>,
    pub cc_routes: Vec<CcRoute>,
    /// Advanced keyboard routes with full configuration.
    pub advanced_keyboard_routes: Vec<KeyboardRouteBuilder>,
    /// Advanced note routes (for drums/pads).
    pub note_routes: Vec<NoteRouteBuilder>,
    /// Transport-transparent advanced CC routes with curves.
    pub advanced_cc_routes: Vec<crate::reload::AdvancedMidiCcRoute>,
    /// Advanced pitch-bend routes with curves.
    pub advanced_bend_routes: Vec<CcRouteBuilder>,
    /// Choke group tracking: group name -> active node IDs.
    pub choke_groups: HashMap<String, Vec<crate::types::NodeId>>,

    // MIDI 2.0 routes
    pub midi2_keyboard_routes: Vec<Midi2KeyboardRoute>,

    pub midi2_per_note_routes: Vec<Midi2PerNoteRoute>,
}

/// A MIDI event notification sent to registered callbacks.
#[derive(Clone, Debug)]
pub struct MidiEventNotification {
    /// Callback ID that should handle this event.
    pub callback_id: u64,
    /// Device the event came from.
    pub device_id: MidiDeviceId,
    /// The MIDI message.
    pub message: MidiMessage,
}

/// Map a transport-transparent CC value with the canonical curve semantics.
pub fn map_cc_to_range(
    value: f32,
    in_min: f32,
    in_max: f32,
    out_min: f32,
    out_max: f32,
    curve: &str,
) -> f32 {
    // Normalize input to 0-1
    let normalized = if (in_max - in_min).abs() > f32::EPSILON {
        (value - in_min) / (in_max - in_min)
    } else {
        0.5
    };

    if curve == "logarithmic" && out_min > 0.0 && out_max > 0.0 {
        let t = normalized.clamp(0.0, 1.0);
        if t == 0.0 {
            return out_min;
        }
        if t == 1.0 {
            return out_max;
        }
        return (out_min.ln() + t * (out_max.ln() - out_min.ln())).exp();
    }

    // Apply curves in normalized space. Logarithmic ranges with a
    // non-positive endpoint deliberately fall back to linear.
    let curved = match curve {
        "exponential" => {
            // Exponential curve (steeper at high values)
            normalized * normalized
        }
        "s_curve" | "scurve" => {
            // S-curve (smooth transition)
            let t = normalized.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        }
        _ => normalized, // Linear
    };

    // Map to output range
    out_min + curved.clamp(0.0, 1.0) * (out_max - out_min)
}

/// Map a MIDI 2 per-note value with the legacy curve semantics.
pub fn map_per_note_to_range(
    value: f32,
    in_min: f32,
    in_max: f32,
    out_min: f32,
    out_max: f32,
    curve: &str,
) -> f32 {
    let normalized = if (in_max - in_min).abs() > f32::EPSILON {
        (value - in_min) / (in_max - in_min)
    } else {
        0.5
    };

    let curved = match curve {
        "logarithmic" => {
            if normalized <= 0.0 {
                0.0
            } else {
                (normalized.ln() + 5.0) / 5.0
            }
        }
        "exponential" => normalized * normalized,
        "s_curve" | "scurve" => {
            let t = normalized.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        }
        _ => normalized,
    };

    out_min + curved.clamp(0.0, 1.0) * (out_max - out_min)
}

/// Backward-compatible name for the legacy MIDI 2 per-note evaluator.
pub fn map_to_range(
    value: f32,
    in_min: f32,
    in_max: f32,
    out_min: f32,
    out_max: f32,
    curve: &str,
) -> f32 {
    map_per_note_to_range(value, in_min, in_max, out_min, out_max, curve)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::{ControlValue, GroupChannel, Velocity};

    #[test]
    fn converts_midi2_note_and_cc_to_legacy_callback_shape() {
        let note = NewMidiMessage::Midi2NoteOn {
            group_channel: GroupChannel::new(3, 4),
            note: 60,
            velocity: Velocity::from_midi2(0xFFFF),
            attribute_type: 0,
            attribute_value: 0,
        };

        assert!(matches!(
            convert_new_to_legacy_message(&note),
            Some(MidiMessage::NoteOn {
                channel: 4,
                note: 60,
                velocity: 127
            })
        ));

        let cc = NewMidiMessage::Midi2ControlChange {
            group_channel: GroupChannel::new(3, 4),
            controller: 70,
            value: ControlValue::MAX,
        };

        assert!(matches!(
            convert_new_to_legacy_message(&cc),
            Some(MidiMessage::ControlChange {
                channel: 4,
                cc: 70,
                value: 127
            })
        ));
    }

    #[test]
    fn cc_curve_tables_preserve_geometric_log_and_all_canonical_shapes() {
        let inputs = [0.0, 0.25, 0.5, 0.75, 1.0];
        let cases: [(&str, [f32; 5]); 4] = [
            ("linear", [200.0, 2150.0, 4100.0, 6050.0, 8000.0]),
            ("exponential", [200.0, 687.5, 2150.0, 4587.5, 8000.0]),
            ("s_curve", [200.0, 1418.75, 4100.0, 6781.25, 8000.0]),
            ("logarithmic", [200.0, 502.973, 1264.911, 3181.083, 8000.0]),
        ];

        for (curve, expected) in cases {
            for ((input, expected), index) in inputs.into_iter().zip(expected).zip(0..) {
                let actual = map_cc_to_range(input, 0.0, 1.0, 200.0, 8000.0, curve);
                if index == 0 || index == 4 {
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "{curve}[{index}] endpoint"
                    );
                }
                assert!(
                    (actual - expected).abs() < 0.02,
                    "{curve}[{index}] expected {expected}, got {actual}"
                );
            }
        }
    }

    #[test]
    fn cc_curve_fallbacks_are_finite_linear_and_support_inverted_ranges() {
        let inputs = [0.0, 0.25, 0.5, 0.75, 1.0];
        let unit_linear: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
        let inverted: [f32; 5] = [8000.0, 6050.0, 4100.0, 2150.0, 200.0];

        for ((input, expected), index) in inputs.into_iter().zip(unit_linear).zip(0..) {
            let invalid_log = map_cc_to_range(input, 0.0, 1.0, 0.0, 1.0, "logarithmic");
            let unknown = map_cc_to_range(input, 0.0, 1.0, 0.0, 1.0, "unknown");
            assert_eq!(
                invalid_log.to_bits(),
                expected.to_bits(),
                "invalid log {index}"
            );
            assert_eq!(unknown.to_bits(), expected.to_bits(), "unknown {index}");
            assert!(invalid_log.is_finite());
        }

        for ((input, expected), index) in inputs.into_iter().zip(inverted).zip(0..) {
            let actual = map_cc_to_range(input, 0.0, 1.0, 8000.0, 200.0, "linear");
            assert_eq!(actual.to_bits(), expected.to_bits(), "inverted {index}");
        }

        let inverted_log = map_cc_to_range(0.25, 0.0, 1.0, 8000.0, 200.0, "logarithmic");
        assert!((inverted_log - 3181.083).abs() < 0.02);
        assert!(inverted_log.is_finite());
    }

    #[test]
    fn per_note_log_curve_preserves_legacy_normalized_warp() {
        let unit_mid = map_per_note_to_range(0.5, 0.0, 1.0, 0.0, 1.0, "logarithmic");
        let frequency_mid = map_per_note_to_range(0.5, 0.0, 1.0, 200.0, 8000.0, "logarithmic");

        assert!((unit_mid - 0.861_370_56).abs() < f32::EPSILON);
        assert!((frequency_mid - 6_918.690_4).abs() < 0.001);
        assert_eq!(
            map_per_note_to_range(0.0, 0.0, 1.0, 200.0, 8000.0, "logarithmic").to_bits(),
            200.0f32.to_bits()
        );
        assert_eq!(
            map_per_note_to_range(1.0, 0.0, 1.0, 200.0, 8000.0, "logarithmic").to_bits(),
            8000.0f32.to_bits()
        );
        assert_eq!(
            map_to_range(0.5, 0.0, 1.0, 200.0, 8000.0, "logarithmic").to_bits(),
            frequency_mid.to_bits()
        );
    }
}
