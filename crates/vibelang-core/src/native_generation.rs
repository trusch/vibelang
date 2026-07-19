//! Native inactive-generation planning, activation, and restoration.
//!
//! Planning performs no backend or live-binding mutation. It captures one
//! confirmed receipt revision, capability snapshot, allocation snapshot, and
//! exclusive resource-stage authority.
//! Execution keeps the previously confirmed graph authoritative until a
//! correlated activation acknowledgment and the local commit boundary both
//! succeed.

use crate::backend::AddAction;
use crate::candidate::{
    AuthoringDeclaration, Candidate, DeclarationOwner, DeclarationPayload, LifecycleAction,
    LifecycleMetadata, StartMode,
};
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
use vibelang_dsp::{encode_synthdef, DspDefinitionIr};

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

#[derive(Debug, PartialEq)]
pub struct NativeGenerationPlan {
    target_revision: RevisionId,
    revision: PlanningRevision,
    atomicity: AtomicAdmission,
    atomic_backend: Option<String>,
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
    requested_beat: Option<BeatTicks>,
    backend_time_seconds: FiniteSeconds,
    deadline: Option<Instant>,
    audible_tail_beats: Option<BeatTicks>,
    audible_tail_seconds: Option<FiniteSeconds>,
    observed_at: Timestamp,
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
        mut resource_stage: ResourceStageSnapshot,
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
        let atomic_backend = (atomicity == AtomicAdmission::Required).then(|| {
            atomic_evidence
                .expect("required atomic evidence was validated")
                .backend
                .clone()
        });
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
        let topology = validate_inactive_operations(&allocation, &components)?;
        sort_components(&mut components, &topology);
        let boundary = quantize_boundary(boundary, sample_rate, block_size)?;
        let digest = plan_digest(
            candidate,
            target_revision,
            &revision,
            atomicity,
            atomic_backend.as_deref(),
            &allocation,
            &resource_stage,
            &components,
            &boundary,
        );
        resource_stage.capture()?;
        Ok(NativeGenerationPlan {
            target_revision,
            revision,
            atomicity,
            atomic_backend,
            allocation,
            resource_stage,
            components,
            boundary,
            digest,
        })
    }
}

#[must_use]
pub fn staged_dsp_definition_name(
    definition: &DspDefinitionIr,
    generation: GraphGeneration,
) -> String {
    format!(
        "{}__h{:016x}__g{}",
        definition.name(),
        definition.content_hash(),
        generation.get()
    )
}

pub fn lower_dsp_definition_components(
    candidate: &Candidate,
    generation: GraphGeneration,
) -> Result<Vec<NativePlanComponent>, GenerationError> {
    let mut components = Vec::new();
    for declaration in candidate.declarations() {
        let DeclarationPayload::Authoring {
            declaration: authoring,
            ..
        } = declaration.payload()
        else {
            continue;
        };
        let definition = match authoring {
            AuthoringDeclaration::SynthDef(definition)
            | AuthoringDeclaration::EffectDef(definition) => &definition.definition,
            _ => continue,
        };
        let name = staged_dsp_definition_name(definition, generation);
        let mut graph = definition.graph().clone();
        graph.name = name.clone();
        let bytes = encode_synthdef(&graph).map_err(|error| {
            GenerationError::InvalidPlan(format!(
                "failed to encode staged DSP definition {}: {error}",
                declaration.address()
            ))
        })?;
        components.push(NativePlanComponent {
            declaration: declaration.address().to_string(),
            path: format!(
                "definitions/{:016x}/{}",
                definition.content_hash(),
                declaration.address()
            ),
            operation: NativeStageOperation::LoadSynthDef {
                name,
                bytes: Arc::from(bytes),
            },
        });
    }
    Ok(components)
}

fn sort_components(components: &mut [NativePlanComponent], topology: &InactiveTopology) {
    components.sort_by(|left, right| {
        left.phase()
            .cmp(&right.phase())
            .then_with(|| operation_rank(&left.operation).cmp(&operation_rank(&right.operation)))
            .then_with(|| {
                topology_order(&left.operation, topology)
                    .cmp(&topology_order(&right.operation, topology))
            })
            .then_with(|| {
                group_depth(&left.operation, &topology.depths)
                    .cmp(&group_depth(&right.operation, &topology.depths))
            })
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| {
                left.operation
                    .action_name()
                    .cmp(right.operation.action_name())
            })
    });
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlannedNodeKind {
    Group,
    Synth,
    Effect,
}

impl PlannedNodeKind {
    const fn rank(self) -> u8 {
        match self {
            Self::Group => 0,
            Self::Synth => 1,
            Self::Effect => 2,
        }
    }
}

#[derive(Debug)]
struct PlannedPlacement {
    path: String,
    target: u32,
    action: AddAction,
    kind: PlannedNodeKind,
}

#[derive(Debug)]
struct InactiveTopology {
    depths: BTreeMap<u32, usize>,
    creation_order: BTreeMap<u32, usize>,
}

fn validate_inactive_operations(
    allocation: &GenerationAllocation,
    components: &[NativePlanComponent],
) -> Result<InactiveTopology, GenerationError> {
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
    let mut placements = BTreeMap::new();
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
            NativeStageOperation::CreateGroup {
                node,
                target,
                action,
            } => {
                let node = node.raw();
                if node == root || node == parent || !created_nodes.insert(node) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "native node {node} is duplicated or aliases the generation root/parent"
                    )));
                }
                created_before_updates.insert(node);
                placements.insert(
                    node,
                    PlannedPlacement {
                        path: component.path.clone(),
                        target: target.raw(),
                        action: *action,
                        kind: PlannedNodeKind::Group,
                    },
                );
            }
            NativeStageOperation::CreateSynth {
                definition,
                node,
                target,
                action,
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
                placements.insert(
                    node,
                    PlannedPlacement {
                        path: component.path.clone(),
                        target: target.raw(),
                        action: *action,
                        kind: PlannedNodeKind::Synth,
                    },
                );
            }
            NativeStageOperation::CreateEffect {
                definition,
                node,
                target,
                action,
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
                placements.insert(
                    node,
                    PlannedPlacement {
                        path: component.path.clone(),
                        target: target.raw(),
                        action: *action,
                        kind: PlannedNodeKind::Effect,
                    },
                );
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

    for (node, placement) in &placements {
        match placement.action {
            AddAction::Head | AddAction::Tail => {
                if placement.target != root
                    && placements
                        .get(&placement.target)
                        .is_none_or(|target| target.kind != PlannedNodeKind::Group)
                {
                    return Err(GenerationError::InvalidPlan(format!(
                        "{} {:?} target {} is not a staged group beneath the inactive root",
                        placement.path, placement.action, placement.target
                    )));
                }
            }
            AddAction::Before | AddAction::After | AddAction::Replace => {
                if placement.target == root || placement.target == parent {
                    return Err(GenerationError::InvalidPlan(format!(
                        "{} {:?} would escape or replace the inactive generation root",
                        placement.path, placement.action
                    )));
                }
                let target = placements.get(&placement.target).ok_or_else(|| {
                    GenerationError::InvalidPlan(format!(
                        "{} {:?} target {} is outside the inactive generation",
                        placement.path, placement.action, placement.target
                    ))
                })?;
                if target.kind.rank() > placement.kind.rank()
                    || (placement.kind == PlannedNodeKind::Group
                        && target.kind != PlannedNodeKind::Group)
                {
                    return Err(GenerationError::InvalidPlan(format!(
                        "{} {:?} target {} is not available in its staging phase",
                        placement.path, placement.action, placement.target
                    )));
                }
            }
        }
        if *node == placement.target {
            return Err(GenerationError::InvalidPlan(format!(
                "{} cannot place a node relative to itself",
                placement.path
            )));
        }
    }

    let mut parents = BTreeMap::new();
    while parents.len() < placements.len() {
        let mut progressed = false;
        for (node, placement) in &placements {
            if parents.contains_key(node) {
                continue;
            }
            let resolved = match placement.action {
                AddAction::Head | AddAction::Tail => Some(placement.target),
                AddAction::Before | AddAction::After | AddAction::Replace => {
                    parents.get(&placement.target).copied()
                }
            };
            if let Some(resolved) = resolved {
                parents.insert(*node, resolved);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    if parents.len() != placements.len() {
        return Err(GenerationError::InvalidPlan(
            "native add-action topology contains an unresolved or cyclic sibling placement".into(),
        ));
    }

    let mut depths = BTreeMap::from([(root, 0usize)]);
    while depths.len() <= parents.len() {
        let mut progressed = false;
        for (node, node_parent) in &parents {
            if depths.contains_key(node) {
                continue;
            }
            if let Some(parent_depth) = depths.get(node_parent).copied() {
                depths.insert(*node, parent_depth + 1);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    if depths.len() != placements.len() + 1 {
        return Err(GenerationError::InvalidPlan(
            "every staged node placement must remain beneath the inactive generation root".into(),
        ));
    }

    let replaced = placements
        .iter()
        .filter_map(|(node, placement)| {
            (placement.action == AddAction::Replace).then_some((*node, placement.target))
        })
        .collect::<Vec<_>>();
    let replaced_targets = replaced
        .iter()
        .map(|(_, target)| *target)
        .collect::<BTreeSet<_>>();
    for (replacement, target) in &replaced {
        if parents.values().any(|parent| parent == target)
            || placements.iter().any(|(node, placement)| {
                *node != *replacement && *node != *target && placement.target == *target
            })
        {
            return Err(GenerationError::InvalidPlan(format!(
                "replacing staged node {target} would invalidate another inactive placement"
            )));
        }
    }

    let mut creation_order = BTreeMap::new();
    while creation_order.len() < placements.len() {
        let next = placements
            .iter()
            .filter(|(node, _)| !creation_order.contains_key(*node))
            .filter(|(_, placement)| {
                placement.target == root || creation_order.contains_key(&placement.target)
            })
            .min_by(|(_, left), (_, right)| {
                left.kind
                    .rank()
                    .cmp(&right.kind.rank())
                    .then_with(|| left.path.cmp(&right.path))
            })
            .map(|(node, _)| *node);
        let Some(next) = next else {
            return Err(GenerationError::InvalidPlan(
                "native add-action topology cannot be staged in dependency order".into(),
            ));
        };
        creation_order.insert(next, creation_order.len());
    }

    for component in components {
        match &component.operation {
            NativeStageOperation::SetParams { node, .. }
            | NativeStageOperation::MapRoute { node, .. } => {
                if !created_before_updates.contains(&node.raw()) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "native update target {} was not created in this inactive generation",
                        node.raw()
                    )));
                }
                if replaced_targets.contains(&node.raw()) {
                    return Err(GenerationError::InvalidPlan(format!(
                        "native update target {} is removed by a staged replacement",
                        node.raw()
                    )));
                }
            }
            _ => {}
        }
    }
    depths.remove(&root);
    Ok(InactiveTopology {
        depths,
        creation_order,
    })
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

fn topology_order(operation: &NativeStageOperation, topology: &InactiveTopology) -> usize {
    match operation {
        NativeStageOperation::CreateGroup { node, .. }
        | NativeStageOperation::CreateSynth { node, .. }
        | NativeStageOperation::CreateEffect { node, .. } => topology
            .creation_order
            .get(&node.raw())
            .copied()
            .unwrap_or(usize::MAX),
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
    let backend_time_seconds =
        FiniteSeconds::new(quantized_seconds).map_err(GenerationError::InvalidPlan)?;
    let tail_seconds = requested.audible_tail_seconds.unwrap_or(0.0);
    if !tail_seconds.is_finite() || tail_seconds < 0.0 {
        return Err(GenerationError::InvalidBoundary);
    }
    if let (Some(beat), Some(tail)) = (requested.requested_beat, requested.audible_tail_beats) {
        beat.get()
            .checked_add(tail.get())
            .ok_or(GenerationError::InvalidBoundary)?;
    }
    if tail_seconds > 0.0 {
        FiniteSeconds::new(quantized_seconds + tail_seconds)
            .map_err(|_| GenerationError::InvalidBoundary)?;
    }
    Ok(QuantizedBoundary {
        requested_beat: requested.requested_beat,
        backend_time_seconds,
        deadline,
        audible_tail_beats: requested.audible_tail_beats,
        audible_tail_seconds: (tail_seconds > 0.0)
            .then(|| FiniteSeconds::new(tail_seconds).expect("validated finite tail")),
        observed_at: Timestamp::from_system_time(requested.observed_at),
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_digest(
    candidate: &Candidate,
    target_revision: RevisionId,
    revision: &PlanningRevision,
    atomicity: AtomicAdmission,
    atomic_backend: Option<&str>,
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
    fn lifecycle(hasher: &mut Sha256, metadata: &LifecycleMetadata) {
        hasher.update([
            metadata.role as u8,
            metadata.phase as u8,
            metadata.terminal_effect as u8,
            metadata.synchronization as u8,
            metadata.cancellation as u8,
            metadata.composition as u8,
            metadata.effect_domain as u8,
        ]);
        hasher.update(
            u64::try_from(metadata.effects.len())
                .expect("lifecycle effect count fits u64")
                .to_be_bytes(),
        );
        for effect in &metadata.effects {
            hasher.update([*effect as u8]);
        }
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
    if let Some(backend) = atomic_backend {
        hasher.update([1]);
        text(&mut hasher, backend);
    } else {
        hasher.update([0]);
    }
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
    hasher.update(b"declarations\0");
    hasher.update(
        u64::try_from(candidate.declarations().len())
            .expect("candidate declaration count fits u64")
            .to_be_bytes(),
    );
    for declaration in candidate.declarations() {
        text(&mut hasher, &declaration.address().to_string());
        hasher.update([declaration.address().kind() as u8]);
        match declaration.owner() {
            DeclarationOwner::Structural(key) => {
                hasher.update([0]);
                text(&mut hasher, &key.to_string());
            }
            DeclarationOwner::Contribution(id) => {
                hasher.update([1]);
                text(&mut hasher, &id.to_string());
            }
            DeclarationOwner::Parent(parent) => {
                hasher.update([2]);
                text(&mut hasher, &parent.to_string());
            }
            DeclarationOwner::Override(id) => {
                hasher.update([3]);
                text(&mut hasher, &id.to_string());
            }
        }
        lifecycle(&mut hasher, declaration.lifecycle());
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
            DeclarationPayload::Authoring {
                canonical_bytes, ..
            } => {
                hasher.update([2]);
                field_bytes(&mut hasher, canonical_bytes);
            }
        }
    }
    hasher.update(b"references\0");
    hasher.update(
        u64::try_from(candidate.references().len())
            .expect("candidate reference count fits u64")
            .to_be_bytes(),
    );
    for reference in candidate.references() {
        text(&mut hasher, &reference.reference().address().to_string());
    }
    hasher.update(b"contributions\0");
    hasher.update(
        u64::try_from(candidate.contributions().len())
            .expect("candidate contribution count fits u64")
            .to_be_bytes(),
    );
    for contribution in candidate.contributions() {
        text(&mut hasher, &contribution.id().to_string());
        text(
            &mut hasher,
            &contribution.target_group().address().to_string(),
        );
        if let Some(order) = contribution.explicit_order() {
            hasher.update([1]);
            hasher.update(order.to_be_bytes());
        } else {
            hasher.update([0]);
        }
        hasher.update(
            u64::try_from(contribution.owned_declarations().len())
                .expect("owned declaration count fits u64")
                .to_be_bytes(),
        );
        for address in contribution.owned_declarations() {
            text(&mut hasher, &address.to_string());
        }
    }
    hasher.update(b"overrides\0");
    hasher.update(
        u64::try_from(candidate.overrides().len())
            .expect("candidate override count fits u64")
            .to_be_bytes(),
    );
    for override_ir in candidate.overrides() {
        text(&mut hasher, &override_ir.id().to_string());
        text(&mut hasher, &override_ir.target().address().to_string());
        hasher.update(override_ir.precedence().to_be_bytes());
        hasher.update(
            u64::try_from(override_ir.fields().len())
                .expect("override field count fits u64")
                .to_be_bytes(),
        );
        for field in override_ir.fields() {
            text(&mut hasher, field);
        }
    }
    hasher.update(b"operations\0");
    hasher.update(
        u64::try_from(candidate.operations().len())
            .expect("candidate operation count fits u64")
            .to_be_bytes(),
    );
    for operation in candidate.operations() {
        text(&mut hasher, &operation.target().address().to_string());
        text(&mut hasher, &operation.source().syntax_key().to_string());
        lifecycle(&mut hasher, operation.lifecycle());
        match operation.action() {
            LifecycleAction::Start(mode) => {
                hasher.update([0]);
                hasher.update([match mode {
                    StartMode::Normal => 0,
                    StartMode::Immediate => 1,
                    StartMode::Continuous => 2,
                }]);
            }
            LifecycleAction::Stop => hasher.update([1]),
            LifecycleAction::Remove => hasher.update([2]),
            LifecycleAction::Cancel => hasher.update([3]),
            LifecycleAction::SetMuted(value) => hasher.update([4, u8::from(*value)]),
            LifecycleAction::SetSoloed(value) => hasher.update([5, u8::from(*value)]),
            LifecycleAction::RemoveContribution(id) => {
                hasher.update([6]);
                text(&mut hasher, &id.to_string());
            }
            LifecycleAction::Restart => hasher.update([7]),
        }
    }
    hasher.update(b"components\0");
    hasher.update(
        u64::try_from(components.len())
            .expect("native component count fits u64")
            .to_be_bytes(),
    );
    for component in components {
        text(&mut hasher, &component.declaration);
        text(&mut hasher, &component.path);
        hash_operation(&mut hasher, &component.operation);
    }
    text(&mut hasher, boundary.observed_at.as_str());
    if let Some(beat) = boundary.requested_beat {
        hasher.update([1]);
        hasher.update(beat.get().to_be_bytes());
    } else {
        hasher.update([0]);
    }
    hasher.update(boundary.backend_time_seconds.get().to_bits().to_be_bytes());
    if let Some(tail) = boundary.audible_tail_beats {
        hasher.update([1]);
        hasher.update(tail.get().to_be_bytes());
    } else {
        hasher.update([0]);
    }
    if let Some(tail) = boundary.audible_tail_seconds {
        hasher.update([1]);
        hasher.update(tail.get().to_bits().to_be_bytes());
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
        if correlation.backend != driver.backend_identity() {
            return Err(format!(
                "the reserved generation correlation belongs to backend {}, not executing backend {}",
                correlation.backend,
                driver.backend_identity()
            ));
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
    pub scheduled_backend_time: Option<FiniteSeconds>,
    pub requested_beat: Option<BeatTicks>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivationTimingKind {
    Scheduled,
    Executed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivationTimingProof {
    pub kind: ActivationTimingKind,
    pub observed_at: Timestamp,
    pub backend_time_seconds: FiniteSeconds,
    pub musical_beat: Option<BeatTicks>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivationAcknowledgement {
    pub correlation: BackendCorrelation,
    pub timing: Option<ActivationTimingProof>,
}

#[async_trait]
pub trait NativeGenerationDriver: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn backend_identity(&self) -> &str;

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
    ) -> Result<ActivationAcknowledgement, Self::Error>;

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
        mut plan: NativeGenerationPlan,
        resources: &ResourceManager,
        driver: &D,
    ) -> GenerationOutcome {
        if !plan.resource_stage.is_captured() {
            if let Err(error) = plan.resource_stage.capture() {
                self.fenced = true;
                return GenerationOutcome::Partial {
                    receipt: Partial {
                        phase: FailurePhase::Planning,
                        code: "resource_stage_capture_failed".into(),
                        components: uncertain_plan_components(&plan, 0),
                        rollback: RollbackState::NotNeeded,
                        fenced: true,
                        last_confirmed_revision: self.active.as_ref().map(|active| active.revision),
                    },
                    cleanup: CleanupHealth::Degraded(vec![error.to_string()]),
                };
            }
        }
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
        if driver.backend_identity().is_empty()
            || plan
                .atomic_backend
                .as_deref()
                .is_some_and(|backend| backend != driver.backend_identity())
        {
            let cleanup =
                cleanup_unactivated_resources(&plan, resources, driver, &mut correlations).await;
            return self.finish_pre_activation_failure(
                &plan,
                FailurePhase::Capability,
                "executing_backend_identity_mismatch",
                "the executing backend does not match the backend that proved atomic capability",
                active_revision,
                0,
                cleanup,
            );
        }
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
            self.fenced = true;
            return GenerationOutcome::Partial {
                receipt: Partial {
                    phase: FailurePhase::Planning,
                    code: "resource_stage_authority_lost".into(),
                    components: uncertain_plan_components(&plan, 0),
                    rollback: RollbackState::Uncertain,
                    fenced: true,
                    last_confirmed_revision: active_revision,
                },
                cleanup: CleanupHealth::Degraded(vec![format!(
                    "resource stage terminal state is {:?}",
                    resources.stage_state(plan.resource_stage.stage())
                )]),
            };
        }
        if let Err(_error) = resources.prepare_snapshot(&plan.resource_stage) {
            let cleanup =
                cleanup_unactivated_resources(&plan, resources, driver, &mut correlations).await;
            self.fenced = true;
            return GenerationOutcome::Partial {
                receipt: Partial {
                    phase: FailurePhase::Planning,
                    code: "resource_authority_changed_after_planning".into(),
                    components: uncertain_plan_components(&plan, 0),
                    rollback: RollbackState::NotNeeded,
                    fenced: true,
                    last_confirmed_revision: active_revision,
                },
                cleanup,
            };
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
            scheduled_backend_time: plan
                .boundary
                .deadline
                .map(|_| plan.boundary.backend_time_seconds),
            requested_beat: plan.boundary.requested_beat,
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
        let activation_acknowledgement = match activation_result {
            Ok(acknowledgement) if acknowledgement.correlation == activation_correlation => {
                acknowledgement
            }
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
        };
        let (effective_at, audible_tail_until) =
            match correlated_effective_at(&plan.boundary, &activation_acknowledgement) {
                Ok(effective) => effective,
                Err((code, message)) => {
                    return self
                        .restore_after_activation_failure(
                            &plan,
                            resources,
                            driver,
                            &mut correlations,
                            activation,
                            active_revision,
                            code,
                            &message,
                            outcomes,
                        )
                        .await;
                }
            };

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
            outcome.effective_at = Some(effective_at.clone());
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
        if let Some(beat) = effective_at.musical_beat {
            confirmations.push(Confirmation::MusicalBoundary {
                beat,
                backend_time: effective_at.backend_time_seconds,
            });
        }
        let receipt = Applied {
            effective_at,
            confirmations,
            components: outcomes,
            audible_tail_until,
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
            if code == "resource_commit_failed" {
                self.fenced = true;
                GenerationOutcome::Partial {
                    receipt: Partial {
                        phase: FailurePhase::Reconcile,
                        code: "resource_commit_failed_authority_unproven".into(),
                        components: components
                            .into_iter()
                            .map(|mut component| {
                                component.state = ComponentState::Uncertain;
                                component
                            })
                            .collect(),
                        rollback: RollbackState::Confirmed,
                        fenced: true,
                        last_confirmed_revision: preserved_revision,
                    },
                    cleanup,
                }
            } else if matches!(cleanup, CleanupHealth::Degraded(_)) {
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
            if let Err(error) = resources.quarantine_snapshot(&plan.resource_stage) {
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

fn correlated_effective_at(
    boundary: &QuantizedBoundary,
    acknowledgement: &ActivationAcknowledgement,
) -> Result<(EffectiveAt, Option<EffectiveAt>), (&'static str, String)> {
    let Some(proof) = &acknowledgement.timing else {
        return Ok((
            EffectiveAt {
                observed_at: Timestamp::from_system_time(SystemTime::now()),
                musical_beat: None,
                backend_time_seconds: None,
            },
            None,
        ));
    };

    if proof.kind == ActivationTimingKind::Scheduled {
        let Some(expected) = boundary.deadline.map(|_| boundary.backend_time_seconds) else {
            return Err((
                "immediate_activation_timing_unproven",
                "a scheduling acknowledgement cannot prove immediate execution time".into(),
            ));
        };
        if proof.backend_time_seconds != expected {
            return Err((
                "activation_backend_time_mismatch",
                format!(
                    "the backend acknowledged scheduled time {}, expected {}",
                    proof.backend_time_seconds.get(),
                    expected.get()
                ),
            ));
        }
        if let (Some(expected), Some(actual)) = (boundary.requested_beat, proof.musical_beat) {
            if actual != expected {
                return Err((
                    "activation_musical_time_mismatch",
                    format!(
                        "the backend acknowledged musical beat {}, expected {}",
                        actual.get(),
                        expected.get()
                    ),
                ));
            }
        }
    }

    let effective = EffectiveAt {
        observed_at: proof.observed_at.clone(),
        musical_beat: proof.musical_beat,
        backend_time_seconds: Some(proof.backend_time_seconds),
    };
    let has_tail = boundary.audible_tail_beats.is_some() || boundary.audible_tail_seconds.is_some();
    let audible_tail_until = if has_tail {
        let musical_beat = match (proof.musical_beat, boundary.audible_tail_beats) {
            (Some(beat), Some(tail)) => Some(BeatTicks::new(
                beat.get().checked_add(tail.get()).ok_or((
                    "activation_timing_overflow",
                    "the acknowledged musical time overflows the audible tail".into(),
                ))?,
            )),
            _ => None,
        };
        let backend_time_seconds = match boundary.audible_tail_seconds {
            Some(tail) => Some(
                FiniteSeconds::new(proof.backend_time_seconds.get() + tail.get()).map_err(
                    |_| {
                        (
                            "activation_timing_overflow",
                            "the acknowledged backend time overflows the audible tail".into(),
                        )
                    },
                )?,
            ),
            None => Some(proof.backend_time_seconds),
        };
        Some(EffectiveAt {
            observed_at: proof.observed_at.clone(),
            musical_beat,
            backend_time_seconds,
        })
    } else {
        None
    };
    Ok((effective, audible_tail_until))
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
    let retirement = match resources.discard_snapshot(&plan.resource_stage) {
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
    let retirement = match resources.discard_snapshot(&plan.resource_stage) {
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
        AuthoringDeclaration, CandidateDraft, CanonicalF64, Composition, ContractDigest,
        DeclarationIr, DeclarationKey, DeclarationOwner, DeclarationPayload,
        DspDefinitionAuthoring, EngineInstanceId, EvaluationIdentity, GroupAuthoring, GroupKind,
        GroupScope, LanguageContract, LifecycleMetadata, ModulePath, ProjectNamespace,
        ReferenceCatalog, SourceAnchor, SyntaxKey, SynthDefKind, TypedAddress,
    };
    use crate::resource_manager::{
        GenerationHealth, LogicalResource, ResourceAccounting, ResourceError, ResourceKind,
        ResourceStage, ResourceStageOwner, ResourceStageState, SampleIdentity,
    };
    use parking_lot::Mutex;
    use std::collections::HashSet;
    use vibelang_dsp::{DspDefinitionIr, GraphIR};

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
        identity: &'static str,
        next: Mutex<u64>,
        faults: Mutex<HashSet<Fault>>,
        wrong_acks: Mutex<HashSet<Fault>>,
        exact_acknowledgements: bool,
        duplicate_correlations: bool,
        timing_proof: Mutex<Option<ActivationTimingProof>>,
        stage_race: Mutex<Option<StageRace>>,
        stage_race_results: Mutex<
            Vec<(
                StageRacePoint,
                Result<(), ResourceError>,
                Result<(), ResourceError>,
            )>,
        >,
        competing_commit: Mutex<Option<(ResourceManager, ResourceStage)>>,
        competing_commit_result: Mutex<Option<Result<(), ResourceError>>>,
        events: Mutex<Vec<String>>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum StageRacePoint {
        AfterPrepare,
        AfterActivation,
        AtCommit,
    }

    #[derive(Debug)]
    struct StageRace {
        point: StageRacePoint,
        resources: ResourceManager,
        stage: ResourceStage,
    }

    impl Default for FaultDriver {
        fn default() -> Self {
            Self {
                identity: "mock",
                next: Mutex::new(0),
                faults: Mutex::new(HashSet::new()),
                wrong_acks: Mutex::new(HashSet::new()),
                exact_acknowledgements: true,
                duplicate_correlations: false,
                timing_proof: Mutex::new(None),
                stage_race: Mutex::new(None),
                stage_race_results: Mutex::new(Vec::new()),
                competing_commit: Mutex::new(None),
                competing_commit_result: Mutex::new(None),
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

        fn with_timing_proof(proof: ActivationTimingProof) -> Self {
            Self {
                timing_proof: Mutex::new(Some(proof)),
                ..Self::default()
            }
        }

        fn with_stage_race(
            point: StageRacePoint,
            resources: ResourceManager,
            stage: ResourceStage,
        ) -> Self {
            Self {
                stage_race: Mutex::new(Some(StageRace {
                    point,
                    resources,
                    stage,
                })),
                ..Self::default()
            }
        }

        fn with_competing_commit(resources: ResourceManager, stage: ResourceStage) -> Self {
            Self {
                competing_commit: Mutex::new(Some((resources, stage))),
                ..Self::default()
            }
        }

        fn with_identity(identity: &'static str) -> Self {
            Self {
                identity,
                ..Self::default()
            }
        }

        fn run_stage_race(&self, point: StageRacePoint) {
            let race = {
                let mut race = self.stage_race.lock();
                if race.as_ref().is_some_and(|race| race.point == point) {
                    race.take()
                } else {
                    None
                }
            };
            if let Some(race) = race {
                let commit = race.resources.commit_stage(race.stage).map(|_| ());
                let discard = race.resources.discard_stage(race.stage).map(|_| ());
                self.stage_race_results
                    .lock()
                    .push((point, commit, discard));
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

        fn backend_identity(&self) -> &str {
            self.identity
        }

        fn has_exact_component_acknowledgements(&self) -> bool {
            self.exact_acknowledgements
        }

        fn reserve_correlation(&self) -> Result<BackendCorrelation, Self::Error> {
            self.fail(Fault::CorrelationExhaustion)?;
            if self.duplicate_correlations {
                return Ok(BackendCorrelation {
                    backend: self.identity.into(),
                    token: "sync-1".into(),
                });
            }
            let mut next = self.next.lock();
            *next += 1;
            Ok(BackendCorrelation {
                backend: self.identity.into(),
                token: format!("sync-{next}"),
            })
        }

        async fn stage_inactive_root(
            &self,
            _allocation: &GenerationAllocation,
            expected: &BackendCorrelation,
        ) -> Result<BackendCorrelation, Self::Error> {
            self.run_stage_race(StageRacePoint::AfterPrepare);
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
        ) -> Result<ActivationAcknowledgement, Self::Error> {
            self.fail(Fault::ActivationSendFailure)?;
            self.run_stage_race(StageRacePoint::AfterActivation);
            Ok(ActivationAcknowledgement {
                correlation: self.ack(Fault::Activation, expected)?,
                timing: self.timing_proof.lock().clone(),
            })
        }

        async fn precommit(&self, _plan: &NativeGenerationPlan) -> Result<(), Self::Error> {
            self.run_stage_race(StageRacePoint::AtCommit);
            if let Some((resources, stage)) = self.competing_commit.lock().take() {
                *self.competing_commit_result.lock() =
                    Some(resources.commit_stage(stage).map(|_| ()));
            }
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

    fn group_candidate(identity: EvaluationIdentity, gain: f64, owner_component: u32) -> Candidate {
        let module = ModulePath::new("song/main").expect("module");
        let address = TypedAddress::<GroupKind>::new(
            ProjectNamespace::new("test-project").expect("project"),
            module.clone(),
            GroupScope::root(),
            DeclarationKey::new("band").expect("key"),
        );
        let owner = SyntaxKey::deterministic(&module, &[owner_component], "owner").expect("owner");
        let source = SourceAnchor::new(
            module.clone(),
            SyntaxKey::deterministic(&module, &[99], "declaration").expect("source"),
            None,
        );
        let declaration = DeclarationIr::new(
            address,
            DeclarationOwner::Structural(owner),
            source,
            LifecycleMetadata::register(Composition::Standalone),
            DeclarationPayload::authoring(AuthoringDeclaration::Group(GroupAuthoring {
                parent: None,
                gain: CanonicalF64::new(gain).expect("gain"),
                muted: false,
                soloed: false,
                params: BTreeMap::new(),
                output_channels: None,
            }))
            .expect("payload"),
        );
        let mut draft = CandidateDraft::new(identity, CandidateOrigin::RhaiHost);
        draft
            .declare::<GroupKind>(declaration)
            .expect("declaration");
        draft
            .finish(&ReferenceCatalog::default())
            .expect("candidate")
    }

    fn synthdef_candidate(identity: EvaluationIdentity, constant: f32) -> Candidate {
        let module = ModulePath::new("song/main").expect("module");
        let address = TypedAddress::<SynthDefKind>::new(
            ProjectNamespace::new("test-project").expect("project"),
            module.clone(),
            GroupScope::root(),
            DeclarationKey::new("tone").expect("key"),
        );
        let definition = DspDefinitionIr::synthdef(
            GraphIR {
                name: address.to_string(),
                constants: vec![constant],
                params: Vec::new(),
                nodes: Vec::new(),
                out_bus: 0,
            },
            Vec::new(),
            Vec::new(),
        )
        .expect("detached definition");
        let declaration = DeclarationIr::new(
            address,
            DeclarationOwner::Structural(
                SyntaxKey::deterministic(&module, &[1], "synthdef").expect("owner"),
            ),
            SourceAnchor::new(
                module,
                SyntaxKey::Explicit(DeclarationKey::new("tone-source").expect("source")),
                None,
            ),
            LifecycleMetadata::register(Composition::Standalone),
            DeclarationPayload::authoring(AuthoringDeclaration::SynthDef(DspDefinitionAuthoring {
                definition,
            }))
            .expect("payload"),
        );
        let mut draft = CandidateDraft::new(identity, CandidateOrigin::RhaiHost);
        draft
            .declare::<SynthDefKind>(declaration)
            .expect("definition declaration");
        draft
            .finish(&ReferenceCatalog::default())
            .expect("candidate")
    }

    #[test]
    fn detached_dsp_lowering_is_module_hash_and_generation_qualified() {
        let epoch = RuntimeEpoch::new();
        let identity = EvaluationIdentity::new(
            LanguageContract::v2(ContractDigest::from_bytes(b"dsp-lowering")),
            EngineInstanceId::new(),
            epoch,
        );
        let first = synthdef_candidate(identity.clone(), 0.0);
        let changed = synthdef_candidate(identity, 1.0);
        let generation = GraphGeneration::new(9).expect("generation");
        let first_components =
            lower_dsp_definition_components(&first, generation).expect("first lowering");
        let changed_components =
            lower_dsp_definition_components(&changed, generation).expect("changed lowering");

        assert_eq!(first.dsp_definitions().definitions().count(), 1);
        assert_eq!(first_components.len(), 1);
        assert_eq!(changed_components.len(), 1);
        let (first_name, first_bytes) = match &first_components[0].operation {
            NativeStageOperation::LoadSynthDef { name, bytes } => (name, bytes),
            operation => panic!("unexpected operation: {operation:?}"),
        };
        let (changed_name, changed_bytes) = match &changed_components[0].operation {
            NativeStageOperation::LoadSynthDef { name, bytes } => (name, bytes),
            operation => panic!("unexpected operation: {operation:?}"),
        };
        assert!(first_name.starts_with("test-project::song/main::synthdef::tone__h"));
        assert!(first_name.ends_with("__g9"));
        assert_ne!(first_name, changed_name);
        assert_ne!(first_bytes, changed_bytes);
        assert_eq!(
            first_components[0].declaration,
            "test-project::song/main::synthdef::tone"
        );
        assert!(first_components[0]
            .path
            .contains("test-project::song/main::synthdef::tone"));
        assert!(!vibelang_dsp::synthdef_exists(
            "test-project::song/main::synthdef::tone"
        ));
        assert_eq!(
            vibelang_dsp::get_synthdef_hash("test-project::song/main::synthdef::tone"),
            None
        );
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

    fn timing_proof(
        kind: ActivationTimingKind,
        backend_time_seconds: f64,
        musical_beat: Option<BeatTicks>,
    ) -> ActivationTimingProof {
        ActivationTimingProof {
            kind,
            observed_at: Timestamp::parse("2026-07-19T01:00:00Z").expect("timestamp"),
            backend_time_seconds: FiniteSeconds::new(backend_time_seconds)
                .expect("finite backend time"),
            musical_beat,
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
            atomic_backend: None,
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
        let make = || {
            let resources = ResourceManager::new();
            let stage = resources.begin_stage().expect("resource stage");
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
                resources.snapshot_stage(stage).expect("resource snapshot"),
                Vec::new(),
                boundary(),
                48_000.0,
                64,
            )
        };
        let first = make().expect("first");
        let second = make().expect("second");
        assert_eq!(first.digest(), second.digest());
        let resources = ResourceManager::new();
        let stage = resources.begin_stage().expect("resource stage");
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
                resources.snapshot_stage(stage).expect("resource snapshot"),
                Vec::new(),
                boundary(),
                48_000.0,
                64,
            ),
            Err(GenerationError::AtomicCapabilityUnavailable)
        );
    }

    #[test]
    fn canonical_plan_digest_commits_authoring_payload_and_structural_owner() {
        let epoch = RuntimeEpoch::new();
        let identity = EvaluationIdentity::new(
            LanguageContract::v2(ContractDigest::from_bytes(b"digest-test")),
            EngineInstanceId::new(),
            epoch,
        );
        let base = group_candidate(identity.clone(), 1.0, 1);
        let changed_payload = group_candidate(identity.clone(), 0.5, 1);
        let changed_owner = group_candidate(identity, 1.0, 2);
        let resources = ResourceManager::new();
        let stage = resources.begin_stage().expect("resource stage");
        let resource_stage = resources.snapshot_stage(stage).expect("resource snapshot");
        let planning_revision = revision(epoch, None);
        let allocation = GenerationAllocation {
            generation: GraphGeneration::new(1).expect("generation"),
            root: NodeId::new(1000),
            parent: NodeId::new(1),
        };
        let boundary = quantize_boundary(boundary(), 48_000.0, 64).expect("boundary");
        let digest = |candidate: &Candidate| {
            plan_digest(
                candidate,
                RevisionId::new(1).expect("revision"),
                &planning_revision,
                AtomicAdmission::BestEffort,
                None,
                &allocation,
                &resource_stage,
                &[],
                &boundary,
            )
        };

        assert_ne!(digest(&base), digest(&changed_payload));
        assert_ne!(digest(&base), digest(&changed_owner));
    }

    #[test]
    fn boundary_and_tail_are_quantized_and_reported() {
        let quantized = quantize_boundary(boundary(), 48_000.0, 64).expect("boundary");
        let seconds = quantized.backend_time_seconds.get();
        assert!((seconds - 1.0013333333333334).abs() < 1e-12);
        assert_eq!(quantized.requested_beat, Some(BeatTicks::new(65_536)));
        assert_eq!(quantized.audible_tail_beats, Some(BeatTicks::new(32_768)));
        assert_eq!(
            quantized.audible_tail_seconds.map(FiniteSeconds::get),
            Some(0.25)
        );
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

    #[tokio::test]
    async fn timing_receipts_omit_unproven_immediate_claims_and_publish_correlated_execution() {
        let base = RevisionId::new(1).expect("revision");
        let resources = ResourceManager::new();
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let unproven = coordinator
            .execute(
                plan(&resources, &[StagePhase::Create], Some(base)),
                &resources,
                &FaultDriver::default(),
            )
            .await;
        assert!(matches!(
            unproven,
            GenerationOutcome::Applied {
                receipt: Applied {
                    effective_at: EffectiveAt {
                        musical_beat: None,
                        backend_time_seconds: None,
                        ..
                    },
                    audible_tail_until: None,
                    ref confirmations,
                    ref components,
                },
                ..
            } if confirmations.len() == 3
                && components.iter().all(|component| component
                    .effective_at
                    .as_ref()
                    .is_some_and(|effective| effective.musical_beat.is_none()
                        && effective.backend_time_seconds.is_none()))
        ));

        let resources = ResourceManager::new();
        let proof = timing_proof(
            ActivationTimingKind::Executed,
            1.25,
            Some(BeatTicks::new(70_000)),
        );
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let proven = coordinator
            .execute(
                plan(&resources, &[StagePhase::Create], Some(base)),
                &resources,
                &FaultDriver::with_timing_proof(proof.clone()),
            )
            .await;
        let GenerationOutcome::Applied { receipt, .. } = proven else {
            panic!("correlated execution timing must apply")
        };
        assert_eq!(receipt.effective_at.observed_at, proof.observed_at);
        assert_eq!(
            receipt.effective_at.backend_time_seconds,
            Some(proof.backend_time_seconds)
        );
        assert_eq!(receipt.effective_at.musical_beat, proof.musical_beat);
        assert_eq!(receipt.confirmations.len(), 4);
        let tail = receipt.audible_tail_until.expect("correlated tail");
        assert_eq!(tail.musical_beat, Some(BeatTicks::new(102_768)));
        assert_eq!(tail.backend_time_seconds.map(FiniteSeconds::get), Some(1.5));
    }

    #[tokio::test]
    async fn scheduled_late_mismatch_and_overflow_timing_paths_are_truthful() {
        let base = RevisionId::new(1).expect("revision");

        let resources = ResourceManager::new();
        let mut immediate = plan(&resources, &[], Some(base));
        immediate.boundary.deadline = None;
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(
                immediate,
                &resources,
                &FaultDriver::with_timing_proof(timing_proof(
                    ActivationTimingKind::Scheduled,
                    1.0013333333333334,
                    Some(BeatTicks::new(65_536)),
                )),
            )
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Rejected {
                receipt: Rejected {
                    ref code,
                    rollback: RollbackState::Confirmed,
                    preserved_revision: Some(revision),
                    ..
                },
                cleanup: CleanupHealth::Clean,
            } if code == "immediate_activation_timing_unproven" && revision == base
        ));

        let resources = ResourceManager::new();
        let mut future = plan(&resources, &[], Some(base));
        future.boundary.deadline = Some(Instant::now());
        let expected = future.boundary.backend_time_seconds;
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let scheduled = coordinator
            .execute(
                future,
                &resources,
                &FaultDriver::with_timing_proof(timing_proof(
                    ActivationTimingKind::Scheduled,
                    expected.get(),
                    Some(BeatTicks::new(65_536)),
                )),
            )
            .await;
        assert!(matches!(
            scheduled,
            GenerationOutcome::Applied {
                receipt: Applied {
                    effective_at: EffectiveAt {
                        backend_time_seconds: Some(actual),
                        musical_beat: Some(beat),
                        ..
                    },
                    ..
                },
                ..
            } if actual == expected && beat == BeatTicks::new(65_536)
        ));

        let resources = ResourceManager::new();
        let mut future = plan(&resources, &[], Some(base));
        future.boundary.deadline = Some(Instant::now());
        let expected = future.boundary.backend_time_seconds.get();
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let mismatch = coordinator
            .execute(
                future,
                &resources,
                &FaultDriver::with_timing_proof(timing_proof(
                    ActivationTimingKind::Scheduled,
                    expected + 1.0,
                    Some(BeatTicks::new(65_536)),
                )),
            )
            .await;
        assert!(matches!(
            mismatch,
            GenerationOutcome::Rejected {
                receipt: Rejected {
                    ref code,
                    rollback: RollbackState::Confirmed,
                    ..
                },
                cleanup: CleanupHealth::Clean,
            } if code == "activation_backend_time_mismatch"
        ));

        let resources = ResourceManager::new();
        let mut late = plan(&resources, &[], Some(base));
        late.boundary.deadline = Some(Instant::now());
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let late = coordinator
            .execute(
                late,
                &resources,
                &FaultDriver::with_timing_proof(timing_proof(
                    ActivationTimingKind::Executed,
                    2.5,
                    Some(BeatTicks::new(80_000)),
                )),
            )
            .await;
        assert!(matches!(
            late,
            GenerationOutcome::Applied {
                receipt: Applied {
                    effective_at: EffectiveAt {
                        backend_time_seconds: Some(actual),
                        musical_beat: Some(beat),
                        ..
                    },
                    ..
                },
                ..
            } if actual.get() == 2.5 && beat == BeatTicks::new(80_000)
        ));

        let resources = ResourceManager::new();
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let overflow = coordinator
            .execute(
                plan(&resources, &[], Some(base)),
                &resources,
                &FaultDriver::with_timing_proof(timing_proof(
                    ActivationTimingKind::Executed,
                    1.0,
                    Some(BeatTicks::new(i64::MAX)),
                )),
            )
            .await;
        assert!(matches!(
            overflow,
            GenerationOutcome::Rejected {
                receipt: Rejected {
                    ref code,
                    rollback: RollbackState::Confirmed,
                    ..
                },
                cleanup: CleanupHealth::Clean,
            } if code == "activation_timing_overflow"
        ));
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

    #[test]
    fn inactive_topology_resolves_all_add_actions_without_escaping_the_root() {
        let allocation = GenerationAllocation {
            generation: GraphGeneration::new(7).expect("generation"),
            root: NodeId::new(1001),
            parent: NodeId::new(1),
        };
        let components = vec![
            NativePlanComponent {
                declaration: "group/anchor".into(),
                path: "group/anchor".into(),
                operation: NativeStageOperation::CreateGroup {
                    node: NodeId::new(2000),
                    target: allocation.root,
                    action: AddAction::Head,
                },
            },
            NativePlanComponent {
                declaration: "group/before".into(),
                path: "group/before".into(),
                operation: NativeStageOperation::CreateGroup {
                    node: NodeId::new(2001),
                    target: NodeId::new(2000),
                    action: AddAction::Before,
                },
            },
            NativePlanComponent {
                declaration: "voice/definition".into(),
                path: "voice/definition".into(),
                operation: NativeStageOperation::LoadSynthDef {
                    name: "voice__g7".into(),
                    bytes: Arc::from([1_u8]),
                },
            },
            NativePlanComponent {
                declaration: "voice/tail".into(),
                path: "voice/tail".into(),
                operation: NativeStageOperation::CreateSynth {
                    definition: "voice__g7".into(),
                    node: NodeId::new(3000),
                    target: allocation.root,
                    action: AddAction::Tail,
                    params: ParamMap::new(),
                },
            },
            NativePlanComponent {
                declaration: "voice/after".into(),
                path: "voice/after".into(),
                operation: NativeStageOperation::CreateSynth {
                    definition: "voice__g7".into(),
                    node: NodeId::new(3001),
                    target: NodeId::new(3000),
                    action: AddAction::After,
                    params: ParamMap::new(),
                },
            },
            NativePlanComponent {
                declaration: "effect/definition".into(),
                path: "effect/definition".into(),
                operation: NativeStageOperation::LoadSynthDef {
                    name: "effect__g7".into(),
                    bytes: Arc::from([2_u8]),
                },
            },
            NativePlanComponent {
                declaration: "effect/replace".into(),
                path: "effect/replace".into(),
                operation: NativeStageOperation::CreateEffect {
                    definition: "effect__g7".into(),
                    node: NodeId::new(4000),
                    target: NodeId::new(3001),
                    action: AddAction::Replace,
                    params: ParamMap::new(),
                },
            },
        ];
        let topology =
            validate_inactive_operations(&allocation, &components).expect("contained topology");
        assert!(topology.depths.values().all(|depth| *depth == 1));
        assert!(topology.creation_order[&2000] < topology.creation_order[&2001]);
        assert!(topology.creation_order[&3000] < topology.creation_order[&3001]);
        assert!(topology.creation_order[&3001] < topology.creation_order[&4000]);
        let mut sorted = components.clone();
        sort_components(&mut sorted, &topology);
        let sorted_nodes = sorted
            .iter()
            .filter_map(|component| match &component.operation {
                NativeStageOperation::CreateGroup { node, .. }
                | NativeStageOperation::CreateSynth { node, .. }
                | NativeStageOperation::CreateEffect { node, .. } => Some(node.raw()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let position = |node| {
            sorted_nodes
                .iter()
                .position(|candidate| *candidate == node)
                .expect("sorted node")
        };
        assert!(position(2000) < position(2001));
        assert!(position(3000) < position(3001));
        assert!(position(3001) < position(4000));

        for kind in [
            PlannedNodeKind::Group,
            PlannedNodeKind::Synth,
            PlannedNodeKind::Effect,
        ] {
            for action in [
                AddAction::Head,
                AddAction::Tail,
                AddAction::Before,
                AddAction::After,
                AddAction::Replace,
            ] {
                let target = if matches!(action, AddAction::Head | AddAction::Tail) {
                    allocation.parent
                } else {
                    allocation.root
                };
                let operation = match kind {
                    PlannedNodeKind::Group => NativeStageOperation::CreateGroup {
                        node: NodeId::new(5000),
                        target,
                        action,
                    },
                    PlannedNodeKind::Synth => NativeStageOperation::CreateSynth {
                        definition: "escape__g7".into(),
                        node: NodeId::new(5000),
                        target,
                        action,
                        params: ParamMap::new(),
                    },
                    PlannedNodeKind::Effect => NativeStageOperation::CreateEffect {
                        definition: "escape__g7".into(),
                        node: NodeId::new(5000),
                        target,
                        action,
                        params: ParamMap::new(),
                    },
                };
                let mut attack = Vec::new();
                if kind != PlannedNodeKind::Group {
                    attack.push(NativePlanComponent {
                        declaration: "escape/definition".into(),
                        path: "escape/definition".into(),
                        operation: NativeStageOperation::LoadSynthDef {
                            name: "escape__g7".into(),
                            bytes: Arc::from([3_u8]),
                        },
                    });
                }
                attack.push(NativePlanComponent {
                    declaration: "escape/node".into(),
                    path: "escape/node".into(),
                    operation,
                });
                assert!(matches!(
                    validate_inactive_operations(&allocation, &attack),
                    Err(GenerationError::InvalidPlan(message))
                        if message.contains("inactive generation root")
                            || message.contains("inactive root")
                ));
            }
        }

        let mut invalid_update = components;
        invalid_update.push(NativePlanComponent {
            declaration: "voice/replaced-update".into(),
            path: "voice/replaced-update".into(),
            operation: NativeStageOperation::SetParams {
                node: NodeId::new(3001),
                params: ParamMap::new(),
            },
        });
        assert!(matches!(
            validate_inactive_operations(&allocation, &invalid_update),
            Err(GenerationError::InvalidPlan(message))
                if message.contains("removed by a staged replacement")
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
    async fn captured_stage_excludes_external_terminal_races_at_every_execution_boundary() {
        let base = RevisionId::new(1).expect("revision");
        for point in [
            StageRacePoint::AfterPrepare,
            StageRacePoint::AfterActivation,
            StageRacePoint::AtCommit,
        ] {
            let resources = ResourceManager::new();
            let mut planned = plan(&resources, &[], Some(base));
            let stage = planned.resource_stage.stage();
            planned.resource_stage.capture().expect("capture");
            assert_eq!(
                resources.commit_stage(stage),
                Err(ResourceError::StageCaptured(stage))
            );
            assert_eq!(
                resources.discard_stage(stage),
                Err(ResourceError::StageCaptured(stage))
            );

            let driver = FaultDriver::with_stage_race(point, resources.clone(), stage);
            let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
            let outcome = coordinator.execute(planned, &resources, &driver).await;
            assert!(
                matches!(outcome, GenerationOutcome::Applied { .. }),
                "{point:?}"
            );
            assert_eq!(
                *driver.stage_race_results.lock(),
                vec![(
                    point,
                    Err(ResourceError::StageCaptured(stage)),
                    Err(ResourceError::StageCaptured(stage)),
                )]
            );
            assert_eq!(
                resources.stage_state(stage),
                Some(ResourceStageState::Committed(
                    ResourceStageOwner::Transaction
                ))
            );
            assert_eq!(
                coordinator.active().map(|active| active.revision.get()),
                Some(2)
            );
            assert!(!coordinator.is_fenced());
        }
    }

    #[tokio::test]
    async fn executing_backend_identity_must_match_required_atomic_proof() {
        let resources = ResourceManager::new();
        let base = RevisionId::new(1).expect("revision");
        let mut planned = plan(&resources, &[], Some(base));
        planned.atomicity = AtomicAdmission::Required;
        planned.atomic_backend = Some("proved-backend".into());
        let stage = planned.resource_stage.stage();
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(
                planned,
                &resources,
                &FaultDriver::with_identity("executing-backend"),
            )
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Rejected {
                receipt: Rejected {
                    phase: FailurePhase::Capability,
                    ref code,
                    rollback: RollbackState::NotNeeded,
                    preserved_revision: Some(revision),
                    ..
                },
                cleanup: CleanupHealth::Clean,
            } if code == "executing_backend_identity_mismatch" && revision == base
        ));
        assert_eq!(
            resources.stage_state(stage),
            Some(ResourceStageState::Discarded(
                ResourceStageOwner::Transaction
            ))
        );
        assert_eq!(
            coordinator.active().map(|active| active.revision),
            Some(base)
        );
        assert!(!coordinator.is_fenced());
    }

    #[tokio::test]
    async fn real_resource_commit_failure_restores_graph_and_fences_split_authority() {
        let resources = ResourceManager::new();
        let key = LogicalResource::new(ResourceKind::Sample, "sample/commit-race")
            .expect("logical resource");
        let old_stage = resources.begin_stage().expect("old stage");
        let old = resources
            .stage_sample(
                old_stage,
                key.clone(),
                SampleIdentity {
                    canonical_source: "/commit-race.wav".into(),
                    content_fingerprint: "sha256:old".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(500)],
            )
            .expect("old resource");
        resources.commit_stage(old_stage).expect("old commit");

        let base = RevisionId::new(1).expect("revision");
        let mut planned = plan(&resources, &[], Some(base));
        let planned_stage = planned.resource_stage.stage();
        let replacement = resources
            .stage_sample(
                planned_stage,
                key.clone(),
                SampleIdentity {
                    canonical_source: "/commit-race.wav".into(),
                    content_fingerprint: "sha256:planned".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(501)],
            )
            .expect("planned replacement");
        planned.components.push(NativePlanComponent {
            declaration: "resource/commit-race".into(),
            path: "resource/commit-race".into(),
            operation: NativeStageOperation::Resource {
                logical: key.clone(),
                generation: replacement.generation,
            },
        });
        planned.resource_stage = resources
            .snapshot_stage(planned_stage)
            .expect("planned snapshot");

        let competing_stage = resources.begin_stage().expect("competing stage");
        let competing = resources
            .stage_sample(
                competing_stage,
                key.clone(),
                SampleIdentity {
                    canonical_source: "/commit-race.wav".into(),
                    content_fingerprint: "sha256:competing".into(),
                    decode_options_digest: "decode".into(),
                    loader_version: "loader".into(),
                    backend: "mock".into(),
                },
                [PhysicalResourceId::Buffer(502)],
            )
            .expect("competing replacement");
        let driver = FaultDriver::with_competing_commit(resources.clone(), competing_stage);
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator.execute(planned, &resources, &driver).await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Partial {
                receipt: Partial {
                    phase: FailurePhase::Reconcile,
                    ref code,
                    rollback: RollbackState::Confirmed,
                    fenced: true,
                    last_confirmed_revision: Some(revision),
                    ref components,
                },
                cleanup: CleanupHealth::Clean,
            } if code == "resource_commit_failed_authority_unproven"
                && revision == base
                && components.iter().all(|component| component.state == ComponentState::Uncertain)
        ));
        assert_eq!(*driver.competing_commit_result.lock(), Some(Ok(())));
        assert_eq!(resources.generation_for(&key), Some(competing.generation));
        assert_ne!(resources.generation_for(&key), Some(old.generation));
        assert_eq!(
            resources.stage_state(planned_stage),
            Some(ResourceStageState::Discarded(
                ResourceStageOwner::Transaction
            ))
        );
        assert_eq!(
            resources.stage_state(competing_stage),
            Some(ResourceStageState::Committed(ResourceStageOwner::External))
        );
        assert_eq!(
            coordinator.active().map(|active| active.revision),
            Some(base)
        );
        assert!(coordinator.is_fenced());
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
