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
#[async_trait]
pub trait Effects: Send + Sync {
    /// Add an effect to a group.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique ID for the effect
    /// * `group` - Group to add the effect to
    /// * `synthdef` - Name of the effect synthdef
    /// * `params` - Initial parameter values
    async fn add(
        &self,
        id: EffectId,
        group: GroupId,
        synthdef: &str,
        params: &ParamMap,
    ) -> Result<()>;

    /// Remove an effect.
    async fn remove(&self, id: EffectId) -> Result<()>;

    /// Set an effect parameter.
    async fn set_param(&self, id: EffectId, param: &str, value: f32) -> Result<()>;
}
