//! VibeLang Runtime - Orchestrates the entire VibeLang system.
//!
//! The runtime manages:
//! - SuperCollider process lifecycle
//! - State manager thread
//! - Beat scheduling
//! - Message passing between API and audio engine
//!
//! # Module Structure
//!
//! - `handle` - RuntimeHandle for API access
//! - `handlers` - Message handlers (transport, groups, voices, etc.)
//! - `thread` - Main runtime thread and Runtime struct

pub mod handle;
pub mod handlers;
pub mod thread;

pub use handle::RuntimeHandle;
pub use handlers::RuntimeContext;
pub use thread::Runtime;

// Re-export system synthdef creation from vibelang-dsp
pub use vibelang_dsp::system_synthdefs::create_system_link_audio_bytes;
