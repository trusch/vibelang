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
//! use vibelang_core::Runtime;
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

// Optional extensions module (feature-gated)
#[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
pub mod extensions;

// Re-exports
pub use engine::{HostMutation, ScriptEngine};
pub use error::{Error, Result};

#[cfg(feature = "api-manifest")]
pub fn public_api_metadata_json() -> std::result::Result<String, String> {
    ScriptEngine::public_api_metadata_json()
}

// Exit code handling for integration tests
pub use api::assert::{get_exit_code, reset_exit_code};

// Re-export extension config when any extension is enabled
#[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
pub use extensions::ExtensionConfig;
