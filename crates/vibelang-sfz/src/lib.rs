//! SFZ instrument support for VibeLang.
//!
//! This crate provides comprehensive SFZ (Sample Format) support including:
//! - SFZ file parsing and loading
//! - Sample buffer management
//! - Region matching based on key/velocity/trigger
//! - Full SFZ opcode support
//!
//! # Architecture
//!
//! The crate is designed to be independent of the audio backend. It provides:
//! - Type definitions for SFZ instruments and regions
//! - A loader that uses a callback for buffer allocation
//! - Region matching logic for note triggering
//!
//! # Example
//!
//! ```ignore
//! use vibelang_sfz::{load_sfz_instrument, find_matching_regions, RoundRobinState, TriggerMode};
//!
//! // Load an SFZ instrument
//! let mut next_buffer_id = 100;
//! let instrument = load_sfz_instrument(
//!     "path/to/instrument.sfz",
//!     "my_instrument".to_string(),
//!     |path, buffer_id| {
//!         // Load buffer into your audio backend
//!         Ok(())
//!     },
//!     &mut next_buffer_id,
//! )?;
//!
//! // Find matching regions for a note
//! let mut rr_state = RoundRobinState::new();
//! let regions = find_matching_regions(&instrument, 60, 100, TriggerMode::Attack, &mut rr_state);
//! ```

pub mod parser;
pub mod types;
pub mod loader;
pub mod region_matcher;
pub mod api;
pub mod synthdef;

pub use types::*;
pub use loader::*;
pub use region_matcher::*;
pub use api::*;
pub use synthdef::create_sfz_synthdefs;

// Re-export parser types for convenience
pub use parser::{TriggerMode, LoopMode};
