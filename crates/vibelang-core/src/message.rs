//! Message types for runtime communication.
//!
//! All operations on the runtime are performed via [`Message`]s sent through
//! an async channel. This enables:
//!
//! - Thread-safe operation
//! - Message replay for testing
//! - Undo/redo functionality
//! - Deterministic behavior

pub use crate::mutation::MutationContext;
use crate::state::PatternOwner;
#[cfg(not(target_arch = "wasm32"))]
use crate::traits::RecordingConfig;
use crate::traits::{
    FadeConfig, FadeTarget, MelodyConfig, PatternConfig, SampleConfig, SequenceConfig, VoiceConfig,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::types::RecordingId;
use crate::types::{
    Beat, EffectId, GroupId, MelodyId, ParamMap, PatternId, SampleId, SequenceId, SfzId,
    TimeSignature, VoiceId,
};
use std::path::PathBuf;

// =============================================================================
// Transport Messages
// =============================================================================

/// Transport-related messages for tempo, time signature, and playback control.
#[derive(Clone, Debug)]
pub enum TransportMessage {
    /// Set the tempo in BPM.
    SetTempo { bpm: f64 },

    /// Set the time signature.
    SetTimeSignature { time_sig: TimeSignature },

    /// Seek to a beat position.
    Seek { beat: Beat },

    /// Start playback.
    Start,

    /// Stop playback.
    Stop,
}

// =============================================================================
// SynthDef Messages
// =============================================================================

/// SynthDef loading messages.
#[derive(Clone, Debug)]
pub enum SynthDefMessage {
    /// Load a synthdef from compiled bytes.
    Load { name: String, data: Vec<u8> },
}

// =============================================================================
// Sample Messages
// =============================================================================

/// Sample loading and management messages.
#[derive(Clone, Debug)]
pub enum SampleMessage {
    /// Load a sample from a file.
    Load { id: SampleId, config: SampleConfig },

    /// Free a loaded sample.
    Free { id: SampleId },
}

// =============================================================================
// SFZ Messages
// =============================================================================

/// SFZ instrument loading and management messages.
#[derive(Clone, Debug)]
pub enum SfzMessage {
    /// Load an SFZ instrument from a file.
    ///
    /// This will parse the SFZ file and load all referenced samples.
    Load { id: SfzId, path: PathBuf },

    /// Unload an SFZ instrument.
    ///
    /// This will free all buffers associated with the instrument.
    Unload { id: SfzId },
}

// =============================================================================
// Recording Messages (native only - recording requires filesystem)
// =============================================================================

/// Audio recording messages for capturing audio to buffers/files.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub enum RecordingMessage {
    /// Start a new recording session.
    ///
    /// Records audio from a group's bus into a buffer, with optional
    /// quantized start, count-in, metronome, and file saving.
    Start {
        id: RecordingId,
        config: RecordingConfig,
    },

    /// Stop an active recording early.
    ///
    /// The recorded audio up to this point is preserved.
    Stop { id: RecordingId },

    /// Cancel a pending or active recording.
    ///
    /// Discards recorded audio and frees the buffer.
    Cancel { id: RecordingId },

    /// Internal: Mark buffer as allocated (received from backend).
    BufferAllocated { id: RecordingId },

    /// Internal: Recording has completed (end beat reached).
    Completed { id: RecordingId },
}

// =============================================================================
// Group Messages
// =============================================================================

/// Group management messages for hierarchical audio routing.
#[derive(Clone, Debug)]
pub enum GroupMessage {
    /// Create a new group.
    Create {
        id: GroupId,
        name: String,
        parent: Option<GroupId>,
    },

    /// Delete a group.
    Delete { id: GroupId },

    /// Set a group parameter.
    SetParam {
        id: GroupId,
        param: String,
        value: f32,
    },

    /// Mute or unmute a group.
    Mute { id: GroupId, muted: bool },

    /// Solo or unsolo a group.
    Solo { id: GroupId, solo: bool },

    /// Finalize groups by creating link synths for audio routing.
    ///
    /// This should be called after all groups are created.
    /// It creates `system_link_audio` synths that route audio from each
    /// group's bus to its parent's bus (or the main output for top-level groups).
    Finalize,
}

// =============================================================================
// Voice Messages
// =============================================================================

/// Voice messages for sound-producing units.
#[derive(Clone, Debug)]
pub enum VoiceMessage {
    /// Create a new voice.
    Create {
        id: VoiceId,
        config: Box<VoiceConfig>,
    },

    /// Delete a voice.
    Delete { id: VoiceId },

    /// Trigger a voice with parameters.
    Trigger { id: VoiceId, params: ParamMap },

    /// Stop a voice.
    Stop { id: VoiceId },

    /// Send a note-on to a voice.
    NoteOn {
        voice: VoiceId,
        note: u8,
        velocity: f32,
    },

    /// Send a note-off to a voice.
    NoteOff { voice: VoiceId, note: u8 },

    /// Mute or unmute a voice.
    Mute { id: VoiceId, muted: bool },

    /// Set a voice parameter on all active synths.
    SetParam {
        id: VoiceId,
        param: String,
        value: f32,
    },
}

// =============================================================================
// Pattern Messages
// =============================================================================

/// Pattern messages for rhythmic sequences.
#[derive(Clone, Debug)]
pub enum PatternMessage {
    /// Create a new pattern.
    Create {
        id: PatternId,
        config: PatternConfig,
        owner: PatternOwner,
    },

    /// Delete a pattern.
    Delete { id: PatternId },

    /// Start a pattern.
    Start { id: PatternId },

    /// Start a pattern phase-locked to the song grid (anchors beat 0 to the
    /// most recent multiple of the pattern's own length). Used by the looper
    /// so a captured loop drops in aligned to the transport bar grid instead
    /// of at the arbitrary instant playback happened to be triggered.
    StartOnGrid { id: PatternId },

    /// Stop a pattern.
    Stop { id: PatternId },

    /// Set a pattern parameter.
    SetParam {
        id: PatternId,
        param: String,
        value: f32,
    },
}

// =============================================================================
// Melody Messages
// =============================================================================

/// Melody messages for pitched sequences.
#[derive(Clone, Debug)]
pub enum MelodyMessage {
    /// Create a new melody.
    Create { id: MelodyId, config: MelodyConfig },

    /// Delete a melody.
    Delete { id: MelodyId },

    /// Start a melody.
    Start { id: MelodyId },

    /// Stop a melody.
    Stop { id: MelodyId },
}

// =============================================================================
// Sequence Messages
// =============================================================================

/// Sequence messages for arrangements of clips.
#[derive(Clone, Debug)]
pub enum SequenceMessage {
    /// Create a new sequence.
    Create {
        id: SequenceId,
        config: SequenceConfig,
    },

    /// Delete a sequence.
    Delete { id: SequenceId },

    /// Start a sequence.
    Start { id: SequenceId, looping: bool },

    /// Stop a sequence.
    Stop { id: SequenceId },

    /// Pause a sequence.
    Pause { id: SequenceId },

    /// Resume a paused sequence.
    Resume { id: SequenceId },
}

// =============================================================================
// Effect Messages
// =============================================================================

/// Effect messages for audio processing.
#[derive(Clone, Debug)]
pub enum EffectMessage {
    /// Add an effect to a group.
    Add {
        id: EffectId,
        group: GroupId,
        synthdef: String,
        params: ParamMap,
    },

    /// Remove an effect.
    Remove { id: EffectId },

    /// Set an effect parameter.
    SetParam {
        id: EffectId,
        param: String,
        value: f32,
    },
}

// =============================================================================
// Fade Messages
// =============================================================================

/// Fade messages for parameter automation.
#[derive(Clone, Debug)]
pub enum FadeMessage {
    /// Start a parameter fade.
    Start { config: FadeConfig },

    /// Cancel a fade.
    Cancel { target: FadeTarget, param: String },
}

// =============================================================================
// MIDI Messages (feature-gated)
// =============================================================================

// =============================================================================
// Reload Messages
// =============================================================================

/// Live reload messages for hot-reloading scripts.
///
/// These messages enable updating the runtime state without stopping playback.
/// The reload system calculates the minimal set of changes needed and applies
/// them incrementally.
#[derive(Clone, Debug)]
pub enum ReloadMessage {
    /// Apply a new script state.
    ///
    /// The runtime will diff the new state against the current state and
    /// apply only the necessary changes (create, update, delete entities).
    Apply { state: crate::reload::ScriptState },

    /// Internal: apply a reload whose expensive buffer loads (samples,
    /// SFZ instruments) were already staged off the runtime task.
    ///
    /// Sent by the runtime's own staging task once all loads completed;
    /// external callers should send [`ReloadMessage::Apply`], which stages
    /// automatically when needed.
    ApplyStaged {
        /// The script state to apply.
        state: crate::reload::ScriptState,
        /// Pre-loaded buffer assets consumed during the apply.
        assets: crate::reload::StagedReloadAssets,
    },
}

// =============================================================================
// MIDI Messages (feature-gated)
// =============================================================================

/// MIDI messages for external MIDI device control.
#[cfg(feature = "midi")]
#[derive(Clone, Debug)]
pub enum MidiMessage {
    // =========================================================================
    // Device Management
    // =========================================================================
    /// Open a MIDI input device.
    OpenInput {
        device: crate::types::ids::MidiDeviceId,
    },

    /// Open a MIDI output device.
    OpenOutput {
        device: crate::types::ids::MidiDeviceId,
    },

    /// Close a MIDI device (input or output).
    CloseDevice {
        device: crate::types::ids::MidiDeviceId,
    },

    /// Reconcile open PipeWire MIDI inputs against the set of devices
    /// currently present on the system. Emitted periodically by the MIDI
    /// hot-plug watcher thread with a fresh device snapshot; the runtime
    /// opens requested devices that have (re)appeared and closes ones that
    /// have vanished, so inputs recover from unplug/replug and power-on.
    ReconcileInputs {
        present: std::collections::HashSet<crate::types::ids::MidiDeviceId>,
    },

    // =========================================================================
    // Note/CC Output
    // =========================================================================
    /// Send a MIDI note-on.
    NoteOn {
        device: crate::types::ids::MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
    },

    /// Send a MIDI note-off.
    NoteOff {
        device: crate::types::ids::MidiDeviceId,
        channel: u8,
        note: u8,
    },

    /// Send a MIDI control change.
    Cc {
        device: crate::types::ids::MidiDeviceId,
        channel: u8,
        cc: u8,
        value: u8,
    },

    /// Send a MIDI note-on (alternate naming for HTTP API).
    SendNoteOn {
        device: crate::types::ids::MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
    },

    /// Send a MIDI note-off (alternate naming for HTTP API).
    SendNoteOff {
        device: crate::types::ids::MidiDeviceId,
        channel: u8,
        note: u8,
    },

    /// Send a MIDI CC (alternate naming for HTTP API).
    SendCC {
        device: crate::types::ids::MidiDeviceId,
        channel: u8,
        cc: u8,
        value: u8,
    },

    // =========================================================================
    // Recording
    // =========================================================================
    /// Start recording MIDI input from a device.
    StartRecording {
        device: crate::types::ids::MidiDeviceId,
    },

    /// Start recording MIDI input from a device on a specific channel.
    StartRecordingChannel {
        device: crate::types::ids::MidiDeviceId,
        channel: u8,
    },

    /// Stop recording.
    /// The recording can be retrieved via a separate query or handled asynchronously.
    StopRecording {
        device: crate::types::ids::MidiDeviceId,
    },

    // =========================================================================
    // Clock Output
    // =========================================================================
    /// Enable MIDI clock output to a device.
    EnableClockOutput {
        device: crate::types::ids::MidiDeviceId,
    },

    /// Disable MIDI clock output from a device.
    DisableClockOutput {
        device: crate::types::ids::MidiDeviceId,
    },

    /// Send MIDI Start message.
    SendStart {
        device: crate::types::ids::MidiDeviceId,
    },

    /// Send MIDI Stop message.
    SendStop {
        device: crate::types::ids::MidiDeviceId,
    },

    /// Send MIDI Continue message.
    SendContinue {
        device: crate::types::ids::MidiDeviceId,
    },

    // =========================================================================
    // Route Management
    // =========================================================================
    /// Add a keyboard route mapping MIDI input to a voice.
    AddKeyboardRoute {
        device: crate::types::ids::MidiDeviceId,
        voice: crate::types::VoiceId,
        channel: Option<u8>,
        note_min: Option<u8>,
        note_max: Option<u8>,
        transpose: Option<i8>,
    },

    /// Remove a keyboard route by index.
    RemoveKeyboardRoute { index: usize },

    /// Clear all MIDI routes.
    ClearRoutes,
}

// =============================================================================
// Sync Messages
// =============================================================================

/// Synchronization messages for ensuring operations complete.
#[derive(Debug)]
pub enum SyncMessage {
    /// Synchronize with the backend and notify when complete.
    ///
    /// This ensures all prior messages have been processed and the backend
    /// is synced (scsynth has processed all pending commands).
    /// The oneshot sender is used to signal completion.
    SyncAndNotify {
        /// Sender to notify when sync is complete.
        notify: crate::compat::OneshotSender<std::result::Result<(), String>>,
    },
}

// Manual Clone implementation that panics (SyncMessage shouldn't be cloned)
impl Clone for SyncMessage {
    fn clone(&self) -> Self {
        panic!("SyncMessage cannot be cloned - it contains a oneshot sender")
    }
}

// =============================================================================
// Top-Level Message Enum
// =============================================================================

/// All messages the runtime can process.
///
/// Messages are the single way to mutate runtime state. The runtime receives
/// messages through an async channel and dispatches them to the appropriate
/// handler.
///
/// # Categories
///
/// - **Transport**: Tempo, time signature, playback control
/// - **SynthDef**: Loading synthesis definitions
/// - **Sample**: Loading and managing audio samples
/// - **Group**: Hierarchical audio routing
/// - **Voice**: Sound-producing units
/// - **Pattern**: Rhythmic sequences
/// - **Melody**: Pitched sequences
/// - **Sequence**: Arrangements of clips
/// - **Effect**: Audio processing
/// - **Fade**: Parameter automation
/// - **Sync**: Backend synchronization
/// - **Midi**: External MIDI device control (feature-gated)
#[derive(Clone, Debug)]
pub enum Message {
    /// Transport control messages.
    Transport(TransportMessage),

    /// SynthDef loading messages.
    SynthDef(SynthDefMessage),

    /// Sample management messages.
    Sample(SampleMessage),

    /// SFZ instrument management messages.
    Sfz(SfzMessage),

    /// Recording control messages (native only).
    #[cfg(not(target_arch = "wasm32"))]
    Recording(RecordingMessage),

    /// Group management messages.
    Group(GroupMessage),

    /// Voice control messages.
    Voice(VoiceMessage),

    /// Pattern control messages.
    Pattern(PatternMessage),

    /// Melody control messages.
    Melody(MelodyMessage),

    /// Sequence control messages.
    Sequence(SequenceMessage),

    /// Effect control messages.
    Effect(EffectMessage),

    /// Fade control messages.
    Fade(FadeMessage),

    /// Reload control messages for live hot-reloading.
    ///
    /// Boxed because ScriptState is large (~600 bytes), and we want to keep
    /// the Message enum small for efficient passing through channels.
    Reload(Box<ReloadMessage>),

    /// Synchronization messages for backend sync operations.
    Sync(SyncMessage),

    /// MIDI control messages (feature-gated).
    #[cfg(feature = "midi")]
    Midi(MidiMessage),
}

/// How one of the 74 v1 runtime message variants participates in mutation
/// receipt processing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageClass {
    /// A caller-originated mutation that owns a canonical attempt/revision.
    ReceiptBearingMutation,
    /// An internal completion that must retain its parent's mutation context.
    ReceiptLinkedCompletion,
    /// Runtime maintenance that is correlated but does not claim a user effect.
    Internal,
}

/// Exhaustive classification of the native default-feature v1 message surface.
///
/// This is deliberately a static catalog rather than a count derived from
/// constructed messages, so adding, removing, or silently reclassifying an
/// enum variant requires an explicit contract update.
#[cfg(all(not(target_arch = "wasm32"), feature = "midi"))]
pub const NATIVE_DEFAULT_MESSAGE_CLASSIFICATION: [(&str, MessageClass); 74] = [
    ("Transport::SetTempo", MessageClass::ReceiptBearingMutation),
    (
        "Transport::SetTimeSignature",
        MessageClass::ReceiptBearingMutation,
    ),
    ("Transport::Seek", MessageClass::ReceiptBearingMutation),
    ("Transport::Start", MessageClass::ReceiptBearingMutation),
    ("Transport::Stop", MessageClass::ReceiptBearingMutation),
    ("SynthDef::Load", MessageClass::ReceiptBearingMutation),
    ("Sample::Load", MessageClass::ReceiptBearingMutation),
    ("Sample::Free", MessageClass::ReceiptBearingMutation),
    ("Sfz::Load", MessageClass::ReceiptBearingMutation),
    ("Sfz::Unload", MessageClass::ReceiptBearingMutation),
    ("Recording::Start", MessageClass::ReceiptBearingMutation),
    ("Recording::Stop", MessageClass::ReceiptBearingMutation),
    ("Recording::Cancel", MessageClass::ReceiptBearingMutation),
    (
        "Recording::BufferAllocated",
        MessageClass::ReceiptLinkedCompletion,
    ),
    (
        "Recording::Completed",
        MessageClass::ReceiptLinkedCompletion,
    ),
    ("Group::Create", MessageClass::ReceiptBearingMutation),
    ("Group::Delete", MessageClass::ReceiptBearingMutation),
    ("Group::SetParam", MessageClass::ReceiptBearingMutation),
    ("Group::Mute", MessageClass::ReceiptBearingMutation),
    ("Group::Solo", MessageClass::ReceiptBearingMutation),
    ("Group::Finalize", MessageClass::ReceiptBearingMutation),
    ("Voice::Create", MessageClass::ReceiptBearingMutation),
    ("Voice::Delete", MessageClass::ReceiptBearingMutation),
    ("Voice::Trigger", MessageClass::ReceiptBearingMutation),
    ("Voice::Stop", MessageClass::ReceiptBearingMutation),
    ("Voice::NoteOn", MessageClass::ReceiptBearingMutation),
    ("Voice::NoteOff", MessageClass::ReceiptBearingMutation),
    ("Voice::Mute", MessageClass::ReceiptBearingMutation),
    ("Voice::SetParam", MessageClass::ReceiptBearingMutation),
    ("Pattern::Create", MessageClass::ReceiptBearingMutation),
    ("Pattern::Delete", MessageClass::ReceiptBearingMutation),
    ("Pattern::Start", MessageClass::ReceiptBearingMutation),
    ("Pattern::StartOnGrid", MessageClass::ReceiptBearingMutation),
    ("Pattern::Stop", MessageClass::ReceiptBearingMutation),
    ("Pattern::SetParam", MessageClass::ReceiptBearingMutation),
    ("Melody::Create", MessageClass::ReceiptBearingMutation),
    ("Melody::Delete", MessageClass::ReceiptBearingMutation),
    ("Melody::Start", MessageClass::ReceiptBearingMutation),
    ("Melody::Stop", MessageClass::ReceiptBearingMutation),
    ("Sequence::Create", MessageClass::ReceiptBearingMutation),
    ("Sequence::Delete", MessageClass::ReceiptBearingMutation),
    ("Sequence::Start", MessageClass::ReceiptBearingMutation),
    ("Sequence::Stop", MessageClass::ReceiptBearingMutation),
    ("Sequence::Pause", MessageClass::ReceiptBearingMutation),
    ("Sequence::Resume", MessageClass::ReceiptBearingMutation),
    ("Effect::Add", MessageClass::ReceiptBearingMutation),
    ("Effect::Remove", MessageClass::ReceiptBearingMutation),
    ("Effect::SetParam", MessageClass::ReceiptBearingMutation),
    ("Fade::Start", MessageClass::ReceiptBearingMutation),
    ("Fade::Cancel", MessageClass::ReceiptBearingMutation),
    ("Reload::Apply", MessageClass::ReceiptBearingMutation),
    ("Reload::ApplyStaged", MessageClass::ReceiptLinkedCompletion),
    ("Sync::SyncAndNotify", MessageClass::ReceiptBearingMutation),
    ("Midi::OpenInput", MessageClass::ReceiptBearingMutation),
    ("Midi::OpenOutput", MessageClass::ReceiptBearingMutation),
    ("Midi::CloseDevice", MessageClass::ReceiptBearingMutation),
    ("Midi::ReconcileInputs", MessageClass::Internal),
    ("Midi::NoteOn", MessageClass::ReceiptBearingMutation),
    ("Midi::NoteOff", MessageClass::ReceiptBearingMutation),
    ("Midi::Cc", MessageClass::ReceiptBearingMutation),
    ("Midi::SendNoteOn", MessageClass::ReceiptBearingMutation),
    ("Midi::SendNoteOff", MessageClass::ReceiptBearingMutation),
    ("Midi::SendCC", MessageClass::ReceiptBearingMutation),
    ("Midi::StartRecording", MessageClass::ReceiptBearingMutation),
    (
        "Midi::StartRecordingChannel",
        MessageClass::ReceiptBearingMutation,
    ),
    ("Midi::StopRecording", MessageClass::ReceiptBearingMutation),
    (
        "Midi::EnableClockOutput",
        MessageClass::ReceiptBearingMutation,
    ),
    (
        "Midi::DisableClockOutput",
        MessageClass::ReceiptBearingMutation,
    ),
    ("Midi::SendStart", MessageClass::ReceiptBearingMutation),
    ("Midi::SendStop", MessageClass::ReceiptBearingMutation),
    ("Midi::SendContinue", MessageClass::ReceiptBearingMutation),
    (
        "Midi::AddKeyboardRoute",
        MessageClass::ReceiptBearingMutation,
    ),
    (
        "Midi::RemoveKeyboardRoute",
        MessageClass::ReceiptBearingMutation,
    ),
    ("Midi::ClearRoutes", MessageClass::ReceiptBearingMutation),
];

/// A runtime message paired with the mutation identity that admitted it.
///
/// Keeping the envelope outside [`Message`] preserves the exact 74-variant v1
/// message denominator and all existing message construction call sites while
/// making context loss at queue, staging, completion, and sync boundaries
/// impossible inside the runtime.
#[derive(Clone, Debug)]
pub struct ContextualMessage {
    pub context: MutationContext,
    pub message: Message,
}

impl ContextualMessage {
    #[must_use]
    pub const fn new(context: MutationContext, message: Message) -> Self {
        Self { context, message }
    }
}

impl Message {
    /// Get a short name for this message type.
    pub fn type_name(&self) -> &'static str {
        match self {
            Message::Transport(msg) => match msg {
                TransportMessage::SetTempo { .. } => "Transport::SetTempo",
                TransportMessage::SetTimeSignature { .. } => "Transport::SetTimeSignature",
                TransportMessage::Seek { .. } => "Transport::Seek",
                TransportMessage::Start => "Transport::Start",
                TransportMessage::Stop => "Transport::Stop",
            },
            Message::SynthDef(msg) => match msg {
                SynthDefMessage::Load { .. } => "SynthDef::Load",
            },
            Message::Sample(msg) => match msg {
                SampleMessage::Load { .. } => "Sample::Load",
                SampleMessage::Free { .. } => "Sample::Free",
            },
            Message::Sfz(msg) => match msg {
                SfzMessage::Load { .. } => "Sfz::Load",
                SfzMessage::Unload { .. } => "Sfz::Unload",
            },
            #[cfg(not(target_arch = "wasm32"))]
            Message::Recording(msg) => match msg {
                RecordingMessage::Start { .. } => "Recording::Start",
                RecordingMessage::Stop { .. } => "Recording::Stop",
                RecordingMessage::Cancel { .. } => "Recording::Cancel",
                RecordingMessage::BufferAllocated { .. } => "Recording::BufferAllocated",
                RecordingMessage::Completed { .. } => "Recording::Completed",
            },
            Message::Group(msg) => match msg {
                GroupMessage::Create { .. } => "Group::Create",
                GroupMessage::Delete { .. } => "Group::Delete",
                GroupMessage::SetParam { .. } => "Group::SetParam",
                GroupMessage::Mute { .. } => "Group::Mute",
                GroupMessage::Solo { .. } => "Group::Solo",
                GroupMessage::Finalize => "Group::Finalize",
            },
            Message::Voice(msg) => match msg {
                VoiceMessage::Create { .. } => "Voice::Create",
                VoiceMessage::Delete { .. } => "Voice::Delete",
                VoiceMessage::Trigger { .. } => "Voice::Trigger",
                VoiceMessage::Stop { .. } => "Voice::Stop",
                VoiceMessage::NoteOn { .. } => "Voice::NoteOn",
                VoiceMessage::NoteOff { .. } => "Voice::NoteOff",
                VoiceMessage::Mute { .. } => "Voice::Mute",
                VoiceMessage::SetParam { .. } => "Voice::SetParam",
            },
            Message::Pattern(msg) => match msg {
                PatternMessage::Create { .. } => "Pattern::Create",
                PatternMessage::Delete { .. } => "Pattern::Delete",
                PatternMessage::Start { .. } => "Pattern::Start",
                PatternMessage::StartOnGrid { .. } => "Pattern::StartOnGrid",
                PatternMessage::Stop { .. } => "Pattern::Stop",
                PatternMessage::SetParam { .. } => "Pattern::SetParam",
            },
            Message::Melody(msg) => match msg {
                MelodyMessage::Create { .. } => "Melody::Create",
                MelodyMessage::Delete { .. } => "Melody::Delete",
                MelodyMessage::Start { .. } => "Melody::Start",
                MelodyMessage::Stop { .. } => "Melody::Stop",
            },
            Message::Sequence(msg) => match msg {
                SequenceMessage::Create { .. } => "Sequence::Create",
                SequenceMessage::Delete { .. } => "Sequence::Delete",
                SequenceMessage::Start { .. } => "Sequence::Start",
                SequenceMessage::Stop { .. } => "Sequence::Stop",
                SequenceMessage::Pause { .. } => "Sequence::Pause",
                SequenceMessage::Resume { .. } => "Sequence::Resume",
            },
            Message::Effect(msg) => match msg {
                EffectMessage::Add { .. } => "Effect::Add",
                EffectMessage::Remove { .. } => "Effect::Remove",
                EffectMessage::SetParam { .. } => "Effect::SetParam",
            },
            Message::Fade(msg) => match msg {
                FadeMessage::Start { .. } => "Fade::Start",
                FadeMessage::Cancel { .. } => "Fade::Cancel",
            },
            Message::Reload(msg) => match msg.as_ref() {
                ReloadMessage::Apply { .. } => "Reload::Apply",
                ReloadMessage::ApplyStaged { .. } => "Reload::ApplyStaged",
            },
            Message::Sync(msg) => match msg {
                SyncMessage::SyncAndNotify { .. } => "Sync::SyncAndNotify",
            },
            #[cfg(feature = "midi")]
            Message::Midi(msg) => match msg {
                MidiMessage::OpenInput { .. } => "Midi::OpenInput",
                MidiMessage::OpenOutput { .. } => "Midi::OpenOutput",
                MidiMessage::CloseDevice { .. } => "Midi::CloseDevice",
                MidiMessage::ReconcileInputs { .. } => "Midi::ReconcileInputs",
                MidiMessage::NoteOn { .. } => "Midi::NoteOn",
                MidiMessage::NoteOff { .. } => "Midi::NoteOff",
                MidiMessage::Cc { .. } => "Midi::Cc",
                MidiMessage::SendNoteOn { .. } => "Midi::SendNoteOn",
                MidiMessage::SendNoteOff { .. } => "Midi::SendNoteOff",
                MidiMessage::SendCC { .. } => "Midi::SendCC",
                MidiMessage::StartRecording { .. } => "Midi::StartRecording",
                MidiMessage::StartRecordingChannel { .. } => "Midi::StartRecordingChannel",
                MidiMessage::StopRecording { .. } => "Midi::StopRecording",
                MidiMessage::EnableClockOutput { .. } => "Midi::EnableClockOutput",
                MidiMessage::DisableClockOutput { .. } => "Midi::DisableClockOutput",
                MidiMessage::SendStart { .. } => "Midi::SendStart",
                MidiMessage::SendStop { .. } => "Midi::SendStop",
                MidiMessage::SendContinue { .. } => "Midi::SendContinue",
                MidiMessage::AddKeyboardRoute { .. } => "Midi::AddKeyboardRoute",
                MidiMessage::RemoveKeyboardRoute { .. } => "Midi::RemoveKeyboardRoute",
                MidiMessage::ClearRoutes => "Midi::ClearRoutes",
            },
        }
    }

    /// Canonical receipt classification for this exact message variant.
    #[must_use]
    pub fn class(&self) -> MessageClass {
        #[cfg(all(not(target_arch = "wasm32"), feature = "midi"))]
        {
            let name = self.type_name();
            return NATIVE_DEFAULT_MESSAGE_CLASSIFICATION
                .iter()
                .find_map(|(candidate, class)| (*candidate == name).then_some(*class))
                .unwrap_or_else(|| panic!("native default message {name} is not classified"));
        }
        #[cfg(any(target_arch = "wasm32", not(feature = "midi")))]
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Message::Recording(
                RecordingMessage::BufferAllocated { .. } | RecordingMessage::Completed { .. },
            ) => MessageClass::ReceiptLinkedCompletion,
            Message::Reload(reload_msg)
                if matches!(reload_msg.as_ref(), ReloadMessage::ApplyStaged { .. }) =>
            {
                MessageClass::ReceiptLinkedCompletion
            }
            #[cfg(feature = "midi")]
            Message::Midi(MidiMessage::ReconcileInputs { .. }) => MessageClass::Internal,
            _ => MessageClass::ReceiptBearingMutation,
        }
    }

    /// Canonical mutation domain for receipt construction.
    #[must_use]
    pub const fn domain(&self) -> crate::mutation::MessageDomain {
        match self {
            Message::Transport(_) => crate::mutation::MessageDomain::Transport,
            Message::SynthDef(_) => crate::mutation::MessageDomain::SynthDef,
            Message::Sample(_) => crate::mutation::MessageDomain::Sample,
            Message::Sfz(_) => crate::mutation::MessageDomain::Sfz,
            #[cfg(not(target_arch = "wasm32"))]
            Message::Recording(_) => crate::mutation::MessageDomain::Recording,
            Message::Group(_) => crate::mutation::MessageDomain::Group,
            Message::Voice(_) => crate::mutation::MessageDomain::Voice,
            Message::Pattern(_) => crate::mutation::MessageDomain::Pattern,
            Message::Melody(_) => crate::mutation::MessageDomain::Melody,
            Message::Sequence(_) => crate::mutation::MessageDomain::Sequence,
            Message::Effect(_) => crate::mutation::MessageDomain::Effect,
            Message::Fade(_) => crate::mutation::MessageDomain::Fade,
            Message::Reload(_) => crate::mutation::MessageDomain::Reload,
            Message::Sync(_) => crate::mutation::MessageDomain::Sync,
            #[cfg(feature = "midi")]
            Message::Midi(_) => crate::mutation::MessageDomain::Midi,
        }
    }

    /// Stable operation spelling used by the canonical command identity.
    #[must_use]
    pub fn operation(&self) -> &'static str {
        self.type_name()
            .split_once("::")
            .map_or(self.type_name(), |(_, operation)| operation)
    }

    /// Stable component path used by focused v1 best-effort receipts.
    #[must_use]
    pub fn component_path(&self) -> String {
        format!(
            "message/{}",
            self.type_name().replace("::", "/").to_lowercase()
        )
    }
}

// =============================================================================
// Convenient From implementations
// =============================================================================

impl From<TransportMessage> for Message {
    fn from(msg: TransportMessage) -> Self {
        Message::Transport(msg)
    }
}

impl From<SynthDefMessage> for Message {
    fn from(msg: SynthDefMessage) -> Self {
        Message::SynthDef(msg)
    }
}

impl From<SampleMessage> for Message {
    fn from(msg: SampleMessage) -> Self {
        Message::Sample(msg)
    }
}

impl From<SfzMessage> for Message {
    fn from(msg: SfzMessage) -> Self {
        Message::Sfz(msg)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<RecordingMessage> for Message {
    fn from(msg: RecordingMessage) -> Self {
        Message::Recording(msg)
    }
}

impl From<GroupMessage> for Message {
    fn from(msg: GroupMessage) -> Self {
        Message::Group(msg)
    }
}

impl From<VoiceMessage> for Message {
    fn from(msg: VoiceMessage) -> Self {
        Message::Voice(msg)
    }
}

impl From<PatternMessage> for Message {
    fn from(msg: PatternMessage) -> Self {
        Message::Pattern(msg)
    }
}

impl From<MelodyMessage> for Message {
    fn from(msg: MelodyMessage) -> Self {
        Message::Melody(msg)
    }
}

impl From<SequenceMessage> for Message {
    fn from(msg: SequenceMessage) -> Self {
        Message::Sequence(msg)
    }
}

impl From<EffectMessage> for Message {
    fn from(msg: EffectMessage) -> Self {
        Message::Effect(msg)
    }
}

impl From<FadeMessage> for Message {
    fn from(msg: FadeMessage) -> Self {
        Message::Fade(msg)
    }
}

impl From<ReloadMessage> for Message {
    fn from(msg: ReloadMessage) -> Self {
        Message::Reload(Box::new(msg))
    }
}

impl From<SyncMessage> for Message {
    fn from(msg: SyncMessage) -> Self {
        Message::Sync(msg)
    }
}

#[cfg(feature = "midi")]
impl From<MidiMessage> for Message {
    fn from(msg: MidiMessage) -> Self {
        Message::Midi(msg)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(not(target_arch = "wasm32"), feature = "midi"))]
    #[test]
    fn native_default_message_classification_is_exact_and_unique() {
        let mut names = std::collections::HashSet::new();
        let mut receipt_bearing = 0;
        let mut linked = 0;
        let mut internal = 0;
        for (name, class) in NATIVE_DEFAULT_MESSAGE_CLASSIFICATION {
            assert!(
                names.insert(name),
                "duplicate message classification: {name}"
            );
            match class {
                MessageClass::ReceiptBearingMutation => receipt_bearing += 1,
                MessageClass::ReceiptLinkedCompletion => linked += 1,
                MessageClass::Internal => internal += 1,
            }
        }
        assert_eq!(names.len(), 74);
        assert_eq!(receipt_bearing, 70);
        assert_eq!(linked, 3);
        assert_eq!(internal, 1);
    }

    #[test]
    fn test_message_type_name() {
        let msg = Message::Transport(TransportMessage::SetTempo { bpm: 120.0 });
        assert_eq!(msg.type_name(), "Transport::SetTempo");

        let msg = Message::Transport(TransportMessage::Start);
        assert_eq!(msg.type_name(), "Transport::Start");

        let msg = Message::Group(GroupMessage::Finalize);
        assert_eq!(msg.type_name(), "Group::Finalize");
    }

    #[test]
    fn test_message_clone() {
        let msg = Message::Transport(TransportMessage::SetTempo { bpm: 140.0 });
        let cloned = msg.clone();
        assert_eq!(cloned.type_name(), "Transport::SetTempo");
    }

    #[test]
    fn test_from_impl() {
        let transport_msg = TransportMessage::Start;
        let msg: Message = transport_msg.into();
        assert_eq!(msg.type_name(), "Transport::Start");

        let group_msg = GroupMessage::Finalize;
        let msg: Message = group_msg.into();
        assert_eq!(msg.type_name(), "Group::Finalize");
    }

    #[test]
    fn test_all_transport_messages() {
        let msgs = vec![
            TransportMessage::SetTempo { bpm: 120.0 },
            TransportMessage::SetTimeSignature {
                time_sig: TimeSignature::default(),
            },
            TransportMessage::Seek { beat: Beat::ZERO },
            TransportMessage::Start,
            TransportMessage::Stop,
        ];

        for msg in msgs {
            let wrapped: Message = msg.into();
            assert!(wrapped.type_name().starts_with("Transport::"));
        }
    }

    #[test]
    fn test_all_group_messages() {
        let msgs = vec![
            GroupMessage::Create {
                id: GroupId::new(1),
                name: "test".to_string(),
                parent: None,
            },
            GroupMessage::Delete {
                id: GroupId::new(1),
            },
            GroupMessage::SetParam {
                id: GroupId::new(1),
                param: "amp".to_string(),
                value: 0.5,
            },
            GroupMessage::Mute {
                id: GroupId::new(1),
                muted: true,
            },
            GroupMessage::Solo {
                id: GroupId::new(1),
                solo: true,
            },
            GroupMessage::Finalize,
        ];

        for msg in msgs {
            let wrapped: Message = msg.into();
            assert!(wrapped.type_name().starts_with("Group::"));
        }
    }

    #[test]
    fn test_all_voice_messages() {
        use crate::types::params::ParamBuilder;

        let msgs = vec![
            VoiceMessage::Create {
                id: VoiceId::new(1),
                config: Box::new(VoiceConfig::new("test_voice", "sine", GroupId::new(1))),
            },
            VoiceMessage::Delete {
                id: VoiceId::new(1),
            },
            VoiceMessage::Trigger {
                id: VoiceId::new(1),
                params: ParamBuilder::new().build(),
            },
            VoiceMessage::Stop {
                id: VoiceId::new(1),
            },
            VoiceMessage::NoteOn {
                voice: VoiceId::new(1),
                note: 60,
                velocity: 0.8,
            },
            VoiceMessage::NoteOff {
                voice: VoiceId::new(1),
                note: 60,
            },
            VoiceMessage::Mute {
                id: VoiceId::new(1),
                muted: true,
            },
        ];

        for msg in msgs {
            let wrapped: Message = msg.into();
            assert!(wrapped.type_name().starts_with("Voice::"));
        }
    }

    #[test]
    fn test_all_pattern_messages() {
        let msgs = vec![
            PatternMessage::Create {
                id: PatternId::new(1),
                config: PatternConfig::new("test_pattern", VoiceId::new(1), Beat::from_f64(4.0)),
                owner: PatternOwner::Script,
            },
            PatternMessage::Delete {
                id: PatternId::new(1),
            },
            PatternMessage::Start {
                id: PatternId::new(1),
            },
            PatternMessage::Stop {
                id: PatternId::new(1),
            },
            PatternMessage::SetParam {
                id: PatternId::new(1),
                param: "amp".to_string(),
                value: 0.5,
            },
        ];

        for msg in msgs {
            let wrapped: Message = msg.into();
            assert!(wrapped.type_name().starts_with("Pattern::"));
        }
    }

    #[test]
    fn test_all_sequence_messages() {
        let msgs = vec![
            SequenceMessage::Create {
                id: SequenceId::new(1),
                config: SequenceConfig::new("test_sequence", Beat::from_f64(16.0)),
            },
            SequenceMessage::Delete {
                id: SequenceId::new(1),
            },
            SequenceMessage::Start {
                id: SequenceId::new(1),
                looping: true,
            },
            SequenceMessage::Stop {
                id: SequenceId::new(1),
            },
            SequenceMessage::Pause {
                id: SequenceId::new(1),
            },
            SequenceMessage::Resume {
                id: SequenceId::new(1),
            },
        ];

        for msg in msgs {
            let wrapped: Message = msg.into();
            assert!(wrapped.type_name().starts_with("Sequence::"));
        }
    }

    #[test]
    fn test_message_debug() {
        let msg = Message::Transport(TransportMessage::SetTempo { bpm: 120.0 });
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("SetTempo"));
        assert!(debug_str.contains("120"));
    }

    #[test]
    fn test_from_all_message_types() {
        // Verify all From implementations exist
        let _: Message = TransportMessage::Start.into();
        let _: Message = SynthDefMessage::Load {
            name: "test".to_string(),
            data: vec![],
        }
        .into();
        let _: Message = SampleMessage::Free {
            id: SampleId::new(1),
        }
        .into();
        let _: Message = SfzMessage::Unload { id: SfzId::new(1) }.into();
        let _: Message = GroupMessage::Finalize.into();
        let _: Message = VoiceMessage::Stop {
            id: VoiceId::new(1),
        }
        .into();
        let _: Message = PatternMessage::Stop {
            id: PatternId::new(1),
        }
        .into();
        let _: Message = MelodyMessage::Stop {
            id: MelodyId::new(1),
        }
        .into();
        let _: Message = SequenceMessage::Stop {
            id: SequenceId::new(1),
        }
        .into();
        let _: Message = EffectMessage::Remove {
            id: EffectId::new(1),
        }
        .into();
        let _: Message = FadeMessage::Cancel {
            target: FadeTarget::Group(GroupId::new(1)),
            param: "amp".to_string(),
        }
        .into();
        #[cfg(not(target_arch = "wasm32"))]
        let _: Message = RecordingMessage::Stop {
            id: RecordingId::new(1),
        }
        .into();
    }

    #[test]
    fn test_all_sfz_messages() {
        let msgs = vec![
            SfzMessage::Load {
                id: SfzId::new(1),
                path: PathBuf::from("test.sfz"),
            },
            SfzMessage::Unload { id: SfzId::new(1) },
        ];

        for msg in msgs {
            let wrapped: Message = msg.into();
            assert!(wrapped.type_name().starts_with("Sfz::"));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_all_recording_messages() {
        use crate::traits::RecordingConfig;

        let msgs = vec![
            RecordingMessage::Start {
                id: RecordingId::new(1),
                config: RecordingConfig::default(),
            },
            RecordingMessage::Stop {
                id: RecordingId::new(1),
            },
            RecordingMessage::Cancel {
                id: RecordingId::new(1),
            },
            RecordingMessage::BufferAllocated {
                id: RecordingId::new(1),
            },
            RecordingMessage::Completed {
                id: RecordingId::new(1),
            },
        ];

        for msg in msgs {
            let wrapped: Message = msg.into();
            assert!(wrapped.type_name().starts_with("Recording::"));
        }
    }
}
