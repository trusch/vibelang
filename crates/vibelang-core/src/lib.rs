#![warn(clippy::unwrap_used)]
//! # vibelang-core2
//!
//! A clean, trait-based runtime for VibeLang that is generic over synthesis backends.
//!
//! This crate provides the core audio engine for VibeLang, handling timing, scheduling,
//! audio graph management, and communication with synthesis backends like SuperCollider.
//!
//! ## Design Goals
//!
//! - **Simplicity**: One core type ([`Runtime<B>`]) with clear responsibilities
//! - **Modularity**: Feature traits with handler delegation for testability
//! - **Platform-agnostic**: Works for native (scsynth) and WASM (WebAudio)
//! - **Async-first**: Uses async channels and traits for cross-platform compatibility
//! - **Type-safe**: Newtype IDs prevent mixing up different resource types
//!
//! ## Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        Runtime<B>                           │
//! │  ┌─────────────┐  ┌──────────────┐  ┌───────────────────┐  │
//! │  │   Message   │──│   Handlers   │──│   Backend<B>      │  │
//! │  │   Channel   │  │  (dispatch)  │  │  (scsynth/WASM)   │  │
//! │  └─────────────┘  └──────────────┘  └───────────────────┘  │
//! │         │                │                    │             │
//! │  ┌──────▼──────┐  ┌──────▼──────┐  ┌─────────▼─────────┐  │
//! │  │ Transport   │  │   Groups    │  │     Voices        │  │
//! │  │ Clock/Tempo │  │   Routing   │  │   Patterns        │  │
//! │  └─────────────┘  └─────────────┘  │   Melodies        │  │
//! │                                     └───────────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! The [`Runtime`] is generic over a [`Backend`] trait, allowing different synthesis
//! engines to be used interchangeably. All operations are performed via [`Message`]s
//! sent through an async channel, which the runtime dispatches to feature handlers.
//!
//! ## Quick Start
//!
//! ```ignore
//! use vibelang_core::{Runtime, Message, GroupId, VoiceId, VoiceConfig};
//! use vibelang_core::message::{TransportMessage, GroupMessage, VoiceMessage};
//! use vibelang_core::backends::{ScsynthBackend, ScsynthProcess, ScsynthConfig};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 1. Start the scsynth process
//!     let process = ScsynthProcess::spawn(ScsynthConfig::default())?;
//!     process.wait_startup(Duration::from_secs(2)).await;
//!
//!     // 2. Connect to scsynth and create the runtime
//!     let backend = ScsynthBackend::connect(&process.addr()).await?;
//!     let mut runtime = Runtime::new(backend);
//!     let handle = runtime.handle();
//!
//!     // 3. Spawn the runtime loop
//!     tokio::spawn(async move {
//!         runtime.run().await;
//!     });
//!
//!     // 4. Send messages to control audio
//!     handle.send(TransportMessage::SetTempo { bpm: 128.0 }.into()).await?;
//!     handle.send(GroupMessage::Create { id: GroupId::new(1), name: "main".to_string(), parent: None }.into()).await?;
//!     handle.send(TransportMessage::Start.into()).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Feature Flags
//!
//! - `native` (default): Enables native platform support with scsynth backend
//! - `midi`: Enables MIDI device enumeration and I/O via midir
//!
//! ## Module Overview
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`types`] | Type-safe IDs ([`NodeId`], [`GroupId`], etc.), time types ([`Beat`], [`Duration`]) |
//! | [`traits`] | Feature trait definitions ([`Transport`], [`Groups`], [`Voices`], etc.) |
//! | [`handlers`] | Trait implementations that manage state and backend calls |
//! | [`backends`] | Backend implementations ([`ScsynthBackend`], process management) |
//! | [`synthdefs`] | Built-in SynthDef generation for essential audio routing |
//!
//! ## Key Concepts
//!
//! ### Messages
//!
//! All runtime mutations happen through [`Message`]s. Messages are organized by feature:
//!
//! - [`TransportMessage`](message::TransportMessage): Tempo, time signature, play/stop
//! - [`GroupMessage`](message::GroupMessage): Audio bus groups for mixing
//! - [`VoiceMessage`](message::VoiceMessage): Synth voice creation and triggering
//! - [`PatternMessage`](message::PatternMessage): Step sequencer patterns
//! - [`MelodyMessage`](message::MelodyMessage): Note sequences with timing
//!
//! ### Groups and Routing
//!
//! Audio flows through a hierarchy of groups. Each group has its own audio bus,
//! and `system_link_audio` synths route audio from child buses to parent buses:
//!
//! ```text
//! Group 1 (drums) ──┐
//!                   ├──► Main Output (bus 0)
//! Group 2 (bass)  ──┘
//! ```
//!
//! ### Timing
//!
//! The [`Beat`] type uses fixed-point arithmetic (16 fractional bits) for
//! sample-accurate timing without floating-point drift over long sessions.

// Core modules
mod backend;
mod clock;
pub mod compat;
mod error;
mod runtime;
mod state;
mod transport_snapshot;

// Public modules
pub mod audio;
pub mod backends;
pub mod handlers;
pub mod message;
pub mod reload;
pub mod synthdefs;
pub mod traits;
pub mod types;
pub mod validation;

#[cfg(feature = "midi")]
pub mod midi;

// Re-exports for convenience
pub use audio::{AudioConnector, AudioError, PortDirection, PortMatcher};
pub use backend::{AddAction, Backend, BufferInfo};
pub use error::{Error, Result, SynthDefRejection};

pub use message::{
    EffectMessage, FadeMessage, GroupMessage, MelodyMessage, Message, PatternMessage,
    ReloadMessage, SampleMessage, SequenceMessage, SyncMessage, SynthDefMessage, TransportMessage,
    VoiceMessage,
};
pub use runtime::{Runtime, RuntimeHandle};
pub use state::{
    ActiveFade, EffectState, GroupState, MelodyState, MeterLevel, PatternOwner, PatternState,
    SequenceState, SfzInstrumentState, SfzRegionState, State, VoiceRole, VoiceState,
};
pub use transport_snapshot::TransportSnapshot;

// Re-export the scsynth backend when available (native only)
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub use backends::{
    setup_metering, setup_node_tracking, ProcessError, ScsynthBackend, ScsynthConfig,
    ScsynthProcess,
};

// Re-export the web backend for WASM
#[cfg(target_arch = "wasm32")]
pub use backends::{WebScsynthBackend, WebScsynthError};

// Re-export types at crate root for convenience
pub use types::{
    ids::{
        BufferId, BusId, EffectId, GroupId, MelodyId, NodeId, PatternId, SampleId, SequenceId,
        VoiceId,
    },
    params::ParamMap,
    time::{Beat, Duration, TimeSignature},
};

#[cfg(feature = "midi")]
pub use types::ids::MidiDeviceId;

// Re-export config types (always available)
pub use traits::{
    Clip, FadeConfig, FadeCurve, FadeTarget, MelodyConfig, NoteEvent, PatternConfig, SampleConfig,
    SampleInfo, SequenceConfig, SfzConfig, Step, VoiceConfig,
};

// Re-export traits at crate root (native only, as they require Send+Sync)
#[cfg(not(target_arch = "wasm32"))]
pub use traits::{
    Effects, Fades, Groups, Melodies, Patterns, Samples, Sequences, SynthDefs, Transport, Voices,
};

// Re-export validation (native only)
#[cfg(not(target_arch = "wasm32"))]
pub use validation::{limits, Validate};

#[cfg(feature = "midi")]
pub use traits::{Midi, MidiDeviceInfo};

#[cfg(feature = "midi")]
pub use midi::{
    decode_packed_midi, pack_cc, pack_note_off, pack_note_on, pack_pitch_bend, parse_note_name,
    trigger_ids, CallbackData, CallbackType, CcRouteBuilder, KeyboardRouteBuilder, MidiCallbacks,
    MidiData, MidiDeviceSender, MidiRealtimeConfig, MidiRealtimeService, MidiRealtimeStats,
    MidiTriggerType, NoteRouteBuilder, ParameterCurve, QueuedMidiEvent, StoredCallback,
    VelocityCurve, VelocityMapping,
};

#[cfg(all(feature = "midi", not(target_arch = "wasm32")))]
pub use handlers::{MidiEventNotification, MidiHandler, MidiMessage};
