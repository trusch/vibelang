//! SFZ API for Rhai scripts.
//!
//! SFZ instruments are sample-based instruments with key/velocity layers.
//!
//! ## Example
//! ```rhai
//! // Load an SFZ instrument
//! let piano = load_sfz("piano", "instruments/piano.sfz");
//!
//! // Create a voice that uses the SFZ instrument
//! let piano_voice = voice("piano_voice")
//!     .on_sfz(piano)
//!     .gain(db(-6))
//!     .apply();
//!
//! // Use in a melody
//! melody("melody")
//!     .on(piano_voice)
//!     .notes("C4 E4 G4 C5")
//!     .start();
//! ```

use rhai::{CustomType, Engine, TypeBuilder};
use std::path::PathBuf;
use vibelang_core::traits::SfzConfig;

use crate::context;

/// Handle to a loaded SFZ instrument.
#[derive(Debug, Clone, CustomType)]
pub struct SfzHandle {
    /// SFZ instrument ID name.
    pub id: String,
    /// Path to the SFZ file.
    pub path: PathBuf,
}

impl SfzHandle {
    /// Create a new SFZ handle.
    pub fn new(id: String, path: PathBuf) -> Self {
        Self { id, path }
    }

    /// Get the SFZ ID name.
    pub fn get_id(&mut self) -> String {
        self.id.clone()
    }

    /// Get the path.
    pub fn get_path(&mut self) -> String {
        self.path.to_string_lossy().to_string()
    }
}

/// Load an SFZ instrument from a file.
///
/// # Arguments
///
/// * `id` - Unique identifier for this instrument
/// * `path` - Path to the .sfz file (relative to script or absolute)
///
/// # Returns
///
/// An SfzHandle that can be assigned to voices using `.on_sfz()`.
pub fn load_sfz(id: String, path: String) -> SfzHandle {
    let sfz_id = context::get_or_create_sfz_id(&id);

    // Resolve path relative to current script file
    let resolved_path = if let Some(current_file) = context::get_current_file() {
        if let Some(parent) = current_file.parent() {
            let p = parent.join(&path);
            if p.exists() {
                p
            } else {
                PathBuf::from(&path)
            }
        } else {
            PathBuf::from(&path)
        }
    } else {
        PathBuf::from(&path)
    };

    let config = SfzConfig::new(resolved_path.clone());

    context::with_state(|state| {
        state.sfz_instruments.insert(sfz_id, config);
    });

    SfzHandle::new(id, resolved_path)
}

/// Register SFZ API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    engine.build_type::<SfzHandle>();

    // Constructor
    engine.register_fn("load_sfz", load_sfz);

    // ID and path getters
    engine.register_fn("id", SfzHandle::get_id);
    engine.register_get("id", SfzHandle::get_id);
    engine.register_fn("path", SfzHandle::get_path);
    engine.register_get("path", SfzHandle::get_path);
}

use rhai::{EvalAltResult, Position};
use vibelang_core::candidate::{
    AuthoringDeclaration, Cancellation, CandidateError, Composition, DeclarationOwner,
    DeclarationPayload, GroupScope, LifecycleAction, LifecycleMetadata, SfzAuthoring, SfzKind,
    TerminalEffect,
};

use crate::foundation::{self, BuilderBase, FoundationError, Observation, RefBase};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SfzRef {
    base: RefBase,
}

impl SfzRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<SfzKind>()?;
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

/// Pure SFZ builder. The logical `.sfz` source path is the entire
/// parse/load configuration; voices bind to the result only through the
/// typed [`SfzRef`], so an instrument that cannot resolve fails every
/// dependent voice transitively instead of leaving it silently unbound.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SfzBuilder {
    base: BuilderBase,
    source: String,
}

impl SfzBuilder {
    pub fn new(base: BuilderBase, source: String) -> Result<Self, FoundationError> {
        if source.is_empty() || source.trim() != source {
            return Err(CandidateError::InvalidAuthoring(
                "SFZ source must be a non-empty path without surrounding whitespace".into(),
            )
            .into());
        }
        if !source.to_ascii_lowercase().ends_with(".sfz") {
            return Err(CandidateError::InvalidAuthoring(
                "SFZ source must name an .sfz file".into(),
            )
            .into());
        }
        Ok(Self { base, source })
    }

    pub fn apply(self) -> Result<SfzRef, FoundationError> {
        let declaration = SfzAuthoring {
            source: self.source.clone(),
        };
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::Sfz(declaration))?;
        let owner = DeclarationOwner::Structural(self.base.source().syntax_key().clone());
        let reference = self.base.apply(
            owner,
            LifecycleMetadata::register(Composition::Standalone),
            payload,
        )?;
        SfzRef::new(reference)
    }
}

// Wired into the shared install_v2_api root by the M09 registration
// integration gate; until then only cfg(test) installs reference it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn sfz_builder_v2(
    name: String,
    source: String,
) -> Result<SfzBuilder, Box<EvalAltResult>> {
    SfzBuilder::new(
        foundation::authoring_builder::<SfzKind>(&name, GroupScope::root())
            .map_err(|error| sfz_v2_error(error, Position::NONE))?,
        source,
    )
    .map_err(|error| sfz_v2_error(error, Position::NONE))
}

/// Effective forwarding alias for the v1 `load_sfz(id, path)` spelling.
/// The result is a pure builder: registration happens only at the
/// explicit `apply` terminal, never as a load side effect.
// Wired into the shared install_v2_api root by the M09 registration
// integration gate; until then only cfg(test) installs reference it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn load_sfz_v2(name: String, source: String) -> Result<SfzBuilder, Box<EvalAltResult>> {
    sfz_builder_v2(name, source)
}

// Wired into the shared install_v2_api root by the M09 registration
// integration gate; until then only cfg(test) installs reference it.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn sfz_ref_v2(name: String) -> Result<SfzRef, Box<EvalAltResult>> {
    SfzRef::new(
        foundation::authoring_ref::<SfzKind>(&name, GroupScope::root())
            .map_err(|error| sfz_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| sfz_v2_error(error, Position::NONE))
}

// Wired into the shared install_v2_api root by the M09 registration
// integration gate; until then only cfg(test) installs reference it.
#[cfg_attr(not(test), allow(dead_code))]
fn sfz_v2_error(error: FoundationError, position: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        error.to_string().into(),
        position,
    ))
}

#[cfg(test)]
fn install_v2_for_tests(engine: &mut Engine) {
    fn strict<T>(result: Result<T, FoundationError>) -> Result<T, Box<EvalAltResult>> {
        result.map_err(|error| sfz_v2_error(error, Position::NONE))
    }

    engine
        .register_type_with_name::<SfzBuilder>("SfzBuilder")
        .register_type_with_name::<SfzRef>("SfzRef")
        .register_fn("sfz", sfz_builder_v2)
        .register_fn("load_sfz", load_sfz_v2)
        .register_fn("sfz_ref", sfz_ref_v2)
        .register_fn("apply", |builder: SfzBuilder| strict(builder.apply()))
        .register_fn("remove", |reference: SfzRef| strict(reference.remove()))
        .register_fn("status", |reference: SfzRef| strict(reference.status()));
}

#[cfg(test)]
mod v2_tests {
    use super::*;
    use crate::api::voice::VoiceBuilder;
    use vibelang_core::candidate::{EntityKind, GroupKind, VoiceKind};

    fn v2_identity() -> vibelang_core::candidate::EvaluationIdentity {
        vibelang_core::candidate::EvaluationIdentity::new(
            vibelang_core::candidate::LanguageContract::v2(
                vibelang_core::candidate::ContractDigest::from_bytes(b"sfz-v2-test"),
            ),
            vibelang_core::candidate::EngineInstanceId::new(),
            vibelang_core::mutation::RuntimeEpoch::new(),
        )
    }

    fn builder(name: &str, source: &str) -> SfzBuilder {
        SfzBuilder::new(
            foundation::authoring_builder::<SfzKind>(name, GroupScope::root()).unwrap(),
            source.into(),
        )
        .unwrap()
    }

    #[test]
    fn v2_sfz_configuration_is_pure_and_strict() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        for invalid in ["", " padded.sfz", "kits/606.sfz ", "notes.txt"] {
            assert!(
                matches!(
                    SfzBuilder::new(
                        foundation::authoring_builder::<SfzKind>("bad", GroupScope::root())
                            .unwrap(),
                        invalid.into(),
                    ),
                    Err(FoundationError::Candidate(
                        CandidateError::InvalidAuthoring(_)
                    ))
                ),
                "SFZ source {invalid:?} must be rejected with a structured failure"
            );
        }

        let candidate = foundation::finish_evaluation().unwrap();
        assert!(
            candidate.declarations().is_empty(),
            "rejected configuration must leave no candidate residue"
        );
    }

    #[test]
    fn v2_sfz_terminal_registers_once_and_returns_typed_ref() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let reference = builder("piano", "instruments/piano.sfz").apply().unwrap();
        assert_eq!(reference.base().kind(), EntityKind::Sfz);
        assert!(matches!(
            reference.status(),
            Err(FoundationError::ObservationUnavailable)
        ));
        assert!(matches!(
            builder("piano", "instruments/piano.sfz").apply(),
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
    fn v2_sfz_ref_remove_is_a_real_operation() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let reference = sfz_ref_v2("piano".into()).unwrap();
        reference.clone().remove().unwrap();

        let candidate = foundation::finish_evaluation();
        assert!(
            candidate.is_err(),
            "an operation against an undeclared, uncataloged SFZ must not resolve"
        );
    }

    #[test]
    fn v2_sfz_typed_binding_fails_dependents_transitively() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        assert!(
            SfzRef::new(
                foundation::authoring_ref::<GroupKind>("drums", GroupScope::root()).unwrap()
            )
            .is_err(),
            "an SfzRef must reject a non-SFZ logical address"
        );
        let kit = builder("kit", "kits/606.sfz").apply().unwrap();
        VoiceBuilder::new(
            foundation::authoring_builder::<VoiceKind>("sampler", GroupScope::root()).unwrap(),
        )
        .on_sfz(kit.base().clone())
        .unwrap()
        .apply()
        .unwrap();
        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.declarations().len(), 2);
        assert!(
            candidate
                .references()
                .iter()
                .any(|use_| use_.reference().address() == kit.base().address()),
            "the voice binding must record a dependency edge on the SFZ address"
        );

        foundation::begin_evaluation(v2_identity()).unwrap();
        let ghost = sfz_ref_v2("ghost".into()).unwrap();
        VoiceBuilder::new(
            foundation::authoring_builder::<VoiceKind>("orphan", GroupScope::root()).unwrap(),
        )
        .on_sfz(ghost.base().clone())
        .unwrap()
        .apply()
        .unwrap();
        assert!(
            matches!(
                foundation::finish_evaluation(),
                Err(FoundationError::Candidate(
                    CandidateError::UnresolvedReference(_)
                ))
            ),
            "a voice bound to an unresolvable SFZ must fail the candidate, not play unbound"
        );
    }

    #[test]
    fn v2_sfz_rhai_surface_and_load_sfz_alias_author_from_script() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        crate::foundation::register(&mut engine);
        install_v2_for_tests(&mut engine);
        let reference = engine
            .eval::<SfzRef>(r#"sfz("piano", "instruments/piano.sfz").apply()"#)
            .unwrap();
        assert_eq!(reference.base().kind(), EntityKind::Sfz);
        let alias = engine
            .eval::<SfzRef>(r#"load_sfz("kit", "kits/606.sfz").apply()"#)
            .unwrap();
        assert_eq!(alias.base().kind(), EntityKind::Sfz);
        assert!(
            engine
                .eval::<SfzBuilder>(r#"sfz("bad", "notes.txt")"#)
                .is_err(),
            "a non-.sfz source must fail at configuration, not at a later no-op"
        );

        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.declarations().len(), 2);
    }
}
