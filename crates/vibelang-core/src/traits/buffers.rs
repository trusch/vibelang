//! Script-allocated audio-rate buffers.
//!
//! Unlike sample-loaded buffers (which read audio from a file), these
//! are empty memory regions sized in frames × channels. They survive
//! hot-reload as long as the `allocate_buffer(name, ...)` call still
//! appears in the script — the reload diff treats unchanged entries as
//! no-ops, so the SC buffer is reused and its contents persist across
//! synthdef recompiles.
//!
//! Used by synthdefs that need persistent audio-rate scratch memory —
//! e.g. `spectraphon`'s 65,536-float Array of stored magnitudes.
//!
//! Buffer alloc/free is handled inline by the reload pipeline in
//! [`crate::Runtime::apply_reload`] via the backend's
//! [`crate::Backend::alloc_buffer`] and [`crate::Backend::free_buffer`].

/// Configuration for a script-allocated buffer.
///
/// Stored in [`crate::reload::ScriptState::buffers`] keyed by a
/// name-derived [`crate::types::BufferId`] (see
/// `vibelang-rhai::api::buffer::allocate_buffer`). The reload diff
/// compares old and new entries by `PartialEq`, so changing
/// `frames`/`channels` triggers a free + re-alloc cycle while an
/// unchanged config is a no-op.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferConfig {
    /// Script-side name (for diagnostics).
    pub name: String,

    /// Number of sample frames.
    pub frames: u32,

    /// Number of channels (1 = mono, 2 = stereo, ...).
    pub channels: u16,
}

impl BufferConfig {
    /// Create a new buffer configuration.
    pub fn new(name: impl Into<String>, frames: u32, channels: u16) -> Self {
        Self {
            name: name.into(),
            frames,
            channels,
        }
    }
}
