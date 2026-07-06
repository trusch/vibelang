//! Feature handlers for vibelang-core.
//!
//! Handlers implement the feature traits and manage specific areas of functionality.
//! Each handler holds references to the shared backend and state, delegating
//! audio operations to the backend while maintaining runtime state.
//!
//! # Architecture
//!
//! ```text
//! Runtime
//!   │
//!   ├── TransportHandler   → Tempo, playback, timing
//!   ├── GroupsHandler      → Audio bus groups, routing
//!   ├── VoicesHandler      → Synth voice management
//!   ├── PatternsHandler    → Step sequencer patterns
//!   ├── MelodiesHandler    → Pitched note sequences
//!   ├── SequencesHandler   → Arrangement clips
//!   ├── FadesHandler       → Parameter automation
//!   ├── EffectsHandler     → Audio effects
//!   ├── SamplesHandler     → Audio file loading
//!   ├── SfzHandler         → SFZ instrument loading
//!   ├── RecordingsHandler  → Audio recording
//!   ├── SynthDefsHandler   → SynthDef management
//!   └── MidiHandler        → MIDI I/O (feature-gated)
//! ```
//!
//! # Handler Pattern
//!
//! All handlers follow this pattern:
//!
//! ```ignore
//! pub struct ExampleHandler<B: Backend> {
//!     backend: Arc<B>,           // Shared backend for audio operations
//!     state: Arc<RwLock<State>>, // Shared mutable state
//! }
//!
//! impl<B: Backend> ExampleHandler<B> {
//!     pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
//!         Self { backend, state }
//!     }
//! }
//!
//! #[async_trait]
//! impl<B: Backend> ExampleTrait for ExampleHandler<B> {
//!     async fn do_something(&self) -> Result<()> {
//!         // 1. Lock state briefly to read/write
//!         let data = {
//!             let state = self.state.read().await;
//!             state.some_data.clone()
//!         };
//!
//!         // 2. Call backend with lock released
//!         self.backend.some_operation().await?;
//!
//!         Ok(())
//!     }
//! }
//! ```
//!
//! # Lock Discipline
//!
//! Handlers follow a strict lock discipline to avoid deadlocks:
//!
//! 1. **Hold locks briefly**: Release locks before await points
//! 2. **Never hold locks across backend calls**: Backend calls may take time
//! 3. **Collect data, then act**: Read state, release lock, then call backend

mod effects;
mod fades;
mod groups;
mod melodies;
mod patterns;
mod routes;
mod samples;
mod sequences;
mod sfz;
mod synthdefs;
mod transport;
mod voices;

#[cfg(not(target_arch = "wasm32"))]
mod recordings;

#[cfg(feature = "midi")]
mod midi;

pub use effects::EffectsHandler;
pub use fades::FadesHandler;
pub use groups::GroupsHandler;
pub use melodies::MelodiesHandler;
pub use patterns::PatternsHandler;
pub use routes::{
    compute_input_route_diff, default_routes_for_voice, merge_default_routes, InputRoute,
    InputRouteDiff, InputRouteMap, InputRouteSrc, ParamRoute, ParamRouteDiff, ParamRouteMap,
    ParamRouteTarget, Route, RouteDest, RouteDiff, RouteMap, RoutesHandler,
};
pub use samples::SamplesHandler;
pub use sequences::SequencesHandler;
pub use sfz::{sfz_note_spawn_params, SfzHandler, SfzNoteSpawn};
pub use synthdefs::SynthDefsHandler;
pub use transport::TransportHandler;
pub(crate) use voices::{voice_is_gated, voice_release_grace};
pub use voices::VoicesHandler;

#[cfg(not(target_arch = "wasm32"))]
pub use recordings::RecordingsHandler;

#[cfg(feature = "midi")]
pub use midi::{MidiEventNotification, MidiHandler, MidiMessage};
