//! Native inactive-generation planning, activation, and restoration.
//!
//! Planning is side-effect free and captures one confirmed receipt revision,
//! one capability snapshot, one allocation snapshot, and one resource stage.
//! Execution keeps the previously confirmed graph authoritative until a
//! correlated activation acknowledgment and the local commit boundary both
//! succeed.

use crate::backend::AddAction;
use crate::candidate::{Candidate, DeclarationPayload};
use crate::capabilities::CapabilitySnapshot;
use crate::compat::Instant;
use crate::mutation::{
    Applied, BeatTicks, CandidateOrigin, ComponentOutcome, ComponentState, Confirmation,
    Diagnostic, DiagnosticSeverity, EffectiveAt, FailurePhase, FiniteSeconds, Partial,
    PublicDigest, Rejected, RevisionId, RollbackState, RuntimeEpoch, Timestamp,
};
use crate::resource_manager::{
    LogicalResource, PhysicalFreeConfirmation, PhysicalResourceId, QuiescenceProof, ResourceError,
    ResourceGeneration, ResourceManager, ResourceRetirement, ResourceStageSnapshot,
};
use crate::types::{NodeId, ParamMap};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;

const ATOMIC_GENERATION_CAPABILITY: &str = "capability.receipt.atomic_generation_activation";
const AVAILABLE_STATE: &str = "availability.available";

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GraphGeneration(u64);

impl GraphGeneration {
    pub fn new(value: u64) -> Result<Self, GenerationError> {
        if value == 0 {
            return Err(GenerationError::InvalidPlan(
                "graph generation must be greater than zero".into(),
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationAllocation {
    pub generation: GraphGeneration,
    pub root: NodeId,
    pub parent: NodeId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativeStageOperation {
    LoadSynthDef {
        name: String,
        bytes: Arc<[u8]>,
    },
    CreateGroup {
        node: NodeId,
        target: NodeId,
        action: AddAction,
    },
    CreateSynth {
        definition: String,
        node: NodeId,
        target: NodeId,
        action: AddAction,
        params: ParamMap,
    },
    SetParams {
        node: NodeId,
        params: ParamMap,
    },
    MapRoute {
        node: NodeId,
        parameter: String,
        bus: u32,
    },
    CreateEffect {
        definition: String,
        node: NodeId,
        target: NodeId,
        action: AddAction,
        params: ParamMap,
    },
    Resource {
        logical: LogicalResource,
        generation: ResourceGeneration,
    },
    RemoveResource {
        logical: LogicalResource,
        generation: ResourceGeneration,
    },
}

impl NativeStageOperation {
    const fn phase(&self) -> StagePhase {
        match self {
            Self::LoadSynthDef { .. }
            | Self::CreateGroup { .. }
            | Self::CreateSynth { .. }
            | Self::Resource { .. }
            | Self::RemoveResource { .. } => StagePhase::Create,
            Self::SetParams { .. } => StagePhase::Update,
            Self::MapRoute { .. } => StagePhase::Route,
            Self::CreateEffect { .. } => StagePhase::Effect,
        }
    }

    fn action_name(&self) -> &'static str {
        match self {
            Self::LoadSynthDef { .. } => "load_synthdef",
            Self::CreateGroup { .. } => "create_group",
            Self::CreateSynth { .. } => "create_synth",
            Self::SetParams { .. } => "update_params",
            Self::MapRoute { .. } => "map_route",
            Self::CreateEffect { .. } => "create_effect",
            Self::Resource { .. } => "bind_resource_generation",
            Self::RemoveResource { .. } => "remove_resource_generation",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StagePhase {
    Create,
    Update,
    Route,
    Effect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePlanComponent {
    /// Candidate declaration that owns this operation. One declaration may
    /// expand into multiple uniquely addressed native operations.
    pub declaration: String,
    pub path: String,
    pub operation: NativeStageOperation,
}

impl NativePlanComponent {
    #[must_use]
    pub const fn phase(&self) -> StagePhase {
        self.operation.phase()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivationBoundary {
    pub requested_beat: Option<BeatTicks>,
    pub requested_backend_seconds: f64,
    pub deadline: Option<Instant>,
    pub audible_tail_beats: Option<BeatTicks>,
    pub audible_tail_seconds: Option<f64>,
    pub observed_at: SystemTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtomicProbeObservation {
    backend: String,
    runtime_instance: String,
    inactive_stage_token: String,
    activation_token: String,
    restoration_token: String,
    cleanup_token: String,
    inactive_graph_was_silent: bool,
    activation_was_one_bundle_or_link: bool,
    restoration_was_confirmed: bool,
    exact_free_was_confirmed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtomicGenerationEvidence {
    snapshot_id: PublicDigest,
    confirmed_revision: Option<RevisionId>,
    backend: String,
    runtime_instance: String,
    tokens: [String; 4],
}

impl AtomicGenerationEvidence {
    pub fn confirm(
        snapshot: &CapabilitySnapshot,
        confirmed_revision: Option<RevisionId>,
        observation: AtomicProbeObservation,
    ) -> Result<Self, GenerationError> {
        snapshot
            .verify_snapshot_id()
            .map_err(|error| GenerationError::Capability(error.to_string()))?;
        if snapshot.mutation_revision().last_confirmed_revision() != confirmed_revision {
            return Err(GenerationError::RevisionSnapshotMismatch);
        }
        let atomic_available = snapshot.capabilities().iter().any(|capability| {
            capability.capability_id() == ATOMIC_GENERATION_CAPABILITY
                && capability.state_id() == AVAILABLE_STATE
        });
        if !atomic_available {
            return Err(GenerationError::AtomicCapabilityUnavailable);
        }
        if observation.backend.is_empty()
            || observation.runtime_instance.is_empty()
            || observation.runtime_instance != snapshot.subject().runtime_id()
            || !observation.inactive_graph_was_silent
            || !observation.activation_was_one_bundle_or_link
            || !observation.restoration_was_confirmed
            || !observation.exact_free_was_confirmed
        {
            return Err(GenerationError::AtomicProbeIncomplete);
        }
        let tokens = [
            observation.inactive_stage_token,
            observation.activation_token,
            observation.restoration_token,
            observation.cleanup_token,
        ];
        if tokens.iter().any(String::is_empty)
            || tokens.iter().collect::<BTreeSet<_>>().len() != tokens.len()
        {
            return Err(GenerationError::AtomicProbeIncomplete);
        }
        Ok(Self {
            snapshot_id: snapshot.snapshot_id().clone(),
            confirmed_revision,
            backend: observation.backend,
            runtime_instance: observation.runtime_instance,
            tokens,
        })
    }

    fn matches(&self, revision: &PlanningRevision) -> bool {
        self.snapshot_id == revision.snapshot_id
            && self.confirmed_revision == revision.confirmed_revision
            && !self.backend.is_empty()
            && !self.runtime_instance.is_empty()
            && self.tokens.iter().all(|token| !token.is_empty())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomicAdmission {
    BestEffort,
    Required,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlanningRevision {
    snapshot_id: PublicDigest,
    snapshot_generation: u64,
    runtime_epoch: RuntimeEpoch,
    confirmed_revision: Option<RevisionId>,
    atomic_capability_available: bool,
}

impl PlanningRevision {
    fn capture(
        snapshot: &CapabilitySnapshot,
        confirmed_revision: Option<RevisionId>,
    ) -> Result<Self, GenerationError> {
        snapshot
            .verify_snapshot_id()
            .map_err(|error| GenerationError::Capability(error.to_string()))?;
        let watermark = snapshot.mutation_revision();
        if watermark.last_confirmed_revision() != confirmed_revision {
            return Err(GenerationError::RevisionSnapshotMismatch);
        }
        Ok(Self {
            snapshot_id: snapshot.snapshot_id().clone(),
            snapshot_generation: snapshot.generation(),
            runtime_epoch: watermark.runtime_epoch(),
            confirmed_revision,
            atomic_capability_available: snapshot.capabilities().iter().any(|capability| {
                capability.capability_id() == ATOMIC_GENERATION_CAPABILITY
                    && capability.state_id() == AVAILABLE_STATE
            }),
        })
    }
}

pub struct NativePlanRequest<'a> {
    pub candidate: &'a Candidate,
    pub target_revision: RevisionId,
    pub confirmed_revision: Option<RevisionId>,
    pub capability_snapshot: &'a CapabilitySnapshot,
    pub atomicity: AtomicAdmission,
    pub atomic_evidence: Option<&'a AtomicGenerationEvidence>,
    pub allocation: GenerationAllocation,
    pub resource_stage: ResourceStageSnapshot,
    pub components: Vec<NativePlanComponent>,
    pub boundary: ActivationBoundary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanDigest(String);

impl PlanDigest {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeGenerationPlan {
    target_revision: RevisionId,
    revision: PlanningRevision,
    atomicity: AtomicAdmission,
    allocation: GenerationAllocation,
    resource_stage: ResourceStageSnapshot,
    components: Vec<NativePlanComponent>,
    boundary: QuantizedBoundary,
    digest: PlanDigest,
}

impl NativeGenerationPlan {
    #[must_use]
    pub const fn target_revision(&self) -> RevisionId {
        self.target_revision
    }

    #[must_use]
    pub fn digest(&self) -> &PlanDigest {
        &self.digest
    }

    #[must_use]
    pub fn components(&self) -> &[NativePlanComponent] {
        &self.components
    }

    #[must_use]
    pub const fn allocation(&self) -> &GenerationAllocation {
        &self.allocation
    }
}

#[derive(Clone, Debug, PartialEq)]
struct QuantizedBoundary {
    effective: EffectiveAt,
    deadline: Option<Instant>,
    audible_tail_until: Option<EffectiveAt>,
}

#[derive(Clone, Debug, Default)]
pub struct NativeGenerationPlanner;

impl NativeGenerationPlanner {
    pub fn plan(
        &self,
        request: NativePlanRequest<'_>,
        sample_rate: f64,
        block_size: u32,
    ) -> Result<NativeGenerationPlan, GenerationError> {
        let revision =
            PlanningRevision::capture(request.capability_snapshot, request.confirmed_revision)?;
        if request.candidate.identity().runtime_epoch() != revision.runtime_epoch {
            return Err(GenerationError::CandidateEpochMismatch);
        }
        self.plan_captured(
            request.candidate,
            request.target_revision,
            revision,
            request.atomicity,
            request.atomic_evidence,
            request.allocation,
            request.resource_stage,
            request.components,
            request.boundary,
            sample_rate,
            block_size,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_captured(
        &self,
        candidate: &Candidate,
        target_revision: RevisionId,
        revision: PlanningRevision,
        atomicity: AtomicAdmission,
        atomic_evidence: Option<&AtomicGenerationEvidence>,
        allocation: GenerationAllocation,
        resource_stage: ResourceStageSnapshot,
        mut components: Vec<NativePlanComponent>,
        boundary: ActivationBoundary,
        sample_rate: f64,
        block_size: u32,
    ) -> Result<NativeGenerationPlan, GenerationError> {
        if revision
            .confirmed_revision
            .is_some_and(|confirmed| target_revision <= confirmed)
        {
            return Err(GenerationError::InvalidPlan(
                "target revision must be newer than the confirmed revision".into(),
            ));
        }
        if atomicity == AtomicAdmission::Required
            && (!revision.atomic_capability_available
                || atomic_evidence.is_none_or(|evidence| !evidence.matches(&revision)))
        {
            return Err(GenerationError::AtomicCapabilityUnavailable);
        }
        let expected = candidate
            .declarations()
            .iter()
            .map(|declaration| declaration.address().to_string())
            .collect::<BTreeSet<_>>();
        let actual = components
            .iter()
            .map(|component| component.declaration.clone())
            .collect::<BTreeSet<_>>();
        let paths = components
            .iter()
            .map(|component| component.path.clone())
            .collect::<BTreeSet<_>>();
        if components
            .iter()
            .any(|component| component.path.is_empty() || component.declaration.is_empty())
        {
            return Err(GenerationError::InvalidPlan(
                "native component paths and declaration owners must be non-empty".into(),
            ));
        }
        if paths.len() != components.len() {
            return Err(GenerationError::InvalidPlan(
                "native staging operation paths must be unique".into(),
            ));
        }
        if expected != actual {
            return Err(GenerationError::ComponentSetMismatch { expected, actual });
        }
        validate_resource_operations(&resource_stage, &components)?;
        let group_depths = validate_inactive_operations(&allocation, &components)?;
        components.sort_by(|left, right| {
            left.phase()
                .cmp(&right.phase())
                .then_with(|| {
                    operation_rank(&left.operation).cmp(&operation_rank(&right.operation))
                })
                .then_with(|| {
                    group_depth(&left.operation, &group_depths)
                        .cmp(&group_depth(&right.operation, &group_depths))
                })
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| {
                    left.operation
                        .action_name()
                        .cmp(right.operation.action_name())
                })
        });
        let boundary = quantize_boundary(boundary, sample_rate, block_size)?;
        let digest = plan_digest(
            candidate,
            target_revision,
            &revision,
            atomicity,
            &allocation,
            &resource_stage,
            &components,
            &boundary,
        );
        Ok(NativeGenerationPlan {
            target_revision,
            revision,
            atomicity,
            allocation,
            resource_stage,
            components,
            boundary,
            digest,
        })
    }
}

fn validate_resource_operations(
    snapshot: &ResourceStageSnapshot,
    components: &[NativePlanComponent],
) -> Result<(), GenerationError> {
    let expected_claims = snapshot.claims().collect::<BTreeSet<_>>();
    let expected_removals = snapshot.removals().collect::<BTreeSet<_>>();
    let actual_claims = components
        .iter()
        .filter_map(|component| match &component.operation {
            NativeStageOperation::Resource {
                logical,
                generation,
            } => Some((logical, *generation)),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let actual_removals = components
        .iter()
        .filter_map(|component| match &component.operation {
            NativeStageOperation::RemoveResource {
                logical,
                generation,
            } => Some((logical, *generation)),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let resource_operation_count = components
        .iter()
        .filter(|component| {
            matches!(
                &component.operation,
                NativeStageOperation::Resource { .. } | NativeStageOperation::RemoveResource { .. }
            )
        })
        .count();
    if actual_claims != expected_claims
        || actual_removals != expected_removals
        || resource_operation_count != actual_claims.len() + actual_removals.len()
    {
        return Err(GenerationError::ResourceComponentSetMismatch);
    }
    Ok(())
}

fn validate_inactive_operations(
    allocation: &GenerationAllocation,
    components: &[NativePlanComponent],
) -> Result<BTreeMap<u32, usize>, GenerationError> {
    if allocation.root == allocation.parent {
        return Err(GenerationError::InvalidPlan(
            "the inactive generation root must differ from its parent".into(),
        ));
    }

    let root = allocation.root.raw();
    let parent = allocation.parent.raw();
    if root == 0 {
        return Err(GenerationError::InvalidPlan(
            "the inactive generation root cannot alias the backend root".into(),
        ));
    }
    let mut created_nodes = BTreeSet::new();
    let mut created_before_updates = BTreeSet::new();
    let mut groups = BTreeMap::new();
    let mut loaded_definitions = BTreeSet::new();
    let mut used_definitions = Vec::new();
    for component in components {
        match &component.operation {
            NativeStageOperation::LoadSynthDef { name, bytes } => {
                let suffix = format!("__g{}", allocation.generation.get());
                if name.is_empty() || bytes.is_empty() || !name.ends_with(&suffix) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "staged synthdef {name} is not qualified for graph generation {}",
                        allocation.generation.get()
                    )));
                }
                if !loaded_definitions.insert(name.clone()) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "synthdef {name} is staged more than once"
                    )));
                }
            }
            NativeStageOperation::CreateGroup { node, target, .. } => {
                let node = node.raw();
                if node == root || node == parent || !created_nodes.insert(node) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "native node {node} is duplicated or aliases the generation root/parent"
                    )));
                }
                created_before_updates.insert(node);
                groups.insert(node, target.raw());
            }
            NativeStageOperation::CreateSynth {
                definition,
                node,
                params,
                ..
            } => {
                let node = node.raw();
                if node == root || node == parent || !created_nodes.insert(node) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "native node {node} is duplicated or aliases the generation root/parent"
                    )));
                }
                validate_params(params)?;
                used_definitions.push(definition.clone());
                created_before_updates.insert(node);
            }
            NativeStageOperation::CreateEffect {
                definition,
                node,
                params,
                ..
            } => {
                let node = node.raw();
                if node == root || node == parent || !created_nodes.insert(node) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "native node {node} is duplicated or aliases the generation root/parent"
                    )));
                }
                validate_params(params)?;
                used_definitions.push(definition.clone());
            }
            NativeStageOperation::SetParams { params, .. } => validate_params(params)?,
            NativeStageOperation::MapRoute { parameter, .. } if parameter.is_empty() => {
                return Err(GenerationError::InvalidPlan(
                    "native route parameter must be non-empty".into(),
                ));
            }
            NativeStageOperation::MapRoute { .. }
            | NativeStageOperation::Resource { .. }
            | NativeStageOperation::RemoveResource { .. } => {}
        }
    }

    let definition_suffix = format!("__g{}", allocation.generation.get());
    for definition in used_definitions {
        if !definition.ends_with(&definition_suffix) || !loaded_definitions.contains(&definition) {
            return Err(GenerationError::InvalidPlan(format!(
                "native synth/effect definition {definition} is not staged in this graph generation"
            )));
        }
    }

    let mut depths = BTreeMap::from([(root, 0usize)]);
    while depths.len() <= groups.len() {
        let mut progressed = false;
        for (node, target) in &groups {
            if depths.contains_key(node) {
                continue;
            }
            if let Some(parent_depth) = depths.get(target).copied() {
                depths.insert(*node, parent_depth + 1);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    if depths.len() != groups.len() + 1 {
        return Err(GenerationError::InvalidPlan(
            "every staged group must descend from the inactive generation root".into(),
        ));
    }

    for component in components {
        match &component.operation {
            NativeStageOperation::CreateSynth { target, .. }
            | NativeStageOperation::CreateEffect { target, .. } => {
                if !depths.contains_key(&target.raw()) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "native node target {} is outside the inactive generation root",
                        target.raw()
                    )));
                }
            }
            NativeStageOperation::SetParams { node, .. }
            | NativeStageOperation::MapRoute { node, .. } => {
                if !created_before_updates.contains(&node.raw()) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "native update target {} was not created in this inactive generation",
                        node.raw()
                    )));
                }
            }
            _ => {}
        }
    }
    depths.remove(&root);
    Ok(depths)
}

fn validate_params(params: &ParamMap) -> Result<(), GenerationError> {
    if params
        .iter()
        .any(|(name, value)| name.is_empty() || !value.is_finite())
    {
        return Err(GenerationError::InvalidPlan(
            "native parameter names must be non-empty and values must be finite".into(),
        ));
    }
    Ok(())
}

const fn operation_rank(operation: &NativeStageOperation) -> u8 {
    match operation {
        NativeStageOperation::LoadSynthDef { .. } => 0,
        NativeStageOperation::CreateGroup { .. } => 1,
        NativeStageOperation::CreateSynth { .. } => 2,
        NativeStageOperation::Resource { .. } | NativeStageOperation::RemoveResource { .. } => 3,
        NativeStageOperation::SetParams { .. } => 0,
        NativeStageOperation::MapRoute { .. } => 0,
        NativeStageOperation::CreateEffect { .. } => 0,
    }
}

fn group_depth(operation: &NativeStageOperation, depths: &BTreeMap<u32, usize>) -> usize {
    match operation {
        NativeStageOperation::CreateGroup { node, .. } => {
            depths.get(&node.raw()).copied().unwrap_or(usize::MAX)
        }
        _ => 0,
    }
}

fn quantize_boundary(
    requested: ActivationBoundary,
    sample_rate: f64,
    block_size: u32,
) -> Result<QuantizedBoundary, GenerationError> {
    if !sample_rate.is_finite()
        || sample_rate <= 0.0
        || block_size == 0
        || !requested.requested_backend_seconds.is_finite()
        || requested.requested_backend_seconds < 0.0
    {
        return Err(GenerationError::InvalidBoundary);
    }
    let block_seconds = f64::from(block_size) / sample_rate;
    let quantized_seconds =
        (requested.requested_backend_seconds / block_seconds).ceil() * block_seconds;
    let delay = quantized_seconds - requested.requested_backend_seconds;
    if !quantized_seconds.is_finite() || !delay.is_finite() || delay < 0.0 {
        return Err(GenerationError::InvalidBoundary);
    }
    let deadline_delay =
        Duration::try_from_secs_f64(delay).map_err(|_| GenerationError::InvalidBoundary)?;
    let deadline = requested
        .deadline
        .map(|deadline| {
            deadline
                .checked_add(deadline_delay)
                .ok_or(GenerationError::InvalidBoundary)
        })
        .transpose()?;
    let effective = EffectiveAt {
        observed_at: Timestamp::from_system_time(requested.observed_at),
        musical_beat: requested.requested_beat,
        backend_time_seconds: Some(
            FiniteSeconds::new(quantized_seconds).map_err(GenerationError::InvalidPlan)?,
        ),
    };
    let tail_seconds = requested.audible_tail_seconds.unwrap_or(0.0);
    if !tail_seconds.is_finite() || tail_seconds < 0.0 {
        return Err(GenerationError::InvalidBoundary);
    }
    let tail_beat = match (requested.requested_beat, requested.audible_tail_beats) {
        (Some(beat), Some(tail)) => Some(BeatTicks::new(
            beat.get()
                .checked_add(tail.get())
                .ok_or(GenerationError::InvalidBoundary)?,
        )),
        _ => None,
    };
    let audible_tail_until = if tail_seconds > 0.0 || requested.audible_tail_beats.is_some() {
        let tail_backend_seconds = quantized_seconds + tail_seconds;
        let tail_backend_seconds = FiniteSeconds::new(tail_backend_seconds)
            .map_err(|_| GenerationError::InvalidBoundary)?;
        Some(EffectiveAt {
            observed_at: Timestamp::from_system_time(requested.observed_at),
            musical_beat: tail_beat,
            backend_time_seconds: Some(tail_backend_seconds),
        })
    } else {
        None
    };
    Ok(QuantizedBoundary {
        effective,
        deadline,
        audible_tail_until,
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_digest(
    candidate: &Candidate,
    target_revision: RevisionId,
    revision: &PlanningRevision,
    atomicity: AtomicAdmission,
    allocation: &GenerationAllocation,
    resource_stage: &ResourceStageSnapshot,
    components: &[NativePlanComponent],
    boundary: &QuantizedBoundary,
) -> PlanDigest {
    fn field_bytes(hasher: &mut Sha256, value: &[u8]) {
        hasher.update(
            u64::try_from(value.len())
                .expect("native plan field length fits u64")
                .to_be_bytes(),
        );
        hasher.update(value);
    }
    fn text(hasher: &mut Sha256, value: &str) {
        field_bytes(hasher, value.as_bytes());
    }

    let mut hasher = Sha256::new();
    hasher.update(b"vibelang.native-generation-plan.v1\0");
    hasher.update(target_revision.get().to_be_bytes());
    text(&mut hasher, revision.snapshot_id.as_str());
    hasher.update(revision.snapshot_generation.to_be_bytes());
    text(&mut hasher, &revision.runtime_epoch.to_string());
    hasher.update(
        revision
            .confirmed_revision
            .map_or(0, RevisionId::get)
            .to_be_bytes(),
    );
    hasher.update([match atomicity {
        AtomicAdmission::BestEffort => 0,
        AtomicAdmission::Required => 1,
    }]);
    hasher.update(allocation.generation.get().to_be_bytes());
    hasher.update(allocation.root.raw().to_be_bytes());
    hasher.update(allocation.parent.raw().to_be_bytes());
    hasher.update(resource_stage.stage().get().to_be_bytes());
    text(&mut hasher, resource_stage.digest());
    let identity = candidate.identity();
    hasher.update([match candidate.origin() {
        CandidateOrigin::ScriptFile => 0,
        CandidateOrigin::WatchReload => 1,
        CandidateOrigin::HttpEval => 2,
        CandidateOrigin::RhaiHost => 3,
        CandidateOrigin::WasmRuntime => 4,
        CandidateOrigin::WasmCompiler => 5,
    }]);
    hasher.update(identity.language().language_major().to_be_bytes());
    hasher.update(identity.language().manifest_schema_version().to_be_bytes());
    text(&mut hasher, identity.language().manifest_digest().as_str());
    text(&mut hasher, &identity.engine_instance().to_string());
    text(&mut hasher, &identity.runtime_epoch().to_string());
    for declaration in candidate.declarations() {
        text(&mut hasher, &declaration.address().to_string());
        hasher.update([declaration.address().kind() as u8]);
        match declaration.payload() {
            DeclarationPayload::Empty => hasher.update([0]),
            DeclarationPayload::Opaque {
                type_id,
                canonical_bytes,
            } => {
                hasher.update([1]);
                text(&mut hasher, type_id);
                field_bytes(&mut hasher, canonical_bytes);
            }
        }
    }
    for component in components {
        text(&mut hasher, &component.declaration);
        text(&mut hasher, &component.path);
        hash_operation(&mut hasher, &component.operation);
    }
    text(&mut hasher, boundary.effective.observed_at.as_str());
    if let Some(beat) = boundary.effective.musical_beat {
        hasher.update([1]);
        hasher.update(beat.get().to_be_bytes());
    } else {
        hasher.update([0]);
    }
    if let Some(seconds) = boundary.effective.backend_time_seconds {
        hasher.update([1]);
        hasher.update(seconds.get().to_bits().to_be_bytes());
    } else {
        hasher.update([0]);
    }
    if let Some(tail) = &boundary.audible_tail_until {
        hasher.update([1]);
        text(&mut hasher, tail.observed_at.as_str());
        if let Some(beat) = tail.musical_beat {
            hasher.update([1]);
            hasher.update(beat.get().to_be_bytes());
        } else {
            hasher.update([0]);
        }
        if let Some(seconds) = tail.backend_time_seconds {
            hasher.update([1]);
            hasher.update(seconds.get().to_bits().to_be_bytes());
        } else {
            hasher.update([0]);
        }
    } else {
        hasher.update([0]);
    }
    PlanDigest(format!("sha256:{:x}", hasher.finalize()))
}

fn hash_params(hasher: &mut Sha256, params: &ParamMap) {
    let mut entries = params.iter().collect::<Vec<_>>();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (name, value) in entries {
        hasher.update(
            u64::try_from(name.len())
                .expect("native parameter name length fits u64")
                .to_be_bytes(),
        );
        hasher.update(name.as_bytes());
        hasher.update(value.to_bits().to_be_bytes());
    }
}

fn hash_operation(hasher: &mut Sha256, operation: &NativeStageOperation) {
    fn field_bytes(hasher: &mut Sha256, value: &[u8]) {
        hasher.update(
            u64::try_from(value.len())
                .expect("native operation field length fits u64")
                .to_be_bytes(),
        );
        hasher.update(value);
    }
    fn text(hasher: &mut Sha256, value: &str) {
        field_bytes(hasher, value.as_bytes());
    }

    let action = operation.action_name();
    text(hasher, action);
    match operation {
        NativeStageOperation::LoadSynthDef { name, bytes } => {
            text(hasher, name);
            field_bytes(hasher, bytes);
        }
        NativeStageOperation::CreateGroup {
            node,
            target,
            action,
        } => {
            hasher.update(node.raw().to_be_bytes());
            hasher.update(target.raw().to_be_bytes());
            hasher.update([*action as u8]);
        }
        NativeStageOperation::CreateSynth {
            definition,
            node,
            target,
            action,
            params,
        }
        | NativeStageOperation::CreateEffect {
            definition,
            node,
            target,
            action,
            params,
        } => {
            text(hasher, definition);
            hasher.update(node.raw().to_be_bytes());
            hasher.update(target.raw().to_be_bytes());
            hasher.update([*action as u8]);
            hash_params(hasher, params);
        }
        NativeStageOperation::SetParams { node, params } => {
            hasher.update(node.raw().to_be_bytes());
            hash_params(hasher, params);
        }
        NativeStageOperation::MapRoute {
            node,
            parameter,
            bus,
        } => {
            hasher.update(node.raw().to_be_bytes());
            text(hasher, parameter);
            hasher.update(bus.to_be_bytes());
        }
        NativeStageOperation::Resource {
            logical,
            generation,
        }
        | NativeStageOperation::RemoveResource {
            logical,
            generation,
        } => {
            text(hasher, logical.address());
            hasher.update([logical.kind() as u8]);
            hasher.update(generation.get().to_be_bytes());
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BackendCorrelation {
    pub backend: String,
    pub token: String,
}

#[derive(Debug, Default)]
struct CorrelationLedger {
    reserved: HashSet<BackendCorrelation>,
}

impl CorrelationLedger {
    fn reserve<D: NativeGenerationDriver>(
        &mut self,
        driver: &D,
    ) -> Result<BackendCorrelation, String> {
        let correlation = driver
            .reserve_correlation()
            .map_err(|error| error.to_string())?;
        if correlation.backend.is_empty() || correlation.token.is_empty() {
            return Err("the backend reserved an empty generation correlation".into());
        }
        if !self.reserved.insert(correlation.clone()) {
            return Err(
                "the backend reused a generation correlation within one transaction".into(),
            );
        }
        Ok(correlation)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivationSwitch {
    pub previous_root: Option<NodeId>,
    pub next_root: NodeId,
    pub deadline: Option<Instant>,
}

#[async_trait]
pub trait NativeGenerationDriver: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// True only when a matching component acknowledgement proves semantic
    /// success, rather than merely proving ordering through a later barrier.
    fn has_exact_component_acknowledgements(&self) -> bool {
        false
    }

    fn reserve_correlation(&self) -> Result<BackendCorrelation, Self::Error>;

    async fn stage_inactive_root(
        &self,
        allocation: &GenerationAllocation,
        expected: &BackendCorrelation,
    ) -> Result<BackendCorrelation, Self::Error>;

    async fn stage_component(
        &self,
        root: NodeId,
        component: &NativePlanComponent,
        expected: &BackendCorrelation,
    ) -> Result<BackendCorrelation, Self::Error>;

    async fn barrier(
        &self,
        expected: &BackendCorrelation,
    ) -> Result<BackendCorrelation, Self::Error>;

    async fn activate(
        &self,
        activation: &ActivationSwitch,
        expected: &BackendCorrelation,
    ) -> Result<BackendCorrelation, Self::Error>;

    async fn precommit(&self, plan: &NativeGenerationPlan) -> Result<(), Self::Error>;

    async fn restore(
        &self,
        activation: &ActivationSwitch,
        expected: &BackendCorrelation,
    ) -> Result<BackendCorrelation, Self::Error>;

    async fn cleanup_generation(
        &self,
        root: NodeId,
        expected: &BackendCorrelation,
    ) -> Result<BackendCorrelation, Self::Error>;

    async fn free_resource(
        &self,
        physical: PhysicalResourceId,
        expected: &BackendCorrelation,
    ) -> Result<BackendCorrelation, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedGeneration {
    pub generation: GraphGeneration,
    pub root: NodeId,
    pub revision: RevisionId,
    pub plan_digest: PlanDigest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CleanupHealth {
    Clean,
    Retained,
    Degraded(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenerationOutcome {
    Applied {
        receipt: Applied,
        generation: AppliedGeneration,
        cleanup: CleanupHealth,
    },
    Rejected {
        receipt: Rejected,
        cleanup: CleanupHealth,
    },
    Partial {
        receipt: Partial,
        cleanup: CleanupHealth,
    },
}

#[derive(Debug, Default)]
pub struct NativeGenerationCoordinator {
    active: Option<AppliedGeneration>,
    fenced: bool,
    quarantined_roots: HashSet<NodeId>,
}

impl NativeGenerationCoordinator {
    #[must_use]
    pub fn new(active: Option<AppliedGeneration>) -> Self {
        Self {
            active,
            fenced: false,
            quarantined_roots: HashSet::new(),
        }
    }

    #[must_use]
    pub fn active(&self) -> Option<&AppliedGeneration> {
        self.active.as_ref()
    }

    #[must_use]
    pub const fn is_fenced(&self) -> bool {
        self.fenced
    }

    pub fn quarantined_roots(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.quarantined_roots.iter().copied()
    }

    /// Revisit generations retained only because readers were still active at
    /// the original post-commit cleanup boundary.
    pub async fn reap_retired_resources<D: NativeGenerationDriver>(
        &mut self,
        resources: &ResourceManager,
        driver: &D,
    ) -> CleanupHealth {
        let retirement = resources.freeable_retirement();
        if retirement.generations().len() == 0 {
            return CleanupHealth::Clean;
        }
        let mut correlations = CorrelationLedger::default();
        let mut issues = Vec::new();
        let quiescence = match correlations.reserve(driver) {
            Ok(expected) => match driver.barrier(&expected).await {
                Ok(actual) if actual == expected => Some(actual),
                Ok(_) => {
                    issues.push("resource reap barrier correlation mismatch".into());
                    None
                }
                Err(error) => {
                    issues.push(format!("resource reap barrier failed: {error}"));
                    None
                }
            },
            Err(error) => {
                issues.push(format!("resource reap correlation unavailable: {error}"));
                None
            }
        };
        cleanup_resources(
            resources,
            &retirement,
            driver,
            &mut correlations,
            quiescence.as_ref(),
            &mut issues,
        )
        .await;
        if issues.is_empty() {
            CleanupHealth::Clean
        } else {
            CleanupHealth::Degraded(issues)
        }
    }

    pub async fn execute<D: NativeGenerationDriver>(
        &mut self,
        plan: NativeGenerationPlan,
        resources: &ResourceManager,
        driver: &D,
    ) -> GenerationOutcome {
        let mut correlations = CorrelationLedger::default();
        if self.fenced {
            let cleanup =
                cleanup_unactivated_resources(&plan, resources, driver, &mut correlations).await;
            return self.finish_pre_activation_failure(
                &plan,
                FailurePhase::Admission,
                "runtime_fenced",
                "generation admission is fenced after an uncertain restoration",
                self.active.as_ref().map(|active| active.revision),
                0,
                cleanup,
            );
        }
        let active_revision = self.active.as_ref().map(|active| active.revision);
        if !driver.has_exact_component_acknowledgements() {
            let cleanup =
                cleanup_unactivated_resources(&plan, resources, driver, &mut correlations).await;
            return self.finish_pre_activation_failure(
                &plan,
                FailurePhase::Capability,
                "exact_component_acknowledgements_unavailable",
                "the backend cannot correlate semantic success for every generation operation",
                active_revision,
                0,
                cleanup,
            );
        }
        if active_revision != plan.revision.confirmed_revision {
            let cleanup =
                cleanup_unactivated_resources(&plan, resources, driver, &mut correlations).await;
            return self.finish_pre_activation_failure(
                &plan,
                FailurePhase::ExpectedRevision,
                "revision_changed_after_planning",
                "the confirmed graph changed after the deterministic plan was captured",
                active_revision,
                0,
                cleanup,
            );
        }
        if let Some(active) = &self.active {
            if plan.allocation.root == active.root
                || plan.allocation.parent == active.root
                || plan.allocation.generation <= active.generation
            {
                let cleanup =
                    cleanup_unactivated_resources(&plan, resources, driver, &mut correlations)
                        .await;
                return self.finish_pre_activation_failure(
                    &plan,
                    FailurePhase::Planning,
                    "generation_allocation_not_inactive",
                    "the next generation must use a newer generation and a root outside the active graph",
                    active_revision,
                    0,
                    cleanup,
                );
            }
        }
        if !resources.stage_exists(plan.resource_stage.stage()) {
            return rejected(
                FailurePhase::Planning,
                "resource_stage_missing",
                "the plan's exact resource stage is no longer retained",
                RollbackState::NotNeeded,
                active_revision,
                CleanupHealth::Retained,
            );
        }
        if let Err(error) = resources.prepare_snapshot(&plan.resource_stage) {
            let cleanup =
                cleanup_unactivated_resources(&plan, resources, driver, &mut correlations).await;
            return self.finish_pre_activation_failure(
                &plan,
                FailurePhase::Planning,
                "resource_stage_invalid",
                &error.to_string(),
                active_revision,
                0,
                cleanup,
            );
        }

        if self.quarantined_roots.contains(&plan.allocation.root) {
            let cleanup =
                cleanup_unactivated_resources(&plan, resources, driver, &mut correlations).await;
            return self.finish_pre_activation_failure(
                &plan,
                FailurePhase::Planning,
                "generation_root_quarantined",
                "the planned generation root has an uncertain prior cleanup",
                active_revision,
                0,
                cleanup,
            );
        }

        let root_correlation = match correlations.reserve(driver) {
            Ok(correlation) => correlation,
            Err(error) => {
                let cleanup =
                    cleanup_unactivated_resources(&plan, resources, driver, &mut correlations)
                        .await;
                return self.finish_pre_activation_failure(
                    &plan,
                    FailurePhase::Staging,
                    "inactive_root_correlation_unavailable",
                    &error.to_string(),
                    active_revision,
                    0,
                    cleanup,
                );
            }
        };
        let root_result = driver
            .stage_inactive_root(&plan.allocation, &root_correlation)
            .await;
        if !matches!(root_result, Ok(ref actual) if actual == &root_correlation) {
            let detail = match root_result {
                Ok(_) => "the backend acknowledged a different inactive-root correlation".into(),
                Err(error) => error.to_string(),
            };
            let cleanup = cleanup_staged(
                &plan,
                resources,
                driver,
                &mut correlations,
                &mut self.quarantined_roots,
            )
            .await;
            return self.finish_pre_activation_failure(
                &plan,
                FailurePhase::Staging,
                "inactive_root_stage_failed",
                &detail,
                active_revision,
                1,
                cleanup,
            );
        }
        let mut outcomes = Vec::with_capacity(plan.components.len() + 1);
        outcomes.push(ComponentOutcome {
            path: "generation/root".into(),
            action: "stage_inactive_root".into(),
            state: ComponentState::NotStarted,
            effective_at: None,
            confirmation: Some(Confirmation::BackendBarrier {
                backend: root_correlation.backend,
                token: root_correlation.token,
            }),
            diagnostic: None,
        });
        for component in &plan.components {
            let correlation = match correlations.reserve(driver) {
                Ok(correlation) => correlation,
                Err(error) => {
                    outcomes.push(failed_component(component, error.to_string()));
                    let completed_components = outcomes.len() - 1;
                    outcomes.extend(
                        plan.components[completed_components..]
                            .iter()
                            .map(not_started),
                    );
                    let cleanup = cleanup_staged(
                        &plan,
                        resources,
                        driver,
                        &mut correlations,
                        &mut self.quarantined_roots,
                    )
                    .await;
                    return self.finish_pre_activation_failure(
                        &plan,
                        FailurePhase::Staging,
                        "stage_correlation_unavailable",
                        &error.to_string(),
                        active_revision,
                        completed_components + 1,
                        cleanup,
                    );
                }
            };
            let result = driver
                .stage_component(plan.allocation.root, component, &correlation)
                .await;
            if !matches!(result, Ok(ref actual) if actual == &correlation) {
                let detail = match result {
                    Ok(_) => "the backend acknowledged a different stage correlation".into(),
                    Err(error) => error.to_string(),
                };
                outcomes.push(failed_component(component, detail.clone()));
                let completed_components = outcomes.len() - 1;
                outcomes.extend(
                    plan.components[completed_components..]
                        .iter()
                        .map(not_started),
                );
                let cleanup = cleanup_staged(
                    &plan,
                    resources,
                    driver,
                    &mut correlations,
                    &mut self.quarantined_roots,
                )
                .await;
                return self.finish_pre_activation_failure(
                    &plan,
                    FailurePhase::Staging,
                    &format!("{}_stage_failed", phase_name(component.phase())),
                    &detail,
                    active_revision,
                    completed_components + 1,
                    cleanup,
                );
            }
            let mut outcome = not_started(component);
            outcome.confirmation = Some(Confirmation::BackendBarrier {
                backend: correlation.backend,
                token: correlation.token,
            });
            outcomes.push(outcome);
        }

        let stage_correlation = match correlations.reserve(driver) {
            Ok(correlation) => correlation,
            Err(error) => {
                let cleanup = cleanup_staged(
                    &plan,
                    resources,
                    driver,
                    &mut correlations,
                    &mut self.quarantined_roots,
                )
                .await;
                return self.finish_pre_activation_failure(
                    &plan,
                    FailurePhase::BackendBarrier,
                    "barrier_correlation_unavailable",
                    &error.to_string(),
                    active_revision,
                    plan.components.len() + 1,
                    cleanup,
                );
            }
        };
        match driver.barrier(&stage_correlation).await {
            Ok(acknowledgement) if acknowledgement == stage_correlation => {}
            Ok(_) => {
                let cleanup = cleanup_staged(
                    &plan,
                    resources,
                    driver,
                    &mut correlations,
                    &mut self.quarantined_roots,
                )
                .await;
                return self.finish_pre_activation_failure(
                    &plan,
                    FailurePhase::BackendBarrier,
                    "barrier_correlation_mismatch",
                    "the backend acknowledged a different staging correlation",
                    active_revision,
                    plan.components.len() + 1,
                    cleanup,
                );
            }
            Err(error) => {
                let cleanup = cleanup_staged(
                    &plan,
                    resources,
                    driver,
                    &mut correlations,
                    &mut self.quarantined_roots,
                )
                .await;
                return self.finish_pre_activation_failure(
                    &plan,
                    FailurePhase::BackendBarrier,
                    "staging_barrier_failed",
                    &error.to_string(),
                    active_revision,
                    plan.components.len() + 1,
                    cleanup,
                );
            }
        }

        let activation = ActivationSwitch {
            previous_root: self.active.as_ref().map(|active| active.root),
            next_root: plan.allocation.root,
            deadline: plan.boundary.deadline,
        };
        let activation_correlation = match correlations.reserve(driver) {
            Ok(correlation) => correlation,
            Err(error) => {
                let cleanup = cleanup_staged(
                    &plan,
                    resources,
                    driver,
                    &mut correlations,
                    &mut self.quarantined_roots,
                )
                .await;
                return self.finish_pre_activation_failure(
                    &plan,
                    FailurePhase::Activate,
                    "activation_correlation_unavailable",
                    &error.to_string(),
                    active_revision,
                    plan.components.len() + 1,
                    cleanup,
                );
            }
        };
        let activation_result = driver.activate(&activation, &activation_correlation).await;
        match activation_result {
            Ok(acknowledgement) if acknowledgement == activation_correlation => {}
            Ok(_) => {
                return self
                    .restore_after_activation_failure(
                        &plan,
                        resources,
                        driver,
                        &mut correlations,
                        activation,
                        active_revision,
                        "activation_correlation_mismatch",
                        "the backend acknowledged a different activation correlation",
                        outcomes,
                    )
                    .await;
            }
            Err(error) => {
                return self
                    .restore_after_activation_failure(
                        &plan,
                        resources,
                        driver,
                        &mut correlations,
                        activation,
                        active_revision,
                        "activation_failed",
                        &error.to_string(),
                        outcomes,
                    )
                    .await;
            }
        }

        if let Err(error) = driver.precommit(&plan).await {
            return self
                .restore_after_activation_failure(
                    &plan,
                    resources,
                    driver,
                    &mut correlations,
                    activation,
                    active_revision,
                    "runtime_commit_failed",
                    &error.to_string(),
                    outcomes,
                )
                .await;
        }

        let resource_retirement = match resources.commit_snapshot(&plan.resource_stage) {
            Ok(retirement) => retirement,
            Err(error) => {
                return self
                    .restore_after_activation_failure(
                        &plan,
                        resources,
                        driver,
                        &mut correlations,
                        activation,
                        active_revision,
                        "resource_commit_failed",
                        &error.to_string(),
                        outcomes,
                    )
                    .await;
            }
        };

        for outcome in &mut outcomes {
            outcome.state = ComponentState::Applied;
            outcome.effective_at = Some(plan.boundary.effective.clone());
        }
        let previous = self.active.replace(AppliedGeneration {
            generation: plan.allocation.generation,
            root: plan.allocation.root,
            revision: plan.target_revision,
            plan_digest: plan.digest.clone(),
        });
        let mut confirmations = vec![
            Confirmation::BackendBarrier {
                backend: stage_correlation.backend,
                token: stage_correlation.token,
            },
            Confirmation::BackendBarrier {
                backend: activation_correlation.backend,
                token: activation_correlation.token,
            },
            Confirmation::RuntimeCommit,
        ];
        if let Some(beat) = plan.boundary.effective.musical_beat {
            confirmations.push(Confirmation::MusicalBoundary {
                beat,
                backend_time: plan.boundary.effective.backend_time_seconds,
            });
        }
        let receipt = Applied {
            effective_at: plan.boundary.effective.clone(),
            confirmations,
            components: outcomes,
            audible_tail_until: plan.boundary.audible_tail_until.clone(),
        };
        let cleanup = cleanup_committed(
            previous,
            &resource_retirement,
            resources,
            driver,
            &mut correlations,
            &mut self.quarantined_roots,
        )
        .await;
        GenerationOutcome::Applied {
            receipt,
            generation: self
                .active
                .clone()
                .expect("active generation was published before cleanup"),
            cleanup,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_pre_activation_failure(
        &mut self,
        plan: &NativeGenerationPlan,
        phase: FailurePhase,
        code: &str,
        message: &str,
        preserved_revision: Option<RevisionId>,
        attempted_components: usize,
        cleanup: CleanupHealth,
    ) -> GenerationOutcome {
        if !matches!(cleanup, CleanupHealth::Degraded(_)) {
            return rejected(
                phase,
                code,
                message,
                RollbackState::NotNeeded,
                preserved_revision,
                cleanup,
            );
        }

        self.fenced = true;
        self.quarantined_roots.insert(plan.allocation.root);
        let mut components = uncertain_plan_components(plan, attempted_components);
        if let Some(component) = components
            .iter_mut()
            .find(|component| component.state == ComponentState::Uncertain)
        {
            component.diagnostic = Some(Diagnostic {
                severity: DiagnosticSeverity::Error,
                code: code.into(),
                message: message.into(),
                component_path: Some(component.path.clone()),
                source_span: None,
            });
        }
        GenerationOutcome::Partial {
            receipt: Partial {
                phase: FailurePhase::Rollback,
                code: format!("{code}_cleanup_unconfirmed"),
                components,
                rollback: RollbackState::Uncertain,
                fenced: true,
                last_confirmed_revision: preserved_revision,
            },
            cleanup,
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn restore_after_activation_failure<D: NativeGenerationDriver>(
        &mut self,
        plan: &NativeGenerationPlan,
        resources: &ResourceManager,
        driver: &D,
        correlations: &mut CorrelationLedger,
        activation: ActivationSwitch,
        preserved_revision: Option<RevisionId>,
        code: &str,
        message: &str,
        components: Vec<ComponentOutcome>,
    ) -> GenerationOutcome {
        let restoration = correlations.reserve(driver).ok().map(|correlation| async {
            let result = driver.restore(&activation, &correlation).await;
            (correlation, result)
        });
        let restored = match restoration {
            Some(restoration) => {
                let (expected, result) = restoration.await;
                matches!(result, Ok(actual) if actual == expected)
            }
            None => false,
        };
        if restored {
            let cleanup = cleanup_staged(
                plan,
                resources,
                driver,
                correlations,
                &mut self.quarantined_roots,
            )
            .await;
            let phase = if code == "runtime_commit_failed" || code == "resource_commit_failed" {
                FailurePhase::Reconcile
            } else {
                FailurePhase::Activate
            };
            if matches!(cleanup, CleanupHealth::Degraded(_)) {
                GenerationOutcome::Partial {
                    receipt: Partial {
                        phase: FailurePhase::Rollback,
                        code: format!("{code}_cleanup_unconfirmed"),
                        components: components
                            .into_iter()
                            .map(|mut component| {
                                component.state = ComponentState::Uncertain;
                                component
                            })
                            .collect(),
                        rollback: RollbackState::Confirmed,
                        fenced: false,
                        last_confirmed_revision: preserved_revision,
                    },
                    cleanup,
                }
            } else {
                rejected(
                    phase,
                    code,
                    message,
                    RollbackState::Confirmed,
                    preserved_revision,
                    cleanup,
                )
            }
        } else {
            self.fenced = true;
            self.quarantined_roots.insert(plan.allocation.root);
            let mut cleanup_issues =
                vec!["staged graph quarantined after unconfirmed restoration".into()];
            if let Err(error) = resources.quarantine_stage(plan.resource_stage.stage()) {
                cleanup_issues.push(format!("resource stage quarantine failed: {error}"));
            }
            GenerationOutcome::Partial {
                receipt: Partial {
                    phase: FailurePhase::Rollback,
                    code: "restoration_unconfirmed".into(),
                    components: components
                        .into_iter()
                        .map(|mut component| {
                            component.state = ComponentState::Uncertain;
                            component
                        })
                        .collect(),
                    rollback: RollbackState::Uncertain,
                    fenced: true,
                    last_confirmed_revision: preserved_revision,
                },
                cleanup: CleanupHealth::Degraded(cleanup_issues),
            }
        }
    }
}

fn uncertain_plan_components(
    plan: &NativeGenerationPlan,
    attempted_components: usize,
) -> Vec<ComponentOutcome> {
    let mut components = std::iter::once(ComponentOutcome {
        path: "generation/root".into(),
        action: "stage_inactive_root".into(),
        state: if attempted_components > 0 {
            ComponentState::Uncertain
        } else {
            ComponentState::NotStarted
        },
        effective_at: None,
        confirmation: None,
        diagnostic: None,
    })
    .chain(
        plan.components
            .iter()
            .enumerate()
            .map(|(index, component)| ComponentOutcome {
                path: component.path.clone(),
                action: component.operation.action_name().into(),
                state: if index + 1 < attempted_components
                    || matches!(
                        &component.operation,
                        NativeStageOperation::Resource { generation, .. }
                            if plan.resource_stage.requires_cleanup(*generation)
                    ) {
                    ComponentState::Uncertain
                } else {
                    ComponentState::NotStarted
                },
                effective_at: None,
                confirmation: None,
                diagnostic: None,
            }),
    )
    .collect::<Vec<_>>();
    if components
        .iter()
        .all(|component| component.state != ComponentState::Uncertain)
    {
        components.push(ComponentOutcome {
            path: "generation/cleanup".into(),
            action: "cleanup_staged_generation".into(),
            state: ComponentState::Uncertain,
            effective_at: None,
            confirmation: None,
            diagnostic: None,
        });
    }
    components
}

fn not_started(component: &NativePlanComponent) -> ComponentOutcome {
    ComponentOutcome {
        path: component.path.clone(),
        action: component.operation.action_name().into(),
        state: ComponentState::NotStarted,
        effective_at: None,
        confirmation: None,
        diagnostic: None,
    }
}

fn failed_component(component: &NativePlanComponent, message: String) -> ComponentOutcome {
    ComponentOutcome {
        path: component.path.clone(),
        action: component.operation.action_name().into(),
        state: ComponentState::Failed,
        effective_at: None,
        confirmation: None,
        diagnostic: Some(Diagnostic {
            severity: DiagnosticSeverity::Error,
            code: format!("{}_stage_failed", phase_name(component.phase())),
            message,
            component_path: Some(component.path.clone()),
            source_span: None,
        }),
    }
}

const fn phase_name(phase: StagePhase) -> &'static str {
    match phase {
        StagePhase::Create => "create",
        StagePhase::Update => "update",
        StagePhase::Route => "route",
        StagePhase::Effect => "effect",
    }
}

fn rejected(
    phase: FailurePhase,
    code: &str,
    message: &str,
    rollback: RollbackState,
    preserved_revision: Option<RevisionId>,
    cleanup: CleanupHealth,
) -> GenerationOutcome {
    GenerationOutcome::Rejected {
        receipt: Rejected {
            phase,
            code: code.into(),
            message: message.into(),
            rollback,
            preserved_revision,
        },
        cleanup,
    }
}

async fn cleanup_unactivated_resources<D: NativeGenerationDriver>(
    plan: &NativeGenerationPlan,
    resources: &ResourceManager,
    driver: &D,
    correlations: &mut CorrelationLedger,
) -> CleanupHealth {
    let mut issues = Vec::new();
    let retirement = match resources.discard_stage(plan.resource_stage.stage()) {
        Ok(retirement) => retirement,
        Err(error) => {
            issues.push(format!("resource stage discard failed: {error}"));
            ResourceRetirement::default()
        }
    };
    if retirement.generations().len() == 0 {
        return if issues.is_empty() {
            CleanupHealth::Clean
        } else {
            CleanupHealth::Degraded(issues)
        };
    }
    let quiescence = match correlations.reserve(driver) {
        Ok(expected) => match driver.barrier(&expected).await {
            Ok(actual) if actual == expected => Some(actual),
            Ok(_) => {
                issues.push("unactivated resource barrier correlation mismatch".into());
                None
            }
            Err(error) => {
                issues.push(format!("unactivated resource barrier failed: {error}"));
                None
            }
        },
        Err(error) => {
            issues.push(format!(
                "unactivated resource cleanup correlation unavailable: {error}"
            ));
            None
        }
    };
    cleanup_resources(
        resources,
        &retirement,
        driver,
        correlations,
        quiescence.as_ref(),
        &mut issues,
    )
    .await;
    if issues.is_empty() {
        CleanupHealth::Clean
    } else {
        CleanupHealth::Degraded(issues)
    }
}

async fn cleanup_staged<D: NativeGenerationDriver>(
    plan: &NativeGenerationPlan,
    resources: &ResourceManager,
    driver: &D,
    correlations: &mut CorrelationLedger,
    quarantined_roots: &mut HashSet<NodeId>,
) -> CleanupHealth {
    let mut issues = Vec::new();
    let retirement = match resources.discard_stage(plan.resource_stage.stage()) {
        Ok(retirement) => retirement,
        Err(error) => {
            issues.push(format!("resource stage discard failed: {error}"));
            ResourceRetirement::default()
        }
    };
    let quiescence = cleanup_graph(
        plan.allocation.root,
        driver,
        correlations,
        quarantined_roots,
        &mut issues,
    )
    .await;
    cleanup_resources(
        resources,
        &retirement,
        driver,
        correlations,
        quiescence.as_ref(),
        &mut issues,
    )
    .await;
    if issues.is_empty() {
        CleanupHealth::Clean
    } else {
        CleanupHealth::Degraded(issues)
    }
}

async fn cleanup_committed<D: NativeGenerationDriver>(
    previous: Option<AppliedGeneration>,
    retirement: &ResourceRetirement,
    resources: &ResourceManager,
    driver: &D,
    correlations: &mut CorrelationLedger,
    quarantined_roots: &mut HashSet<NodeId>,
) -> CleanupHealth {
    let mut issues = Vec::new();
    let quiescence = if let Some(previous) = previous {
        cleanup_graph(
            previous.root,
            driver,
            correlations,
            quarantined_roots,
            &mut issues,
        )
        .await
    } else {
        None
    };
    cleanup_resources(
        resources,
        retirement,
        driver,
        correlations,
        quiescence.as_ref(),
        &mut issues,
    )
    .await;
    if !issues.is_empty() {
        CleanupHealth::Degraded(issues)
    } else if resources.retirement_is_pending(retirement) {
        CleanupHealth::Retained
    } else {
        CleanupHealth::Clean
    }
}

async fn cleanup_graph<D: NativeGenerationDriver>(
    root: NodeId,
    driver: &D,
    correlations: &mut CorrelationLedger,
    quarantined_roots: &mut HashSet<NodeId>,
    issues: &mut Vec<String>,
) -> Option<BackendCorrelation> {
    let expected = match correlations.reserve(driver) {
        Ok(expected) => expected,
        Err(error) => {
            issues.push(format!("cleanup correlation unavailable: {error}"));
            quarantined_roots.insert(root);
            return None;
        }
    };
    match driver.cleanup_generation(root, &expected).await {
        Ok(actual) if actual == expected => Some(actual),
        Ok(_) => {
            issues.push("cleanup correlation mismatch; graph root quarantined".into());
            quarantined_roots.insert(root);
            None
        }
        Err(error) => {
            issues.push(format!(
                "graph cleanup uncertain; root quarantined: {error}"
            ));
            quarantined_roots.insert(root);
            None
        }
    }
}

async fn cleanup_resources<D: NativeGenerationDriver>(
    resources: &ResourceManager,
    retirement: &ResourceRetirement,
    driver: &D,
    correlations: &mut CorrelationLedger,
    quiescence: Option<&BackendCorrelation>,
    issues: &mut Vec<String>,
) {
    let Some(quiescence) = quiescence else {
        if let Err(error) = resources.quarantine_retirement(retirement) {
            issues.push(format!("resource retirement quarantine failed: {error}"));
        }
        return;
    };
    loop {
        let freeable = resources.freeable_from(retirement);
        if freeable.is_empty() {
            break;
        }
        for generation in freeable {
            let batch = match resources.begin_free(
                generation,
                match QuiescenceProof::confirmed(
                    quiescence.backend.clone(),
                    quiescence.token.clone(),
                ) {
                    Ok(proof) => proof,
                    Err(error) => {
                        issues.push(format!("resource quiescence proof rejected: {error}"));
                        continue;
                    }
                },
            ) {
                Ok(batch) => batch,
                Err(error) => {
                    issues.push(format!("resource free admission failed: {error}"));
                    let _ = resources.quarantine(generation);
                    continue;
                }
            };
            let mut confirmations = Vec::with_capacity(batch.physical().len());
            for physical in batch.physical().iter().copied() {
                let expected = match correlations.reserve(driver) {
                    Ok(expected) => expected,
                    Err(error) => {
                        let detail = format!("resource free correlation unavailable: {error}");
                        confirmations.push(PhysicalFreeConfirmation::Uncertain {
                            physical,
                            detail: detail.clone(),
                        });
                        issues.push(detail);
                        continue;
                    }
                };
                match driver.free_resource(physical, &expected).await {
                    Ok(actual) if actual == expected => {
                        confirmations.push(PhysicalFreeConfirmation::Confirmed {
                            physical,
                            backend: actual.backend,
                            token: actual.token,
                        });
                    }
                    Ok(_) => {
                        let detail = format!("resource free correlation mismatch for {physical:?}");
                        confirmations.push(PhysicalFreeConfirmation::Uncertain {
                            physical,
                            detail: detail.clone(),
                        });
                        issues.push(detail);
                    }
                    Err(error) => {
                        let detail = format!("resource free uncertain for {physical:?}: {error}");
                        confirmations.push(PhysicalFreeConfirmation::Uncertain {
                            physical,
                            detail: detail.clone(),
                        });
                        issues.push(detail);
                    }
                }
            }
            if let Err(error) = resources.finish_free(&batch, confirmations) {
                issues.push(format!("resource free accounting failed: {error}"));
            }
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum GenerationError {
    #[error("invalid generation plan: {0}")]
    InvalidPlan(String),
    #[error("capability snapshot is invalid: {0}")]
    Capability(String),
    #[error("capability snapshot and confirmed revision do not match")]
    RevisionSnapshotMismatch,
    #[error("candidate runtime epoch and capability snapshot do not match")]
    CandidateEpochMismatch,
    #[error("required atomic generation activation is unavailable")]
    AtomicCapabilityUnavailable,
    #[error("the runtime/backend atomic probe is incomplete")]
    AtomicProbeIncomplete,
    #[error("candidate and native operation component sets differ: expected {expected:?}, actual {actual:?}")]
    ComponentSetMismatch {
        expected: BTreeSet<String>,
        actual: BTreeSet<String>,
    },
    #[error("the native resource operations do not exactly match the captured resource stage")]
    ResourceComponentSetMismatch,
    #[error("activation boundary is invalid")]
    InvalidBoundary,
    #[error(transparent)]
    Resource(#[from] ResourceError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::{
        CandidateDraft, ContractDigest, EngineInstanceId, EvaluationIdentity, LanguageContract,
        ReferenceCatalog,
    };
    use crate::resource_manager::{
        GenerationHealth, LogicalResource, ResourceAccounting, ResourceKind, SampleIdentity,
    };
    use parking_lot::Mutex;
    use std::collections::HashSet;

    #[path = "m07_integration_gate.rs"]
    mod m07_integration_gate;

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    enum Fault {
        CorrelationExhaustion,
        Root,
        Create,
        Update,
        Route,
        Effect,
        Barrier,
        BarrierTimeout,
        Activation,
        ActivationSendFailure,
        Commit,
        Restoration,
        Cleanup,
        Free,
    }

    #[derive(Debug, Error)]
    #[error("injected {0:?} fault")]
    struct DriverError(Fault);

    #[derive(Debug)]
    struct FaultDriver {
        next: Mutex<u64>,
        faults: Mutex<HashSet<Fault>>,
        wrong_acks: Mutex<HashSet<Fault>>,
        exact_acknowledgements: bool,
        duplicate_correlations: bool,
        events: Mutex<Vec<String>>,
    }

    impl Default for FaultDriver {
        fn default() -> Self {
            Self {
                next: Mutex::new(0),
                faults: Mutex::new(HashSet::new()),
                wrong_acks: Mutex::new(HashSet::new()),
                exact_acknowledgements: true,
                duplicate_correlations: false,
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl FaultDriver {
        fn with_fault(fault: Fault) -> Self {
            Self {
                faults: Mutex::new(HashSet::from([fault])),
                ..Self::default()
            }
        }

        fn with_wrong_ack(fault: Fault) -> Self {
            Self {
                wrong_acks: Mutex::new(HashSet::from([fault])),
                ..Self::default()
            }
        }

        fn with_duplicate_correlations() -> Self {
            Self {
                duplicate_correlations: true,
                ..Self::default()
            }
        }

        fn fail(&self, fault: Fault) -> Result<(), DriverError> {
            self.events.lock().push(format!("{fault:?}"));
            if self.faults.lock().contains(&fault) {
                Err(DriverError(fault))
            } else {
                Ok(())
            }
        }

        fn ack(
            &self,
            fault: Fault,
            expected: &BackendCorrelation,
        ) -> Result<BackendCorrelation, DriverError> {
            self.fail(fault)?;
            if self.wrong_acks.lock().contains(&fault) {
                Ok(BackendCorrelation {
                    backend: expected.backend.clone(),
                    token: "wrong-token".into(),
                })
            } else {
                Ok(expected.clone())
            }
        }
    }

    #[async_trait]
    impl NativeGenerationDriver for FaultDriver {
        type Error = DriverError;

        fn has_exact_component_acknowledgements(&self) -> bool {
            self.exact_acknowledgements
        }

        fn reserve_correlation(&self) -> Result<BackendCorrelation, Self::Error> {
            self.fail(Fault::CorrelationExhaustion)?;
            if self.duplicate_correlations {
                return Ok(BackendCorrelation {
                    backend: "mock".into(),
                    token: "sync-1".into(),
                });
            }
            let mut next = self.next.lock();
            *next += 1;
            Ok(BackendCorrelation {
                backend: "mock".into(),
                token: format!("sync-{next}"),
            })
        }

        async fn stage_inactive_root(
            &self,
            _allocation: &GenerationAllocation,
            expected: &BackendCorrelation,
        ) -> Result<BackendCorrelation, Self::Error> {
            self.ack(Fault::Root, expected)
        }

        async fn stage_component(
            &self,
            _root: NodeId,
            component: &NativePlanComponent,
            expected: &BackendCorrelation,
        ) -> Result<BackendCorrelation, Self::Error> {
            self.ack(
                match component.phase() {
                    StagePhase::Create => Fault::Create,
                    StagePhase::Update => Fault::Update,
                    StagePhase::Route => Fault::Route,
                    StagePhase::Effect => Fault::Effect,
                },
                expected,
            )
        }

        async fn barrier(
            &self,
            expected: &BackendCorrelation,
        ) -> Result<BackendCorrelation, Self::Error> {
            self.fail(Fault::BarrierTimeout)?;
            self.ack(Fault::Barrier, expected)
        }

        async fn activate(
            &self,
            _activation: &ActivationSwitch,
            expected: &BackendCorrelation,
        ) -> Result<BackendCorrelation, Self::Error> {
            self.fail(Fault::ActivationSendFailure)?;
            self.ack(Fault::Activation, expected)
        }

        async fn precommit(&self, _plan: &NativeGenerationPlan) -> Result<(), Self::Error> {
            self.fail(Fault::Commit)
        }

        async fn restore(
            &self,
            _activation: &ActivationSwitch,
            expected: &BackendCorrelation,
        ) -> Result<BackendCorrelation, Self::Error> {
            self.ack(Fault::Restoration, expected)
        }

        async fn cleanup_generation(
            &self,
            _root: NodeId,
            expected: &BackendCorrelation,
        ) -> Result<BackendCorrelation, Self::Error> {
            self.ack(Fault::Cleanup, expected)
        }

        async fn free_resource(
            &self,
            _physical: PhysicalResourceId,
            expected: &BackendCorrelation,
        ) -> Result<BackendCorrelation, Self::Error> {
            self.ack(Fault::Free, expected)
        }
    }

    fn empty_candidate(epoch: RuntimeEpoch) -> Candidate {
        let language = LanguageContract::v2(ContractDigest::from_bytes(b"test manifest"));
        CandidateDraft::new(
            EvaluationIdentity::new(language, EngineInstanceId::new(), epoch),
            CandidateOrigin::ScriptFile,
        )
        .finish(&ReferenceCatalog::default())
        .expect("empty candidate")
    }

    fn revision(epoch: RuntimeEpoch, confirmed: Option<RevisionId>) -> PlanningRevision {
        PlanningRevision {
            snapshot_id: PublicDigest::parse(format!("sha256:{}", "1".repeat(64))).expect("digest"),
            snapshot_generation: 1,
            runtime_epoch: epoch,
            confirmed_revision: confirmed,
            atomic_capability_available: false,
        }
    }

    fn boundary() -> ActivationBoundary {
        ActivationBoundary {
            requested_beat: Some(BeatTicks::new(65_536)),
            requested_backend_seconds: 1.0001,
            deadline: None,
            audible_tail_beats: Some(BeatTicks::new(32_768)),
            audible_tail_seconds: Some(0.25),
            observed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
        }
    }

    fn plan(
        resources: &ResourceManager,
        phases: &[StagePhase],
        confirmed: Option<RevisionId>,
    ) -> NativeGenerationPlan {
        let stage = resources.begin_stage().expect("resource stage");
        let components = phases
            .iter()
            .enumerate()
            .map(|(index, phase)| NativePlanComponent {
                declaration: format!("test/{index}"),
                path: format!("test/{index}"),
                operation: match phase {
                    StagePhase::Create => NativeStageOperation::CreateGroup {
                        node: NodeId::new(2000 + index as u32),
                        target: NodeId::new(1001),
                        action: AddAction::Tail,
                    },
                    StagePhase::Update => NativeStageOperation::SetParams {
                        node: NodeId::new(2000 + index as u32),
                        params: ParamMap::new(),
                    },
                    StagePhase::Route => NativeStageOperation::MapRoute {
                        node: NodeId::new(2000 + index as u32),
                        parameter: "freq".into(),
                        bus: 9,
                    },
                    StagePhase::Effect => NativeStageOperation::CreateEffect {
                        definition: "fx".into(),
                        node: NodeId::new(2000 + index as u32),
                        target: NodeId::new(1001),
                        action: AddAction::Tail,
                        params: ParamMap::new(),
                    },
                },
            })
            .collect();
        NativeGenerationPlan {
            target_revision: RevisionId::new(confirmed.map_or(1, |value| value.get() + 1))
                .expect("revision"),
            revision: revision(RuntimeEpoch::new(), confirmed),
            atomicity: AtomicAdmission::BestEffort,
            allocation: GenerationAllocation {
                generation: GraphGeneration::new(2).expect("generation"),
                root: NodeId::new(1001),
                parent: NodeId::new(1),
            },
            resource_stage: resources.snapshot_stage(stage).expect("resource snapshot"),
            components,
            boundary: quantize_boundary(boundary(), 48_000.0, 64).expect("boundary"),
            digest: PlanDigest(format!("sha256:{}", "2".repeat(64))),
        }
    }

    fn active(revision: RevisionId) -> AppliedGeneration {
        AppliedGeneration {
            generation: GraphGeneration::new(1).expect("generation"),
            root: NodeId::new(1000),
            revision,
            plan_digest: PlanDigest(format!("sha256:{}", "0".repeat(64))),
        }
    }

    #[test]
    fn planning_is_deterministic_and_required_atomicity_fails_closed_without_probe() {
        let epoch = RuntimeEpoch::new();
        let candidate = empty_candidate(epoch);
        let planner = NativeGenerationPlanner;
        let revision = revision(epoch, None);
        let resources = ResourceManager::new();
        let stage = resources.begin_stage().expect("resource stage");
        let snapshot = resources.snapshot_stage(stage).expect("resource snapshot");
        let make = || {
            planner.plan_captured(
                &candidate,
                RevisionId::new(1).expect("revision"),
                revision.clone(),
                AtomicAdmission::BestEffort,
                None,
                GenerationAllocation {
                    generation: GraphGeneration::new(1).expect("generation"),
                    root: NodeId::new(1000),
                    parent: NodeId::new(1),
                },
                snapshot.clone(),
                Vec::new(),
                boundary(),
                48_000.0,
                64,
            )
        };
        assert_eq!(
            make().expect("first").digest(),
            make().expect("second").digest()
        );
        assert_eq!(
            planner.plan_captured(
                &candidate,
                RevisionId::new(1).expect("revision"),
                revision,
                AtomicAdmission::Required,
                None,
                GenerationAllocation {
                    generation: GraphGeneration::new(1).expect("generation"),
                    root: NodeId::new(1000),
                    parent: NodeId::new(1),
                },
                snapshot,
                Vec::new(),
                boundary(),
                48_000.0,
                64,
            ),
            Err(GenerationError::AtomicCapabilityUnavailable)
        );
    }

    #[test]
    fn boundary_and_tail_are_quantized_and_reported() {
        let quantized = quantize_boundary(boundary(), 48_000.0, 64).expect("boundary");
        let seconds = quantized
            .effective
            .backend_time_seconds
            .expect("backend time")
            .get();
        assert!((seconds - 1.0013333333333334).abs() < 1e-12);
        let tail = quantized.audible_tail_until.expect("tail");
        assert_eq!(tail.musical_beat, Some(BeatTicks::new(98_304)));
        assert!((tail.backend_time_seconds.expect("time").get() - (seconds + 0.25)).abs() < 1e-12);
    }

    #[test]
    fn boundary_overflow_is_rejected_without_dropping_the_deadline() {
        let mut overflow = boundary();
        overflow.requested_backend_seconds = f64::MAX;
        overflow.deadline = Some(Instant::now());
        assert_eq!(
            quantize_boundary(overflow, f64::MIN_POSITIVE, 1),
            Err(GenerationError::InvalidBoundary)
        );

        let mut tail_overflow = boundary();
        tail_overflow.requested_backend_seconds = f64::MAX / 2.0;
        tail_overflow.audible_tail_seconds = Some(f64::MAX);
        assert_eq!(
            quantize_boundary(tail_overflow, 1.0, 1),
            Err(GenerationError::InvalidBoundary)
        );
    }

    #[test]
    fn inactive_validation_rejects_unstaged_definitions_and_unknown_update_targets() {
        let allocation = GenerationAllocation {
            generation: GraphGeneration::new(7).expect("generation"),
            root: NodeId::new(1001),
            parent: NodeId::new(1),
        };
        let create = NativePlanComponent {
            declaration: "voice/bass".into(),
            path: "voice/bass/create".into(),
            operation: NativeStageOperation::CreateSynth {
                definition: "bass__g7".into(),
                node: NodeId::new(2001),
                target: allocation.root,
                action: AddAction::Tail,
                params: ParamMap::new(),
            },
        };
        assert!(matches!(
            validate_inactive_operations(&allocation, &[create]),
            Err(GenerationError::InvalidPlan(message))
                if message.contains("not staged in this graph generation")
        ));
        let update = NativePlanComponent {
            declaration: "voice/bass".into(),
            path: "voice/bass/update".into(),
            operation: NativeStageOperation::SetParams {
                node: NodeId::new(2999),
                params: ParamMap::new(),
            },
        };
        assert!(matches!(
            validate_inactive_operations(&allocation, &[update]),
            Err(GenerationError::InvalidPlan(message))
                if message.contains("was not created in this inactive generation")
        ));

        let backend_root = GenerationAllocation {
            generation: GraphGeneration::new(8).expect("generation"),
            root: NodeId::new(0),
            parent: NodeId::new(1),
        };
        assert!(matches!(
            validate_inactive_operations(&backend_root, &[]),
            Err(GenerationError::InvalidPlan(message))
                if message.contains("cannot alias the backend root")
        ));

        let definition = NativePlanComponent {
            declaration: "effect/reverb".into(),
            path: "effect/reverb/definition".into(),
            operation: NativeStageOperation::LoadSynthDef {
                name: "reverb__g7".into(),
                bytes: Arc::from([1_u8]),
            },
        };
        let effect = NativePlanComponent {
            declaration: "effect/reverb".into(),
            path: "effect/reverb/create".into(),
            operation: NativeStageOperation::CreateEffect {
                definition: "reverb__g7".into(),
                node: NodeId::new(2100),
                target: allocation.root,
                action: AddAction::Tail,
                params: ParamMap::new(),
            },
        };
        let premature_update = NativePlanComponent {
            declaration: "effect/reverb".into(),
            path: "effect/reverb/update".into(),
            operation: NativeStageOperation::SetParams {
                node: NodeId::new(2100),
                params: ParamMap::new(),
            },
        };
        assert!(matches!(
            validate_inactive_operations(&allocation, &[definition, effect, premature_update]),
            Err(GenerationError::InvalidPlan(message))
                if message.contains("was not created in this inactive generation")
        ));
    }

    #[tokio::test]
    async fn next_generation_cannot_alias_or_descend_from_the_active_root() {
        let base = RevisionId::new(1).expect("revision");
        for aliases_root in [true, false] {
            let resources = ResourceManager::new();
            let mut planned = plan(&resources, &[], Some(base));
            if aliases_root {
                planned.allocation.root = NodeId::new(1000);
            } else {
                planned.allocation.parent = NodeId::new(1000);
            }
            let driver = FaultDriver::default();
            let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
            let outcome = coordinator.execute(planned, &resources, &driver).await;
            assert!(matches!(
                outcome,
                GenerationOutcome::Rejected {
                    receipt: Rejected {
                        phase: FailurePhase::Planning,
                        ref code,
                        ..
                    },
                    cleanup: CleanupHealth::Clean,
                } if code == "generation_allocation_not_inactive"
            ));
            assert_eq!(coordinator.active().map(|value| value.revision), Some(base));
            assert!(driver.events.lock().is_empty());
        }
    }

    #[tokio::test]
    async fn preactivation_stage_create_update_route_effect_and_barrier_faults_preserve_authority()
    {
        for (fault, phases) in [
            (Fault::Root, vec![]),
            (Fault::Create, vec![StagePhase::Create]),
            (Fault::Update, vec![StagePhase::Update]),
            (Fault::Route, vec![StagePhase::Route]),
            (Fault::Effect, vec![StagePhase::Effect]),
            (Fault::Barrier, vec![]),
        ] {
            let resources = ResourceManager::new();
            let base = RevisionId::new(1).expect("revision");
            let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
            let outcome = coordinator
                .execute(
                    plan(&resources, &phases, Some(base)),
                    &resources,
                    &FaultDriver::with_fault(fault),
                )
                .await;
            assert!(
                matches!(outcome, GenerationOutcome::Rejected { .. }),
                "{fault:?}"
            );
            assert_eq!(coordinator.active().map(|value| value.revision), Some(base));
            assert!(!coordinator.is_fenced());
        }
    }

    #[tokio::test]
    async fn activation_or_commit_failure_is_rejected_only_after_confirmed_restoration() {
        for fault in [Fault::Activation, Fault::Commit] {
            let resources = ResourceManager::new();
            let base = RevisionId::new(1).expect("revision");
            let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
            let outcome = coordinator
                .execute(
                    plan(&resources, &[], Some(base)),
                    &resources,
                    &FaultDriver::with_fault(fault),
                )
                .await;
            assert!(matches!(
                outcome,
                GenerationOutcome::Rejected {
                    receipt: Rejected {
                        rollback: RollbackState::Confirmed,
                        ..
                    },
                    ..
                }
            ));
            assert_eq!(coordinator.active().map(|value| value.revision), Some(base));
        }
    }

    #[tokio::test]
    async fn unconfirmed_restoration_returns_partial_and_fences_later_admission() {
        let resources = ResourceManager::new();
        let base = RevisionId::new(1).expect("revision");
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let driver = FaultDriver::with_fault(Fault::Activation);
        driver.faults.lock().insert(Fault::Restoration);
        let outcome = coordinator
            .execute(plan(&resources, &[], Some(base)), &resources, &driver)
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Partial {
                receipt: Partial { fenced: true, .. },
                ..
            }
        ));
        assert!(coordinator.is_fenced());
        let later = coordinator
            .execute(
                plan(&resources, &[], Some(base)),
                &resources,
                &FaultDriver::default(),
            )
            .await;
        assert!(matches!(later, GenerationOutcome::Rejected { .. }));
    }

    #[tokio::test]
    async fn correlated_ack_mismatch_is_not_accepted_as_a_barrier() {
        for (fault, phases, phase) in [
            (Fault::Root, vec![], FailurePhase::Staging),
            (
                Fault::Create,
                vec![StagePhase::Create],
                FailurePhase::Staging,
            ),
            (
                Fault::Update,
                vec![StagePhase::Update],
                FailurePhase::Staging,
            ),
            (Fault::Route, vec![StagePhase::Route], FailurePhase::Staging),
            (
                Fault::Effect,
                vec![StagePhase::Effect],
                FailurePhase::Staging,
            ),
            (Fault::Barrier, vec![], FailurePhase::BackendBarrier),
        ] {
            let resources = ResourceManager::new();
            let mut coordinator = NativeGenerationCoordinator::new(None);
            let outcome = coordinator
                .execute(
                    plan(&resources, &phases, None),
                    &resources,
                    &FaultDriver::with_wrong_ack(fault),
                )
                .await;
            assert!(matches!(
                outcome,
                GenerationOutcome::Rejected {
                    receipt: Rejected { phase: actual, .. },
                    ..
                } if actual == phase
            ));
            assert!(coordinator.active().is_none());
        }
    }

    #[tokio::test]
    async fn activation_and_restoration_acknowledgements_must_match_exactly() {
        let base = RevisionId::new(1).expect("revision");

        let resources = ResourceManager::new();
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(
                plan(&resources, &[], Some(base)),
                &resources,
                &FaultDriver::with_wrong_ack(Fault::Activation),
            )
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Rejected {
                receipt: Rejected {
                    rollback: RollbackState::Confirmed,
                    ..
                },
                ..
            }
        ));
        assert_eq!(coordinator.active().map(|value| value.revision), Some(base));

        let resources = ResourceManager::new();
        let driver = FaultDriver::with_fault(Fault::Activation);
        driver.wrong_acks.lock().insert(Fault::Restoration);
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(plan(&resources, &[], Some(base)), &resources, &driver)
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Partial {
                receipt: Partial {
                    rollback: RollbackState::Uncertain,
                    fenced: true,
                    ..
                },
                ..
            }
        ));
        assert!(coordinator.is_fenced());
    }

    #[tokio::test]
    async fn duplicate_reserved_correlation_is_rejected_and_fences_uncertain_cleanup() {
        let resources = ResourceManager::new();
        let base = RevisionId::new(1).expect("revision");
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(
                plan(&resources, &[], Some(base)),
                &resources,
                &FaultDriver::with_duplicate_correlations(),
            )
            .await;
        assert!(matches!(
            &outcome,
            GenerationOutcome::Partial {
                receipt: Partial { fenced: true, .. },
                cleanup: CleanupHealth::Degraded(_),
            }
        ));
        let GenerationOutcome::Partial { receipt, .. } = outcome else {
            unreachable!("the partial outcome was asserted above")
        };
        assert_eq!(receipt.components[0].state, ComponentState::Uncertain);
        assert_eq!(coordinator.active().map(|value| value.revision), Some(base));
        assert!(coordinator.is_fenced());
    }

    #[tokio::test]
    async fn cleanup_acknowledgement_mismatch_cannot_rewrite_an_applied_commit() {
        let resources = ResourceManager::new();
        let base = RevisionId::new(1).expect("revision");
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(
                plan(&resources, &[], Some(base)),
                &resources,
                &FaultDriver::with_wrong_ack(Fault::Cleanup),
            )
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Applied {
                cleanup: CleanupHealth::Degraded(_),
                ..
            }
        ));
        assert_eq!(
            coordinator.active().map(|value| value.revision.get()),
            Some(2)
        );
        assert!(!coordinator.is_fenced());
    }

    #[tokio::test]
    async fn nonsemantic_barrier_acknowledgements_fail_closed_before_staging() {
        let resources = ResourceManager::new();
        let planned = plan(&resources, &[], None);
        let resource_stage = planned.resource_stage.stage();
        let driver = FaultDriver {
            exact_acknowledgements: false,
            ..FaultDriver::default()
        };
        let mut coordinator = NativeGenerationCoordinator::new(None);
        let outcome = coordinator.execute(planned, &resources, &driver).await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Rejected {
                receipt: Rejected {
                    phase: FailurePhase::Capability,
                    rollback: RollbackState::NotNeeded,
                    ..
                },
                cleanup: CleanupHealth::Clean,
            }
        ));
        assert!(!resources.stage_exists(resource_stage));
        assert!(driver.events.lock().is_empty());
    }

    #[tokio::test]
    async fn uncertain_preactivation_cleanup_is_partial_and_fenced() {
        let resources = ResourceManager::new();
        let driver = FaultDriver::with_fault(Fault::Root);
        driver.faults.lock().insert(Fault::Cleanup);
        let mut coordinator = NativeGenerationCoordinator::new(None);
        let outcome = coordinator
            .execute(
                plan(&resources, &[StagePhase::Create, StagePhase::Update], None),
                &resources,
                &driver,
            )
            .await;
        assert!(matches!(
            &outcome,
            GenerationOutcome::Partial {
                receipt: Partial {
                    rollback: RollbackState::Uncertain,
                    fenced: true,
                    ..
                },
                cleanup: CleanupHealth::Degraded(_),
            }
        ));
        let GenerationOutcome::Partial { receipt, .. } = outcome else {
            unreachable!("the partial outcome was asserted above")
        };
        assert_eq!(receipt.components[0].state, ComponentState::Uncertain);
        assert!(receipt.components[1..]
            .iter()
            .all(|component| component.state == ComponentState::NotStarted));
        assert!(coordinator.is_fenced());
    }

    #[tokio::test]
    async fn revision_change_discards_unactivated_resources_and_uncertain_free_is_partial() {
        let resources = ResourceManager::new();
        let planned_base = RevisionId::new(1).expect("revision");
        let active_base = RevisionId::new(2).expect("revision");
        let mut planned = plan(&resources, &[], Some(planned_base));
        let key =
            LogicalResource::new(ResourceKind::Sample, "sample/unactivated").expect("logical");
        let staged = resources
            .stage_sample(
                planned.resource_stage.stage(),
                key.clone(),
                SampleIdentity {
                    canonical_source: "/unactivated.wav".into(),
                    content_fingerprint: "sha256:unactivated".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(80)],
            )
            .expect("staged sample");
        planned.components.push(NativePlanComponent {
            declaration: "resource/unactivated".into(),
            path: "resource/unactivated".into(),
            operation: NativeStageOperation::Resource {
                logical: key,
                generation: staged.generation,
            },
        });
        planned.resource_stage = resources
            .snapshot_stage(planned.resource_stage.stage())
            .expect("snapshot");

        let mut coordinator = NativeGenerationCoordinator::new(Some(active(active_base)));
        let outcome = coordinator
            .execute(planned, &resources, &FaultDriver::with_fault(Fault::Free))
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Partial {
                receipt: Partial {
                    phase: FailurePhase::Rollback,
                    rollback: RollbackState::Uncertain,
                    fenced: true,
                    last_confirmed_revision: Some(revision),
                    ..
                },
                cleanup: CleanupHealth::Degraded(_),
            } if revision == active_base
        ));
        assert_eq!(
            resources.health(staged.generation),
            Some(GenerationHealth::Quarantined)
        );
        assert_eq!(
            coordinator.active().map(|value| value.revision),
            Some(active_base)
        );
        assert!(coordinator.is_fenced());
    }

    #[tokio::test]
    async fn confirmed_restoration_with_uncertain_cleanup_preserves_prior_authority() {
        let resources = ResourceManager::new();
        let base = RevisionId::new(1).expect("revision");
        let driver = FaultDriver::with_fault(Fault::Activation);
        driver.faults.lock().insert(Fault::Cleanup);
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(plan(&resources, &[], Some(base)), &resources, &driver)
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Partial {
                receipt: Partial {
                    rollback: RollbackState::Confirmed,
                    fenced: false,
                    last_confirmed_revision: Some(revision),
                    ..
                },
                cleanup: CleanupHealth::Degraded(_),
            } if revision == base
        ));
        assert_eq!(
            coordinator.active().map(|active| active.revision),
            Some(base)
        );
        assert!(!coordinator.is_fenced());
    }

    #[tokio::test]
    async fn successful_commit_publishes_once_and_cleanup_failure_cannot_rewrite_applied() {
        let resources = ResourceManager::new();
        let old_key = LogicalResource::new(ResourceKind::Sample, "sample/old").expect("logical");
        let old_stage = resources.begin_stage().expect("stage");
        resources
            .stage_sample(
                old_stage,
                old_key.clone(),
                SampleIdentity {
                    canonical_source: "/old.wav".into(),
                    content_fingerprint: "sha256:old".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(51)],
            )
            .expect("sample");
        resources.commit_stage(old_stage).expect("commit");
        let mut planned = plan(&resources, &[], Some(RevisionId::new(1).expect("revision")));
        let replacement = resources
            .stage_sample(
                planned.resource_stage.stage(),
                old_key,
                SampleIdentity {
                    canonical_source: "/new.wav".into(),
                    content_fingerprint: "sha256:new".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(52)],
            )
            .expect("replacement");
        planned.components.push(NativePlanComponent {
            declaration: "resource/replacement".into(),
            path: "resource/replacement".into(),
            operation: NativeStageOperation::Resource {
                logical: LogicalResource::new(ResourceKind::Sample, "sample/old").expect("logical"),
                generation: replacement.generation,
            },
        });
        planned.resource_stage = resources
            .snapshot_stage(planned.resource_stage.stage())
            .expect("replacement snapshot");
        let base = RevisionId::new(1).expect("revision");
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let driver = FaultDriver::with_fault(Fault::Cleanup);
        let outcome = coordinator.execute(planned, &resources, &driver).await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Applied {
                cleanup: CleanupHealth::Degraded(_),
                ..
            }
        ));
        assert_eq!(
            coordinator.active().map(|value| value.revision.get()),
            Some(2)
        );
        assert_eq!(
            resources.health(replacement.generation),
            Some(GenerationHealth::Live)
        );
        assert_eq!(resources.accounting().logical_bindings, 1);
    }

    #[tokio::test]
    async fn resource_free_failure_is_quarantined_with_exact_accounting_after_applied_commit() {
        let resources = ResourceManager::new();
        let key = LogicalResource::new(ResourceKind::Sample, "sample/key").expect("logical");
        let old_stage = resources.begin_stage().expect("stage");
        let old = resources
            .stage_sample(
                old_stage,
                key.clone(),
                SampleIdentity {
                    canonical_source: "/same.wav".into(),
                    content_fingerprint: "sha256:old".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(60)],
            )
            .expect("old");
        resources.commit_stage(old_stage).expect("commit");
        let mut planned = plan(&resources, &[], Some(RevisionId::new(1).expect("revision")));
        resources
            .stage_sample(
                planned.resource_stage.stage(),
                key,
                SampleIdentity {
                    canonical_source: "/same.wav".into(),
                    content_fingerprint: "sha256:new".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(61)],
            )
            .expect("replacement");
        let replacement = resources
            .snapshot_stage(planned.resource_stage.stage())
            .expect("replacement snapshot");
        let (logical, generation) = replacement.claims().next().expect("resource claim");
        planned.components.push(NativePlanComponent {
            declaration: "resource/replacement".into(),
            path: "resource/replacement".into(),
            operation: NativeStageOperation::Resource {
                logical: logical.clone(),
                generation,
            },
        });
        planned.resource_stage = replacement;
        let base = RevisionId::new(1).expect("revision");
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(planned, &resources, &FaultDriver::with_fault(Fault::Free))
            .await;
        assert!(matches!(outcome, GenerationOutcome::Applied { .. }));
        assert_eq!(
            resources.health(old.generation),
            Some(GenerationHealth::Quarantined)
        );
        assert_eq!(
            resources.accounting(),
            ResourceAccounting {
                logical_bindings: 1,
                live_generations: 1,
                quarantined_generations: 1,
                live_physical: 1,
                quarantined_physical: 1,
                ..ResourceAccounting::default()
            }
        );
    }

    #[tokio::test]
    async fn reader_delayed_retirement_is_reaped_after_the_reader_drops() {
        let resources = ResourceManager::new();
        let key = LogicalResource::new(ResourceKind::Sample, "sample/reader").expect("logical");
        let old_stage = resources.begin_stage().expect("stage");
        let old = resources
            .stage_sample(
                old_stage,
                key.clone(),
                SampleIdentity {
                    canonical_source: "/reader.wav".into(),
                    content_fingerprint: "sha256:old".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(70)],
            )
            .expect("old");
        resources.commit_stage(old_stage).expect("commit");
        let reader = resources.acquire(&key).expect("reader");
        let base = RevisionId::new(1).expect("revision");
        let mut planned = plan(&resources, &[], Some(base));
        let replacement = resources
            .stage_sample(
                planned.resource_stage.stage(),
                key.clone(),
                SampleIdentity {
                    canonical_source: "/reader.wav".into(),
                    content_fingerprint: "sha256:new".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(71)],
            )
            .expect("replacement");
        planned.components.push(NativePlanComponent {
            declaration: "resource/reader".into(),
            path: "resource/reader".into(),
            operation: NativeStageOperation::Resource {
                logical: key,
                generation: replacement.generation,
            },
        });
        planned.resource_stage = resources
            .snapshot_stage(planned.resource_stage.stage())
            .expect("snapshot");
        let driver = FaultDriver::default();
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        assert!(matches!(
            coordinator.execute(planned, &resources, &driver).await,
            GenerationOutcome::Applied {
                cleanup: CleanupHealth::Retained,
                ..
            }
        ));
        assert_eq!(
            resources.health(old.generation),
            Some(GenerationHealth::Live)
        );
        drop(reader);
        assert_eq!(
            coordinator
                .reap_retired_resources(&resources, &driver)
                .await,
            CleanupHealth::Clean
        );
        assert_eq!(
            resources.health(old.generation),
            Some(GenerationHealth::Freed)
        );
    }
}
