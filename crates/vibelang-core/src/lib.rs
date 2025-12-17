//! VibeLang Core - State management and core types for the VibeLang music language.
//!
//! This crate provides the fundamental building blocks for VibeLang:
//!
//! - **Timing** - Transport clock, beat time, time signatures
//! - **Events** - Beat events, patterns, melodies, fades
//! - **Sequences** - Declarative clip arrangement system
//! - **State** - Central state model and message passing
//! - **OSC** - Open Sound Control client for SuperCollider communication
//! - **Scheduler** - Beat-based event scheduling engine
//! - **Scsynth** - High-level SuperCollider server API
//! - **API** - Rhai scripting API
//!
//! # Architecture
//!
//! VibeLang uses a message-passing architecture where all state mutations
//! flow through the [`StateMessage`] enum. The [`StateManager`] maintains
//! the single source of truth, while the [`EventScheduler`] handles
//! beat-accurate event timing.

pub mod api;
pub mod events;
pub mod midi;
pub mod osc;
pub mod osc_sender;
pub mod reload;
pub mod runtime;
pub mod sample_synthdef;
pub mod scheduler;
pub mod score;
pub mod scsynth;
pub mod scsynth_process;
pub mod sequences;
pub mod state;
pub mod timing;
pub mod validation;

// Re-export main types for convenience
pub use events::{ActiveFade, BeatEvent, FadeClip, FadeTargetType, Pattern};
pub use osc::OscClient;
pub use osc_sender::{OscSender, OscTiming, ScoreCaptureState};
pub use scheduler::{EventScheduler, LoopKind, LoopSnapshot};
pub use scsynth::{AddAction, BufNum, NodeId, Scsynth, Target};
pub use scsynth_process::ScsynthProcess;
pub use runtime::{Runtime, RuntimeHandle};
pub use score::{ScoreWriter, ScoredEvent, beats_to_seconds, seconds_to_osc_time, extract_synthdef_name};
pub use sequences::{ClipMode, ClipSource, FadeDefinition, SequenceClip, SequenceDefinition};
pub use state::{
    ActiveFadeJob, ActiveSequence, ActiveSynth, EffectState, GroupState,
    LoopStatus, MelodyState, PatternState, SampleInfo, SampleSlice, ScheduledEvent,
    ScheduledNoteOff, ScriptState, SequenceRunLog, StateManager, StateMessage, VoiceState,
    VstInstrumentInfo,
};
pub use timing::{
    Bars, BeatTime, Beats, LatencyCompensation, TimeSignature, TransportClock,
};
pub use midi::{
    CcCallback, CcRoute, CcTarget, JackMidiClient, JackMidiOutput, KeyboardRoute, MidiBackend,
    MidiDeviceInfo, MidiInputManager, MidiMessage, MidiRouting, NoteCallback, NoteRoute,
    ParameterCurve, PendingMidiCallback, QueuedMidiEvent, SharedMidiState, VelocityCurve,
    is_jack_running, list_all_midi_devices, list_jack_midi_sources,
};

// Re-export API module
pub use api::{init_api, get_handle, require_handle, register_api, create_engine, create_engine_with_paths};

// Re-export validation module
pub use validation::{validate_script, ValidationResult, ValidationError, SynthdefReference};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_time_arithmetic() {
        let a = BeatTime::from_float(1.5);
        let b = BeatTime::from_float(2.25);
        let sum = a + b;
        assert!((sum.to_float() - 3.75).abs() < 0.001);
    }

    #[test]
    fn test_time_signature_beats_per_bar() {
        let sig_4_4 = TimeSignature::new(4, 4);
        assert!((sig_4_4.beats_per_bar() - 4.0).abs() < 0.001);

        let sig_3_4 = TimeSignature::new(3, 4);
        assert!((sig_3_4.beats_per_bar() - 3.0).abs() < 0.001);

        let sig_6_8 = TimeSignature::new(6, 8);
        assert!((sig_6_8.beats_per_bar() - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_pattern_creation() {
        let pattern = Pattern {
            name: "test".to_string(),
            events: vec![],
            loop_length_beats: 4.0,
            phase_offset: 0.0,
        };
        assert_eq!(pattern.name, "test");
        assert!((pattern.loop_length_beats - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_sequence_definition() {
        let mut seq = SequenceDefinition::new("test_seq".to_string());
        seq.loop_beats = 16.0;
        seq.clips.push(SequenceClip::new(
            0.0,
            8.0,
            ClipSource::Pattern("kick".to_string()),
            ClipMode::Loop,
        ));
        assert_eq!(seq.name, "test_seq");
        assert_eq!(seq.clips.len(), 1);
    }

    #[test]
    fn test_script_state_defaults() {
        let state = ScriptState::new();
        assert!((state.tempo - 120.0).abs() < 0.001);
        assert!((state.quantization_beats - 4.0).abs() < 0.001);
        assert!(!state.transport_running);
    }

    #[test]
    fn test_node_id_conversions() {
        let id = NodeId::new(42);
        assert_eq!(id.as_i32(), 42);
        assert_eq!(NodeId::auto().as_i32(), -1);
        assert_eq!(NodeId::root().as_i32(), 0);
    }
}
