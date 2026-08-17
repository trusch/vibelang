//! Pure v2 Builder, Ref, RevisionRef, and Observation host foundations.

use rhai::Engine;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;
use vibelang_core::candidate::{
    Candidate, CandidateDraft, CandidateError, CandidateFragment, CompatibilityError,
    DeclarationIr, DeclarationKey, DeclarationOwner, DeclarationPayload, EntityKind, ErasedRef,
    EvaluationIdentity, GroupScope, LifecycleAction, LifecycleMetadata, LifecycleOperationIr,
    LogicalAddress, ModulePath, ProjectNamespace, RefKind, ReferenceCatalog, ReferenceUse,
    SourceAnchor, SyntaxKey, TypedAddress, TypedRef,
};
use vibelang_core::mutation::{
    AttemptId, CandidateOrigin, Diagnostic, EventSequence, MutationReceipt, RevisionId,
    RuntimeEpoch, Timestamp,
};

#[derive(Debug, Error)]
pub enum FoundationError {
    #[error("a v2 evaluation is already active on this thread")]
    EvaluationAlreadyActive,
    #[error("no v2 evaluation is active on this thread")]
    NoActiveEvaluation,
    #[error("invalid Builder configuration field: {0}")]
    InvalidConfigurationField(String),
    #[error("invalid Observation: {0}")]
    InvalidObservation(String),
    #[error("live Observation is unavailable without a runtime observation source")]
    ObservationUnavailable,
    #[error("no detached authoring fragment is active")]
    NoActiveFragment,
    #[error("a detached authoring fragment was left open")]
    UnclosedFragment,
    #[error("receipt has no accepted revision")]
    ReceiptHasNoRevision,
    #[error(transparent)]
    Candidate(#[from] CandidateError),
    #[error(transparent)]
    Compatibility(#[from] CompatibilityError),
}

struct EvaluationState {
    draft: CandidateDraft,
    references: ReferenceCatalog,
    captures: Vec<FragmentCapture>,
}

struct FragmentCapture {
    owner: DeclarationOwner,
    fragment: CandidateFragment,
}

thread_local! {
    static EVALUATION: RefCell<Option<EvaluationState>> = const { RefCell::new(None) };
}

pub(crate) fn begin_evaluation(identity: EvaluationIdentity) -> Result<(), FoundationError> {
    EVALUATION.with(|state| {
        let mut state = state.borrow_mut();
        if state.is_some() {
            return Err(FoundationError::EvaluationAlreadyActive);
        }
        *state = Some(EvaluationState {
            draft: CandidateDraft::new(identity, CandidateOrigin::RhaiHost),
            references: ReferenceCatalog::default(),
            captures: Vec::new(),
        });
        Ok(())
    })
}

pub(crate) fn finish_evaluation() -> Result<Candidate, FoundationError> {
    EVALUATION.with(|state| {
        let state = state
            .borrow_mut()
            .take()
            .ok_or(FoundationError::NoActiveEvaluation)?;
        if !state.captures.is_empty() {
            return Err(FoundationError::UnclosedFragment);
        }
        Ok(state.draft.finish(&state.references)?)
    })
}

pub(crate) fn abort_evaluation() {
    EVALUATION.with(|state| {
        state.borrow_mut().take();
    });
}

pub(crate) fn current_identity() -> Result<EvaluationIdentity, FoundationError> {
    EVALUATION.with(|state| {
        state
            .borrow()
            .as_ref()
            .map(|state| state.draft.identity().clone())
            .ok_or(FoundationError::NoActiveEvaluation)
    })
}

pub(crate) fn authoring_builder<K: RefKind>(
    key: &str,
    group_scope: GroupScope,
) -> Result<BuilderBase, FoundationError> {
    let identity = current_identity()?;
    let module = ModulePath::new("main")?;
    let key = DeclarationKey::new(key)?;
    Ok(BuilderBase::new(
        identity,
        TypedAddress::<K>::new(
            ProjectNamespace::new("vibelang")?,
            module.clone(),
            group_scope,
            key.clone(),
        ),
        SourceAnchor::new(module, SyntaxKey::Explicit(key), None),
    ))
}

pub(crate) fn authoring_ref<K: RefKind>(
    key: &str,
    group_scope: GroupScope,
) -> Result<RefBase, FoundationError> {
    let builder = authoring_builder::<K>(key, group_scope)?;
    Ok(builder.reference())
}

pub(crate) fn begin_fragment(owner: DeclarationOwner) -> Result<(), FoundationError> {
    EVALUATION.with(|state| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().ok_or(FoundationError::NoActiveEvaluation)?;
        state.captures.push(FragmentCapture {
            owner,
            fragment: CandidateFragment::default(),
        });
        Ok(())
    })
}

pub(crate) fn finish_fragment() -> Result<CandidateFragment, FoundationError> {
    EVALUATION.with(|state| {
        state
            .borrow_mut()
            .as_mut()
            .ok_or(FoundationError::NoActiveEvaluation)?
            .captures
            .pop()
            .map(|capture| capture.fragment)
            .ok_or(FoundationError::NoActiveFragment)
    })
}

pub(crate) fn abort_fragment() {
    EVALUATION.with(|state| {
        if let Some(state) = state.borrow_mut().as_mut() {
            state.captures.pop();
        }
    });
}

pub(crate) fn capture_fragment<T>(
    owner: DeclarationOwner,
    capture: impl FnOnce() -> Result<T, FoundationError>,
) -> Result<(T, CandidateFragment), FoundationError> {
    begin_fragment(owner)?;
    match capture() {
        Ok(value) => Ok((value, finish_fragment()?)),
        Err(error) => {
            abort_fragment();
            Err(error)
        }
    }
}

pub(crate) fn commit_fragment(fragment: CandidateFragment) -> Result<(), FoundationError> {
    EVALUATION.with(|state| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().ok_or(FoundationError::NoActiveEvaluation)?;
        if let Some(capture) = state.captures.last_mut() {
            capture.fragment.extend(fragment);
            Ok(())
        } else {
            state.draft.append_fragment(fragment)?;
            Ok(())
        }
    })
}

pub(crate) fn commit_action(
    reference: RefBase,
    lifecycle: LifecycleMetadata,
    action: LifecycleAction,
    source: SourceAnchor,
) -> Result<RefBase, FoundationError> {
    let operation = LifecycleOperationIr::new(reference.0.clone(), lifecycle, action, source)?;
    commit_fragment(CandidateFragment::default().operation(operation))?;
    Ok(reference)
}

pub(crate) fn observe(reference: &RefBase) -> Result<Observation, FoundationError> {
    reference.validate(&current_identity()?)?;
    Err(FoundationError::ObservationUnavailable)
}

pub(crate) fn operation_source(
    reference: &RefBase,
    role: &str,
) -> Result<SourceAnchor, FoundationError> {
    let key = DeclarationKey::new(format!("{}-{role}", reference.address().key().as_str()))?;
    Ok(SourceAnchor::new(
        reference.address().module().clone(),
        SyntaxKey::Explicit(key),
        None,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuilderBase {
    identity: EvaluationIdentity,
    address: LogicalAddress,
    source: SourceAnchor,
    configuration: BTreeMap<String, Arc<[u8]>>,
}

impl BuilderBase {
    #[must_use]
    pub fn new<K: RefKind>(
        identity: EvaluationIdentity,
        address: TypedAddress<K>,
        source: SourceAnchor,
    ) -> Self {
        Self {
            identity,
            address: address.erase(),
            source,
            configuration: BTreeMap::new(),
        }
    }

    pub fn configured(
        mut self,
        field: impl Into<String>,
        canonical_value: impl Into<Arc<[u8]>>,
    ) -> Result<Self, FoundationError> {
        let field = field.into();
        if field.is_empty()
            || field.len() > 255
            || !field
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(FoundationError::InvalidConfigurationField(field));
        }
        self.configuration.insert(field, canonical_value.into());
        Ok(self)
    }

    #[must_use]
    pub fn identity(&self) -> &EvaluationIdentity {
        &self.identity
    }

    #[must_use]
    pub fn address(&self) -> &LogicalAddress {
        &self.address
    }

    #[must_use]
    pub fn source(&self) -> &SourceAnchor {
        &self.source
    }

    #[must_use]
    pub fn configuration(&self) -> &BTreeMap<String, Arc<[u8]>> {
        &self.configuration
    }

    #[must_use]
    pub fn in_group(mut self, group_scope: GroupScope) -> Self {
        self.address = LogicalAddress::new(
            self.address.project().clone(),
            self.address.module().clone(),
            self.address.kind(),
            group_scope,
            self.address.key().clone(),
        );
        self
    }

    #[must_use]
    pub fn reference(&self) -> RefBase {
        RefBase::new(ErasedRef::new(self.identity.clone(), self.address.clone()))
    }

    pub fn fragment(
        self,
        owner: DeclarationOwner,
        mut lifecycle: LifecycleMetadata,
        payload: DeclarationPayload,
        references: impl IntoIterator<Item = (RefBase, SourceAnchor)>,
    ) -> Result<(CandidateFragment, RefBase), FoundationError> {
        self.identity.ensure_compatible(&current_identity()?)?;
        let effective_owner = EVALUATION.with(|state| {
            state
                .borrow()
                .as_ref()
                .and_then(|state| state.captures.last())
                .map_or(owner, |capture| capture.owner.clone())
        });
        lifecycle.composition = effective_owner.composition();
        let reference = self.reference();
        let declaration = DeclarationIr::new_erased(
            self.address,
            effective_owner,
            self.source,
            lifecycle,
            payload,
        );
        let mut fragment = CandidateFragment::default().declaration(declaration);
        for (dependency, source) in references {
            dependency.validate(&self.identity)?;
            fragment = fragment.reference(ReferenceUse::new(dependency.0, source));
        }
        Ok((fragment, reference))
    }

    pub fn apply(
        self,
        owner: DeclarationOwner,
        lifecycle: LifecycleMetadata,
        payload: DeclarationPayload,
    ) -> Result<RefBase, FoundationError> {
        let identity = current_identity()?;
        self.identity.ensure_compatible(&identity)?;
        let (fragment, reference) = self.fragment(owner, lifecycle, payload, Vec::new())?;
        commit_fragment(fragment)?;
        Ok(reference)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefBase(ErasedRef);

impl RefBase {
    #[must_use]
    pub fn new(reference: ErasedRef) -> Self {
        Self(reference)
    }

    #[must_use]
    pub fn identity(&self) -> &EvaluationIdentity {
        self.0.identity()
    }

    #[must_use]
    pub fn address(&self) -> &LogicalAddress {
        self.0.address()
    }

    #[must_use]
    pub fn kind(&self) -> EntityKind {
        self.0.address().kind()
    }

    pub fn typed<K: RefKind>(&self) -> Result<TypedRef<K>, FoundationError> {
        Ok(self.0.try_typed()?)
    }

    pub fn validate(&self, identity: &EvaluationIdentity) -> Result<(), FoundationError> {
        Ok(self.0.identity().ensure_compatible(identity)?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionRef {
    identity: EvaluationIdentity,
    attempt_id: AttemptId,
    revision: RevisionId,
}

impl RevisionRef {
    #[must_use]
    pub fn new(identity: EvaluationIdentity, attempt_id: AttemptId, revision: RevisionId) -> Self {
        Self {
            identity,
            attempt_id,
            revision,
        }
    }

    pub fn from_receipt(
        identity: EvaluationIdentity,
        receipt: &MutationReceipt,
    ) -> Result<Self, FoundationError> {
        if identity.runtime_epoch() != receipt.runtime_epoch {
            return Err(CompatibilityError::Epoch {
                expected: identity.runtime_epoch(),
                actual: receipt.runtime_epoch,
            }
            .into());
        }
        let revision = receipt
            .revision
            .ok_or(FoundationError::ReceiptHasNoRevision)?;
        Ok(Self::new(identity, receipt.attempt_id, revision))
    }

    #[must_use]
    pub fn identity(&self) -> &EvaluationIdentity {
        &self.identity
    }

    #[must_use]
    pub const fn attempt_id(&self) -> AttemptId {
        self.attempt_id
    }

    #[must_use]
    pub const fn revision(&self) -> RevisionId {
        self.revision
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationState {
    Pending,
    Dormant,
    Running,
    Stopped,
    Removed,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationStaleness {
    RuntimeEpochChanged {
        expected: RuntimeEpoch,
        actual: RuntimeEpoch,
    },
    Disconnected,
    RevisionBehind {
        requested: RevisionId,
        fresh_through: Option<RevisionId>,
    },
    Timeout {
        requested: RevisionId,
        fresh_through: Option<RevisionId>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    reference: RefBase,
    runtime_epoch: RuntimeEpoch,
    requested_revision: Option<RevisionId>,
    fresh_through_revision: Option<RevisionId>,
    applied_revision: Option<RevisionId>,
    event_sequence: EventSequence,
    observed_at: Timestamp,
    state: ObservationState,
    staleness: Option<ObservationStaleness>,
    diagnostics: Arc<[Diagnostic]>,
}

impl Observation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reference: RefBase,
        runtime_epoch: RuntimeEpoch,
        requested_revision: Option<RevisionId>,
        fresh_through_revision: Option<RevisionId>,
        applied_revision: Option<RevisionId>,
        event_sequence: EventSequence,
        observed_at: Timestamp,
        state: ObservationState,
        staleness: Option<ObservationStaleness>,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Self, FoundationError> {
        let reference_epoch = reference.identity().runtime_epoch();
        if reference_epoch != runtime_epoch {
            let expected = ObservationStaleness::RuntimeEpochChanged {
                expected: reference_epoch,
                actual: runtime_epoch,
            };
            if staleness.as_ref() != Some(&expected) {
                return Err(FoundationError::InvalidObservation(
                    "an epoch-mismatched Ref needs exact runtime_epoch_changed staleness".into(),
                ));
            }
        } else if matches!(
            &staleness,
            Some(ObservationStaleness::RuntimeEpochChanged { .. })
        ) {
            return Err(FoundationError::InvalidObservation(
                "runtime_epoch_changed staleness requires an actual epoch mismatch".into(),
            ));
        }
        let exact_revision_staleness = match (requested_revision, staleness.as_ref()) {
            (
                Some(requested),
                Some(
                    ObservationStaleness::RevisionBehind {
                        requested: stale_requested,
                        fresh_through: stale_fresh,
                    }
                    | ObservationStaleness::Timeout {
                        requested: stale_requested,
                        fresh_through: stale_fresh,
                    },
                ),
            ) => {
                fresh_through_revision.is_none_or(|fresh| fresh < requested)
                    && *stale_requested == requested
                    && *stale_fresh == fresh_through_revision
            }
            (None, Some(ObservationStaleness::RevisionBehind { .. }))
            | (None, Some(ObservationStaleness::Timeout { .. })) => false,
            _ => true,
        };
        if !exact_revision_staleness {
            return Err(FoundationError::InvalidObservation(
                "revision staleness must match an unmet requested revision".into(),
            ));
        }
        if let Some(requested) = requested_revision {
            if fresh_through_revision.is_none_or(|fresh| fresh < requested)
                && !matches!(
                    &staleness,
                    Some(
                        ObservationStaleness::RevisionBehind { .. }
                            | ObservationStaleness::Timeout { .. }
                    )
                )
            {
                return Err(FoundationError::InvalidObservation(
                    "an observation behind its requested revision needs exact stale metadata"
                        .into(),
                ));
            }
        }

        Ok(Self {
            reference,
            runtime_epoch,
            requested_revision,
            fresh_through_revision,
            applied_revision,
            event_sequence,
            observed_at,
            state,
            staleness,
            diagnostics: Arc::from(diagnostics),
        })
    }

    #[must_use]
    pub fn reference(&self) -> &RefBase {
        &self.reference
    }

    #[must_use]
    pub const fn runtime_epoch(&self) -> RuntimeEpoch {
        self.runtime_epoch
    }

    #[must_use]
    pub const fn requested_revision(&self) -> Option<RevisionId> {
        self.requested_revision
    }

    #[must_use]
    pub const fn fresh_through_revision(&self) -> Option<RevisionId> {
        self.fresh_through_revision
    }

    #[must_use]
    pub const fn applied_revision(&self) -> Option<RevisionId> {
        self.applied_revision
    }

    #[must_use]
    pub const fn event_sequence(&self) -> EventSequence {
        self.event_sequence
    }

    #[must_use]
    pub fn observed_at(&self) -> &Timestamp {
        &self.observed_at
    }

    #[must_use]
    pub const fn state(&self) -> ObservationState {
        self.state
    }

    #[must_use]
    pub fn staleness(&self) -> Option<&ObservationStaleness> {
        self.staleness.as_ref()
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

pub(crate) fn register(engine: &mut Engine) {
    engine
        .register_type_with_name::<BuilderBase>("Builder")
        .register_get("kind", |builder: &mut BuilderBase| {
            builder.address().kind().to_string()
        })
        .register_get("address", |builder: &mut BuilderBase| {
            builder.address().to_string()
        })
        .register_type_with_name::<RefBase>("Ref")
        .register_get("kind", |reference: &mut RefBase| {
            reference.kind().to_string()
        })
        .register_get("address", |reference: &mut RefBase| {
            reference.address().to_string()
        })
        .register_type_with_name::<RevisionRef>("RevisionRef")
        .register_get("attempt_id", |reference: &mut RevisionRef| {
            reference.attempt_id().to_string()
        })
        .register_get("runtime_epoch", |reference: &mut RevisionRef| {
            reference.identity().runtime_epoch().to_string()
        })
        .register_get("revision", |reference: &mut RevisionRef| {
            reference.revision().to_string()
        })
        .register_type_with_name::<Observation>("Observation")
        .register_get("reference", |observation: &mut Observation| {
            observation.reference().clone()
        })
        .register_get("event_sequence", |observation: &mut Observation| {
            observation.event_sequence().to_string()
        })
        .register_get("observed_at", |observation: &mut Observation| {
            observation.observed_at().as_str().to_string()
        })
        .register_get("stale", |observation: &mut Observation| {
            observation.staleness().is_some()
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibelang_core::candidate::{
        Composition, ContractDigest, DeclarationKey, GroupScope, LanguageContract, ModulePath,
        ProjectNamespace, SyntaxKey, VoiceKind,
    };

    fn identity() -> EvaluationIdentity {
        EvaluationIdentity::new(
            LanguageContract::v2(ContractDigest::from_bytes(b"foundation-test")),
            vibelang_core::candidate::EngineInstanceId::new(),
            RuntimeEpoch::new(),
        )
    }

    fn builder(identity: EvaluationIdentity) -> BuilderBase {
        let module = ModulePath::new("song/main").unwrap();
        BuilderBase::new(
            identity,
            TypedAddress::<VoiceKind>::new(
                ProjectNamespace::new("test-project").unwrap(),
                module.clone(),
                GroupScope::root(),
                DeclarationKey::new("lead").unwrap(),
            ),
            SourceAnchor::new(
                module.clone(),
                SyntaxKey::deterministic(&module, &[1], "voice").unwrap(),
                None,
            ),
        )
    }

    #[test]
    fn builder_clones_configure_independently_without_candidate_residue() {
        abort_evaluation();
        let base = builder(identity());
        let left = base
            .clone()
            .configured("gain", Arc::<[u8]>::from([1_u8]))
            .unwrap();
        let right = base.configured("gain", Arc::<[u8]>::from([2_u8])).unwrap();

        assert_eq!(left.configuration()["gain"].as_ref(), &[1]);
        assert_eq!(right.configuration()["gain"].as_ref(), &[2]);
        assert!(matches!(
            left.apply(
                DeclarationOwner::Structural(
                    SyntaxKey::deterministic(&ModulePath::new("song/main").unwrap(), &[1], "owner")
                        .unwrap()
                ),
                LifecycleMetadata::register(Composition::Standalone),
                DeclarationPayload::Empty,
            ),
            Err(FoundationError::NoActiveEvaluation)
        ));
    }

    #[test]
    fn builder_terminal_contributes_once_and_returns_typed_ref() {
        abort_evaluation();
        let identity = identity();
        begin_evaluation(identity.clone()).unwrap();
        let reference = builder(identity)
            .apply(
                DeclarationOwner::Structural(
                    SyntaxKey::deterministic(&ModulePath::new("song/main").unwrap(), &[1], "owner")
                        .unwrap(),
                ),
                LifecycleMetadata::register(Composition::Standalone),
                DeclarationPayload::Empty,
            )
            .unwrap();
        let typed = reference.typed::<VoiceKind>().unwrap();
        let candidate = finish_evaluation().unwrap();

        assert_eq!(typed.address().kind(), EntityKind::Voice);
        assert_eq!(candidate.declarations().len(), 1);
    }

    #[test]
    fn observation_and_revision_ref_are_immutable_qualified_values() {
        let identity = identity();
        begin_evaluation(identity.clone()).unwrap();
        let reference = builder(identity.clone())
            .apply(
                DeclarationOwner::Structural(
                    SyntaxKey::deterministic(&ModulePath::new("song/main").unwrap(), &[1], "owner")
                        .unwrap(),
                ),
                LifecycleMetadata::register(Composition::Standalone),
                DeclarationPayload::Empty,
            )
            .unwrap();
        finish_evaluation().unwrap();
        let revision = RevisionId::new(3).unwrap();
        let revision_ref = RevisionRef::new(identity.clone(), AttemptId::new(), revision);
        let observation = Observation::new(
            reference,
            identity.runtime_epoch(),
            Some(revision),
            Some(revision),
            Some(revision),
            EventSequence::new(4).unwrap(),
            Timestamp::parse("2026-07-18T08:00:00Z").unwrap(),
            ObservationState::Running,
            None,
            Vec::new(),
        )
        .unwrap();
        let clone = observation.clone();

        assert_eq!(revision_ref.revision(), revision);
        assert_eq!(clone, observation);
        assert_eq!(clone.state(), ObservationState::Running);
        assert!(clone.staleness().is_none());
    }

    #[test]
    fn stale_observation_requires_exact_epoch_or_revision_reason() {
        let identity = identity();
        begin_evaluation(identity.clone()).unwrap();
        let reference = builder(identity.clone())
            .apply(
                DeclarationOwner::Structural(
                    SyntaxKey::deterministic(&ModulePath::new("song/main").unwrap(), &[1], "owner")
                        .unwrap(),
                ),
                LifecycleMetadata::register(Composition::Standalone),
                DeclarationPayload::Empty,
            )
            .unwrap();
        finish_evaluation().unwrap();
        let requested = RevisionId::new(4).unwrap();
        let fresh = RevisionId::new(3).unwrap();

        assert!(matches!(
            Observation::new(
                reference,
                identity.runtime_epoch(),
                Some(requested),
                Some(fresh),
                Some(fresh),
                EventSequence::new(5).unwrap(),
                Timestamp::parse("2026-07-18T08:00:00Z").unwrap(),
                ObservationState::Pending,
                None,
                Vec::new(),
            ),
            Err(FoundationError::InvalidObservation(_))
        ));
    }
}
