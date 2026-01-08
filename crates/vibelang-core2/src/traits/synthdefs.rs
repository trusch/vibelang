//! SynthDefs trait for synthesis definition management.
//!
//! SynthDefs are compiled synthesis graphs that can be instantiated as synths.

use crate::Result;
use async_trait::async_trait;

/// SynthDef management for synthesis definitions.
///
/// SynthDefs must be loaded before they can be used by voices and effects.
///
/// All methods are async for WASM compatibility.
#[async_trait]
pub trait SynthDefs: Send + Sync {
    /// Load a synthdef from compiled bytes.
    ///
    /// # Arguments
    ///
    /// * `name` - Name to register the synthdef under
    /// * `data` - Compiled synthdef bytes
    async fn load(&self, name: &str, data: &[u8]) -> Result<()>;

    /// Check if a synthdef is loaded.
    async fn is_loaded(&self, name: &str) -> bool;
}
