//! Effects trait for audio processing.
//!
//! Effects process audio passing through a group.

use crate::types::{EffectId, GroupId, ParamMap};
use crate::Result;
use async_trait::async_trait;

/// Effect management for audio processing.
///
/// Effects are inserted into groups to process audio. Multiple effects
/// can be chained in order.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait Effects: Send + Sync {
    /// Add an effect to a group.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique ID for the effect
    /// * `name` - Script name the effect was declared under (`fx("name")`),
    ///   stored so callers can resolve the effect by name without inverting
    ///   the id hash
    /// * `group` - Group to add the effect to
    /// * `synthdef` - Name of the effect synthdef
    /// * `params` - Initial parameter values
    async fn add(
        &self,
        id: EffectId,
        name: &str,
        group: GroupId,
        synthdef: &str,
        params: &ParamMap,
    ) -> Result<()>;

    /// Remove an effect.
    async fn remove(&self, id: EffectId) -> Result<()>;

    /// Set an effect parameter.
    async fn set_param(&self, id: EffectId, param: &str, value: f32) -> Result<()>;
}

/// Effect management for audio processing (WASM version).
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait Effects {
    /// Add an effect to a group.
    async fn add(
        &self,
        id: EffectId,
        name: &str,
        group: GroupId,
        synthdef: &str,
        params: &ParamMap,
    ) -> Result<()>;

    /// Remove an effect.
    async fn remove(&self, id: EffectId) -> Result<()>;

    /// Set an effect parameter.
    async fn set_param(&self, id: EffectId, param: &str, value: f32) -> Result<()>;
}
