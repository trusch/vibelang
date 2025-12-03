//! VibeLang Runtime - Orchestrates the entire VibeLang system.
//!
//! The runtime manages:
//! - SuperCollider process lifecycle
//! - State manager thread
//! - Beat scheduling
//! - Message passing between API and audio engine

pub mod thread;

pub use thread::{Runtime, RuntimeHandle};
