//! MIDI infrastructure for vibelang-core2.
//!
//! This module provides a professional-grade MIDI implementation with:
//!
//! ## Core Infrastructure
//! - **Event-driven processing** - Dedicated async task (no polling)
//! - **Timestamp preservation** - Microsecond-accurate timing from midir
//! - **Jitter compensation** - Statistical analysis for stable timing
//! - **Lock-free queues** - High-performance event routing
//!
//! ## Full MIDI 1.0 Support
//! - All channel voice messages (note, CC, aftertouch, pitch bend)
//! - System exclusive (SysEx) with multi-packet support
//! - System realtime (clock, start, stop, continue)
//! - NRPN/RPN 14-bit parameter control
//!
//! ## Advanced Features
//! - **MPE (MIDI Polyphonic Expression)** - Per-note expression for Seaboard, Linnstrument, etc.
//! - **MIDI Learn** - Automatic CC-to-parameter mapping
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
//! │    - SPSC crossbeam channel             │
//! │    - 4096 event capacity                │
//! └─────────────────────────────────────────┘
//!     │
//!     ▼
//! ┌─────────────────────────────────────────┐
//! │    MidiInputTask (dedicated async)      │
//! │    - Immediate processing (<1ms)        │
//! │    - Jitter compensation                │
//! │    - MPE/NRPN decoding                  │
//! └─────────────────────────────────────────┘
//!     │
//!     ▼
//! ┌─────────────────────────────────────────┐
//! │    MidiClock                            │
//! │    - Timestamp → audio frame            │
//! │    - External sync (BPM derivation)     │
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
mod clock;
mod devices;
mod encoder;
mod events;
mod hotplug;
mod input_task;
mod jitter;
mod learn;
mod mpe;
mod nrpn;
mod parser;
mod queue;
mod recording;
mod voice_output;

// =============================================================================
// Legacy re-exports (existing API, maintained for compatibility)
// =============================================================================
pub use callbacks::{
    CallbackData, CallbackType, CcRouteBuilder, KeyboardRouteBuilder, MidiCallbacks,
    NoteRouteBuilder, ParameterCurve, StoredCallback, VelocityCurve, VelocityMapping,
    parse_note_name,
};
pub use constants::{decode_packed_midi, trigger_ids, MidiData, MidiTriggerType, pack_note_on, pack_note_off, pack_cc, pack_pitch_bend};
pub use realtime::{MidiRealtimeConfig, MidiRealtimeService, MidiRealtimeStats, QueuedMidiEvent, MidiDeviceSender};

// =============================================================================
// New infrastructure re-exports
// =============================================================================

// Core event types (MIDI 2.0-ready)
pub use events::{
    Channel, ControlValue, MidiMessage, TimestampedMidiEvent, Velocity,
};

// Device management with separate input/output namespaces
pub use devices::{
    MidiDeviceInfo, MidiDeviceManager, MidiInputId, MidiInputInfo, MidiOutputId, MidiOutputInfo,
};

// Event queue for lock-free MIDI processing
pub use queue::{MidiEventQueue, AsyncMidiEventReceiver, MidiEventSender, QueueStats};

// Clock synchronization
pub use clock::{MidiClock, MidiClockSync};

// Jitter compensation
pub use jitter::{JitterCompensator, JitterStats};

// Dedicated input task (replaces polling)
pub use input_task::{
    CallbackHandler, ChannelHandler, MidiEventHandler, MidiInputTask, MidiInputTaskBuilder,
};

// MIDI message parsing
pub use parser::{parse_midi_bytes, MidiParser};

// MIDI message encoding
pub use encoder::{
    encode_cc, encode_channel_aftertouch, encode_note_off, encode_note_on, encode_pitch_bend,
    encode_poly_aftertouch, encode_program_change, MidiEncoder,
};

// MPE support
pub use mpe::{MpeConfig, MpeNoteState, MpeState, MpeZone, CC_TIMBRE, DEFAULT_PITCH_BEND_RANGE};

// NRPN/RPN support
pub use nrpn::{NrpnDecoder, ParameterMessage, CC_DATA_ENTRY_LSB, CC_DATA_ENTRY_MSB, CC_NRPN_LSB, CC_NRPN_MSB, CC_RPN_LSB, CC_RPN_MSB};

// MIDI Learn
pub use learn::{LearnManager, LearnTarget, LearnedMapping, MidiLearn};

// Device hot-plug detection
pub use hotplug::{AutoReconnect, AutoReconnectConfig, HotPlugEvent, HotPlugWatcher};

// MIDI Recording
pub use recording::{MidiRecording, MidiRecordingInfo, RecordedMidiCc, RecordedMidiNote};

// MIDI Voice Output (for MIDI voices with CC mappings)
pub use voice_output::{
    get_modulator_cc_mappings, has_modulator_cc_mappings, is_midi_voice, send_cc_for_param,
    send_modulator_ccs, value_to_cc, ModulatorCcMapping,
};
