//! MIDI infrastructure for vibelang-core.
//!
//! This module provides:
//!
//! ## Core Infrastructure
//! - **Timestamp preservation** - Microsecond-accurate timing from midir
//! - **Lock-free queues** - High-performance event routing
//!
//! ## Full MIDI 1.0 Support
//! - All channel voice messages (note, CC, aftertouch, pitch bend)
//! - System exclusive (SysEx) with multi-packet support
//! - System realtime message parsing (clock, start, stop, continue).
//!   TODO: external clock sync is unimplemented — incoming clock/transport
//!   messages are parsed and logged but never drive the transport or tempo.
//! - NRPN/RPN 14-bit parameter control
//!
//! ## Advanced Features
//! - **MPE (MIDI Polyphonic Expression)** - Per-note expression for Seaboard, Linnstrument, etc.
//! - **Device Hot-Plug** - Detection of device connection/disconnection
//! - **MIDI 2.0 Ready** - Types designed for future expansion
//!
//! ## Architecture
//!
//! ```text
//! midir callback (μs timestamp preserved)
//!     │
//!     ▼
//! ┌─────────────────────────────────────────┐
//! │    MidiEventQueue (lock-free)           │
//! │    - MPSC crossbeam channel             │
//! │    - 4096 event capacity                │
//! └─────────────────────────────────────────┘
//!     │
//!     ▼
//! ┌─────────────────────────────────────────┐
//! │    MidiHandler::tick (runtime loop)     │
//! │    - Drains queue, routes to voices     │
//! │    - Non-blocking runtime dispatch      │
//! └─────────────────────────────────────────┘
//!     │
//!     ▼
//! ┌─────────────────────────────────────────┐
//! │    MidiClock                            │
//! │    - Timestamp → audio frame            │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Output Path
//!
//! ```text
//! SuperCollider (scsynth)
//!     │
//!     │ SendTrig /tr messages
//!     ▼
//! ┌─────────────────────────────────────────┐
//! │    MidiRealtimeService                  │
//! │    - Dedicated thread with own socket   │
//! │    - Decodes trigger data               │
//! │    - Routes to MIDI output devices      │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Callbacks & Routing
//!
//! - `CallbackType::AllData` - Receive all MIDI messages
//! - `CallbackType::KeyboardNote` - Receive note on/off events
//! - `CallbackType::ControlChange(cc)` - Receive specific CC events
//! - `CallbackType::PitchBend` - Receive pitch bend events
//!
//! ## Routing Builders
//!
//! - `KeyboardRouteBuilder` - Map keyboard range with transpose and velocity curves
//! - `NoteRouteBuilder` - Map single notes (e.g., drum pads) with choke groups
//! - `CcRouteBuilder` - Map CCs with parameter curves and ranges

// =============================================================================
// Legacy modules (existing API)
// =============================================================================
mod callbacks;
mod constants;
mod realtime;

// =============================================================================
// New infrastructure modules
// =============================================================================
#[cfg(target_os = "linux")]
mod alsa_ump_input;
mod clock;
mod devices;
mod encoder;
mod events;
mod hotplug;
pub mod looper;
mod mpe;
mod nrpn;
mod parser;
mod per_note_state;
#[cfg(feature = "pipewire-midi2")]
mod pipewire_input;
mod queue;
mod readiness;
mod recording;
mod ump;
mod voice_output;

const MIDI_INPUT_TRANSPORT_MASK: u32 = 0xC000_0000;

// =============================================================================
// Legacy re-exports (existing API, maintained for compatibility)
// =============================================================================
pub use callbacks::{
    canonical_cc_curve_name, parse_note_name, CallbackData, CallbackType, CcRouteBuilder,
    KeyboardRouteBuilder, MidiCallbacks, NoteRouteBuilder, ParameterCurve, StoredCallback,
    VelocityCurve, VelocityMapping,
};
pub use constants::{
    decode_packed_midi, pack_cc, pack_note_off, pack_note_on, pack_pitch_bend, trigger_ids,
    MidiData, MidiTriggerType,
};
pub use realtime::{
    MidiDeviceSender, MidiRealtimeConfig, MidiRealtimeService, MidiRealtimeStats, QueuedMidiEvent,
    ScheduledMidiEvent,
};

// =============================================================================
// New infrastructure re-exports
// =============================================================================

// Core event types (MIDI 2.0-ready)
pub use events::{
    Channel, ControlValue, Group, GroupChannel, MidiMessage, TimestampedMidiEvent, Velocity,
};

// Device management with separate input/output namespaces
pub use devices::{
    MidiDeviceInfo, MidiDeviceManager, MidiInputId, MidiInputInfo, MidiOutputId, MidiOutputInfo,
};

// Event queue for lock-free MIDI processing
pub use queue::{AsyncMidiEventReceiver, MidiEventQueue, MidiEventSender, QueueStats};

// Clock synchronization
pub use clock::{MidiClock, MidiClockSync};

// MIDI message parsing
pub use parser::{parse_midi_bytes, MidiParser};

#[cfg(feature = "pipewire-midi2")]
pub use pipewire_input::{
    is_pipewire_midi_input_id, list_pipewire_midi2_inputs, open_pipewire_midi2_input,
    parse_pipewire_midi_pod, pipewire_midi_input_id, PipeWireMidiInputConnection,
    PipeWireMidiInputInfo, PIPEWIRE_MIDI_INPUT_FLAG,
};

// ALSA raw UMP endpoints (Linux): full-resolution MIDI 2.0 without PipeWire.
#[cfg(target_os = "linux")]
pub use alsa_ump_input::{
    alsa_ump_input_id, is_alsa_ump_input_id, list_alsa_ump_inputs, open_alsa_ump_input,
    AlsaUmpInputConnection, AlsaUmpInputInfo, ALSA_UMP_INPUT_FLAG,
};

/// No ALSA UMP endpoints off Linux, so nothing can match one.
#[cfg(not(target_os = "linux"))]
#[inline]
pub fn is_alsa_ump_input_id(_id: crate::types::ids::MidiDeviceId) -> bool {
    false
}

/// Placeholder descriptor so the list has a return type off Linux.
#[cfg(not(target_os = "linux"))]
#[derive(Clone, Debug)]
pub struct AlsaUmpInputInfo {
    pub id: crate::types::ids::MidiDeviceId,
    pub name: String,
    pub node: String,
}

#[cfg(not(target_os = "linux"))]
#[inline]
pub fn list_alsa_ump_inputs() -> Vec<AlsaUmpInputInfo> {
    Vec::new()
}

/// Without the `pipewire-midi2` feature there are no PipeWire device ids, so
/// nothing can ever match one. Kept unconditional so call sites stay free of
/// `#[cfg]` noise.
#[cfg(not(feature = "pipewire-midi2"))]
#[inline]
pub fn is_pipewire_midi_input_id(_id: crate::types::ids::MidiDeviceId) -> bool {
    false
}

/// No PipeWire device ids exist without the feature.
#[cfg(not(feature = "pipewire-midi2"))]
#[inline]
pub fn list_pipewire_midi2_inputs() -> Vec<PipeWireMidiInputInfo> {
    Vec::new()
}

/// Placeholder device descriptor so `list_pipewire_midi2_inputs` has a return
/// type without the feature. Field-compatible with the real one; never
/// constructed, because the list is always empty.
#[cfg(not(feature = "pipewire-midi2"))]
pub struct PipeWireMidiInputInfo {
    pub id: crate::types::ids::MidiDeviceId,
    pub name: String,
    pub target_object: String,
}

// MIDI message encoding
pub use encoder::{
    encode_cc, encode_channel_aftertouch, encode_note_off, encode_note_on, encode_pitch_bend,
    encode_poly_aftertouch, encode_program_change, MidiEncoder,
};

// MPE support
pub use mpe::{MpeConfig, MpeNoteState, MpeState, MpeZone, CC_TIMBRE, DEFAULT_PITCH_BEND_RANGE};

// NRPN/RPN support
pub use nrpn::{
    NrpnDecoder, ParameterMessage, CC_DATA_ENTRY_LSB, CC_DATA_ENTRY_MSB, CC_NRPN_LSB, CC_NRPN_MSB,
    CC_RPN_LSB, CC_RPN_MSB,
};

// Device hot-plug detection
pub use hotplug::{AutoReconnect, AutoReconnectConfig, HotPlugEvent, HotPlugWatcher};

// MIDI Recording
pub use recording::{MidiRecording, MidiRecordingInfo, RecordedMidiCc, RecordedMidiNote};

pub use readiness::{
    is_midi_input_intent_id, midi_input_intent_id, MidiEndpointReadiness, MidiInputIntent,
    MidiReadiness, MidiReadinessState,
};
pub(crate) use readiness::{LegacyInputAction, LegacyMidiPort, MidiInputIntentRuntime};

// MIDI Voice Output (for MIDI voices with CC mappings)
pub use voice_output::{is_midi_voice, send_cc_for_param, value_to_cc};

// Per-note state tracking for MIDI 2.0 and MPE
pub use per_note_state::{PerNoteState, PerNoteStateManager};

// UMP (Universal MIDI Packet) encoding/decoding
pub use ump::{encode_ump, UmpParser};

// Looper
pub use looper::{LooperAction, LooperInstance, LooperManager, LooperPhase};
