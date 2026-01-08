//! VibeLang Rhai - Scripting layer for VibeLang.
//!
//! This crate provides the Rhai scripting API for VibeLang, allowing users to write
//! `.vibe` scripts that define musical compositions.
//!
//! # Architecture
//!
//! The scripting layer works by:
//! 1. Executing a Rhai script that calls builder APIs (voice(), pattern(), etc.)
//! 2. These builders collect their configuration into a thread-local ScriptState
//! 3. After execution, the ScriptState is applied to the Runtime via the reload system
//!
//! # Example
//!
//! ```ignore
//! use vibelang_rhai::ScriptEngine;
//! use vibelang_core2::Runtime;
//!
//! let backend = /* create backend */;
//! let runtime = Runtime::new(backend).await?;
//! let mut engine = ScriptEngine::new(runtime.handle());
//!
//! engine.execute_file("song.vibe").await?;
//! ```

pub mod api;
pub mod context;
pub mod engine;
pub mod error;

// Re-exports
pub use engine::ScriptEngine;
pub use error::{Error, Result};
