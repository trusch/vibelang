//! System SynthDefs for VibeLang.
//!
//! This module provides pre-built synthdefs for common audio operations:
//!
//! - **MIDI** - MIDI trigger synthdefs for sample-accurate MIDI output
//! - **Recording** - Audio recording and metronome click generation
//! - **Sample** - PlayBuf and Warp1-based sample playback
//! - **SFZ** - SFZ instrument voice synthdefs
//! - **Routing** - Audio bus routing with metering (system_link_audio)
//!
//! All synthdefs use the vibelang-dsp graph builder infrastructure and can be
//! encoded to SuperCollider's scsyndef binary format.

pub mod midi;
pub mod recording;
pub mod routing;
pub mod sample;
pub mod sfz;

pub use midi::{create_midi_synthdefs, trigger_ids, MidiTriggerType};
pub use midi::{
    MIDI_REPLY_CC, MIDI_REPLY_CLOCK, MIDI_REPLY_CONTINUE, MIDI_REPLY_NOTE_OFF, MIDI_REPLY_NOTE_ON,
    MIDI_REPLY_PITCH_BEND, MIDI_REPLY_START, MIDI_REPLY_STOP,
};
pub use recording::create_recording_synthdefs;
pub use routing::{
    create_param_kr_sum_n_bytes, create_port_to_group_link_1_bytes,
    create_port_to_group_link_2_bytes, create_system_link_audio_bytes, PARAM_KR_SUM_MAX,
};
pub use sample::create_sample_synthdefs;
pub use sfz::{create_sfz_synthdefs, create_sfz_voice_synthdef};

/// Create all system synthdefs.
///
/// Returns a vector of (name, encoded_bytes) pairs for all built-in synthdefs.
/// This does NOT include system_link_audio which has a separate creation function
/// due to its special binary encoding.
pub fn create_all_system_synthdefs() -> Vec<(String, Vec<u8>)> {
    let mut all_defs = Vec::new();

    // Add MIDI synthdefs
    all_defs.extend(create_midi_synthdefs());

    // Add recording synthdefs
    all_defs.extend(create_recording_synthdefs());

    // Add sample voice synthdefs
    all_defs.extend(create_sample_synthdefs());

    // Add SFZ synthdefs
    all_defs.extend(create_sfz_synthdefs());

    all_defs
}
