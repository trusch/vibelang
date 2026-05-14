//! Script state types for live reloading.
//!
//! This module defines the state extracted from a `.vibe` script,
//! without runtime-specific IDs (node IDs, buffer IDs, etc.).

use crate::handlers::{InputRouteMap, InputRouteSrc, ParamRouteMap, ParamRouteTarget, RouteMap};
#[cfg(feature = "midi")]
use crate::traits::FadeTarget;
#[cfg(not(target_arch = "wasm32"))]
use crate::traits::RecordingConfig;
use crate::traits::{
    BufferConfig, FadeConfig, MelodyConfig, PatternConfig, SampleConfig, SequenceConfig, SfzConfig,
    VoiceConfig,
};
#[cfg(feature = "midi")]
use crate::types::MidiDeviceId;
#[cfg(not(target_arch = "wasm32"))]
use crate::types::RecordingId;
use crate::types::{
    BufferId, EffectId, FadeId, GroupId, MelodyId, ParamMap, PatternId, SampleId, SequenceId,
    SfzId, TimeSignature, VoiceId,
};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Ordered contribution from one evaluated `group(...).body(...)` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyContribution {
    /// Stable-per-execution contribution id.
    pub id: u64,

    /// Canonical group receiving this body.
    pub target_group: GroupId,

    /// Canonical target path used for current-group context.
    pub target_path: String,

    /// Evaluation-order index for this script execution.
    pub ordinal: u64,

    /// Best available script source identity for this body call.
    pub source: Option<String>,

    /// Include/import stack identity placeholder for downstream reload work.
    pub include_stack: Vec<String>,

    /// 1-based source line of the body call when available.
    pub line: Option<u32>,

    /// 1-based source column of the body call when available.
    pub column: Option<u32>,
}

/// Configuration for a group (from script, no runtime IDs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupConfig {
    /// Group name.
    pub name: String,

    /// Parent group (None for root level).
    pub parent: Option<GroupId>,

    /// Initial parameter values.
    pub params: ParamMap,

    /// Effects in this group.
    ///
    /// Entries are appended in total script evaluation order. Repeated effect
    /// ids keep the existing config replacement semantics in [`ScriptState::effects`],
    /// while this list preserves the ordered chain contribution.
    pub effects: Vec<EffectId>,

    /// Whether this group is muted.
    pub muted: bool,

    /// Whether this group is soloed.
    pub soloed: bool,

    /// Hardware output bus override (0-indexed channel number).
    /// When set, the link synth routes directly to this hardware bus
    /// instead of mixing into the parent group.
    pub output_bus: Option<u32>,

    /// Number of hardware output channels this group writes to.
    /// `Some(1)` selects the `system_link_audio_mono` mixdown variant;
    /// `Some(2)` selects the stereo `system_link_audio` (default behaviour).
    /// Coupled with `output_bus`: must be `Some(_)` iff `output_bus` is
    /// `Some(_)`. Set together by the Rhai surface.
    pub output_channels: Option<u32>,
}

/// Canonical target for a group alias or contextual raw-token claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupAliasTarget {
    /// Stable canonical group identity.
    pub group_id: GroupId,

    /// Canonical full group path rooted at `main`.
    pub path: String,
}

impl GroupAliasTarget {
    pub fn new(group_id: GroupId, path: impl Into<String>) -> Self {
        Self {
            group_id,
            path: path.into(),
        }
    }
}

/// Error raised when an alias registry operation would make group resolution ambiguous.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupAliasError {
    InvalidAliasName {
        alias: String,
        reason: &'static str,
    },
    ConflictingAliasTarget {
        alias: String,
        existing: GroupAliasTarget,
        attempted: GroupAliasTarget,
    },
    ConflictingCanonicalGroupName {
        alias: String,
        existing: GroupAliasTarget,
        attempted: GroupAliasTarget,
    },
    ConflictingContextualClaims {
        alias: String,
        existing: Vec<GroupAliasTarget>,
        attempted: GroupAliasTarget,
    },
}

/// Configuration for an effect (from script, no runtime IDs).
#[derive(Clone, Debug, PartialEq)]
pub struct EffectConfig {
    /// Group this effect belongs to.
    pub group: GroupId,

    /// SynthDef name.
    pub synthdef: String,

    /// Initial parameter values.
    pub params: ParamMap,
}

/// Configuration for routing a MIDI keyboard to a voice.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiKeyboardRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Voice to trigger.
    pub voice: VoiceId,
}

/// Configuration for routing a MIDI CC to a parameter.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiCcRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// CC number (0-127).
    pub cc: u8,

    /// Target to control.
    pub target: FadeTarget,

    /// Parameter name.
    pub param: String,

    /// Min value when CC is 0.
    pub min_value: f32,

    /// Max value when CC is 127.
    pub max_value: f32,
}

/// Configuration for a MIDI event callback.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiCallbackConfig {
    /// Unique callback ID for matching with FnPtr storage.
    pub callback_id: u64,

    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// Callback type: "note", "cc", "cc:N" (where N is CC number), "clock", "all".
    pub callback_type: String,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,
}

/// A MIDI message to be sent to an output device.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub enum MidiOutputMessage {
    // MIDI 1.0 messages
    /// Note on message.
    NoteOn {
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
    },
    /// Note off message.
    NoteOff {
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
    },
    /// Control change message.
    ControlChange {
        device_id: MidiDeviceId,
        channel: u8,
        cc: u8,
        value: u8,
    },
    /// Program change message.
    ProgramChange {
        device_id: MidiDeviceId,
        channel: u8,
        program: u8,
    },
    /// Pitch bend message.
    PitchBend {
        device_id: MidiDeviceId,
        channel: u8,
        value: i16,
    },

    // MIDI 2.0 messages
    /// MIDI 2.0 note on with 16-bit velocity.
    Midi2NoteOn {
        device_id: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
    },
    /// MIDI 2.0 note off with 16-bit velocity.
    Midi2NoteOff {
        device_id: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
    },
    /// MIDI 2.0 control change with 32-bit value.
    Midi2ControlChange {
        device_id: MidiDeviceId,
        group: u8,
        channel: u8,
        controller: u8,
        value: u32,
    },
    /// MIDI 2.0 pitch bend with 32-bit value.
    Midi2PitchBend {
        device_id: MidiDeviceId,
        group: u8,
        channel: u8,
        value: u32,
    },
    /// Per-note pitch bend (MIDI 2.0 only).
    Midi2PerNotePitchBend {
        device_id: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        value: u32,
    },
    /// Per-note controller (MIDI 2.0 only).
    Midi2PerNoteController {
        device_id: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        controller: u8,
        value: u32,
    },
    /// MIDI 2.0 polyphonic aftertouch with 32-bit value.
    Midi2PolyPressure {
        device_id: MidiDeviceId,
        group: u8,
        channel: u8,
        note: u8,
        pressure: u32,
    },

    // MIDI Real-Time messages (for sync with external gear)
    /// MIDI Start message (0xFA) - starts playback from beginning.
    Start { device_id: MidiDeviceId },
    /// MIDI Stop message (0xFC) - stops playback.
    Stop { device_id: MidiDeviceId },
    /// MIDI Continue message (0xFB) - resumes playback from current position.
    Continue { device_id: MidiDeviceId },
}

/// Advanced MIDI keyboard route with range, transpose, and velocity curves.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancedMidiKeyboardRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Minimum note (inclusive).
    pub note_min: u8,

    /// Maximum note (inclusive).
    pub note_max: u8,

    /// Transpose in semitones.
    pub transpose: i8,

    /// Velocity curve name.
    pub velocity_curve: String,

    /// Voice to trigger.
    pub voice: VoiceId,
}

/// Advanced MIDI note route (for drums/pads).
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancedMidiNoteRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// Source note number.
    pub source_note: u8,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Choke group name (notes in same group stop each other).
    pub choke_group: Option<String>,

    /// Velocity parameter name (if velocity mapping is enabled).
    pub velocity_param: Option<String>,

    /// Velocity minimum value.
    pub velocity_min: Option<f32>,

    /// Velocity maximum value.
    pub velocity_max: Option<f32>,

    /// Voice to trigger.
    pub voice: VoiceId,
}

/// Advanced MIDI CC route with curves.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancedMidiCcRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// CC number (0-127).
    pub cc: u8,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Curve type name.
    pub curve: String,

    /// Target to control.
    pub target: FadeTarget,

    /// Parameter name.
    pub param: String,

    /// Min value when CC is 0.
    pub min: f32,

    /// Max value when CC is 127.
    pub max: f32,
}

// ============================================================================
// MIDI 2.0 Route Types
// ============================================================================

/// MIDI 2.0 keyboard routing configuration.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct Midi2KeyboardRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// MIDI 2.0 group (0-15).
    pub group: Option<u8>,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Minimum note (inclusive).
    pub note_min: u8,

    /// Maximum note (inclusive).
    pub note_max: u8,

    /// Transpose in semitones.
    pub transpose: i8,

    /// Velocity curve name.
    pub velocity_curve: String,

    /// Voice to trigger.
    pub voice: VoiceId,
}

/// MIDI 2.0 per-note controller type.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub enum Midi2PerNoteControllerType {
    /// Per-note pitch bend with range in semitones.
    PitchBend { range: u8 },
    /// Per-note pressure (poly aftertouch).
    Pressure,
    /// Per-note timbre (CC 74 equivalent).
    Timbre,
    /// Per-note controller by number.
    Controller(u8),
    /// Registered per-note controller.
    RegisteredController(u8),
    /// Assignable per-note controller.
    AssignableController(u8),
}

/// MIDI 2.0 per-note route configuration.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct Midi2PerNoteRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// MIDI 2.0 group (0-15).
    pub group: Option<u8>,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// Controller type.
    pub controller_type: Midi2PerNoteControllerType,

    /// Voice to control.
    pub voice: VoiceId,

    /// Parameter name.
    pub param: String,

    /// Min value.
    pub min_value: f32,

    /// Max value.
    pub max_value: f32,

    /// Curve type name.
    pub curve: String,
}

/// MIDI 2.0 high-resolution CC route configuration.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct Midi2CcRoute {
    /// MIDI device ID.
    pub device_id: MidiDeviceId,

    /// MIDI 2.0 group (0-15).
    pub group: Option<u8>,

    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,

    /// CC number (0-127).
    pub cc: u8,

    /// Voice to control.
    pub voice: VoiceId,

    /// Parameter name.
    pub param: String,

    /// Min value.
    pub min_value: f32,

    /// Max value.
    pub max_value: f32,

    /// Curve type name.
    pub curve: String,
}

/// State extracted from a `.vibe` script.
///
/// This represents the desired state without runtime-specific IDs.
/// It's used for diffing against the current runtime state to determine
/// what needs to be created, deleted, or updated.
#[derive(Clone, Debug, Default)]
pub struct ScriptState {
    /// Tempo in BPM.
    pub tempo: f64,

    /// Time signature.
    pub time_sig: TimeSignature,

    /// Quantization in beats (0 = no quantization).
    pub quantization: f64,

    /// Groups defined in the script.
    pub groups: HashMap<GroupId, GroupConfig>,

    /// Global group aliases declared during script evaluation.
    ///
    /// Alias names map directly to canonical group identity/path, never to
    /// another alias. BTreeMap keeps debug/snapshot representation stable.
    pub group_aliases: BTreeMap<String, GroupAliasTarget>,

    /// Raw contextual group tokens already resolved during this evaluation.
    ///
    /// This lets later alias declarations reject ambiguous rewrites, e.g. a
    /// prior contextual `group("kit")` under `main/Song` conflicting with a
    /// later `kit -> main/Drums` alias.
    pub contextual_group_claims: BTreeMap<String, Vec<GroupAliasTarget>>,

    /// Ordered `group(...).body(...)` contributions seen during evaluation.
    ///
    /// Contributions are recorded in total script evaluation order after group
    /// aliases and contextual references have resolved to canonical group ids.
    pub body_contributions: Vec<BodyContribution>,

    /// First successful voice declarations in total script evaluation order.
    ///
    /// Duplicate voice ids keep their original order slot; the later config in
    /// `voices` still wins, matching the existing replacement semantics.
    pub voice_order: Vec<VoiceId>,

    /// Voices defined in the script.
    pub voices: HashMap<VoiceId, VoiceConfig>,

    /// Patterns defined in the script.
    pub patterns: HashMap<PatternId, PatternConfig>,

    /// Melodies defined in the script.
    pub melodies: HashMap<MelodyId, MelodyConfig>,

    /// Sequences defined in the script.
    pub sequences: HashMap<SequenceId, SequenceConfig>,

    /// Effects defined in the script.
    pub effects: HashMap<EffectId, EffectConfig>,

    /// First successful effect declarations in total script evaluation order.
    ///
    /// Group-local effect chain order lives in [`GroupConfig::effects`]; this
    /// vector gives downstream reload/diff code a stable global effect order
    /// without depending on `HashMap` iteration.
    pub effect_order: Vec<EffectId>,

    /// Samples to load.
    pub samples: HashMap<SampleId, SampleConfig>,

    /// Script-allocated audio-rate buffers (empty memory, name-keyed via BufferId).
    ///
    /// Populated by the Rhai `allocate_buffer(name, frames, channels)` API.
    /// The reload diff treats unchanged entries as no-ops, so the backing
    /// SC buffer survives synthdef recompiles — captured Array data
    /// (e.g. spectraphon SAM-mode magnitudes) persists across hot-reload.
    pub buffers: HashMap<BufferId, BufferConfig>,

    /// SFZ instruments to load.
    pub sfz_instruments: HashMap<SfzId, SfzConfig>,

    /// Per-voice output port routing.
    ///
    /// Keyed by `(voice_id, port_name)` → `RouteDest`. Populated by Story 8's
    /// Rhai surface; for Story 6a this is data-only (the runtime carries the
    /// map but does not yet emit mixer synths — see [`RoutesHandler::finalize`]
    /// stub in `handlers/routes.rs`).
    pub routes: RouteMap,

    /// First successful audio route keys in total script evaluation order.
    ///
    /// Replacing the destination for an existing `(voice, port)` key does not
    /// duplicate the key. Clearing a key removes it from this order so a later
    /// re-add receives a new deterministic order slot.
    pub route_order: Vec<(VoiceId, String)>,

    /// Per-voice **input** routes, keyed by `(target_voice_id, input_port_name)`.
    ///
    /// Script-side mirror of [`crate::state::State::input_routes`]. Populated by
    /// the Rhai `voice.input("name").from(...)` surface (P2.1). Per the
    /// named-inputs design (kb/named-inputs-design-notes.md decision #1),
    /// `.from(x)` produces a Vec of length 1 (replace by default); explicit
    /// fan-in verbs (`.add_from`, `.from_all` — P2.3) are the only path to a
    /// Vec with length > 1.
    pub input_routes: InputRouteMap,

    /// First successful input route keys in total script evaluation order.
    ///
    /// Mirrors [`Self::route_order`] on the output side so downstream reload
    /// diff code has a stable iteration order independent of `HashMap`.
    pub input_route_order: Vec<(VoiceId, String)>,

    /// SET-semantic CV-to-param routes (`.to_param` verb) from kr output
    /// ports to target-voice parameters.
    ///
    /// Keyed by source `(voice_id, port_name)` → list of
    /// `(target_voice_id, target_param_name)` pairs. SET semantics: the
    /// source's control bus is `/n_map`-bound directly to the target param
    /// and overrides any `set_param` value while the mapping is active.
    /// Multi-source on the same target is rejected at script time.
    pub param_routes_set: ParamRouteMap,

    /// First successful SET route keys in total script evaluation order.
    pub param_route_set_order: Vec<(VoiceId, String)>,

    /// BEND-semantic CV-to-param routes (`.modulate_by` verb) from kr output
    /// ports to target-voice parameters.
    ///
    /// Keyed by source `(voice_id, port_name)` → list of
    /// `(target_voice_id, target_param_name)` pairs. BEND semantics: the
    /// runtime spawns a `param_kr_modulate_<n>` summer that reads
    /// `baseline + Σ source kr buses` (where `baseline` is the user's
    /// `set_param` value), and the target param is `/n_map`-bound to the
    /// summer's intermediate bus. Multi-source fan-in is supported up to
    /// [`vibelang_dsp::system_synthdefs::PARAM_KR_MODULATE_MAX`].
    pub param_routes_bend: ParamRouteMap,

    /// First successful BEND route keys in total script evaluation order.
    pub param_route_bend_order: Vec<(VoiceId, String)>,

    /// TRIGGER-semantic CV-to-param routes (`.to_trigger` verb) from
    /// trigger-rate (`Tr`) output ports to target-voice parameters.
    ///
    /// Keyed by source `(voice_id, port_name)` → list of
    /// `(target_voice_id, target_param_name)` pairs. TRIGGER semantics: the
    /// runtime spawns a `port_tr_to_param_link_1` link synth that forwards
    /// the source's Tr bus to an intermediate kr bus 1:1 (no scale/offset
    /// shaping — triggers don't bend), and the target param is `/n_map`-bound
    /// to the link's intermediate bus. Single-source per `(target, param)`;
    /// cross-verb-exclusive with both [`Self::param_routes_set`] and
    /// [`Self::param_routes_bend`].
    pub param_routes_trigger: ParamRouteMap,

    /// First successful TRIGGER route keys in total script evaluation order.
    pub param_route_trigger_order: Vec<(VoiceId, String)>,

    /// Per-route affine shaping for SET routes, populated by chained
    /// `.scale(s)` / `.offset(o)` modifiers in the Rhai surface.
    ///
    /// Keyed by the full quad `(source_voice, source_port, target,
    /// target_param)` → `(scale, offset)`. The `target` is a
    /// [`ParamRouteTarget`] enum so fx-target shaping shares the same map.
    /// Defaults to `(1.0, 0.0)` when an entry is created (the summer
    /// identity); `.scale(s)` overwrites the `scale` slot, `.offset(o)`
    /// overwrites the `offset` slot. The runtime reads these in
    /// `RoutesHandler::finalize_params` to seed each `param_kr_modulate_<n>`'s
    /// `scale_<i>` / `offset_<i>` per source slot.
    pub param_route_set_shaping: HashMap<(VoiceId, String, ParamRouteTarget, String), (f32, f32)>,

    /// Per-route affine shaping for BEND routes; same shape and defaults as
    /// [`Self::param_route_set_shaping`].
    pub param_route_bend_shaping: HashMap<(VoiceId, String, ParamRouteTarget, String), (f32, f32)>,

    /// Recordings to start (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub recordings: HashMap<RecordingId, RecordingConfig>,

    /// Patterns that should be playing.
    pub playing_patterns: HashSet<PatternId>,

    /// Melodies that should be playing.
    pub playing_melodies: HashSet<MelodyId>,

    /// Sequences that should be playing.
    pub playing_sequences: HashSet<SequenceId>,

    /// Voices that should be running continuously (e.g., line-in monitors).
    /// These voices are auto-triggered on startup and after reload.
    pub running_voices: HashSet<VoiceId>,

    /// Fades defined in the script (stateful, participate in reload diffing).
    ///
    /// Each fade has a stable ID derived from its name. On reload, unchanged
    /// fades are not re-fired — only new or modified fades are started.
    pub fades: HashMap<FadeId, FadeConfig>,

    /// Fades that should be actively running (started via `.start()` or `.now()`).
    pub playing_fades: HashSet<FadeId>,

    /// Fades marked for forced restart (via `.restart()`).
    ///
    /// These fades will be re-fired on reload even if their config is unchanged.
    pub force_restart_fades: HashSet<FadeId>,

    /// Fades to start immediately on reload (no quantization).
    ///
    /// **Deprecated**: Use stateful fades via `state.fades` instead.
    /// These are fades triggered via `fade(...).now()` in the script.
    /// They are processed during `apply_reload` and converted to active fades.
    pub pending_fades: Vec<FadeConfig>,

    /// Fades to start with quantization.
    ///
    /// **Deprecated**: Use stateful fades via `state.fades` instead.
    /// These are fades triggered via `fade(...).start()` in the script.
    /// They are processed at the next quantization boundary.
    pub pending_fades_quantized: Vec<FadeConfig>,

    /// MIDI keyboard routes.
    #[cfg(feature = "midi")]
    pub midi_keyboard_routes: Vec<MidiKeyboardRoute>,

    /// MIDI CC routes.
    #[cfg(feature = "midi")]
    pub midi_cc_routes: Vec<MidiCcRoute>,

    /// Advanced MIDI keyboard routes with range, transpose, and velocity curves.
    #[cfg(feature = "midi")]
    pub advanced_keyboard_routes: Vec<AdvancedMidiKeyboardRoute>,

    /// Advanced MIDI note routes for drums/pads.
    #[cfg(feature = "midi")]
    pub advanced_note_routes: Vec<AdvancedMidiNoteRoute>,

    /// Advanced MIDI CC routes with curves.
    #[cfg(feature = "midi")]
    pub advanced_cc_routes: Vec<AdvancedMidiCcRoute>,

    /// MIDI devices to open for input.
    #[cfg(feature = "midi")]
    pub midi_inputs: HashSet<MidiDeviceId>,

    /// MIDI devices to open for output.
    #[cfg(feature = "midi")]
    pub midi_outputs: HashSet<MidiDeviceId>,

    /// Pending MIDI output messages to send immediately.
    #[cfg(feature = "midi")]
    pub midi_output_messages: Vec<MidiOutputMessage>,

    /// MIDI event callbacks to register.
    #[cfg(feature = "midi")]
    pub midi_callbacks: Vec<MidiCallbackConfig>,

    /// MIDI recording requests.
    #[cfg(feature = "midi")]
    pub midi_recording_requests: Vec<MidiRecordingRequest>,

    /// Looper configurations.
    #[cfg(feature = "midi")]
    pub loopers: Vec<LooperConfig>,

    /// MIDI clock output configurations.
    #[cfg(feature = "midi")]
    pub midi_clock_outputs: Vec<MidiClockOutputRequest>,

    /// MIDI 2.0 keyboard routes.
    #[cfg(feature = "midi")]
    pub midi2_keyboard_routes: Vec<Midi2KeyboardRoute>,

    /// MIDI 2.0 per-note routes.
    #[cfg(feature = "midi")]
    pub midi2_per_note_routes: Vec<Midi2PerNoteRoute>,

    /// MIDI 2.0 CC routes.
    #[cfg(feature = "midi")]
    pub midi2_cc_routes: Vec<Midi2CcRoute>,
}

/// Declarative config written by the .vibe script.
/// Tells the runtime: "this MIDI device should use looper mode for this voice."
#[cfg(feature = "midi")]
#[derive(Debug, Clone, PartialEq)]
pub struct LooperConfig {
    pub device_id: MidiDeviceId,
    pub voice_id: VoiceId,
    pub channel: Option<u8>, // Optional MIDI channel filter (0-15)
    pub silence_bars: f64,   // How many bars of silence before playback. Default: 1.0
    pub quantize_beats: f64, // Quantization grid for recorded notes. Default: 0.25 (16th notes)
}

#[cfg(feature = "midi")]
impl Default for LooperConfig {
    fn default() -> Self {
        Self {
            device_id: MidiDeviceId::default(),
            voice_id: VoiceId::default(),
            channel: None,
            silence_bars: 1.0,
            quantize_beats: 0.25,
        }
    }
}

fn validate_group_alias_name(alias: &str) -> Result<(), GroupAliasError> {
    let reason = if alias.is_empty() {
        Some("aliases must be non-empty")
    } else if alias.contains('/') {
        Some("aliases must be single relative names")
    } else {
        None
    };

    if let Some(reason) = reason {
        Err(GroupAliasError::InvalidAliasName {
            alias: alias.to_string(),
            reason,
        })
    } else {
        Ok(())
    }
}

/// Configuration for a MIDI recording request.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiRecordingRequest {
    /// Device ID to record from.
    pub device_id: MidiDeviceId,

    /// Optional channel filter.
    pub channel: Option<u8>,

    /// Start (true) or stop (false) recording.
    pub start: bool,
}

/// Configuration for MIDI clock output.
#[cfg(feature = "midi")]
#[derive(Clone, Debug, PartialEq)]
pub struct MidiClockOutputRequest {
    /// Device ID to send clock to.
    pub device_id: MidiDeviceId,

    /// Enable (true) or disable (false) clock output.
    pub enabled: bool,
}

impl ScriptState {
    /// Create a new empty script state with default tempo.
    pub fn new() -> Self {
        Self {
            tempo: 120.0,
            time_sig: TimeSignature::default(),
            ..Default::default()
        }
    }

    /// Builder method to set tempo.
    pub fn with_tempo(mut self, bpm: f64) -> Self {
        self.tempo = bpm;
        self
    }

    /// Builder method to set time signature.
    pub fn with_time_signature(mut self, time_sig: TimeSignature) -> Self {
        self.time_sig = time_sig;
        self
    }

    /// Add a group.
    pub fn add_group(&mut self, id: GroupId, config: GroupConfig) {
        self.groups.insert(id, config);
    }

    /// Register a global group alias for a canonical target.
    pub fn add_group_alias(
        &mut self,
        alias: impl Into<String>,
        target: GroupAliasTarget,
    ) -> Result<(), GroupAliasError> {
        let alias = alias.into();
        validate_group_alias_name(&alias)?;

        if let Some(existing) = self.group_aliases.get(&alias) {
            if existing == &target {
                return Ok(());
            }

            return Err(GroupAliasError::ConflictingAliasTarget {
                alias,
                existing: existing.clone(),
                attempted: target,
            });
        }

        if let Some(existing) = self.contextual_group_claims.get(&alias) {
            if existing.iter().any(|claim| claim != &target) {
                return Err(GroupAliasError::ConflictingContextualClaims {
                    alias,
                    existing: existing.clone(),
                    attempted: target,
                });
            }
        }

        self.group_aliases.insert(alias, target);
        Ok(())
    }

    /// Record that a raw contextual token resolved to a canonical group target.
    pub fn add_contextual_group_claim(
        &mut self,
        raw_token: impl Into<String>,
        target: GroupAliasTarget,
    ) -> Result<(), GroupAliasError> {
        let raw_token = raw_token.into();

        if let Some(existing) = self.group_aliases.get(&raw_token) {
            if existing != &target {
                return Err(GroupAliasError::ConflictingAliasTarget {
                    alias: raw_token,
                    existing: existing.clone(),
                    attempted: target,
                });
            }
        }

        let claims = self.contextual_group_claims.entry(raw_token).or_default();

        if !claims.iter().any(|claim| claim == &target) {
            claims.push(target);
            claims.sort_by(|a, b| {
                a.path
                    .cmp(&b.path)
                    .then_with(|| a.group_id.raw().cmp(&b.group_id.raw()))
            });
        }

        Ok(())
    }

    /// Add a voice.
    pub fn add_voice(&mut self, id: VoiceId, config: VoiceConfig) {
        self.record_voice_order(id);
        self.voices.insert(id, config);
    }

    /// Add a pattern.
    pub fn add_pattern(&mut self, id: PatternId, config: PatternConfig) {
        self.patterns.insert(id, config);
    }

    /// Add a melody.
    pub fn add_melody(&mut self, id: MelodyId, config: MelodyConfig) {
        self.melodies.insert(id, config);
    }

    /// Add a sequence.
    pub fn add_sequence(&mut self, id: SequenceId, config: SequenceConfig) {
        self.sequences.insert(id, config);
    }

    /// Add an effect.
    pub fn add_effect(&mut self, id: EffectId, config: EffectConfig) {
        self.record_effect_order(id);
        self.effects.insert(id, config);
    }

    /// Remove an effect and its deterministic order slot.
    pub fn remove_effect(&mut self, id: &EffectId) -> Option<EffectConfig> {
        self.effect_order.retain(|ordered| ordered != id);
        self.effects.remove(id)
    }

    /// Add a sample.
    pub fn add_sample(&mut self, id: SampleId, config: SampleConfig) {
        self.samples.insert(id, config);
    }

    /// Add a script-allocated buffer.
    pub fn add_buffer(&mut self, id: BufferId, config: BufferConfig) {
        self.buffers.insert(id, config);
    }

    /// Add an SFZ instrument.
    pub fn add_sfz(&mut self, id: SfzId, config: SfzConfig) {
        self.sfz_instruments.insert(id, config);
    }

    /// Install a route destination for a voice's named output port.
    ///
    /// Variant-dependent semantics:
    /// - [`RouteDest::Main`] / [`RouteDest::Muted`]: replace any prior list
    ///   for the key (the hardware bus and silence are exclusive — no
    ///   meaningful fan-out).
    /// - [`RouteDest::Group`]: additively appended to the prior list,
    ///   deduplicated against existing entries. If the prior list held a
    ///   non-Group dest (Main or Muted), it is replaced with the new Group
    ///   list — the variants are mutually exclusive on a single port.
    /// - [`RouteDest::Param`]: never stored here (Param routes live in
    ///   `param_routes_set` / `param_routes_bend`); calls with this variant
    ///   are a no-op against the route map.
    pub fn set_route(
        &mut self,
        voice_id: VoiceId,
        port_name: impl Into<String>,
        dest: crate::handlers::RouteDest,
    ) {
        use crate::handlers::RouteDest;
        let key = (voice_id, port_name.into());
        match dest {
            RouteDest::Param { .. } => {}
            RouteDest::Main | RouteDest::Muted => {
                self.record_route_order(&key);
                self.routes.insert(key, vec![dest]);
            }
            RouteDest::Group(_) => {
                self.record_route_order(&key);
                let entry = self.routes.entry(key).or_default();
                let group_only = entry.iter().all(|d| matches!(d, RouteDest::Group(_)));
                if !group_only {
                    entry.clear();
                }
                if !entry.iter().any(|d| d == &dest) {
                    entry.push(dest);
                }
            }
        }
    }

    /// Remove all route entries for a voice's named output port, if any.
    pub fn clear_route(&mut self, voice_id: VoiceId, port_name: &str) {
        let key = (voice_id, port_name.to_string());
        self.routes.remove(&key);
        self.route_order.retain(|ordered| ordered != &key);
    }

    /// Install a single-source input route for `(target_voice, input_port)`.
    ///
    /// Per the named-inputs design (decision #1), `.from(x)` is replace-by-
    /// default — this method overwrites any prior Vec on the key with a fresh
    /// single-element Vec. Explicit fan-in verbs (`.add_from`, `.from_all`)
    /// land via a separate entry point in P2.3.
    pub fn set_input_route(
        &mut self,
        target_voice: VoiceId,
        input_port: impl Into<String>,
        src: InputRouteSrc,
    ) {
        let key = (target_voice, input_port.into());
        self.record_input_route_order(&key);
        self.input_routes.insert(key, vec![src]);
    }

    /// Remove all input-route entries for a voice's named input port, if any.
    pub fn clear_input_route(&mut self, target_voice: VoiceId, input_port: &str) {
        let key = (target_voice, input_port.to_string());
        self.input_routes.remove(&key);
        self.input_route_order.retain(|ordered| ordered != &key);
    }

    /// Install a SET-semantic CV-to-param route (`.to_param` verb) from
    /// `(source_voice, source_port)` to `(target, target_param)`. The target
    /// can be either a Voice or an Effect — see [`ParamRouteTarget`].
    ///
    /// Returns an error if `(target, target_param)` is already routed via
    /// the BEND map (`.modulate_by`) or the TRIGGER map (`.to_trigger`) —
    /// the three verbs are mutually exclusive on the same target — or if a
    /// *different* SET source already targets the same `(target,
    /// target_param)` (multi-source SET is disallowed; use `.modulate_by`
    /// for additive multi-source fan-in). A repeat call with the same
    /// `(source, target)` quad is a deduplicated no-op, so re-runs of a
    /// script don't accumulate dupes.
    pub fn add_param_route_set(
        &mut self,
        source_voice: VoiceId,
        source_port: impl Into<String>,
        target: ParamRouteTarget,
        target_param: impl Into<String>,
    ) -> Result<(), ParamRouteConflict> {
        let source_port = source_port.into();
        let target_param = target_param.into();
        let target_pair = (target, target_param.clone());

        if route_map_contains_target(&self.param_routes_bend, &target_pair)
            || route_map_contains_target(&self.param_routes_trigger, &target_pair)
        {
            return Err(ParamRouteConflict::CrossVerb {
                target,
                target_param,
            });
        }
        // Reject a *different* SET source pointing at the same target — only
        // the existing identical entry is allowed (idempotent re-run).
        for ((sv, sp), targets) in self.param_routes_set.iter() {
            if (*sv != source_voice || *sp != source_port)
                && targets.iter().any(|t| t == &target_pair)
            {
                return Err(ParamRouteConflict::MultiSourceSet {
                    target,
                    target_param,
                });
            }
        }

        let source_key = (source_voice, source_port.clone());
        let should_add = !self
            .param_routes_set
            .get(&source_key)
            .map(|entry| entry.iter().any(|t| t == &target_pair))
            .unwrap_or(false);
        if should_add {
            self.record_param_route_set_order(source_voice, &source_port);
        }
        let entry = self.param_routes_set.entry(source_key).or_default();
        if !entry.iter().any(|t| t == &target_pair) {
            entry.push(target_pair);
        }
        self.param_route_set_shaping
            .entry((source_voice, source_port, target, target_param))
            .or_insert((1.0, 0.0));
        Ok(())
    }

    /// Install a BEND-semantic CV-to-param route (`.modulate_by` verb) from
    /// `(source_voice, source_port)` to `(target, target_param)`. The target
    /// can be either a Voice or an Effect — see [`ParamRouteTarget`].
    ///
    /// Returns an error if `(target, target_param)` is already routed via
    /// the SET map (`.to_param`) or the TRIGGER map (`.to_trigger`).
    /// Additive across multiple sources on the same target — that's the
    /// whole point of BEND. A repeat call with the same `(source, target)`
    /// quad is deduplicated.
    pub fn add_param_route_bend(
        &mut self,
        source_voice: VoiceId,
        source_port: impl Into<String>,
        target: ParamRouteTarget,
        target_param: impl Into<String>,
    ) -> Result<(), ParamRouteConflict> {
        let source_port = source_port.into();
        let target_param = target_param.into();
        let target_pair = (target, target_param.clone());

        if route_map_contains_target(&self.param_routes_set, &target_pair)
            || route_map_contains_target(&self.param_routes_trigger, &target_pair)
        {
            return Err(ParamRouteConflict::CrossVerb {
                target,
                target_param,
            });
        }

        let source_key = (source_voice, source_port.clone());
        let should_add = !self
            .param_routes_bend
            .get(&source_key)
            .map(|entry| entry.iter().any(|t| t == &target_pair))
            .unwrap_or(false);
        if should_add {
            self.record_param_route_bend_order(source_voice, &source_port);
        }
        let entry = self.param_routes_bend.entry(source_key).or_default();
        if !entry.iter().any(|t| t == &target_pair) {
            entry.push(target_pair);
        }
        self.param_route_bend_shaping
            .entry((source_voice, source_port, target, target_param))
            .or_insert((1.0, 0.0));
        Ok(())
    }

    /// Install a TRIGGER-semantic CV-to-param route (`.to_trigger` verb)
    /// from a Tr-rate `(source_voice, source_port)` to `(target,
    /// target_param)`. The target can be either a Voice or an Effect — see
    /// [`ParamRouteTarget`].
    ///
    /// Returns an error if `(target, target_param)` is already routed via
    /// the SET map (`.to_param`) or the BEND map (`.modulate_by`) — the
    /// three verbs are mutually exclusive on the same target — or if a
    /// *different* TRIGGER source already targets the same `(target,
    /// target_param)` (multi-source TRIGGER fan-in is disallowed; trigger
    /// routing is 1:1, not summable). A repeat call with the same
    /// `(source, target)` quad is a deduplicated no-op.
    pub fn add_param_route_trigger(
        &mut self,
        source_voice: VoiceId,
        source_port: impl Into<String>,
        target: ParamRouteTarget,
        target_param: impl Into<String>,
    ) -> Result<(), ParamRouteConflict> {
        let source_port = source_port.into();
        let target_param = target_param.into();
        let target_pair = (target, target_param.clone());

        if route_map_contains_target(&self.param_routes_set, &target_pair)
            || route_map_contains_target(&self.param_routes_bend, &target_pair)
        {
            return Err(ParamRouteConflict::CrossVerb {
                target,
                target_param,
            });
        }
        for ((sv, sp), targets) in self.param_routes_trigger.iter() {
            if (*sv != source_voice || *sp != source_port)
                && targets.iter().any(|t| t == &target_pair)
            {
                return Err(ParamRouteConflict::MultiSourceTrigger {
                    target,
                    target_param,
                });
            }
        }

        let source_key = (source_voice, source_port.clone());
        let should_add = !self
            .param_routes_trigger
            .get(&source_key)
            .map(|entry| entry.iter().any(|t| t == &target_pair))
            .unwrap_or(false);
        if should_add {
            self.record_param_route_trigger_order(source_voice, &source_port);
        }
        let entry = self.param_routes_trigger.entry(source_key).or_default();
        if !entry.iter().any(|t| t == &target_pair) {
            entry.push(target_pair);
        }
        Ok(())
    }

    fn record_voice_order(&mut self, id: VoiceId) {
        if !self.voice_order.contains(&id) {
            self.voice_order.push(id);
        }
    }

    fn record_effect_order(&mut self, id: EffectId) {
        if !self.effect_order.contains(&id) {
            self.effect_order.push(id);
        }
    }

    fn record_route_order(&mut self, key: &(VoiceId, String)) {
        if !self.route_order.iter().any(|ordered| ordered == key) {
            self.route_order.push(key.clone());
        }
    }

    fn record_input_route_order(&mut self, key: &(VoiceId, String)) {
        if !self.input_route_order.iter().any(|ordered| ordered == key) {
            self.input_route_order.push(key.clone());
        }
    }

    fn record_param_route_set_order(&mut self, source_voice: VoiceId, source_port: &str) {
        let key = (source_voice, source_port.to_string());
        if !self
            .param_route_set_order
            .iter()
            .any(|ordered| ordered == &key)
        {
            self.param_route_set_order.push(key);
        }
    }

    fn record_param_route_bend_order(&mut self, source_voice: VoiceId, source_port: &str) {
        let key = (source_voice, source_port.to_string());
        if !self
            .param_route_bend_order
            .iter()
            .any(|ordered| ordered == &key)
        {
            self.param_route_bend_order.push(key);
        }
    }

    fn record_param_route_trigger_order(&mut self, source_voice: VoiceId, source_port: &str) {
        let key = (source_voice, source_port.to_string());
        if !self
            .param_route_trigger_order
            .iter()
            .any(|ordered| ordered == &key)
        {
            self.param_route_trigger_order.push(key);
        }
    }

    /// Update the per-source `scale` shaping factor on a SET route.
    ///
    /// Lazily creates the entry at the default `(1.0, 0.0)` if not already
    /// present (the route should have been installed via
    /// [`Self::add_param_route_set`] first; bare callers still get sensible
    /// defaults). Multi-call: last `.scale()` wins — this overwrites the
    /// scale slot leaving offset untouched.
    pub fn set_param_route_set_scale(
        &mut self,
        source_voice: VoiceId,
        source_port: impl Into<String>,
        target: ParamRouteTarget,
        target_param: impl Into<String>,
        scale: f32,
    ) {
        let key = (
            source_voice,
            source_port.into(),
            target,
            target_param.into(),
        );
        let entry = self
            .param_route_set_shaping
            .entry(key)
            .or_insert((1.0, 0.0));
        entry.0 = scale;
    }

    /// Update the per-source `offset` shaping factor on a SET route. See
    /// [`Self::set_param_route_set_scale`] for semantics.
    pub fn set_param_route_set_offset(
        &mut self,
        source_voice: VoiceId,
        source_port: impl Into<String>,
        target: ParamRouteTarget,
        target_param: impl Into<String>,
        offset: f32,
    ) {
        let key = (
            source_voice,
            source_port.into(),
            target,
            target_param.into(),
        );
        let entry = self
            .param_route_set_shaping
            .entry(key)
            .or_insert((1.0, 0.0));
        entry.1 = offset;
    }

    /// Update the per-source `scale` shaping factor on a BEND route. See
    /// [`Self::set_param_route_set_scale`] for semantics.
    pub fn set_param_route_bend_scale(
        &mut self,
        source_voice: VoiceId,
        source_port: impl Into<String>,
        target: ParamRouteTarget,
        target_param: impl Into<String>,
        scale: f32,
    ) {
        let key = (
            source_voice,
            source_port.into(),
            target,
            target_param.into(),
        );
        let entry = self
            .param_route_bend_shaping
            .entry(key)
            .or_insert((1.0, 0.0));
        entry.0 = scale;
    }

    /// Update the per-source `offset` shaping factor on a BEND route. See
    /// [`Self::set_param_route_set_scale`] for semantics.
    pub fn set_param_route_bend_offset(
        &mut self,
        source_voice: VoiceId,
        source_port: impl Into<String>,
        target: ParamRouteTarget,
        target_param: impl Into<String>,
        offset: f32,
    ) {
        let key = (
            source_voice,
            source_port.into(),
            target,
            target_param.into(),
        );
        let entry = self
            .param_route_bend_shaping
            .entry(key)
            .or_insert((1.0, 0.0));
        entry.1 = offset;
    }
}

fn route_map_contains_target(map: &ParamRouteMap, pair: &(ParamRouteTarget, String)) -> bool {
    map.values()
        .any(|targets| targets.iter().any(|t| t == pair))
}

/// Reasons a `add_param_route_set` / `add_param_route_bend` /
/// `add_param_route_trigger` call may fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamRouteConflict {
    /// The same `(target, target_param)` is already wired via a different
    /// verb (any of SET / BEND / TRIGGER). The three verbs are mutually
    /// exclusive on a single target.
    CrossVerb {
        target: ParamRouteTarget,
        target_param: String,
    },
    /// A different SET source already targets the same `(target,
    /// target_param)`. Multi-source on a `.to_param` is disallowed; use
    /// `.modulate_by` for additive fan-in.
    MultiSourceSet {
        target: ParamRouteTarget,
        target_param: String,
    },
    /// A different TRIGGER source already targets the same `(target,
    /// target_param)`. Trigger routing is 1:1; multi-source fan-in is not
    /// supported.
    MultiSourceTrigger {
        target: ParamRouteTarget,
        target_param: String,
    },
}

impl ParamRouteConflict {
    pub fn target(&self) -> ParamRouteTarget {
        match self {
            Self::CrossVerb { target, .. }
            | Self::MultiSourceSet { target, .. }
            | Self::MultiSourceTrigger { target, .. } => *target,
        }
    }
    pub fn target_param(&self) -> &str {
        match self {
            Self::CrossVerb { target_param, .. }
            | Self::MultiSourceSet { target_param, .. }
            | Self::MultiSourceTrigger { target_param, .. } => target_param,
        }
    }
}

impl std::fmt::Display for ParamRouteConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CrossVerb {
                target,
                target_param,
            } => write!(
                f,
                "cannot combine .to_param / .modulate_by / .to_trigger on the \
                 same ({:?}, param {:?}) — pick exactly one verb",
                target, target_param
            ),
            Self::MultiSourceSet {
                target,
                target_param,
            } => write!(
                f,
                ".to_param on ({:?}, param {:?}) is already routed from \
                 another source — multi-source SET is disallowed; use \
                 .modulate_by for additive fan-in",
                target, target_param
            ),
            Self::MultiSourceTrigger {
                target,
                target_param,
            } => write!(
                f,
                ".to_trigger on ({:?}, param {:?}) is already routed \
                 from another source — multi-source TRIGGER fan-in is not \
                 supported (trigger routing is 1:1)",
                target, target_param
            ),
        }
    }
}

impl std::error::Error for ParamRouteConflict {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_state_new() {
        let state = ScriptState::new();
        assert_eq!(state.tempo, 120.0);
        assert!(state.groups.is_empty());
        assert!(state.group_aliases.is_empty());
        assert!(state.contextual_group_claims.is_empty());
        assert!(state.voices.is_empty());
    }

    #[test]
    fn test_script_state_builder() {
        let state = ScriptState::new()
            .with_tempo(140.0)
            .with_time_signature(TimeSignature::new(3, 4));

        assert_eq!(state.tempo, 140.0);
        assert_eq!(state.time_sig.numerator, 3);
    }

    #[test]
    fn test_add_group() {
        let mut state = ScriptState::new();
        state.add_group(
            GroupId::new(1),
            GroupConfig {
                name: "test".to_string(),
                parent: None,
                params: ParamMap::new(),
                effects: Vec::new(),
                muted: false,
                soloed: false,
                output_bus: None,
                output_channels: None,
            },
        );

        assert!(state.groups.contains_key(&GroupId::new(1)));
    }

    #[test]
    fn test_add_voice() {
        let mut state = ScriptState::new();
        let mut config = VoiceConfig {
            name: "test".to_string(),
            synthdef: "sine".to_string(),
            group: GroupId::new(1),
            polyphony: 8,
            params: ParamMap::new(),
            muted: false,
            soloed: false,
            sfz_instrument: None,
            sample_id: None,
            trigger_mode: "gate".to_string(),
            choke_group: None,
            round_robin_count: 0,
            modulator_only: false,
            mono_legato: false,
            #[cfg(feature = "midi")]
            midi_output: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
            #[cfg(feature = "midi")]
            param_cc_map: std::collections::HashMap::new(),
        };
        state.add_voice(VoiceId::new(1), config.clone());
        config.synthdef = "saw".to_string();
        state.add_voice(VoiceId::new(1), config);

        assert!(state.voices.contains_key(&VoiceId::new(1)));
        assert_eq!(state.voice_order, vec![VoiceId::new(1)]);
        assert_eq!(state.voices[&VoiceId::new(1)].synthdef, "saw");
    }

    #[test]
    fn test_route_order_is_recorded_deterministically() {
        let mut state = ScriptState::new();
        state.set_route(
            VoiceId::new(2),
            "out",
            crate::handlers::RouteDest::Group(GroupId::new(10)),
        );
        state.set_route(VoiceId::new(1), "out", crate::handlers::RouteDest::Main);
        state.set_route(
            VoiceId::new(2),
            "out",
            crate::handlers::RouteDest::Group(GroupId::new(11)),
        );

        assert_eq!(
            state.route_order,
            vec![
                (VoiceId::new(2), "out".to_string()),
                (VoiceId::new(1), "out".to_string())
            ]
        );

        state.clear_route(VoiceId::new(2), "out");
        state.set_route(VoiceId::new(2), "out", crate::handlers::RouteDest::Muted);

        assert_eq!(
            state.route_order,
            vec![
                (VoiceId::new(1), "out".to_string()),
                (VoiceId::new(2), "out".to_string())
            ]
        );
    }

    #[test]
    fn test_param_route_order_is_recorded_deterministically() {
        let mut state = ScriptState::new();
        state
            .add_param_route_bend(
                VoiceId::new(2),
                "env",
                ParamRouteTarget::Voice(VoiceId::new(10)),
                "cutoff",
            )
            .unwrap();
        state
            .add_param_route_bend(
                VoiceId::new(1),
                "lfo",
                ParamRouteTarget::Voice(VoiceId::new(10)),
                "cutoff",
            )
            .unwrap();
        state
            .add_param_route_bend(
                VoiceId::new(2),
                "env",
                ParamRouteTarget::Voice(VoiceId::new(10)),
                "cutoff",
            )
            .unwrap();

        assert_eq!(
            state.param_route_bend_order,
            vec![
                (VoiceId::new(2), "env".to_string()),
                (VoiceId::new(1), "lfo".to_string())
            ]
        );
    }

    #[test]
    fn test_group_config_default() {
        let config = GroupConfig::default();
        assert!(config.name.is_empty());
        assert!(config.parent.is_none());
        assert!(config.params.is_empty());
        assert!(config.effects.is_empty());
    }

    #[test]
    fn test_add_group_alias_records_canonical_target() {
        let mut state = ScriptState::new();
        let target = GroupAliasTarget::new(GroupId::new(42), "main/Drums");

        state.add_group_alias("kit", target.clone()).unwrap();

        assert_eq!(state.group_aliases.get("kit"), Some(&target));
    }

    #[test]
    fn test_add_group_alias_same_target_is_idempotent() {
        let mut state = ScriptState::new();
        let target = GroupAliasTarget::new(GroupId::new(42), "main/Drums");

        state.add_group_alias("kit", target.clone()).unwrap();
        state.add_group_alias("kit", target.clone()).unwrap();

        assert_eq!(state.group_aliases.len(), 1);
        assert_eq!(state.group_aliases.get("kit"), Some(&target));
    }

    #[test]
    fn test_add_group_alias_rejects_different_target() {
        let mut state = ScriptState::new();
        let existing = GroupAliasTarget::new(GroupId::new(42), "main/Drums");
        let attempted = GroupAliasTarget::new(GroupId::new(7), "main/Bass");

        state.add_group_alias("kit", existing.clone()).unwrap();
        let err = state.add_group_alias("kit", attempted.clone()).unwrap_err();

        assert_eq!(
            err,
            GroupAliasError::ConflictingAliasTarget {
                alias: "kit".to_string(),
                existing,
                attempted,
            }
        );
    }

    #[test]
    fn test_add_group_alias_rejects_invalid_alias_names() {
        let target = GroupAliasTarget::new(GroupId::new(42), "main/Drums");

        for alias in ["", "main/kit", "drums/kit"] {
            let mut state = ScriptState::new();
            assert!(
                matches!(
                    state.add_group_alias(alias, target.clone()),
                    Err(GroupAliasError::InvalidAliasName { .. })
                ),
                "alias {alias:?} should be invalid"
            );
        }
    }

    #[test]
    fn test_contextual_group_claims_are_deduplicated_and_sorted() {
        let mut state = ScriptState::new();
        let song_kit = GroupAliasTarget::new(GroupId::new(12), "main/Song/kit");
        let other_kit = GroupAliasTarget::new(GroupId::new(7), "main/Other/kit");

        state
            .add_contextual_group_claim("kit", song_kit.clone())
            .unwrap();
        state
            .add_contextual_group_claim("kit", other_kit.clone())
            .unwrap();
        state.add_contextual_group_claim("kit", song_kit).unwrap();

        assert_eq!(
            state.contextual_group_claims.get("kit"),
            Some(&vec![
                other_kit,
                GroupAliasTarget::new(GroupId::new(12), "main/Song/kit")
            ])
        );
    }

    #[test]
    fn test_add_group_alias_rejects_conflicting_contextual_claims() {
        let mut state = ScriptState::new();
        let song_kit = GroupAliasTarget::new(GroupId::new(12), "main/Song/kit");
        let other_kit = GroupAliasTarget::new(GroupId::new(7), "main/Other/kit");
        let drums = GroupAliasTarget::new(GroupId::new(42), "main/Drums");

        state
            .add_contextual_group_claim("kit", song_kit.clone())
            .unwrap();
        state
            .add_contextual_group_claim("kit", other_kit.clone())
            .unwrap();
        let err = state.add_group_alias("kit", drums.clone()).unwrap_err();

        assert_eq!(
            err,
            GroupAliasError::ConflictingContextualClaims {
                alias: "kit".to_string(),
                existing: vec![other_kit, song_kit],
                attempted: drums,
            }
        );
    }

    #[test]
    fn test_add_group_alias_allows_matching_contextual_claim() {
        let mut state = ScriptState::new();
        let target = GroupAliasTarget::new(GroupId::new(12), "main/Song/kit");

        state
            .add_contextual_group_claim("kit", target.clone())
            .unwrap();
        state.add_group_alias("kit", target.clone()).unwrap();

        assert_eq!(state.group_aliases.get("kit"), Some(&target));
    }

    #[test]
    fn test_contextual_group_claim_rejects_conflicting_existing_alias() {
        let mut state = ScriptState::new();
        let alias_target = GroupAliasTarget::new(GroupId::new(42), "main/Drums");
        let contextual_target = GroupAliasTarget::new(GroupId::new(12), "main/Song/kit");

        state.add_group_alias("kit", alias_target.clone()).unwrap();
        let err = state
            .add_contextual_group_claim("kit", contextual_target.clone())
            .unwrap_err();

        assert_eq!(
            err,
            GroupAliasError::ConflictingAliasTarget {
                alias: "kit".to_string(),
                existing: alias_target,
                attempted: contextual_target,
            }
        );
    }
}
