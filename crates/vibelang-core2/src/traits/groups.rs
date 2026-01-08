//! Groups trait for hierarchical audio routing.
//!
//! Groups organize voices and effects into a hierarchy for mixing and routing.

use crate::types::GroupId;
use crate::Result;
use async_trait::async_trait;

/// Group management for hierarchical audio routing.
///
/// Groups contain voices and effects, and can be nested to create
/// complex mixing hierarchies (e.g., drums.kicks, drums.snares).
#[async_trait]
pub trait Groups: Send + Sync {
    /// Create a new group.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique ID for the new group
    /// * `parent` - Optional parent group (None for root-level)
    async fn create(&self, id: GroupId, parent: Option<GroupId>) -> Result<()>;

    /// Delete a group and all its contents.
    async fn delete(&self, id: GroupId) -> Result<()>;

    /// Set a parameter on a group.
    ///
    /// Common parameters: "gain", "pan", etc.
    async fn set_param(&self, id: GroupId, param: &str, value: f32) -> Result<()>;

    /// Mute or unmute a group.
    ///
    /// Muted groups don't produce audio but retain their state.
    async fn mute(&self, id: GroupId, muted: bool) -> Result<()>;

    /// Solo a group.
    ///
    /// When any group is soloed, only soloed groups produce audio.
    async fn solo(&self, id: GroupId, solo: bool) -> Result<()>;
}
