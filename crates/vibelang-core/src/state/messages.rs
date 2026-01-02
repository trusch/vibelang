//! State messages for VibeLang.
//!
//! All state mutations are represented as [`StateMessage`] variants.
//! This enum is the single point of truth for all possible changes
//! to the audio state.

use crate::api::context::SourceLocation;
use crate::events::{BeatEvent, Pattern};
#[cfg(feature = "native")]
use crate::midi::{CcRoute, KeyboardRoute, MidiBackend, MidiDeviceInfo, MidiOutputDeviceInfo, NoteRoute, QueuedMidiEvent};
#[cfg(feature = "native")]
use crossbeam_channel::Sender;
use crate::sequences::{FadeDefinition, SequenceDefinition};
use std::collections::HashMap;
use std::path::PathBuf;

/// Messages sent to the state manager to mutate state.
///
/// All changes to the audio state must go through this enum.
/// This ensures a single source of truth and enables features
/// like undo, replay, and hot-reload.
#[derive(Clone, Debug)]
pub enum StateMessage {
    // === Transport & Tempo ===
    /// Set the tempo in BPM.
    SetBpm { bpm: f64 },

    /// Set the quantization grid in beats.
    SetQuantization { beats: f64 },

    /// Set the time signature.
    SetTimeSignature { numerator: u32, denominator: u32 },

    /// Seek the transport to an absolute beat position.
    SeekTransport { beat: f64 },

    /// Start the scheduler.
    StartScheduler,

    /// Stop the scheduler.
    StopScheduler,

    /// Begin a reload cycle (increments generation).
    BeginReload,

    /// Finalize groups after script execution.
    FinalizeGroups,

    // === SynthDefs ===
    /// Load a synthdef from bytes.
    LoadSynthDef { name: String, bytes: Vec<u8> },

    // === Samples & Buffers ===
    /// Load a sample from a file.
    LoadSample {
        id: String,
        path: String,
        /// Pre-resolved absolute path (resolved on Rhai thread where context is available)
        resolved_path: Option<String>,
        analyze_bpm: bool,
        warp_to_bpm: Option<f64>,
    },

    /// Free a loaded sample.
    FreeSample { id: String },

    // === SFZ Instruments ===
    /// Load an SFZ instrument.
    LoadSfzInstrument { id: String, sfz_path: PathBuf },

    // === VST Instruments ===
    /// Load a VST instrument.
    LoadVstInstrument {
        id: String,
        plugin_key: String,
        group_path: String,
    },

    /// Send MIDI note-on to a VST instrument.
    VstNoteOn {
        instrument_id: String,
        note: u8,
        velocity: u8,
    },

    /// Send MIDI note-off to a VST instrument.
    VstNoteOff { instrument_id: String, note: u8 },

    /// Set a VST parameter by index.
    SetVstParam {
        instrument_id: String,
        param_index: i32,
        value: f32,
    },

    /// Set a VST parameter by name.
    SetVstParamByName {
        instrument_id: String,
        param_name: String,
        value: f32,
    },

    // === Groups ===
    /// Register a new group.
    RegisterGroup {
        name: String,
        path: String,
        parent_path: Option<String>,
        node_id: i32,
        source_location: SourceLocation,
    },

    /// Unregister a group.
    UnregisterGroup { path: String },

    /// Set a group parameter.
    SetGroupParam {
        path: String,
        param: String,
        value: f32,
    },

    /// Fade a group parameter.
    FadeGroupParam {
        path: String,
        param: String,
        target: f32,
        duration: String,
        delay: Option<String>,
        quantize: Option<String>,
    },

    /// Mute a group.
    MuteGroup { path: String },

    /// Unmute a group.
    UnmuteGroup { path: String },

    /// Set global scrub mute state.
    SetScrubMute { muted: bool },

    /// Solo/unsolo a group.
    SoloGroup { path: String, solo: bool },

    // === Voices ===
    /// Create or update a voice.
    UpsertVoice {
        name: String,
        group_path: String,
        group_name: Option<String>,
        synth_name: Option<String>,
        polyphony: i64,
        gain: f64,
        muted: bool,
        soloed: bool,
        output_bus: Option<i64>,
        params: HashMap<String, f32>,
        sfz_instrument: Option<String>,
        vst_instrument: Option<String>,
        source_location: SourceLocation,
        /// MIDI output device ID (if routing to external MIDI hardware).
        midi_output_device_id: Option<u32>,
        /// MIDI channel for output (0-15).
        midi_channel: Option<u8>,
        /// CC mappings: parameter_name -> CC number.
        cc_mappings: HashMap<String, u8>,
    },

    /// Delete a voice.
    DeleteVoice { name: String },

    /// Set a voice parameter.
    SetVoiceParam {
        name: String,
        param: String,
        value: f32,
    },

    /// Fade a voice parameter.
    FadeVoiceParam {
        name: String,
        param: String,
        target: f32,
        duration: String,
        delay: Option<String>,
        quantize: Option<String>,
    },

    /// Mute a voice.
    MuteVoice { name: String },

    /// Unmute a voice.
    UnmuteVoice { name: String },

    /// Trigger a voice (create a synth).
    TriggerVoice {
        name: String,
        synth_name: Option<String>,
        group_path: Option<String>,
        params: Vec<(String, f32)>,
    },

    /// Stop a voice.
    StopVoice { name: String },

    /// Run a voice continuously (for line-in, drones, etc.).
    /// Creates a synth immediately without note triggers.
    RunVoice { name: String },

    /// Send a note-on to a voice.
    NoteOn {
        voice_name: String,
        note: u8,
        velocity: u8,
        duration: Option<f64>,
    },

    /// Send a note-off to a voice.
    NoteOff { voice_name: String, note: u8 },

    /// Send a control change to a voice.
    ControlChange {
        voice_name: String,
        cc_num: u8,
        value: u8,
    },

    // === Patterns ===
    /// Create a pattern.
    CreatePattern {
        name: String,
        group_path: String,
        voice_name: Option<String>,
        pattern: Pattern,
        source_location: SourceLocation,
        /// Original step pattern string for visual editing.
        step_pattern: Option<String>,
    },

    /// Delete a pattern.
    DeletePattern { name: String },

    /// Set a pattern parameter.
    SetPatternParam {
        name: String,
        param: String,
        value: f32,
    },

    /// Fade a pattern parameter.
    FadePatternParam {
        name: String,
        param: String,
        target: f32,
        duration: String,
        delay: Option<String>,
        quantize: Option<String>,
    },

    /// Start a pattern.
    StartPattern { name: String },

    /// Stop a pattern.
    StopPattern { name: String },

    // === Melodies ===
    /// Create a melody.
    CreateMelody {
        name: String,
        group_path: String,
        voice_name: Option<String>,
        pattern: Pattern,
        source_location: SourceLocation,
        /// Original notes pattern strings for visual editing (one per lane).
        /// Multiple lanes support polyphonic melodies.
        notes_patterns: Vec<String>,
    },

    /// Delete a melody.
    DeleteMelody { name: String },

    /// Set a melody parameter.
    SetMelodyParam {
        name: String,
        param: String,
        value: f32,
    },

    /// Fade a melody parameter.
    FadeMelodyParam {
        name: String,
        param: String,
        target: f32,
        duration: String,
        delay: Option<String>,
        quantize: Option<String>,
    },

    /// Start a melody.
    StartMelody { name: String },

    /// Stop a melody.
    StopMelody { name: String },

    // === Fades ===
    /// Create a fade definition.
    CreateFadeDefinition { fade: FadeDefinition },

    // === Sequences ===
    /// Create a sequence.
    CreateSequence { sequence: SequenceDefinition },

    /// Start a sequence.
    StartSequence { name: String },

    /// Start a sequence that plays once (no loop).
    StartSequenceOnce { name: String },

    /// Pause a sequence.
    PauseSequence { name: String },

    /// Resume a sequence.
    ResumeSequence { name: String },

    /// Stop a sequence.
    StopSequence { name: String },

    /// Delete a sequence.
    DeleteSequence { name: String },

    /// Notification that a play_once sequence has completed.
    SequenceCompleted { name: String },

    /// Register a sequence run (for logging).
    RegisterSequenceRun { name: String, anchor_beat: f64 },

    // === Scheduled Events ===
    /// Schedule a one-shot event.
    ScheduleEvent { event: BeatEvent, start_beat: f64 },

    // === Effects ===
    /// Add an effect to a group.
    AddEffect {
        id: String,
        synthdef: String,
        group_path: String,
        params: HashMap<String, f32>,
        bus_in: i32,
        bus_out: i32,
        source_location: SourceLocation,
    },

    /// Remove an effect.
    RemoveEffect { id: String },

    /// Set an effect parameter.
    SetEffectParam {
        id: String,
        param: String,
        value: f32,
    },

    /// Fade an effect parameter.
    FadeEffectParam {
        id: String,
        param: String,
        target: f32,
        duration: String,
        delay: Option<String>,
        quantize: Option<String>,
    },

    /// Cancel an active fade on a target parameter.
    CancelFade {
        target_type: crate::FadeTargetType,
        target_name: String,
        param_name: String,
    },

    // === MIDI Device Management (native only) ===
    #[cfg(feature = "native")]
    /// Open a MIDI device and register it in state.
    /// The device_id is pre-allocated by the API layer.
    MidiOpenDevice {
        device_id: u32,
        info: MidiDeviceInfo,
        backend: MidiBackend,
    },

    #[cfg(feature = "native")]
    /// Close a specific MIDI device.
    MidiCloseDevice { device_id: u32 },

    #[cfg(feature = "native")]
    /// Close all MIDI devices.
    MidiCloseAllDevices,

    // === MIDI Routing (native only) ===
    #[cfg(feature = "native")]
    /// Add a keyboard route (notes to voice).
    MidiAddKeyboardRoute { route: KeyboardRoute },

    #[cfg(feature = "native")]
    /// Add a note-specific route (for drum pads).
    MidiAddNoteRoute {
        channel: Option<u8>,
        note: u8,
        route: NoteRoute,
    },

    #[cfg(feature = "native")]
    /// Add a CC route.
    MidiAddCcRoute {
        channel: Option<u8>,
        cc_number: u8,
        route: CcRoute,
    },

    #[cfg(feature = "native")]
    /// Add a pitch bend route.
    MidiAddPitchBendRoute {
        channel: Option<u8>,
        route: CcRoute,
    },

    #[cfg(feature = "native")]
    /// Clear all MIDI routing (keeps devices connected).
    MidiClearRouting,

    // === MIDI Callbacks (native only) ===
    #[cfg(feature = "native")]
    /// Register a note callback.
    MidiRegisterNoteCallback {
        callback_id: u64,
        channel: Option<u8>,
        note: u8,
        on_note_on: bool,
        on_note_off: bool,
    },

    #[cfg(feature = "native")]
    /// Register a CC callback.
    MidiRegisterCcCallback {
        callback_id: u64,
        channel: Option<u8>,
        cc_number: u8,
        threshold: Option<u8>,
        above_threshold: bool,
    },

    #[cfg(feature = "native")]
    /// Set MIDI monitoring on/off.
    MidiSetMonitoring { enabled: bool },

    // === MIDI Recording (native only) ===
    #[cfg(feature = "native")]
    /// Set MIDI recording quantization (4, 8, 16, 32, 64 positions per bar).
    MidiSetRecordingQuantization { positions_per_bar: u8 },

    #[cfg(feature = "native")]
    /// Enable/disable MIDI recording.
    MidiSetRecordingEnabled { enabled: bool },

    #[cfg(feature = "native")]
    /// Clear all recorded MIDI notes.
    MidiClearRecording,

    // === MIDI Output (native only) ===
    #[cfg(feature = "native")]
    /// Open a MIDI output device.
    ///
    /// Note: MIDI timing is now managed by SuperCollider via SendTrig-based
    /// MIDI trigger synths, so only the immediate event sender is needed.
    MidiOutputOpenDevice {
        device_id: u32,
        info: MidiOutputDeviceInfo,
        event_tx: Sender<QueuedMidiEvent>,
    },

    #[cfg(feature = "native")]
    /// Close a MIDI output device.
    MidiOutputCloseDevice { device_id: u32 },

    #[cfg(feature = "native")]
    /// Close all MIDI output devices.
    MidiOutputCloseAllDevices,

    #[cfg(feature = "native")]
    /// Send a note-on to MIDI output.
    MidiOutputNoteOn {
        device_id: u32,
        channel: u8,
        note: u8,
        velocity: u8,
    },

    #[cfg(feature = "native")]
    /// Send a note-off to MIDI output.
    MidiOutputNoteOff {
        device_id: u32,
        channel: u8,
        note: u8,
    },

    #[cfg(feature = "native")]
    /// Send a control change to MIDI output.
    MidiOutputControlChange {
        device_id: u32,
        channel: u8,
        controller: u8,
        value: u8,
    },

    #[cfg(feature = "native")]
    /// Send a pitch bend to MIDI output.
    MidiOutputPitchBend {
        device_id: u32,
        channel: u8,
        value: i16,
    },

    #[cfg(feature = "native")]
    /// Enable/disable MIDI clock output.
    MidiOutputSetClockEnabled { enabled: bool },

    #[cfg(feature = "native")]
    /// Set the device to send MIDI clock to (None = all devices).
    MidiOutputSetClockDevice { device_id: Option<u32> },

    #[cfg(feature = "native")]
    /// Send MIDI start message.
    MidiOutputSendStart,

    #[cfg(feature = "native")]
    /// Send MIDI stop message.
    MidiOutputSendStop,

    #[cfg(feature = "native")]
    /// Send MIDI continue message.
    MidiOutputSendContinue,

    // === OSC Feedback ===
    /// Node created notification from scsynth.
    NodeCreated {
        node_id: i32,
        group_id: i32,
        is_group: bool,
    },

    /// Node destroyed notification from scsynth.
    NodeDestroyed { node_id: i32 },

    /// Buffer loaded notification from scsynth.
    BufferLoaded { buffer_id: i32 },

    // === Score Capture ===
    /// Enable score capture to a file path.
    EnableScoreCapture { path: PathBuf },

    /// Disable score capture and write the score file.
    DisableScoreCapture,
}

impl StateMessage {
    /// Get a short description of this message type.
    pub fn type_name(&self) -> &'static str {
        match self {
            StateMessage::SetBpm { .. } => "SetBpm",
            StateMessage::SetQuantization { .. } => "SetQuantization",
            StateMessage::SetTimeSignature { .. } => "SetTimeSignature",
            StateMessage::SeekTransport { .. } => "SeekTransport",
            StateMessage::StartScheduler => "StartScheduler",
            StateMessage::StopScheduler => "StopScheduler",
            StateMessage::BeginReload => "BeginReload",
            StateMessage::FinalizeGroups => "FinalizeGroups",
            StateMessage::LoadSynthDef { .. } => "LoadSynthDef",
            StateMessage::LoadSample { .. } => "LoadSample",
            StateMessage::FreeSample { .. } => "FreeSample",
            StateMessage::LoadSfzInstrument { .. } => "LoadSfzInstrument",
            StateMessage::LoadVstInstrument { .. } => "LoadVstInstrument",
            StateMessage::VstNoteOn { .. } => "VstNoteOn",
            StateMessage::VstNoteOff { .. } => "VstNoteOff",
            StateMessage::SetVstParam { .. } => "SetVstParam",
            StateMessage::SetVstParamByName { .. } => "SetVstParamByName",
            StateMessage::RegisterGroup { .. } => "RegisterGroup",
            StateMessage::UnregisterGroup { .. } => "UnregisterGroup",
            StateMessage::SetGroupParam { .. } => "SetGroupParam",
            StateMessage::FadeGroupParam { .. } => "FadeGroupParam",
            StateMessage::MuteGroup { .. } => "MuteGroup",
            StateMessage::UnmuteGroup { .. } => "UnmuteGroup",
            StateMessage::SetScrubMute { .. } => "SetScrubMute",
            StateMessage::SoloGroup { .. } => "SoloGroup",
            StateMessage::UpsertVoice { .. } => "UpsertVoice",
            StateMessage::DeleteVoice { .. } => "DeleteVoice",
            StateMessage::SetVoiceParam { .. } => "SetVoiceParam",
            StateMessage::FadeVoiceParam { .. } => "FadeVoiceParam",
            StateMessage::MuteVoice { .. } => "MuteVoice",
            StateMessage::UnmuteVoice { .. } => "UnmuteVoice",
            StateMessage::TriggerVoice { .. } => "TriggerVoice",
            StateMessage::StopVoice { .. } => "StopVoice",
            StateMessage::RunVoice { .. } => "RunVoice",
            StateMessage::NoteOn { .. } => "NoteOn",
            StateMessage::NoteOff { .. } => "NoteOff",
            StateMessage::ControlChange { .. } => "ControlChange",
            StateMessage::CreatePattern { .. } => "CreatePattern",
            StateMessage::DeletePattern { .. } => "DeletePattern",
            StateMessage::SetPatternParam { .. } => "SetPatternParam",
            StateMessage::FadePatternParam { .. } => "FadePatternParam",
            StateMessage::StartPattern { .. } => "StartPattern",
            StateMessage::StopPattern { .. } => "StopPattern",
            StateMessage::CreateMelody { .. } => "CreateMelody",
            StateMessage::DeleteMelody { .. } => "DeleteMelody",
            StateMessage::SetMelodyParam { .. } => "SetMelodyParam",
            StateMessage::FadeMelodyParam { .. } => "FadeMelodyParam",
            StateMessage::StartMelody { .. } => "StartMelody",
            StateMessage::StopMelody { .. } => "StopMelody",
            StateMessage::CreateFadeDefinition { .. } => "CreateFadeDefinition",
            StateMessage::CreateSequence { .. } => "CreateSequence",
            StateMessage::StartSequence { .. } => "StartSequence",
            StateMessage::StartSequenceOnce { .. } => "StartSequenceOnce",
            StateMessage::PauseSequence { .. } => "PauseSequence",
            StateMessage::ResumeSequence { .. } => "ResumeSequence",
            StateMessage::StopSequence { .. } => "StopSequence",
            StateMessage::DeleteSequence { .. } => "DeleteSequence",
            StateMessage::SequenceCompleted { .. } => "SequenceCompleted",
            StateMessage::RegisterSequenceRun { .. } => "RegisterSequenceRun",
            StateMessage::ScheduleEvent { .. } => "ScheduleEvent",
            StateMessage::AddEffect { .. } => "AddEffect",
            StateMessage::RemoveEffect { .. } => "RemoveEffect",
            StateMessage::SetEffectParam { .. } => "SetEffectParam",
            StateMessage::FadeEffectParam { .. } => "FadeEffectParam",
            StateMessage::CancelFade { .. } => "CancelFade",
            // MIDI variants (native only)
            #[cfg(feature = "native")]
            StateMessage::MidiOpenDevice { .. } => "MidiOpenDevice",
            #[cfg(feature = "native")]
            StateMessage::MidiCloseDevice { .. } => "MidiCloseDevice",
            #[cfg(feature = "native")]
            StateMessage::MidiCloseAllDevices => "MidiCloseAllDevices",
            #[cfg(feature = "native")]
            StateMessage::MidiAddKeyboardRoute { .. } => "MidiAddKeyboardRoute",
            #[cfg(feature = "native")]
            StateMessage::MidiAddNoteRoute { .. } => "MidiAddNoteRoute",
            #[cfg(feature = "native")]
            StateMessage::MidiAddCcRoute { .. } => "MidiAddCcRoute",
            #[cfg(feature = "native")]
            StateMessage::MidiAddPitchBendRoute { .. } => "MidiAddPitchBendRoute",
            #[cfg(feature = "native")]
            StateMessage::MidiClearRouting => "MidiClearRouting",
            #[cfg(feature = "native")]
            StateMessage::MidiRegisterNoteCallback { .. } => "MidiRegisterNoteCallback",
            #[cfg(feature = "native")]
            StateMessage::MidiRegisterCcCallback { .. } => "MidiRegisterCcCallback",
            #[cfg(feature = "native")]
            StateMessage::MidiSetMonitoring { .. } => "MidiSetMonitoring",
            #[cfg(feature = "native")]
            StateMessage::MidiSetRecordingQuantization { .. } => "MidiSetRecordingQuantization",
            #[cfg(feature = "native")]
            StateMessage::MidiSetRecordingEnabled { .. } => "MidiSetRecordingEnabled",
            #[cfg(feature = "native")]
            StateMessage::MidiClearRecording => "MidiClearRecording",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputOpenDevice { .. } => "MidiOutputOpenDevice",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputCloseDevice { .. } => "MidiOutputCloseDevice",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputCloseAllDevices => "MidiOutputCloseAllDevices",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputNoteOn { .. } => "MidiOutputNoteOn",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputNoteOff { .. } => "MidiOutputNoteOff",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputControlChange { .. } => "MidiOutputControlChange",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputPitchBend { .. } => "MidiOutputPitchBend",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputSetClockEnabled { .. } => "MidiOutputSetClockEnabled",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputSetClockDevice { .. } => "MidiOutputSetClockDevice",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputSendStart => "MidiOutputSendStart",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputSendStop => "MidiOutputSendStop",
            #[cfg(feature = "native")]
            StateMessage::MidiOutputSendContinue => "MidiOutputSendContinue",
            StateMessage::NodeCreated { .. } => "NodeCreated",
            StateMessage::NodeDestroyed { .. } => "NodeDestroyed",
            StateMessage::BufferLoaded { .. } => "BufferLoaded",
            StateMessage::EnableScoreCapture { .. } => "EnableScoreCapture",
            StateMessage::DisableScoreCapture => "DisableScoreCapture",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_names() {
        let msg = StateMessage::SetBpm { bpm: 120.0 };
        assert_eq!(msg.type_name(), "SetBpm");

        let msg = StateMessage::StartPattern {
            name: "kick".to_string(),
        };
        assert_eq!(msg.type_name(), "StartPattern");
    }
}
