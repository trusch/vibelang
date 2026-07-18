//! Deterministic, privacy-minimal runtime capability discovery.
//!
//! The catalog is derived from the canonical conventions metadata. Runtime
//! callers provide explicit evidence for every gate required by that catalog;
//! compilation alone is therefore never treated as live backend truth.

use crate::compat::{timeout, Duration, Instant};
use crate::mutation::{
    EventSequence, PublicDigest, RevisionId, RuntimeEpoch, RuntimeMutationStatus, Timestamp,
    MUTATION_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use thiserror::Error;
use vibelang_api_manifest::canonical::{canonical_json_of, canonical_sha256_hex};
use vibelang_api_manifest::conventions::{
    AvailabilityBinding, AvailabilityReasonDefinition, AvailabilityStateDefinition,
    CapabilityDefinition, ConventionsMetadata, SecurityModeDefinition,
};

pub use vibelang_api_manifest::conventions::AvailabilityGate;

pub const CAPABILITY_SNAPSHOT_SCHEMA_ID: &str = "schema.vibelang.capability_snapshot.v1";
pub const MI_UGENS_PROBE_CACHE_TTL: Duration = Duration::from_secs(1);
pub const MI_UGENS_PROBE_TIMEOUT: Duration = Duration::from_secs(1);

const AVAILABLE_STATE_ID: &str = "availability.available";
const DEGRADED_STATE_ID: &str = "availability.degraded";
const UNAVAILABLE_STATE_ID: &str = "availability.unavailable";
const UNKNOWN_STATE_ID: &str = "availability.unknown";
const MI_UGENS_PROBE_ID: &str = "probe.plugin.mi_ugens.v1";
const PLUGIN_MISSING_REASON_ID: &str = "reason.plugin_missing";
const PROBE_FAILED_REASON_ID: &str = "reason.probe_failed";
const PROBE_PENDING_REASON_ID: &str = "reason.probe_pending";
const MAX_CANONICAL_JSON_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Error)]
pub enum CapabilityError {
    #[error("invalid conventions metadata: {0}")]
    InvalidMetadata(String),
    #[error("unknown capability {0}")]
    UnknownCapability(String),
    #[error("unknown security mode {0}")]
    UnknownSecurityMode(String),
    #[error("unknown availability state {0}")]
    UnknownAvailabilityState(String),
    #[error("unknown availability reason {0}")]
    UnknownAvailabilityReason(String),
    #[error("unknown availability binding {0}")]
    UnknownAvailabilityBinding(String),
    #[error("capability status is missing {0}")]
    MissingCapabilityStatus(String),
    #[error("capability status is duplicated {0}")]
    DuplicateCapabilityStatus(String),
    #[error("capability {capability_id} does not require gate {gate:?}")]
    UnexpectedGate {
        capability_id: String,
        gate: AvailabilityGate,
    },
    #[error("capability {capability_id} is missing evidence for gate {gate:?}")]
    MissingGate {
        capability_id: String,
        gate: AvailabilityGate,
    },
    #[error("capability {capability_id} has invalid {gate:?} evidence: {message}")]
    InvalidGateEvidence {
        capability_id: String,
        gate: AvailabilityGate,
        message: String,
    },
    #[error("capability matrix is missing {0}")]
    MissingCapability(String),
    #[error("capability matrix contains unexpected capability {0}")]
    UnexpectedCapability(String),
    #[error("conflicting values for capability constraint {0}")]
    ConflictingConstraint(String),
    #[error("invalid stable identifier {field}: {value}")]
    InvalidStableId { field: &'static str, value: String },
    #[error("invalid build revision: {0}")]
    InvalidBuildRevision(String),
    #[error("security mode {mode_id} requires authenticated={required}, got {actual}")]
    AuthenticationMismatch {
        mode_id: String,
        required: bool,
        actual: bool,
    },
    #[error("mutation status schema version {0} is not supported")]
    MutationSchemaVersion(u16),
    #[error("capability generation exhausted the RFC 8785 safe integer range")]
    GenerationExhausted,
    #[error("canonical JSON failed: {0}")]
    CanonicalJson(String),
    #[error("snapshot identifier does not match its semantic payload")]
    SnapshotIdMismatch,
}

#[derive(Clone, Debug)]
pub struct CapabilityCatalog {
    definitions: BTreeMap<String, CapabilityDefinition>,
    reasons: BTreeMap<String, AvailabilityReasonDefinition>,
    states: BTreeMap<String, AvailabilityStateDefinition>,
    security_modes: BTreeMap<String, SecurityModeDefinition>,
    availability_bindings: BTreeMap<String, AvailabilityBinding>,
}

impl CapabilityCatalog {
    pub fn from_conventions(metadata: &ConventionsMetadata) -> Result<Self, CapabilityError> {
        metadata
            .validate()
            .map_err(|error| CapabilityError::InvalidMetadata(error.to_string()))?;
        let catalog = Self {
            definitions: metadata
                .capabilities
                .iter()
                .cloned()
                .map(|definition| (definition.capability_id.clone(), definition))
                .collect(),
            reasons: metadata
                .availability_reasons
                .iter()
                .cloned()
                .map(|reason| (reason.reason_id.clone(), reason))
                .collect(),
            states: metadata
                .availability_states
                .iter()
                .cloned()
                .map(|state| (state.state_id.clone(), state))
                .collect(),
            security_modes: metadata
                .security_modes
                .iter()
                .cloned()
                .map(|mode| (mode.mode_id.clone(), mode))
                .collect(),
            availability_bindings: metadata
                .availability_bindings
                .iter()
                .cloned()
                .map(|binding| (binding.target_id.clone(), binding))
                .collect(),
        };
        for state_id in [
            AVAILABLE_STATE_ID,
            DEGRADED_STATE_ID,
            UNAVAILABLE_STATE_ID,
            UNKNOWN_STATE_ID,
        ] {
            if !catalog.states.contains_key(state_id) {
                return Err(CapabilityError::UnknownAvailabilityState(state_id.into()));
            }
        }
        for reason_id in [
            PLUGIN_MISSING_REASON_ID,
            PROBE_FAILED_REASON_ID,
            PROBE_PENDING_REASON_ID,
        ] {
            if !catalog.reasons.contains_key(reason_id) {
                return Err(CapabilityError::UnknownAvailabilityReason(reason_id.into()));
            }
        }
        Ok(catalog)
    }

    pub fn definitions(&self) -> impl ExactSizeIterator<Item = &CapabilityDefinition> {
        self.definitions.values()
    }

    pub fn definition(&self, capability_id: &str) -> Option<&CapabilityDefinition> {
        self.definitions.get(capability_id)
    }

    pub fn reason(&self, reason_id: &str) -> Option<&AvailabilityReasonDefinition> {
        self.reasons.get(reason_id)
    }

    pub fn security_modes(&self) -> impl ExactSizeIterator<Item = &SecurityModeDefinition> {
        self.security_modes.values()
    }

    pub fn availability_bindings(&self) -> impl ExactSizeIterator<Item = &AvailabilityBinding> {
        self.availability_bindings.values()
    }

    pub fn evaluate_availability_binding(
        &self,
        target_id: &str,
        capabilities: &[CapabilityStatus],
    ) -> Result<AvailabilityPredicateStatus, CapabilityError> {
        let binding = self
            .availability_bindings
            .get(target_id)
            .ok_or_else(|| CapabilityError::UnknownAvailabilityBinding(target_id.into()))?;
        let mut by_id = BTreeMap::new();
        for capability in capabilities {
            if by_id
                .insert(capability.capability_id.as_str(), capability)
                .is_some()
            {
                return Err(CapabilityError::DuplicateCapabilityStatus(
                    capability.capability_id.clone(),
                ));
            }
        }

        let mut state = match binding.declared_status.as_str() {
            "available" | "conditional" | "importable" => RuntimeCapabilityState::Available,
            "documentation_only" | "quarantined" => RuntimeCapabilityState::Unavailable,
            status => {
                return Err(CapabilityError::InvalidMetadata(format!(
                    "availability binding {} has unsupported declaration {status}",
                    binding.target_id
                )))
            }
        };
        let mut reason_ids = binding
            .unavailable_reason_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut predicate_capability_ids = binding.predicate_capability_ids.clone();
        predicate_capability_ids.sort();
        predicate_capability_ids.dedup();
        for capability_id in &predicate_capability_ids {
            let capability = by_id
                .get(capability_id.as_str())
                .ok_or_else(|| CapabilityError::MissingCapabilityStatus(capability_id.clone()))?;
            state = combine_runtime_states(state, self.runtime_state(&capability.state_id)?);
            reason_ids.extend(capability.reason_ids.iter().cloned());
        }
        Ok(AvailabilityPredicateStatus {
            target_id: binding.target_id.clone(),
            declared_status: binding.declared_status.clone(),
            state_id: self.state_id(state)?,
            predicate_capability_ids,
            reason_ids: reason_ids.into_iter().collect(),
        })
    }

    fn state_id(&self, state: RuntimeCapabilityState) -> Result<String, CapabilityError> {
        let state_id = match state {
            RuntimeCapabilityState::Available => AVAILABLE_STATE_ID,
            RuntimeCapabilityState::Degraded => DEGRADED_STATE_ID,
            RuntimeCapabilityState::Unavailable => UNAVAILABLE_STATE_ID,
            RuntimeCapabilityState::Unknown => UNKNOWN_STATE_ID,
        };
        self.states
            .get(state_id)
            .map(|definition| definition.state_id.clone())
            .ok_or_else(|| CapabilityError::UnknownAvailabilityState(state_id.into()))
    }

    fn runtime_state(&self, state_id: &str) -> Result<RuntimeCapabilityState, CapabilityError> {
        match state_id {
            AVAILABLE_STATE_ID => Ok(RuntimeCapabilityState::Available),
            DEGRADED_STATE_ID => Ok(RuntimeCapabilityState::Degraded),
            UNAVAILABLE_STATE_ID => Ok(RuntimeCapabilityState::Unavailable),
            UNKNOWN_STATE_ID => Ok(RuntimeCapabilityState::Unknown),
            _ => Err(CapabilityError::UnknownAvailabilityState(state_id.into())),
        }
    }

    fn canonical_reason(
        &self,
        reason_id: &str,
        gate: AvailabilityGate,
    ) -> Result<String, CapabilityError> {
        let reason = self
            .reasons
            .get(reason_id)
            .ok_or_else(|| CapabilityError::UnknownAvailabilityReason(reason_id.into()))?;
        if reason.gate != gate {
            return Err(CapabilityError::InvalidGateEvidence {
                capability_id: "capability.plugin.mi_ugens".into(),
                gate,
                message: format!("reason {reason_id} belongs to {:?}", reason.gate),
            });
        }
        Ok(reason.reason_id.clone())
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum ProvenanceKind {
    #[serde(rename = "provenance.contract")]
    Contract,
    #[serde(rename = "provenance.build")]
    Build,
    #[serde(rename = "provenance.operator_policy")]
    OperatorPolicy,
    #[serde(rename = "provenance.runtime_probe")]
    RuntimeProbe,
    #[serde(rename = "provenance.backend_probe")]
    BackendProbe,
    #[serde(rename = "provenance.consumer_projection")]
    ConsumerProjection,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityProvenance {
    pub kind_id: ProvenanceKind,
    pub evidence_id: String,
}

impl CapabilityProvenance {
    pub fn new(kind_id: ProvenanceKind, evidence_id: impl Into<String>) -> Self {
        Self {
            kind_id,
            evidence_id: evidence_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CapabilityConstraint {
    Bool(bool),
    Integer(i64),
    Identifier(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateOutcome {
    Available,
    Degraded,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateEvidence {
    outcome: GateOutcome,
    reason_ids: Vec<String>,
    constraints: BTreeMap<String, CapabilityConstraint>,
    provenance: Vec<CapabilityProvenance>,
}

impl GateEvidence {
    pub fn available(provenance: CapabilityProvenance) -> Self {
        Self {
            outcome: GateOutcome::Available,
            reason_ids: Vec::new(),
            constraints: BTreeMap::new(),
            provenance: vec![provenance],
        }
    }

    pub fn degraded(reason_id: impl Into<String>, provenance: CapabilityProvenance) -> Self {
        Self::non_available(GateOutcome::Degraded, reason_id, provenance)
    }

    pub fn unavailable(reason_id: impl Into<String>, provenance: CapabilityProvenance) -> Self {
        Self::non_available(GateOutcome::Unavailable, reason_id, provenance)
    }

    pub fn unknown(reason_id: impl Into<String>, provenance: CapabilityProvenance) -> Self {
        Self::non_available(GateOutcome::Unknown, reason_id, provenance)
    }

    fn non_available(
        outcome: GateOutcome,
        reason_id: impl Into<String>,
        provenance: CapabilityProvenance,
    ) -> Self {
        Self {
            outcome,
            reason_ids: vec![reason_id.into()],
            constraints: BTreeMap::new(),
            provenance: vec![provenance],
        }
    }

    pub fn with_reason(mut self, reason_id: impl Into<String>) -> Self {
        self.reason_ids.push(reason_id.into());
        self
    }

    pub fn with_constraint(
        mut self,
        constraint_id: impl Into<String>,
        value: CapabilityConstraint,
    ) -> Self {
        self.constraints.insert(constraint_id.into(), value);
        self
    }

    pub fn with_provenance(mut self, provenance: CapabilityProvenance) -> Self {
        self.provenance.push(provenance);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeCapabilityState {
    Available,
    Degraded,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityStatus {
    pub capability_id: String,
    pub state_id: String,
    pub scope_id: String,
    pub reason_ids: Vec<String>,
    pub constraints: BTreeMap<String, CapabilityConstraint>,
    pub provenance: Vec<CapabilityProvenance>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvailabilityPredicateStatus {
    pub target_id: String,
    pub declared_status: String,
    pub state_id: String,
    pub predicate_capability_ids: Vec<String>,
    pub reason_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CapabilityMatrix {
    scope_id: String,
    gates: BTreeMap<String, BTreeMap<AvailabilityGate, GateEvidence>>,
}

impl CapabilityMatrix {
    pub fn new(
        catalog: &CapabilityCatalog,
        scope_id: impl Into<String>,
    ) -> Result<Self, CapabilityError> {
        let scope_id = scope_id.into();
        validate_stable_id("scope_id", &scope_id)?;
        Ok(Self {
            scope_id,
            gates: catalog
                .definitions()
                .map(|definition| (definition.capability_id.clone(), BTreeMap::new()))
                .collect(),
        })
    }

    pub fn set_gate(
        &mut self,
        catalog: &CapabilityCatalog,
        capability_id: &str,
        gate: AvailabilityGate,
        evidence: GateEvidence,
    ) -> Result<(), CapabilityError> {
        let definition = catalog
            .definition(capability_id)
            .ok_or_else(|| CapabilityError::UnknownCapability(capability_id.into()))?;
        if !definition.required_gates.contains(&gate) {
            return Err(CapabilityError::UnexpectedGate {
                capability_id: capability_id.into(),
                gate,
            });
        }
        let gates = self
            .gates
            .get_mut(capability_id)
            .ok_or_else(|| CapabilityError::UnexpectedCapability(capability_id.into()))?;
        gates.insert(gate, evidence);
        Ok(())
    }

    pub fn evaluate(
        &self,
        catalog: &CapabilityCatalog,
    ) -> Result<Vec<CapabilityStatus>, CapabilityError> {
        for capability_id in self.gates.keys() {
            if !catalog.definitions.contains_key(capability_id) {
                return Err(CapabilityError::UnexpectedCapability(capability_id.clone()));
            }
        }
        let mut statuses = Vec::with_capacity(catalog.definitions.len());
        for definition in catalog.definitions() {
            let gates = self.gates.get(&definition.capability_id).ok_or_else(|| {
                CapabilityError::MissingCapability(definition.capability_id.clone())
            })?;
            statuses.push(evaluate_capability(
                catalog,
                definition,
                &self.scope_id,
                gates,
            )?);
        }
        Ok(statuses)
    }
}

fn evaluate_capability(
    catalog: &CapabilityCatalog,
    definition: &CapabilityDefinition,
    scope_id: &str,
    gates: &BTreeMap<AvailabilityGate, GateEvidence>,
) -> Result<CapabilityStatus, CapabilityError> {
    let required = definition
        .required_gates
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for gate in gates.keys() {
        if !required.contains(gate) {
            return Err(CapabilityError::UnexpectedGate {
                capability_id: definition.capability_id.clone(),
                gate: *gate,
            });
        }
    }

    let mut state = RuntimeCapabilityState::Available;
    let mut reason_ids = BTreeSet::new();
    let mut constraints = BTreeMap::new();
    let mut provenance = BTreeSet::new();
    for gate in &definition.required_gates {
        let evidence = gates
            .get(gate)
            .ok_or_else(|| CapabilityError::MissingGate {
                capability_id: definition.capability_id.clone(),
                gate: *gate,
            })?;
        validate_gate_evidence(catalog, &definition.capability_id, *gate, evidence)?;
        state = combine_state(state, evidence.outcome);
        reason_ids.extend(evidence.reason_ids.iter().cloned());
        provenance.extend(evidence.provenance.iter().cloned());
        for (constraint_id, value) in &evidence.constraints {
            if let Some(previous) = constraints.insert(constraint_id.clone(), value.clone()) {
                if previous != *value {
                    return Err(CapabilityError::ConflictingConstraint(
                        constraint_id.clone(),
                    ));
                }
            }
        }
    }

    Ok(CapabilityStatus {
        capability_id: definition.capability_id.clone(),
        state_id: catalog.state_id(state)?,
        scope_id: scope_id.into(),
        reason_ids: reason_ids.into_iter().collect(),
        constraints,
        provenance: provenance.into_iter().collect(),
    })
}

fn combine_state(current: RuntimeCapabilityState, gate: GateOutcome) -> RuntimeCapabilityState {
    let gate = match gate {
        GateOutcome::Available => RuntimeCapabilityState::Available,
        GateOutcome::Degraded => RuntimeCapabilityState::Degraded,
        GateOutcome::Unavailable => RuntimeCapabilityState::Unavailable,
        GateOutcome::Unknown => RuntimeCapabilityState::Unknown,
    };
    combine_runtime_states(current, gate)
}

fn combine_runtime_states(
    current: RuntimeCapabilityState,
    additional: RuntimeCapabilityState,
) -> RuntimeCapabilityState {
    match (current, additional) {
        (RuntimeCapabilityState::Unavailable, _) | (_, RuntimeCapabilityState::Unavailable) => {
            RuntimeCapabilityState::Unavailable
        }
        (RuntimeCapabilityState::Unknown, _) | (_, RuntimeCapabilityState::Unknown) => {
            RuntimeCapabilityState::Unknown
        }
        (RuntimeCapabilityState::Degraded, _) | (_, RuntimeCapabilityState::Degraded) => {
            RuntimeCapabilityState::Degraded
        }
        _ => RuntimeCapabilityState::Available,
    }
}

fn validate_gate_evidence(
    catalog: &CapabilityCatalog,
    capability_id: &str,
    gate: AvailabilityGate,
    evidence: &GateEvidence,
) -> Result<(), CapabilityError> {
    if evidence.provenance.is_empty() {
        return Err(invalid_evidence(capability_id, gate, "provenance is empty"));
    }
    if evidence.outcome == GateOutcome::Available && !evidence.reason_ids.is_empty() {
        return Err(invalid_evidence(
            capability_id,
            gate,
            "available evidence cannot carry unavailable reasons",
        ));
    }
    if evidence.outcome != GateOutcome::Available && evidence.reason_ids.is_empty() {
        return Err(invalid_evidence(
            capability_id,
            gate,
            "non-available evidence requires a stable reason",
        ));
    }
    for reason_id in &evidence.reason_ids {
        let reason = catalog
            .reason(reason_id)
            .ok_or_else(|| CapabilityError::UnknownAvailabilityReason(reason_id.clone()))?;
        if reason.gate != gate {
            return Err(invalid_evidence(
                capability_id,
                gate,
                &format!("reason {reason_id} belongs to {:?}", reason.gate),
            ));
        }
    }
    for item in &evidence.provenance {
        validate_stable_id("evidence_id", &item.evidence_id)?;
        if !provenance_matches_gate(item.kind_id, gate) {
            return Err(invalid_evidence(
                capability_id,
                gate,
                &format!("provenance {:?} cannot prove this gate", item.kind_id),
            ));
        }
    }
    for (constraint_id, value) in &evidence.constraints {
        validate_stable_id("constraint_id", constraint_id)?;
        if let CapabilityConstraint::Identifier(identifier) = value {
            validate_stable_id("constraint_value", identifier)?;
        }
    }
    Ok(())
}

fn provenance_matches_gate(kind: ProvenanceKind, gate: AvailabilityGate) -> bool {
    match gate {
        AvailabilityGate::Declaration => kind == ProvenanceKind::Contract,
        AvailabilityGate::Target => {
            matches!(kind, ProvenanceKind::Contract | ProvenanceKind::Build)
        }
        AvailabilityGate::BuildFeature => kind == ProvenanceKind::Build,
        AvailabilityGate::OperatorPolicy => kind == ProvenanceKind::OperatorPolicy,
        AvailabilityGate::RuntimeProbe => kind == ProvenanceKind::RuntimeProbe,
        AvailabilityGate::BackendSemantic => kind == ProvenanceKind::BackendProbe,
        AvailabilityGate::ConsumerProjection => kind == ProvenanceKind::ConsumerProjection,
    }
}

fn invalid_evidence(capability_id: &str, gate: AvailabilityGate, message: &str) -> CapabilityError {
    CapabilityError::InvalidGateEvidence {
        capability_id: capability_id.into(),
        gate,
        message: message.into(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitySubject {
    pub runtime_id: String,
    pub target_id: String,
    pub build_revision: PublicDigest,
}

impl CapabilitySubject {
    pub fn new(
        runtime_id: impl Into<String>,
        target_id: impl Into<String>,
        build_revision: impl Into<String>,
    ) -> Result<Self, CapabilityError> {
        let runtime_id = runtime_id.into();
        let target_id = target_id.into();
        validate_stable_id("runtime_id", &runtime_id)?;
        validate_stable_id("target_id", &target_id)?;
        let build_revision = PublicDigest::parse(build_revision.into())
            .map_err(CapabilityError::InvalidBuildRevision)?;
        Ok(Self {
            runtime_id,
            target_id,
            build_revision,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitySecurity {
    pub mode_id: String,
    pub authenticated: bool,
    pub origin_policy_id: String,
}

impl CapabilitySecurity {
    pub fn from_mode(
        catalog: &CapabilityCatalog,
        mode_id: &str,
        authenticated: bool,
    ) -> Result<Self, CapabilityError> {
        let mode = catalog
            .security_modes
            .get(mode_id)
            .ok_or_else(|| CapabilityError::UnknownSecurityMode(mode_id.into()))?;
        if mode.authentication_required != authenticated {
            return Err(CapabilityError::AuthenticationMismatch {
                mode_id: mode_id.into(),
                required: mode.authentication_required,
                actual: authenticated,
            });
        }
        Ok(Self {
            mode_id: mode.mode_id.clone(),
            authenticated,
            origin_policy_id: mode.origin_policy_id.clone(),
        })
    }

    pub fn operator_policy_evidence(
        &self,
        catalog: &CapabilityCatalog,
    ) -> Result<GateEvidence, CapabilityError> {
        let mode = catalog
            .security_modes
            .get(&self.mode_id)
            .ok_or_else(|| CapabilityError::UnknownSecurityMode(self.mode_id.clone()))?;
        let provenance = CapabilityProvenance::new(ProvenanceKind::OperatorPolicy, &mode.mode_id);
        if let Some(reason_id) = &mode.degraded_reason_id {
            Ok(GateEvidence::degraded(reason_id, provenance))
        } else {
            Ok(GateEvidence::available(provenance))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationReceiptWatermark {
    pub schema_version: u16,
    pub runtime_epoch: RuntimeEpoch,
    pub event_sequence: Option<EventSequence>,
    pub accepted_through: Option<RevisionId>,
    pub last_confirmed_revision: Option<RevisionId>,
    pub last_rejected_revision: Option<RevisionId>,
}

impl TryFrom<&RuntimeMutationStatus> for MutationReceiptWatermark {
    type Error = CapabilityError;

    fn try_from(status: &RuntimeMutationStatus) -> Result<Self, Self::Error> {
        if status.schema_version != MUTATION_SCHEMA_VERSION {
            return Err(CapabilityError::MutationSchemaVersion(
                status.schema_version,
            ));
        }
        Ok(Self {
            schema_version: status.schema_version,
            runtime_epoch: status.runtime_epoch,
            event_sequence: status.event_sequence,
            accepted_through: status.accepted_through,
            last_confirmed_revision: status.last_confirmed_revision,
            last_rejected_revision: status.last_rejected_revision,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilitySnapshotInput {
    pub contract_revision: PublicDigest,
    pub subject: CapabilitySubject,
    pub mutation_revision: MutationReceiptWatermark,
    pub security: CapabilitySecurity,
    pub capabilities: Vec<CapabilityStatus>,
}

impl CapabilitySnapshotInput {
    pub fn new(
        contract_revision: impl Into<String>,
        subject: CapabilitySubject,
        mutation_status: &RuntimeMutationStatus,
        security: CapabilitySecurity,
        capabilities: Vec<CapabilityStatus>,
    ) -> Result<Self, CapabilityError> {
        let contract_revision = PublicDigest::parse(contract_revision.into())
            .map_err(CapabilityError::InvalidBuildRevision)?;
        Ok(Self {
            contract_revision,
            subject,
            mutation_revision: MutationReceiptWatermark::try_from(mutation_status)?,
            security,
            capabilities,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitySnapshot {
    pub schema_id: String,
    pub contract_revision: PublicDigest,
    pub generation: u64,
    pub snapshot_id: PublicDigest,
    pub observed_at: Timestamp,
    pub subject: CapabilitySubject,
    pub mutation_revision: MutationReceiptWatermark,
    pub security: CapabilitySecurity,
    pub capabilities: Vec<CapabilityStatus>,
}

impl CapabilitySnapshot {
    pub fn canonical_semantic_json(&self) -> Result<Vec<u8>, CapabilityError> {
        canonical_json_of(&SnapshotSemanticPayload::from(self))
            .map_err(|error| CapabilityError::CanonicalJson(error.to_string()))
    }

    pub fn verify_snapshot_id(&self) -> Result<(), CapabilityError> {
        let expected = semantic_snapshot_id(&SnapshotSemanticPayload::from(self))?;
        if expected == self.snapshot_id {
            Ok(())
        } else {
            Err(CapabilityError::SnapshotIdMismatch)
        }
    }
}

#[derive(Default)]
pub struct CapabilitySnapshotAssembler {
    generation: u64,
    previous_semantics: Option<Vec<u8>>,
}

impl CapabilitySnapshotAssembler {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn assemble(
        &mut self,
        mut input: CapabilitySnapshotInput,
        observed_at: Timestamp,
    ) -> Result<CapabilitySnapshot, CapabilityError> {
        normalize_capabilities(&mut input.capabilities)?;
        validate_snapshot_input(&input)?;

        let seed = SnapshotSemanticSeed::from(&input);
        let semantics = canonical_json_of(&seed)
            .map_err(|error| CapabilityError::CanonicalJson(error.to_string()))?;
        if self.previous_semantics.as_ref() != Some(&semantics) {
            self.generation = self
                .generation
                .checked_add(1)
                .filter(|generation| *generation <= MAX_CANONICAL_JSON_INTEGER)
                .ok_or(CapabilityError::GenerationExhausted)?;
            self.previous_semantics = Some(semantics);
        }

        let payload = SnapshotSemanticPayload {
            schema_id: CAPABILITY_SNAPSHOT_SCHEMA_ID,
            contract_revision: &input.contract_revision,
            generation: self.generation,
            subject: &input.subject,
            mutation_revision: &input.mutation_revision,
            security: &input.security,
            capabilities: &input.capabilities,
        };
        let snapshot_id = semantic_snapshot_id(&payload)?;
        Ok(CapabilitySnapshot {
            schema_id: CAPABILITY_SNAPSHOT_SCHEMA_ID.into(),
            contract_revision: input.contract_revision,
            generation: self.generation,
            snapshot_id,
            observed_at,
            subject: input.subject,
            mutation_revision: input.mutation_revision,
            security: input.security,
            capabilities: input.capabilities,
        })
    }
}

#[derive(Serialize)]
struct SnapshotSemanticSeed<'a> {
    schema_id: &'static str,
    contract_revision: &'a PublicDigest,
    subject: &'a CapabilitySubject,
    mutation_revision: &'a MutationReceiptWatermark,
    security: &'a CapabilitySecurity,
    capabilities: &'a [CapabilityStatus],
}

impl<'a> From<&'a CapabilitySnapshotInput> for SnapshotSemanticSeed<'a> {
    fn from(input: &'a CapabilitySnapshotInput) -> Self {
        Self {
            schema_id: CAPABILITY_SNAPSHOT_SCHEMA_ID,
            contract_revision: &input.contract_revision,
            subject: &input.subject,
            mutation_revision: &input.mutation_revision,
            security: &input.security,
            capabilities: &input.capabilities,
        }
    }
}

#[derive(Serialize)]
struct SnapshotSemanticPayload<'a> {
    schema_id: &'a str,
    contract_revision: &'a PublicDigest,
    generation: u64,
    subject: &'a CapabilitySubject,
    mutation_revision: &'a MutationReceiptWatermark,
    security: &'a CapabilitySecurity,
    capabilities: &'a [CapabilityStatus],
}

impl<'a> From<&'a CapabilitySnapshot> for SnapshotSemanticPayload<'a> {
    fn from(snapshot: &'a CapabilitySnapshot) -> Self {
        Self {
            schema_id: &snapshot.schema_id,
            contract_revision: &snapshot.contract_revision,
            generation: snapshot.generation,
            subject: &snapshot.subject,
            mutation_revision: &snapshot.mutation_revision,
            security: &snapshot.security,
            capabilities: &snapshot.capabilities,
        }
    }
}

fn semantic_snapshot_id(
    payload: &SnapshotSemanticPayload<'_>,
) -> Result<PublicDigest, CapabilityError> {
    let digest = canonical_sha256_hex(payload)
        .map_err(|error| CapabilityError::CanonicalJson(error.to_string()))?;
    PublicDigest::parse(format!("sha256:{digest}")).map_err(CapabilityError::InvalidBuildRevision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutation::{LiveState, ReceiptWindow, RuntimeMutationStatus};
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    struct AcceptedFixture {
        catalog: CapabilityCatalog,
    }

    fn accepted() -> &'static AcceptedFixture {
        static FIXTURE: OnceLock<AcceptedFixture> = OnceLock::new();
        FIXTURE.get_or_init(|| {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../api/effective-metadata-v1.json");
            let source = std::fs::read_to_string(path).expect("accepted M05 metadata must exist");
            let metadata: ConventionsMetadata =
                serde_json::from_str(&source).expect("accepted M05 metadata must decode");
            AcceptedFixture {
                catalog: CapabilityCatalog::from_conventions(&metadata)
                    .expect("accepted M05 metadata must validate"),
            }
        })
    }

    fn provenance_for(gate: AvailabilityGate) -> CapabilityProvenance {
        let (kind, evidence_id) = match gate {
            AvailabilityGate::Declaration => (ProvenanceKind::Contract, "contract.test.v1"),
            AvailabilityGate::Target => (ProvenanceKind::Build, "build.target.test"),
            AvailabilityGate::BuildFeature => (ProvenanceKind::Build, "build.feature.test"),
            AvailabilityGate::OperatorPolicy => (ProvenanceKind::OperatorPolicy, "policy.test.v1"),
            AvailabilityGate::RuntimeProbe => (ProvenanceKind::RuntimeProbe, "probe.runtime.test"),
            AvailabilityGate::BackendSemantic => {
                (ProvenanceKind::BackendProbe, "probe.backend.test")
            }
            AvailabilityGate::ConsumerProjection => (
                ProvenanceKind::ConsumerProjection,
                "projection.consumer.test",
            ),
        };
        CapabilityProvenance::new(kind, evidence_id)
    }

    fn available_gate(gate: AvailabilityGate) -> GateEvidence {
        GateEvidence::available(provenance_for(gate))
    }

    fn unavailable_gate(gate: AvailabilityGate, reason_id: &str) -> GateEvidence {
        GateEvidence::unavailable(reason_id, provenance_for(gate))
    }

    fn unknown_gate(gate: AvailabilityGate, reason_id: &str) -> GateEvidence {
        GateEvidence::unknown(reason_id, provenance_for(gate))
    }

    fn available_matrix(reverse: bool) -> CapabilityMatrix {
        let catalog = &accepted().catalog;
        let mut matrix = CapabilityMatrix::new(catalog, "scope.runtime").expect("valid scope");
        let mut pairs = catalog
            .definitions()
            .flat_map(|definition| {
                definition
                    .required_gates
                    .iter()
                    .copied()
                    .map(move |gate| (definition.capability_id.clone(), gate))
            })
            .collect::<Vec<_>>();
        if reverse {
            pairs.reverse();
        }
        for (capability_id, gate) in pairs {
            matrix
                .set_gate(catalog, &capability_id, gate, available_gate(gate))
                .expect("catalog gate must accept evidence");
        }
        matrix
    }

    fn fixed_status(event_sequence: u64, revision: u64) -> RuntimeMutationStatus {
        RuntimeMutationStatus {
            schema_version: MUTATION_SCHEMA_VERSION,
            runtime_epoch: RuntimeEpoch::parse("01890f3c-7b5a-7cc0-98c4-dc0c0c0c0c0c")
                .expect("valid fixed UUIDv7"),
            event_sequence: Some(EventSequence::new(event_sequence).expect("positive sequence")),
            accepted_through: Some(RevisionId::new(revision).expect("positive revision")),
            last_confirmed_revision: Some(
                RevisionId::new(revision).expect("positive confirmed revision"),
            ),
            last_rejected_revision: None,
            live_state: LiveState::Clean,
            pending: Vec::new(),
            receipt_window: ReceiptWindow {
                first_event_sequence: None,
                last_event_sequence: None,
                first_revision: None,
                last_revision: None,
                expires_before: None,
            },
        }
    }

    fn fixed_subject(target_id: &str) -> CapabilitySubject {
        CapabilitySubject::new(
            "runtime.local",
            target_id,
            format!("sha256:{}", "a".repeat(64)),
        )
        .expect("fixed subject must be valid")
    }

    fn snapshot_input(
        matrix: &CapabilityMatrix,
        status: &RuntimeMutationStatus,
        target_id: &str,
        mode_id: &str,
        authenticated: bool,
    ) -> CapabilitySnapshotInput {
        let catalog = &accepted().catalog;
        CapabilitySnapshotInput::new(
            format!("sha256:{}", "b".repeat(64)),
            fixed_subject(target_id),
            status,
            CapabilitySecurity::from_mode(catalog, mode_id, authenticated)
                .expect("canonical security mode must validate"),
            matrix.evaluate(catalog).expect("matrix must evaluate"),
        )
        .expect("snapshot input must validate")
    }

    fn observed(second: u8) -> Timestamp {
        Timestamp::parse(format!("2026-07-18T06:00:{second:02}Z"))
            .expect("fixed timestamp must parse")
    }

    fn evaluate_one(
        capability_id: &str,
        evidence: impl IntoIterator<Item = (AvailabilityGate, GateEvidence)>,
    ) -> Result<CapabilityStatus, CapabilityError> {
        let catalog = &accepted().catalog;
        let definition = catalog
            .definition(capability_id)
            .expect("canonical capability must exist");
        evaluate_capability(
            catalog,
            definition,
            "scope.runtime",
            &evidence.into_iter().collect(),
        )
    }

    #[test]
    fn catalog_is_derived_from_the_accepted_m05_registry() {
        let catalog = &accepted().catalog;
        assert_eq!(catalog.definitions().len(), 28);
        assert_eq!(catalog.security_modes().len(), 4);
        assert_eq!(catalog.availability_bindings().len(), 3626);
        assert!(catalog.definition("capability.plugin.mi_ugens").is_some());
        assert_eq!(
            catalog
                .definition("capability.midi.clock")
                .expect("MIDI clock capability")
                .required_gates,
            [
                AvailabilityGate::Target,
                AvailabilityGate::BuildFeature,
                AvailabilityGate::OperatorPolicy,
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ]
        );
    }

    #[test]
    fn receipt_capabilities_require_positive_runtime_truth() {
        let catalog = &accepted().catalog;
        for capability_id in [
            "capability.receipt.atomic_generation_activation",
            "capability.receipt.backend_barrier",
            "capability.receipt.cancellation_window",
            "capability.receipt.expected_revision",
            "capability.receipt.idempotency",
            "capability.receipt.ledger_retention",
            "capability.receipt.musical_boundary",
        ] {
            let definition = catalog
                .definition(capability_id)
                .expect("receipt capability must be canonical");
            let pending = evaluate_one(
                capability_id,
                definition.required_gates.iter().copied().map(|gate| {
                    if gate == AvailabilityGate::RuntimeProbe {
                        (gate, unknown_gate(gate, "reason.probe_pending"))
                    } else {
                        (gate, available_gate(gate))
                    }
                }),
            )
            .expect("pending receipt capability must evaluate");
            assert_eq!(pending.state_id, UNKNOWN_STATE_ID);
            assert_eq!(pending.reason_ids, ["reason.probe_pending"]);

            let unavailable = evaluate_one(
                capability_id,
                definition.required_gates.iter().copied().map(|gate| {
                    if gate == AvailabilityGate::RuntimeProbe {
                        (
                            gate,
                            unavailable_gate(gate, "reason.runtime_dependency_missing"),
                        )
                    } else {
                        (gate, available_gate(gate))
                    }
                }),
            )
            .expect("unavailable receipt capability must evaluate");
            assert_eq!(unavailable.state_id, UNAVAILABLE_STATE_ID);
            assert_eq!(
                unavailable.reason_ids,
                ["reason.runtime_dependency_missing"]
            );

            let available = evaluate_one(
                capability_id,
                definition
                    .required_gates
                    .iter()
                    .copied()
                    .map(|gate| (gate, available_gate(gate))),
            )
            .expect("proven receipt capability must evaluate");
            assert_eq!(available.state_id, AVAILABLE_STATE_ID);
            assert_eq!(available.provenance.len(), definition.required_gates.len());
        }
    }

    #[test]
    fn accepted_entry_predicates_follow_runtime_capability_truth_without_source_paths() {
        let catalog = &accepted().catalog;
        let available_statuses = available_matrix(false)
            .evaluate(catalog)
            .expect("available matrix");
        let plugin_binding = catalog
            .availability_bindings()
            .find(|binding| {
                binding.declared_status == "conditional"
                    && binding.predicate_capability_ids == ["capability.plugin.mi_ugens"]
            })
            .expect("accepted mi-UGens availability binding");
        let available = catalog
            .evaluate_availability_binding(&plugin_binding.target_id, &available_statuses)
            .expect("available predicate");
        assert_eq!(available.state_id, AVAILABLE_STATE_ID);

        let mut missing_plugin = available_statuses.clone();
        let plugin = missing_plugin
            .iter_mut()
            .find(|status| status.capability_id == "capability.plugin.mi_ugens")
            .expect("mi-UGens runtime status");
        plugin.state_id = UNAVAILABLE_STATE_ID.into();
        plugin.reason_ids = vec!["reason.plugin_missing".into()];
        missing_plugin.reverse();
        let unavailable = catalog
            .evaluate_availability_binding(&plugin_binding.target_id, &missing_plugin)
            .expect("unavailable predicate");
        assert_eq!(unavailable.state_id, UNAVAILABLE_STATE_ID);
        assert_eq!(unavailable.reason_ids, ["reason.plugin_missing"]);

        for (declared_status, expected_reason) in [
            ("documentation_only", "reason.documentation_only"),
            ("quarantined", "reason.quarantined"),
        ] {
            let binding = catalog
                .availability_bindings()
                .find(|binding| binding.declared_status == declared_status)
                .expect("accepted unavailable declaration binding");
            let evaluated = catalog
                .evaluate_availability_binding(&binding.target_id, &available_statuses)
                .expect("declaration availability");
            assert_eq!(evaluated.state_id, UNAVAILABLE_STATE_ID);
            assert!(evaluated.reason_ids.contains(&expected_reason.into()));
            let json = serde_json::to_string(&evaluated).expect("predicate status JSON");
            assert!(!json.contains("source_anchor"));
            assert!(!json.contains("crates/"));
        }
    }

    #[test]
    fn compile_feature_alone_never_claims_live_midi_availability() {
        let pending = evaluate_one(
            "capability.midi.output",
            [
                (
                    AvailabilityGate::Target,
                    available_gate(AvailabilityGate::Target),
                ),
                (
                    AvailabilityGate::BuildFeature,
                    available_gate(AvailabilityGate::BuildFeature),
                ),
                (
                    AvailabilityGate::OperatorPolicy,
                    available_gate(AvailabilityGate::OperatorPolicy),
                ),
                (
                    AvailabilityGate::RuntimeProbe,
                    unknown_gate(AvailabilityGate::RuntimeProbe, "reason.probe_pending"),
                ),
            ],
        )
        .expect("pending MIDI matrix must evaluate");
        assert_eq!(pending.state_id, UNKNOWN_STATE_ID);
        assert_eq!(pending.reason_ids, ["reason.probe_pending"]);

        let available = evaluate_one(
            "capability.midi.output",
            [
                (
                    AvailabilityGate::Target,
                    available_gate(AvailabilityGate::Target),
                ),
                (
                    AvailabilityGate::BuildFeature,
                    available_gate(AvailabilityGate::BuildFeature),
                ),
                (
                    AvailabilityGate::OperatorPolicy,
                    available_gate(AvailabilityGate::OperatorPolicy),
                ),
                (
                    AvailabilityGate::RuntimeProbe,
                    available_gate(AvailabilityGate::RuntimeProbe),
                ),
            ],
        )
        .expect("positive MIDI matrix must evaluate");
        assert_eq!(available.state_id, AVAILABLE_STATE_ID);
    }

    #[test]
    fn native_wasm_and_backend_noop_matrices_are_source_truthful() {
        let native_on_wasm = evaluate_one(
            "capability.backend.scsynth.native",
            [
                (
                    AvailabilityGate::Target,
                    unavailable_gate(AvailabilityGate::Target, "reason.target_unsupported"),
                ),
                (
                    AvailabilityGate::RuntimeProbe,
                    unknown_gate(AvailabilityGate::RuntimeProbe, "reason.probe_pending"),
                ),
                (
                    AvailabilityGate::BackendSemantic,
                    unavailable_gate(
                        AvailabilityGate::BackendSemantic,
                        "reason.backend_semantics_missing",
                    ),
                ),
            ],
        )
        .expect("native capability on WASM must evaluate");
        assert_eq!(native_on_wasm.state_id, UNAVAILABLE_STATE_ID);
        assert!(native_on_wasm
            .reason_ids
            .contains(&"reason.target_unsupported".into()));

        let wasm_backend = evaluate_one(
            "capability.backend.web_scsynth.wasm",
            [
                (
                    AvailabilityGate::Target,
                    available_gate(AvailabilityGate::Target),
                ),
                (
                    AvailabilityGate::RuntimeProbe,
                    available_gate(AvailabilityGate::RuntimeProbe),
                ),
                (
                    AvailabilityGate::BackendSemantic,
                    available_gate(AvailabilityGate::BackendSemantic),
                ),
            ],
        )
        .expect("WASM backend matrix must evaluate");
        assert_eq!(wasm_backend.state_id, AVAILABLE_STATE_ID);

        let wasm_write_noop = evaluate_one(
            "capability.audio.buffer.write_file",
            [
                (
                    AvailabilityGate::Target,
                    available_gate(AvailabilityGate::Target),
                ),
                (
                    AvailabilityGate::RuntimeProbe,
                    available_gate(AvailabilityGate::RuntimeProbe),
                ),
                (
                    AvailabilityGate::BackendSemantic,
                    unavailable_gate(
                        AvailabilityGate::BackendSemantic,
                        "reason.implementation_noop",
                    ),
                ),
            ],
        )
        .expect("WASM write no-op matrix must evaluate");
        assert_eq!(wasm_write_noop.state_id, UNAVAILABLE_STATE_ID);
        assert_eq!(wasm_write_noop.reason_ids, ["reason.implementation_noop"]);
    }

    #[test]
    fn recording_and_extension_feature_policy_permutations_remain_explicit() {
        let recording_disabled = evaluate_one(
            "capability.recording.audio",
            [
                (
                    AvailabilityGate::Target,
                    available_gate(AvailabilityGate::Target),
                ),
                (
                    AvailabilityGate::BuildFeature,
                    unavailable_gate(
                        AvailabilityGate::BuildFeature,
                        "reason.compile_feature_disabled",
                    ),
                ),
                (
                    AvailabilityGate::OperatorPolicy,
                    unavailable_gate(AvailabilityGate::OperatorPolicy, "reason.operator_disabled"),
                ),
                (
                    AvailabilityGate::RuntimeProbe,
                    unknown_gate(AvailabilityGate::RuntimeProbe, "reason.probe_pending"),
                ),
                (
                    AvailabilityGate::BackendSemantic,
                    unavailable_gate(
                        AvailabilityGate::BackendSemantic,
                        "reason.backend_semantics_missing",
                    ),
                ),
            ],
        )
        .expect("disabled recording matrix must evaluate");
        assert_eq!(recording_disabled.state_id, UNAVAILABLE_STATE_ID);
        assert_eq!(
            recording_disabled.reason_ids,
            [
                "reason.backend_semantics_missing",
                "reason.compile_feature_disabled",
                "reason.operator_disabled",
                "reason.probe_pending",
            ]
        );

        for capability_id in [
            "capability.extension.filesystem",
            "capability.extension.network",
            "capability.extension.process",
        ] {
            let disabled = evaluate_one(
                capability_id,
                [
                    (
                        AvailabilityGate::BuildFeature,
                        available_gate(AvailabilityGate::BuildFeature),
                    ),
                    (
                        AvailabilityGate::OperatorPolicy,
                        unavailable_gate(
                            AvailabilityGate::OperatorPolicy,
                            "reason.operator_disabled",
                        ),
                    ),
                ],
            )
            .expect("disabled extension matrix must evaluate");
            assert_eq!(disabled.state_id, UNAVAILABLE_STATE_ID);

            let enabled = evaluate_one(
                capability_id,
                [
                    (
                        AvailabilityGate::BuildFeature,
                        available_gate(AvailabilityGate::BuildFeature),
                    ),
                    (
                        AvailabilityGate::OperatorPolicy,
                        available_gate(AvailabilityGate::OperatorPolicy),
                    ),
                ],
            )
            .expect("enabled extension matrix must evaluate");
            assert_eq!(enabled.state_id, AVAILABLE_STATE_ID);
        }
    }

    #[test]
    fn security_modes_consume_canonical_policy_and_degradation_data() {
        let catalog = &accepted().catalog;
        for (mode_id, authenticated, expected_state) in [
            (
                "security.http.authenticated_remote",
                true,
                AVAILABLE_STATE_ID,
            ),
            ("security.http.loopback_local", false, AVAILABLE_STATE_ID),
            ("security.http.insecure_remote", false, DEGRADED_STATE_ID),
            (
                "security.http.legacy_loopback_unrestricted_cors",
                false,
                DEGRADED_STATE_ID,
            ),
        ] {
            let security = CapabilitySecurity::from_mode(catalog, mode_id, authenticated)
                .expect("canonical security mode");
            let status = evaluate_one(
                "capability.http.eval",
                [
                    (
                        AvailabilityGate::Declaration,
                        available_gate(AvailabilityGate::Declaration),
                    ),
                    (
                        AvailabilityGate::OperatorPolicy,
                        security
                            .operator_policy_evidence(catalog)
                            .expect("canonical policy evidence"),
                    ),
                ],
            )
            .expect("security matrix must evaluate");
            assert_eq!(status.state_id, expected_state, "{mode_id}");
        }
        assert!(CapabilitySecurity::from_mode(
            catalog,
            "security.http.authenticated_remote",
            false,
        )
        .is_err());
    }

    #[test]
    fn reasons_and_provenance_are_sorted_deduplicated_and_gate_truthful() {
        let evidence = GateEvidence::unavailable(
            "reason.probe_failed",
            CapabilityProvenance::new(ProvenanceKind::RuntimeProbe, "probe.zeta"),
        )
        .with_reason("reason.plugin_missing")
        .with_reason("reason.probe_failed")
        .with_provenance(CapabilityProvenance::new(
            ProvenanceKind::RuntimeProbe,
            "probe.alpha",
        ))
        .with_provenance(CapabilityProvenance::new(
            ProvenanceKind::RuntimeProbe,
            "probe.alpha",
        ));
        let status = evaluate_one(
            "capability.plugin.mi_ugens",
            [
                (AvailabilityGate::RuntimeProbe, evidence),
                (
                    AvailabilityGate::BackendSemantic,
                    available_gate(AvailabilityGate::BackendSemantic),
                ),
            ],
        )
        .expect("truthful probe evidence must evaluate");
        assert_eq!(
            status.reason_ids,
            ["reason.plugin_missing", "reason.probe_failed"]
        );
        assert_eq!(status.provenance[0].evidence_id, "probe.alpha");
        assert_eq!(status.provenance[1].evidence_id, "probe.zeta");

        let wrong_reason = evaluate_one(
            "capability.plugin.mi_ugens",
            [
                (
                    AvailabilityGate::RuntimeProbe,
                    unavailable_gate(
                        AvailabilityGate::RuntimeProbe,
                        "reason.compile_feature_disabled",
                    ),
                ),
                (
                    AvailabilityGate::BackendSemantic,
                    available_gate(AvailabilityGate::BackendSemantic),
                ),
            ],
        );
        assert!(matches!(
            wrong_reason,
            Err(CapabilityError::InvalidGateEvidence { .. })
        ));

        let wrong_provenance = evaluate_one(
            "capability.plugin.mi_ugens",
            [
                (
                    AvailabilityGate::RuntimeProbe,
                    GateEvidence::available(CapabilityProvenance::new(
                        ProvenanceKind::Build,
                        "build.feature.test",
                    )),
                ),
                (
                    AvailabilityGate::BackendSemantic,
                    available_gate(AvailabilityGate::BackendSemantic),
                ),
            ],
        );
        assert!(matches!(
            wrong_provenance,
            Err(CapabilityError::InvalidGateEvidence { .. })
        ));
    }

    #[test]
    fn snapshot_ids_are_order_independent_and_ignore_observation_time() {
        let status = fixed_status(1, 1);
        let mut first_assembler = CapabilitySnapshotAssembler::new();
        let first = first_assembler
            .assemble(
                snapshot_input(
                    &available_matrix(false),
                    &status,
                    "target.native.linux",
                    "security.http.loopback_local",
                    false,
                ),
                observed(1),
            )
            .expect("first snapshot");
        let mut second_assembler = CapabilitySnapshotAssembler::new();
        let second = second_assembler
            .assemble(
                snapshot_input(
                    &available_matrix(true),
                    &status,
                    "target.native.linux",
                    "security.http.loopback_local",
                    false,
                ),
                observed(2),
            )
            .expect("second snapshot");

        assert_eq!(first.generation, 1);
        assert_eq!(first.snapshot_id, second.snapshot_id);
        assert_eq!(
            first
                .canonical_semantic_json()
                .expect("canonical semantic JSON"),
            second
                .canonical_semantic_json()
                .expect("canonical semantic JSON")
        );
        assert_ne!(first.observed_at, second.observed_at);
        first.verify_snapshot_id().expect("snapshot ID must verify");
        assert!(first
            .capabilities
            .windows(2)
            .all(|pair| pair[0].capability_id < pair[1].capability_id));
    }

    #[test]
    fn generation_changes_once_per_semantic_transition_and_not_for_receipt_retention_time() {
        let catalog = &accepted().catalog;
        let status = fixed_status(1, 1);
        let mut assembler = CapabilitySnapshotAssembler::new();
        let first_matrix = available_matrix(false);
        let first = assembler
            .assemble(
                snapshot_input(
                    &first_matrix,
                    &status,
                    "target.native.linux",
                    "security.http.loopback_local",
                    false,
                ),
                observed(1),
            )
            .expect("first snapshot");

        let mut retention_only = status.clone();
        retention_only.receipt_window.expires_before = Some(observed(2));
        let unchanged = assembler
            .assemble(
                snapshot_input(
                    &first_matrix,
                    &retention_only,
                    "target.native.linux",
                    "security.http.loopback_local",
                    false,
                ),
                observed(2),
            )
            .expect("unchanged snapshot");
        assert_eq!(unchanged.generation, 1);
        assert_eq!(unchanged.snapshot_id, first.snapshot_id);

        let mut missing_plugin = available_matrix(false);
        missing_plugin
            .set_gate(
                catalog,
                "capability.plugin.mi_ugens",
                AvailabilityGate::RuntimeProbe,
                MiUgensProbeResult::Missing
                    .runtime_evidence(catalog)
                    .expect("canonical missing-plugin evidence"),
            )
            .expect("mi-UGens gate");
        let changed = assembler
            .assemble(
                snapshot_input(
                    &missing_plugin,
                    &status,
                    "target.native.linux",
                    "security.http.loopback_local",
                    false,
                ),
                observed(3),
            )
            .expect("changed snapshot");
        assert_eq!(changed.generation, 2);
        assert_ne!(changed.snapshot_id, first.snapshot_id);

        let repeated = assembler
            .assemble(
                snapshot_input(
                    &missing_plugin,
                    &status,
                    "target.native.linux",
                    "security.http.loopback_local",
                    false,
                ),
                observed(4),
            )
            .expect("repeated snapshot");
        assert_eq!(repeated.generation, 2);
        assert_eq!(repeated.snapshot_id, changed.snapshot_id);

        let advanced_status = fixed_status(2, 2);
        let advanced = assembler
            .assemble(
                snapshot_input(
                    &missing_plugin,
                    &advanced_status,
                    "target.native.linux",
                    "security.http.loopback_local",
                    false,
                ),
                observed(5),
            )
            .expect("advanced receipt watermark snapshot");
        assert_eq!(advanced.generation, 3);
        assert_eq!(
            advanced.mutation_revision.event_sequence,
            advanced_status.event_sequence
        );
        assert_eq!(
            advanced.mutation_revision.last_confirmed_revision,
            advanced_status.last_confirmed_revision
        );
    }

    #[test]
    fn default_snapshot_shape_cannot_carry_private_local_strings() {
        let status = fixed_status(1, 1);
        let mut assembler = CapabilitySnapshotAssembler::new();
        let snapshot = assembler
            .assemble(
                snapshot_input(
                    &available_matrix(false),
                    &status,
                    "target.native.linux",
                    "security.http.loopback_local",
                    false,
                ),
                observed(1),
            )
            .expect("privacy-minimal snapshot");
        let json = serde_json::to_string(&snapshot).expect("snapshot JSON");
        for forbidden in [
            "device_name",
            "device_path",
            "/home/",
            "credential",
            "authentication_material",
            "environment_value",
            "executed_command",
            "network_origin",
            "project_source",
        ] {
            assert!(
                !json.contains(forbidden),
                "leaked forbidden field {forbidden}"
            );
        }

        let private_constraint =
            GateEvidence::available(provenance_for(AvailabilityGate::RuntimeProbe))
                .with_constraint(
                    "device.identity",
                    CapabilityConstraint::Identifier("/dev/midi1".into()),
                );
        let rejected = evaluate_one(
            "capability.midi.input",
            [
                (
                    AvailabilityGate::Target,
                    available_gate(AvailabilityGate::Target),
                ),
                (
                    AvailabilityGate::BuildFeature,
                    available_gate(AvailabilityGate::BuildFeature),
                ),
                (
                    AvailabilityGate::OperatorPolicy,
                    available_gate(AvailabilityGate::OperatorPolicy),
                ),
                (AvailabilityGate::RuntimeProbe, private_constraint),
            ],
        );
        assert!(matches!(
            rejected,
            Err(CapabilityError::InvalidStableId {
                field: "constraint_value",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn mi_ugens_cache_expires_at_one_second_and_refreshes_explicitly() {
        let mut cache = MiUgensProbeCache::new();
        let calls = Cell::new(0);
        let start = Instant::now();
        let first = cache
            .get_or_probe(1, start, || {
                calls.set(calls.get() + 1);
                std::future::ready(MiUgensProbeResult::Present)
            })
            .await;
        assert_eq!(first, MiUgensProbeResult::Present);
        let cached = cache
            .get_or_probe(
                1,
                start + MI_UGENS_PROBE_CACHE_TTL - Duration::from_nanos(1),
                || {
                    calls.set(calls.get() + 1);
                    std::future::ready(MiUgensProbeResult::Missing)
                },
            )
            .await;
        assert_eq!(cached, MiUgensProbeResult::Present);
        assert_eq!(calls.get(), 1);

        let expired = cache
            .get_or_probe(1, start + MI_UGENS_PROBE_CACHE_TTL, || {
                calls.set(calls.get() + 1);
                std::future::ready(MiUgensProbeResult::Missing)
            })
            .await;
        assert_eq!(expired, MiUgensProbeResult::Missing);
        assert_eq!(calls.get(), 2);

        let refreshed = cache
            .refresh(1, start + Duration::from_millis(1100), || {
                calls.set(calls.get() + 1);
                std::future::ready(MiUgensProbeResult::Present)
            })
            .await;
        assert_eq!(refreshed, MiUgensProbeResult::Present);
        assert_eq!(calls.get(), 3);
    }

    #[tokio::test]
    async fn mi_ugens_cache_invalidates_on_disconnect_reconnect_and_stale_time() {
        let mut cache = MiUgensProbeCache::new();
        let calls = Cell::new(0);
        let start = Instant::now();
        let mut probe = || {
            calls.set(calls.get() + 1);
            std::future::ready(MiUgensProbeResult::Present)
        };
        cache.get_or_probe(1, start, &mut probe).await;
        cache.invalidate();
        cache
            .get_or_probe(1, start + Duration::from_millis(1), &mut probe)
            .await;
        cache.on_reconnect(1);
        cache
            .get_or_probe(1, start + Duration::from_millis(2), &mut probe)
            .await;
        cache
            .get_or_probe(2, start + Duration::from_millis(3), &mut probe)
            .await;
        cache.on_disconnect();
        cache
            .get_or_probe(2, start + Duration::from_millis(4), &mut probe)
            .await;
        let stale_time = start
            .checked_sub(Duration::from_millis(1))
            .expect("test instant supports subtraction");
        cache.get_or_probe(2, stale_time, &mut probe).await;
        assert_eq!(calls.get(), 6);
    }

    #[tokio::test]
    async fn mi_ugens_probe_timeout_is_cached_as_unavailable() {
        let catalog = &accepted().catalog;
        let mut cache = MiUgensProbeCache::new();
        let start = Instant::now();
        let timed_out = cache
            .refresh_with_timeout(1, start, Duration::from_millis(1), || {
                std::future::pending::<MiUgensProbeResult>()
            })
            .await;
        assert_eq!(timed_out, MiUgensProbeResult::Failed);

        let evidence = timed_out
            .runtime_evidence(catalog)
            .expect("canonical timed-out evidence");
        assert_eq!(evidence.outcome, GateOutcome::Unavailable);
        assert_eq!(evidence.reason_ids, ["reason.probe_failed"]);
        assert_eq!(
            evidence.provenance[0].evidence_id,
            "probe.plugin.mi_ugens.v1"
        );

        let calls = Cell::new(0);
        let cached = cache
            .get_or_probe(1, start + Duration::from_millis(2), || {
                calls.set(calls.get() + 1);
                std::future::ready(MiUgensProbeResult::Present)
            })
            .await;
        assert_eq!(cached, MiUgensProbeResult::Failed);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn mi_ugens_probe_results_preserve_pending_missing_and_failed_truth() {
        let catalog = &accepted().catalog;
        let pending =
            MiUgensProbeResult::pending_evidence(catalog).expect("canonical pending evidence");
        let missing = MiUgensProbeResult::Missing
            .runtime_evidence(catalog)
            .expect("canonical missing evidence");
        let failed = MiUgensProbeResult::Failed
            .runtime_evidence(catalog)
            .expect("canonical failure evidence");
        assert_eq!(pending.outcome, GateOutcome::Unknown);
        assert_eq!(pending.reason_ids, ["reason.probe_pending"]);
        assert_eq!(missing.outcome, GateOutcome::Unavailable);
        assert_eq!(missing.reason_ids, ["reason.plugin_missing"]);
        assert_eq!(failed.outcome, GateOutcome::Unavailable);
        assert_eq!(failed.reason_ids, ["reason.probe_failed"]);
        assert!(pending
            .provenance
            .iter()
            .chain(&missing.provenance)
            .chain(&failed.provenance)
            .all(|item| item.evidence_id == "probe.plugin.mi_ugens.v1"));
    }
}

fn normalize_capabilities(capabilities: &mut Vec<CapabilityStatus>) -> Result<(), CapabilityError> {
    capabilities.sort_by(|left, right| left.capability_id.cmp(&right.capability_id));
    for pair in capabilities.windows(2) {
        if pair[0].capability_id == pair[1].capability_id {
            return Err(CapabilityError::UnexpectedCapability(
                pair[0].capability_id.clone(),
            ));
        }
    }
    for capability in capabilities {
        validate_stable_id("capability_id", &capability.capability_id)?;
        validate_stable_id("state_id", &capability.state_id)?;
        validate_stable_id("scope_id", &capability.scope_id)?;
        capability.reason_ids.sort();
        capability.reason_ids.dedup();
        capability.provenance.sort();
        capability.provenance.dedup();
    }
    Ok(())
}

fn validate_snapshot_input(input: &CapabilitySnapshotInput) -> Result<(), CapabilityError> {
    validate_stable_id("runtime_id", &input.subject.runtime_id)?;
    validate_stable_id("target_id", &input.subject.target_id)?;
    validate_stable_id("security_mode_id", &input.security.mode_id)?;
    validate_stable_id("origin_policy_id", &input.security.origin_policy_id)?;
    for capability in &input.capabilities {
        for reason_id in &capability.reason_ids {
            validate_stable_id("reason_id", reason_id)?;
        }
        for provenance in &capability.provenance {
            validate_stable_id("evidence_id", &provenance.evidence_id)?;
        }
        for (constraint_id, value) in &capability.constraints {
            validate_stable_id("constraint_id", constraint_id)?;
            if let CapabilityConstraint::Identifier(identifier) = value {
                validate_stable_id("constraint_value", identifier)?;
            }
        }
    }
    Ok(())
}

fn validate_stable_id(field: &'static str, value: &str) -> Result<(), CapabilityError> {
    let valid = !value.is_empty()
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        });
    if valid {
        Ok(())
    } else {
        Err(CapabilityError::InvalidStableId {
            field,
            value: value.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MiUgensProbeResult {
    Present,
    Missing,
    Failed,
}

impl MiUgensProbeResult {
    pub fn runtime_evidence(
        self,
        catalog: &CapabilityCatalog,
    ) -> Result<GateEvidence, CapabilityError> {
        let provenance = CapabilityProvenance::new(ProvenanceKind::RuntimeProbe, MI_UGENS_PROBE_ID);
        match self {
            Self::Present => Ok(GateEvidence::available(provenance)),
            Self::Missing => Ok(GateEvidence::unavailable(
                catalog
                    .canonical_reason(PLUGIN_MISSING_REASON_ID, AvailabilityGate::RuntimeProbe)?,
                provenance,
            )),
            Self::Failed => Ok(GateEvidence::unavailable(
                catalog.canonical_reason(PROBE_FAILED_REASON_ID, AvailabilityGate::RuntimeProbe)?,
                provenance,
            )),
        }
    }

    pub fn pending_evidence(catalog: &CapabilityCatalog) -> Result<GateEvidence, CapabilityError> {
        Ok(GateEvidence::unknown(
            catalog.canonical_reason(PROBE_PENDING_REASON_ID, AvailabilityGate::RuntimeProbe)?,
            CapabilityProvenance::new(ProvenanceKind::RuntimeProbe, MI_UGENS_PROBE_ID),
        ))
    }
}

#[derive(Clone, Copy, Debug)]
struct CachedMiUgensProbe {
    connection_generation: u64,
    observed_at: Instant,
    result: MiUgensProbeResult,
}

#[derive(Default)]
pub struct MiUgensProbeCache {
    cached: Option<CachedMiUgensProbe>,
    connection_generation: Option<u64>,
}

impl MiUgensProbeCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_or_probe<F, Fut>(
        &mut self,
        connection_generation: u64,
        now: Instant,
        probe: F,
    ) -> MiUgensProbeResult
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = MiUgensProbeResult>,
    {
        self.ensure_connection(connection_generation);
        if let Some(cached) = self.cached {
            if cached.connection_generation == connection_generation
                && now
                    .checked_duration_since(cached.observed_at)
                    .is_some_and(|age| age < MI_UGENS_PROBE_CACHE_TTL)
            {
                return cached.result;
            }
        }
        self.refresh(connection_generation, now, probe).await
    }

    pub async fn refresh<F, Fut>(
        &mut self,
        connection_generation: u64,
        now: Instant,
        probe: F,
    ) -> MiUgensProbeResult
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = MiUgensProbeResult>,
    {
        self.refresh_with_timeout(connection_generation, now, MI_UGENS_PROBE_TIMEOUT, probe)
            .await
    }

    async fn refresh_with_timeout<F, Fut>(
        &mut self,
        connection_generation: u64,
        now: Instant,
        probe_timeout: Duration,
        probe: F,
    ) -> MiUgensProbeResult
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = MiUgensProbeResult>,
    {
        self.ensure_connection(connection_generation);
        let result = timeout(probe_timeout, probe())
            .await
            .unwrap_or(MiUgensProbeResult::Failed);
        self.cached = Some(CachedMiUgensProbe {
            connection_generation,
            observed_at: now,
            result,
        });
        result
    }

    pub fn invalidate(&mut self) {
        self.cached = None;
    }

    pub fn on_disconnect(&mut self) {
        self.cached = None;
        self.connection_generation = None;
    }

    pub fn on_reconnect(&mut self, connection_generation: u64) {
        self.connection_generation = Some(connection_generation);
        self.cached = None;
    }

    fn ensure_connection(&mut self, connection_generation: u64) {
        if self.connection_generation != Some(connection_generation) {
            self.connection_generation = Some(connection_generation);
            self.cached = None;
        }
    }
}
