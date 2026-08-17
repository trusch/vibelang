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
//!     .synth("spectraphon")
//!     .set_param("array_buf", arr.bufnum);
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

use rhai::{EvalAltResult, Position};
use vibelang_core::candidate::{
    AuthoringDeclaration, BufferAuthoring, BufferKind, BufferReplacementAuthoring, Cancellation,
    CandidateError, Composition, DeclarationOwner, DeclarationPayload, GroupScope, LifecycleAction,
    LifecycleMetadata, TerminalEffect,
};

use crate::foundation::{self, BuilderBase, FoundationError, Observation, RefBase};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BufferRef {
    base: RefBase,
}

impl BufferRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<BufferKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        let source = foundation::operation_source(&self.base, "remove")?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::Remove,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BufferBuilder {
    base: BuilderBase,
    frames: Option<u32>,
    channels: u16,
    replacement: Option<BufferReplacementAuthoring>,
}

impl BufferBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        Self {
            base,
            frames: None,
            channels: 1,
            replacement: None,
        }
    }

    pub fn frames(mut self, frames: i64) -> Result<Self, FoundationError> {
        if frames < 1 || frames > i64::from(u32::MAX) {
            return Err(CandidateError::InvalidAuthoring(
                "Buffer frames must be in 1..=4294967295".into(),
            )
            .into());
        }
        self.frames = Some(frames as u32);
        Ok(self)
    }

    pub fn channels(mut self, channels: i64) -> Result<Self, FoundationError> {
        if !(1..=16).contains(&channels) {
            return Err(CandidateError::InvalidAuthoring(
                "Buffer channels must be in 1..=16".into(),
            )
            .into());
        }
        self.channels = channels as u16;
        Ok(self)
    }

    /// Declare the v2-required replacement policy: clear on shape change.
    #[must_use]
    pub fn clear(mut self) -> Self {
        self.replacement = Some(BufferReplacementAuthoring::Clear);
        self
    }

    /// Declare the v2-required replacement policy: copy the overlapping
    /// region on shape change.
    #[must_use]
    pub fn copy_overlap(mut self) -> Self {
        self.replacement = Some(BufferReplacementAuthoring::CopyOverlap);
        self
    }

    pub fn apply(self) -> Result<BufferRef, FoundationError> {
        let frames = self.frames.ok_or_else(|| {
            CandidateError::InvalidAuthoring("BufferBuilder needs .frames(..) before apply".into())
        })?;
        let replacement = self.replacement.ok_or_else(|| {
            CandidateError::InvalidAuthoring(
                "BufferBuilder needs an explicit replacement policy: declare .clear() or .copy_overlap()"
                    .into(),
            )
        })?;
        let declaration = BufferAuthoring {
            frames,
            channels: self.channels,
            replacement,
        };
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::Buffer(declaration))?;
        let owner = DeclarationOwner::Structural(self.base.source().syntax_key().clone());
        let reference = self.base.apply(
            owner,
            LifecycleMetadata::register(Composition::Standalone),
            payload,
        )?;
        BufferRef::new(reference)
    }
}

pub(crate) fn buffer_builder_v2(name: String) -> Result<BufferBuilder, Box<EvalAltResult>> {
    Ok(BufferBuilder::new(
        foundation::authoring_builder::<BufferKind>(&name, GroupScope::root())
            .map_err(|error| buffer_v2_error(error, Position::NONE))?,
    ))
}

/// Effective forwarding alias for the v1 `allocate_buffer(name, frames,
/// channels)` shape. The result is still a pure builder: the v2-required
/// replacement policy and the `apply` terminal remain explicit.
pub(crate) fn allocate_buffer_v2(
    name: String,
    frames: i64,
    channels: i64,
) -> Result<BufferBuilder, Box<EvalAltResult>> {
    buffer_builder_v2(name)?
        .frames(frames)
        .and_then(|builder| builder.channels(channels))
        .map_err(|error| buffer_v2_error(error, Position::NONE))
}

pub(crate) fn buffer_ref_v2(name: String) -> Result<BufferRef, Box<EvalAltResult>> {
    BufferRef::new(
        foundation::authoring_ref::<BufferKind>(&name, GroupScope::root())
            .map_err(|error| buffer_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| buffer_v2_error(error, Position::NONE))
}

fn buffer_v2_error(error: FoundationError, position: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        error.to_string().into(),
        position,
    ))
}

pub(crate) fn install_v2(engine: &mut Engine) {
    fn strict<T>(result: Result<T, FoundationError>) -> Result<T, Box<EvalAltResult>> {
        result.map_err(|error| buffer_v2_error(error, Position::NONE))
    }

    engine
        .register_type_with_name::<BufferBuilder>("BufferBuilder")
        .register_type_with_name::<BufferRef>("BufferRef")
        .register_fn("buffer", buffer_builder_v2)
        .register_fn("allocate_buffer", allocate_buffer_v2)
        .register_fn("buffer_ref", buffer_ref_v2)
        .register_fn("frames", |builder: BufferBuilder, frames: i64| {
            strict(builder.frames(frames))
        })
        .register_fn("channels", |builder: BufferBuilder, channels: i64| {
            strict(builder.channels(channels))
        })
        .register_fn("clear", BufferBuilder::clear)
        .register_fn("copy_overlap", BufferBuilder::copy_overlap)
        .register_fn("apply", |builder: BufferBuilder| strict(builder.apply()))
        .register_fn("remove", |reference: BufferRef| strict(reference.remove()))
        .register_fn("status", |reference: BufferRef| strict(reference.status()));
}

#[cfg(test)]
mod v2_tests {
    use super::*;
    use vibelang_core::candidate::EntityKind;

    fn v2_identity() -> vibelang_core::candidate::EvaluationIdentity {
        vibelang_core::candidate::EvaluationIdentity::new(
            vibelang_core::candidate::LanguageContract::v2(
                vibelang_core::candidate::ContractDigest::from_bytes(b"buffer-v2-test"),
            ),
            vibelang_core::candidate::EngineInstanceId::new(),
            vibelang_core::mutation::RuntimeEpoch::new(),
        )
    }

    fn builder(name: &str) -> BufferBuilder {
        BufferBuilder::new(
            foundation::authoring_builder::<BufferKind>(name, GroupScope::root()).unwrap(),
        )
    }

    #[test]
    fn v2_buffer_requires_explicit_replacement_policy_and_strict_shape() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        assert!(matches!(
            builder("scratch").frames(0),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            builder("scratch").channels(0),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            builder("scratch").channels(17),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            builder("scratch").apply(),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(message)
            )) if message.contains("frames")
        ));
        assert!(matches!(
            builder("scratch").frames(1024).unwrap().apply(),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(message)
            )) if message.contains("replacement policy")
        ));

        let candidate = foundation::finish_evaluation().unwrap();
        assert!(
            candidate.declarations().is_empty(),
            "rejected configuration must leave no candidate residue"
        );
    }

    #[test]
    fn v2_buffer_clones_diverge_independently_and_terminals_register_once() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let base = builder("scratch").frames(1024).unwrap();
        let cleared = base.clone().clear();
        let copied = base.copy_overlap();
        assert_eq!(cleared.replacement, Some(BufferReplacementAuthoring::Clear));
        assert_eq!(
            copied.replacement,
            Some(BufferReplacementAuthoring::CopyOverlap)
        );

        let reference = cleared.apply().unwrap();
        assert_eq!(reference.base().kind(), EntityKind::Buffer);
        assert!(matches!(
            copied.apply(),
            Err(FoundationError::Candidate(
                CandidateError::DuplicateDeclaration { .. }
            ))
        ));

        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.declarations().len(), 1);
        assert_eq!(
            candidate.declarations()[0].lifecycle().terminal_effect,
            TerminalEffect::Register
        );
    }

    #[test]
    fn v2_buffer_rhai_surface_and_allocate_alias_author_from_script() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        crate::foundation::register(&mut engine);
        install_v2(&mut engine);
        let reference = engine
            .eval::<BufferRef>(r#"allocate_buffer("spec_arrays", 65536, 1).clear().apply()"#)
            .unwrap();
        assert_eq!(reference.base().kind(), EntityKind::Buffer);
        assert!(
            engine
                .eval::<BufferRef>(r#"buffer("plain").frames(64).apply()"#)
                .is_err(),
            "a terminal without a declared replacement policy must fail, not no-op"
        );
        assert!(engine
            .eval::<BufferBuilder>(r#"buffer("bad").channels(17)"#)
            .is_err());

        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.declarations().len(), 1);
    }
}
