//! Script-allocated buffer API for Rhai scripts.
//!
//! Top-level audio-rate buffers that survive hot-reload. Use this when a
//! synthdef needs persistent scratch memory whose contents must outlive
//! synthdef recompiles — e.g. spectraphon's 65,536-float Array of stored
//! magnitudes captured in SAM mode.
//!
//! ## Example
//!
//! ```rhai
//! // Allocate once at script top level. The same name → same bufnum
//! // across reloads, so the SC buffer is reused (not freed/realloced)
//! // and any captured contents persist.
//! let arr = allocate_buffer("spec_arrays", 65536, 1);
//!
//! voice("spec")
//!     .synth("spectraphon_side")
//!     .set_param("bufnum", arr.bufnum);
//! ```
//!
//! ## ID derivation
//!
//! `BufferId` is hashed (FNV-1a) from the script-side `name` and folded
//! into the reserved range `2048..4096` — well above the
//! sample/recording free-list allocator (which starts at 0) and inside
//! scsynth's bumped `-b 4096` buffer pool. Collisions inside a single
//! script run are detected and resolved via linear probing, mirroring
//! the entity-ID logic in [`crate::context`]. Across reloads the same
//! name resolves to the same ID deterministically.

use rhai::{CustomType, Engine, TypeBuilder};
use vibelang_core::traits::BufferConfig;
use vibelang_core::types::BufferId;

use crate::context;

/// Reserved bufnum range for script-allocated buffers.
///
/// Lower bound is well above the sample/recording free-list allocator
/// (which grows from 0). Upper bound matches scsynth's bumped `-b 4096`
/// default — see `ScsynthConfig::num_buffers`.
const SCRIPT_BUFFER_MIN: u32 = 2048;
const SCRIPT_BUFFER_MAX: u32 = 4096;
const SCRIPT_BUFFER_RANGE: u32 = SCRIPT_BUFFER_MAX - SCRIPT_BUFFER_MIN;

/// FNV-1a hash → bufnum in `SCRIPT_BUFFER_MIN..SCRIPT_BUFFER_MAX`.
fn hash_name_to_bufnum(name: &str) -> u32 {
    const FNV_OFFSET_BASIS: u32 = 2166136261;
    const FNV_PRIME: u32 = 16777619;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    SCRIPT_BUFFER_MIN + (hash % SCRIPT_BUFFER_RANGE)
}

/// Handle to a script-allocated buffer.
///
/// `bufnum` is exposed as `f64` (Rhai `FLOAT`) because synthdef params
/// are float-typed — `voice.set_param("bufnum", h.bufnum)` would
/// otherwise hit Rhai's strict no-implicit-int→float coercion. SC
/// bufnums comfortably fit in `f64`'s 53-bit integer range.
#[derive(Clone, Debug, CustomType)]
pub struct BufferHandle {
    /// Script-side name.
    pub name: String,
    /// SC buffer number — feed to `set_param("bufnum", h.bufnum)`.
    pub bufnum: f64,
}

impl BufferHandle {
    pub fn get_name(&mut self) -> String {
        self.name.clone()
    }

    pub fn get_bufnum(&mut self) -> f64 {
        self.bufnum
    }
}

/// Allocate a top-level audio-rate buffer that survives hot-reload.
///
/// The returned [`BufferHandle`] exposes `.bufnum` for wiring to
/// synthdef parameters via `voice.set_param("bufnum", h.bufnum)`. The
/// SC buffer is `b_alloc`-ed by the runtime on the first reload that
/// sees the call; subsequent reloads with an unchanged
/// `(name, frames, channels)` tuple are no-ops and the buffer's
/// contents are preserved.
pub fn allocate_buffer(name: String, frames: i64, channels: i64) -> BufferHandle {
    let frames = frames.max(1) as u32;
    let channels = channels.clamp(1, 16) as u16;

    let buffer_id = context::with_state(|state| {
        // Linear-probe within the reserved range until we find a slot
        // that is either free or already owned by this name. The probe
        // is deterministic, so the same name always lands at the same ID
        // across reloads (assuming the script's set of named buffers is
        // stable — collision-induced shifts only kick in when *another*
        // name with a colliding hash is also present).
        let start_raw = hash_name_to_bufnum(&name);
        let mut raw = start_raw;
        loop {
            let candidate = BufferId::new(raw);
            match state.buffers.get(&candidate) {
                Some(existing) if existing.name == name => break candidate,
                Some(_) => {
                    // Collision with a different name — probe forward.
                    raw = SCRIPT_BUFFER_MIN + ((raw - SCRIPT_BUFFER_MIN + 1) % SCRIPT_BUFFER_RANGE);
                    if raw == start_raw {
                        panic!(
                            "allocate_buffer: script buffer ID space exhausted \
                             ({} slots)",
                            SCRIPT_BUFFER_RANGE
                        );
                    }
                }
                None => break candidate,
            }
        }
    });

    let config = BufferConfig::new(name.clone(), frames, channels);
    context::with_state(|state| {
        state.buffers.insert(buffer_id, config);
    });

    BufferHandle {
        name,
        bufnum: buffer_id.raw() as f64,
    }
}

/// Register the buffer API with a Rhai engine.
pub fn register(engine: &mut Engine) {
    engine.build_type::<BufferHandle>();
    engine.register_fn("allocate_buffer", allocate_buffer);
    engine.register_fn("name", BufferHandle::get_name);
    engine.register_get("name", BufferHandle::get_name);
    engine.register_fn("bufnum", BufferHandle::get_bufnum);
    engine.register_get("bufnum", BufferHandle::get_bufnum);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_in_range() {
        for name in &["spec_arrays", "delay_buf", "x", "verylongnamehere"] {
            let bufnum = hash_name_to_bufnum(name);
            assert!(
                (SCRIPT_BUFFER_MIN..SCRIPT_BUFFER_MAX).contains(&bufnum),
                "name {} → bufnum {} out of range",
                name,
                bufnum
            );
        }
    }

    #[test]
    fn test_hash_deterministic() {
        // Same name → same bufnum across calls (and therefore across reloads).
        assert_eq!(
            hash_name_to_bufnum("spec_arrays"),
            hash_name_to_bufnum("spec_arrays")
        );
        assert_ne!(
            hash_name_to_bufnum("spec_arrays"),
            hash_name_to_bufnum("other_buf")
        );
    }

    #[test]
    fn test_allocate_buffer_basic() {
        context::init_context();
        let h = allocate_buffer("spec_arrays".to_string(), 65536, 1);
        assert_eq!(h.name, "spec_arrays");
        let bufnum_u32 = h.bufnum as u32;
        assert!(
            (SCRIPT_BUFFER_MIN..SCRIPT_BUFFER_MAX).contains(&bufnum_u32),
            "bufnum {} not in script range",
            bufnum_u32
        );

        let id = BufferId::new(bufnum_u32);
        let frames_channels = context::with_state(|s| {
            let cfg = s.buffers.get(&id).expect("buffer should be in state");
            (cfg.name.clone(), cfg.frames, cfg.channels)
        });
        assert_eq!(frames_channels, ("spec_arrays".to_string(), 65536, 1));
        context::clear_context();
    }

    #[test]
    fn test_allocate_buffer_idempotent_same_name() {
        context::init_context();
        let h1 = allocate_buffer("spec_arrays".to_string(), 65536, 1);
        let h2 = allocate_buffer("spec_arrays".to_string(), 65536, 1);
        assert_eq!(h1.bufnum, h2.bufnum);
        let n_buffers = context::with_state(|s| s.buffers.len());
        assert_eq!(
            n_buffers, 1,
            "duplicate allocate_buffer with same name must not add a second entry"
        );
        context::clear_context();
    }

    #[test]
    fn test_allocate_buffer_different_names_different_ids() {
        context::init_context();
        let a = allocate_buffer("buf_a".to_string(), 1024, 1);
        let b = allocate_buffer("buf_b".to_string(), 1024, 1);
        assert_ne!(a.bufnum, b.bufnum);
        context::clear_context();
    }

    #[test]
    fn test_allocate_buffer_clamps_channels() {
        context::init_context();
        let h = allocate_buffer("clamped".to_string(), 1024, 99);
        let id = BufferId::new(h.bufnum as u32);
        let channels = context::with_state(|s| s.buffers[&id].channels);
        assert_eq!(channels, 16);
        context::clear_context();
    }
}
