//! Generation-aware resource ownership for atomic candidate activation.
//!
//! The manager is runtime-local. Logical bindings, staged claims, and reader
//! leases all point at immutable physical generations so an inactive graph can
//! be prepared without changing the resources observed by the applied graph.

use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceGeneration(u64);

impl ResourceGeneration {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceStage(u64);

impl ResourceStage {
    pub fn new(value: u64) -> Result<Self, ResourceError> {
        if value == 0 {
            return Err(ResourceError::Invalid(
                "resource stage must be greater than zero".into(),
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
pub struct ResourceStageSnapshot {
    stage: ResourceStage,
    digest: String,
    claims: BTreeMap<LogicalResource, ResourceGeneration>,
    removals: BTreeMap<LogicalResource, ResourceGeneration>,
    cleanup_generations: BTreeSet<ResourceGeneration>,
}

impl ResourceStageSnapshot {
    #[must_use]
    pub const fn stage(&self) -> ResourceStage {
        self.stage
    }

    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn claims(&self) -> impl ExactSizeIterator<Item = (&LogicalResource, ResourceGeneration)> {
        self.claims
            .iter()
            .map(|(logical, generation)| (logical, *generation))
    }

    pub fn removals(
        &self,
    ) -> impl ExactSizeIterator<Item = (&LogicalResource, ResourceGeneration)> {
        self.removals
            .iter()
            .map(|(logical, generation)| (logical, *generation))
    }

    pub(crate) fn requires_cleanup(&self, generation: ResourceGeneration) -> bool {
        self.cleanup_generations.contains(&generation)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LogicalResource {
    kind: ResourceKind,
    address: String,
}

impl LogicalResource {
    pub fn new(kind: ResourceKind, address: impl Into<String>) -> Result<Self, ResourceError> {
        let address = address.into();
        if address.trim() != address
            || address.is_empty()
            || address.chars().any(char::is_whitespace)
        {
            return Err(ResourceError::Invalid(
                "logical resource address must be non-empty and contain no whitespace".into(),
            ));
        }
        Ok(Self { kind, address })
    }

    #[must_use]
    pub const fn kind(&self) -> ResourceKind {
        self.kind
    }

    #[must_use]
    pub fn address(&self) -> &str {
        &self.address
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResourceKind {
    Sample,
    Buffer,
    Sfz,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PhysicalResourceId {
    Buffer(u32),
    Native(u64),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SampleIdentity {
    pub canonical_source: String,
    pub content_fingerprint: String,
    pub decode_options_digest: String,
    pub loader_version: String,
    pub backend: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BufferPersistence {
    Ephemeral,
    Persistent,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BufferSpec {
    pub frames: u32,
    pub channels: u16,
    pub sample_format: String,
    pub backend: String,
    pub persistence: BufferPersistence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferShapePolicy {
    PreserveCompatible,
    Clear,
    CopyOverlap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferStageAction {
    Reused,
    Created,
    Cleared,
    CopiedOverlap,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SfzIdentity {
    pub canonical_root: String,
    pub transitive_fingerprint: String,
    pub load_options_digest: String,
    pub loader_version: String,
    pub backend: String,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum ResourceIdentity {
    Sample(SampleIdentity),
    Buffer(BufferSpec),
    Sfz(SfzIdentity),
}

impl ResourceIdentity {
    const fn kind(&self) -> ResourceKind {
        match self {
            Self::Sample(_) => ResourceKind::Sample,
            Self::Buffer(_) => ResourceKind::Buffer,
            Self::Sfz(_) => ResourceKind::Sfz,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagedResource {
    pub generation: ResourceGeneration,
    pub reused: bool,
    pub buffer_action: Option<BufferStageAction>,
}

/// Correlated proof that every backend reader preceding `token` is quiescent.
///
/// Construction is crate-private so a caller cannot turn a non-empty string
/// into resource-release authority. The native generation coordinator creates
/// this value only after comparing the received acknowledgement with the
/// exact reserved correlation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuiescenceProof {
    backend: String,
    token: String,
}

impl QuiescenceProof {
    pub(crate) fn confirmed(
        backend: impl Into<String>,
        token: impl Into<String>,
    ) -> Result<Self, ResourceError> {
        let backend = backend.into();
        let token = token.into();
        if backend.is_empty() || token.is_empty() {
            return Err(ResourceError::QuiescenceNotConfirmed);
        }
        Ok(Self { backend, token })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FreeBatch {
    generation: ResourceGeneration,
    attempt: u64,
    physical: Vec<PhysicalResourceId>,
    quiescence: QuiescenceProof,
}

impl FreeBatch {
    pub(crate) fn physical(&self) -> &[PhysicalResourceId] {
        &self.physical
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PhysicalFreeConfirmation {
    Confirmed {
        physical: PhysicalResourceId,
        backend: String,
        token: String,
    },
    Uncertain {
        physical: PhysicalResourceId,
        detail: String,
    },
}

impl PhysicalFreeConfirmation {
    const fn physical(&self) -> PhysicalResourceId {
        match self {
            Self::Confirmed { physical, .. } | Self::Uncertain { physical, .. } => *physical,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceRetirement {
    generations: BTreeSet<ResourceGeneration>,
}

impl ResourceRetirement {
    #[must_use]
    pub fn generations(&self) -> impl ExactSizeIterator<Item = ResourceGeneration> + '_ {
        self.generations.iter().copied()
    }

    pub(crate) fn from_generations(
        generations: impl IntoIterator<Item = ResourceGeneration>,
    ) -> Self {
        Self {
            generations: generations.into_iter().collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationHealth {
    Live,
    FreePending,
    Quarantined,
    Freed,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceAccounting {
    pub logical_bindings: usize,
    pub live_generations: usize,
    pub free_pending_generations: usize,
    pub quarantined_generations: usize,
    pub freed_generations: usize,
    pub live_physical: usize,
    pub quarantined_physical: usize,
    pub freed_physical: usize,
    pub staged_claims: usize,
    pub reader_claims: usize,
    pub dependency_claims: usize,
}

#[derive(Clone)]
pub struct ResourceManager {
    inner: Arc<Mutex<ResourceInner>>,
}

impl fmt::Debug for ResourceManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResourceManager")
            .field("accounting", &self.accounting())
            .finish()
    }
}

#[derive(Debug, Default)]
struct ResourceInner {
    next_generation: u64,
    next_stage: u64,
    next_reader: u64,
    next_free_attempt: u64,
    bindings: BTreeMap<LogicalResource, ResourceGeneration>,
    generations: BTreeMap<ResourceGeneration, GenerationRecord>,
    stages: BTreeMap<ResourceStage, StageRecord>,
    physical_owner: BTreeMap<PhysicalResourceId, ResourceGeneration>,
}

#[derive(Debug)]
struct GenerationRecord {
    identity: ResourceIdentity,
    physical: BTreeSet<PhysicalResourceId>,
    committed_bindings: BTreeSet<LogicalResource>,
    staged_claims: BTreeSet<(ResourceStage, LogicalResource)>,
    cleanup_stages: BTreeSet<ResourceStage>,
    readers: BTreeSet<u64>,
    dependencies: BTreeSet<ResourceGeneration>,
    dependents: BTreeSet<ResourceGeneration>,
    health: GenerationHealth,
    cleanup_only: bool,
    free_attempt: Option<u64>,
    freed_physical: usize,
}

#[derive(Debug, Default)]
struct StageRecord {
    claims: BTreeMap<LogicalResource, ResourceGeneration>,
    expected_bindings: BTreeMap<LogicalResource, Option<ResourceGeneration>>,
    removals: BTreeMap<LogicalResource, ResourceGeneration>,
    cleanup_only: BTreeSet<ResourceGeneration>,
}

pub struct ReaderLease {
    manager: ResourceManager,
    reader: u64,
    generation: ResourceGeneration,
    generations: Vec<ResourceGeneration>,
    physical: Vec<PhysicalResourceId>,
}

impl ReaderLease {
    #[must_use]
    pub const fn generation(&self) -> ResourceGeneration {
        self.generation
    }

    #[must_use]
    pub fn physical(&self) -> &[PhysicalResourceId] {
        &self.physical
    }
}

impl fmt::Debug for ReaderLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReaderLease")
            .field("reader", &self.reader)
            .field("generation", &self.generation)
            .field("physical", &self.physical)
            .finish()
    }
}

impl Drop for ReaderLease {
    fn drop(&mut self) {
        self.manager.release_reader(&self.generations, self.reader);
    }
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ResourceInner::default())),
        }
    }

    pub fn begin_stage(&self) -> Result<ResourceStage, ResourceError> {
        let mut inner = self.inner.lock();
        inner.next_stage = inner
            .next_stage
            .checked_add(1)
            .ok_or(ResourceError::Exhausted("resource stage"))?;
        let stage = ResourceStage(inner.next_stage);
        inner.stages.insert(stage, StageRecord::default());
        Ok(stage)
    }

    pub fn stage_sample(
        &self,
        stage: ResourceStage,
        logical: LogicalResource,
        identity: SampleIdentity,
        physical: impl IntoIterator<Item = PhysicalResourceId>,
    ) -> Result<StagedResource, ResourceError> {
        if logical.kind != ResourceKind::Sample {
            return Err(ResourceError::KindMismatch);
        }
        validate_sample_identity(&identity)?;
        self.stage_immutable(
            stage,
            logical,
            ResourceIdentity::Sample(identity),
            [],
            physical,
        )
    }

    pub fn stage_sfz(
        &self,
        stage: ResourceStage,
        logical: LogicalResource,
        identity: SfzIdentity,
        physical: impl IntoIterator<Item = PhysicalResourceId>,
    ) -> Result<StagedResource, ResourceError> {
        self.stage_sfz_with_dependencies(stage, logical, identity, [], physical)
    }

    pub fn stage_sfz_with_dependencies(
        &self,
        stage: ResourceStage,
        logical: LogicalResource,
        identity: SfzIdentity,
        dependencies: impl IntoIterator<Item = ResourceGeneration>,
        physical: impl IntoIterator<Item = PhysicalResourceId>,
    ) -> Result<StagedResource, ResourceError> {
        if logical.kind != ResourceKind::Sfz {
            return Err(ResourceError::KindMismatch);
        }
        validate_sfz_identity(&identity)?;
        self.stage_immutable(
            stage,
            logical,
            ResourceIdentity::Sfz(identity),
            dependencies,
            physical,
        )
    }

    fn stage_immutable(
        &self,
        stage: ResourceStage,
        logical: LogicalResource,
        identity: ResourceIdentity,
        dependencies: impl IntoIterator<Item = ResourceGeneration>,
        physical: impl IntoIterator<Item = PhysicalResourceId>,
    ) -> Result<StagedResource, ResourceError> {
        let mut inner = self.inner.lock();
        inner.ensure_claim_slot(stage, &logical)?;
        let dependencies = dependencies.into_iter().collect::<BTreeSet<_>>();
        let physical = physical.into_iter().collect::<BTreeSet<_>>();
        inner.validate_stage_dependencies(stage, &dependencies)?;
        if let Some(generation) = inner.find_reusable(&identity, &dependencies) {
            if !physical.is_empty() {
                inner.record_cleanup_generation(stage, identity, physical)?;
                return Err(ResourceError::UnexpectedPhysicalForReuse);
            }
            inner.add_stage_claim(stage, logical, generation)?;
            return Ok(StagedResource {
                generation,
                reused: true,
                buffer_action: None,
            });
        }
        let generation = match inner.create_generation(
            identity.clone(),
            dependencies,
            physical.clone(),
            false,
        ) {
            Ok(generation) => generation,
            Err(error) => {
                if !physical.is_empty() && !matches!(error, ResourceError::PhysicalResourceInUse(_))
                {
                    inner.record_cleanup_generation(stage, identity, physical)?;
                }
                return Err(error);
            }
        };
        inner.add_stage_claim(stage, logical, generation)?;
        Ok(StagedResource {
            generation,
            reused: false,
            buffer_action: None,
        })
    }

    pub fn stage_buffer(
        &self,
        stage: ResourceStage,
        logical: LogicalResource,
        spec: BufferSpec,
        policy: BufferShapePolicy,
        physical: impl IntoIterator<Item = PhysicalResourceId>,
    ) -> Result<StagedResource, ResourceError> {
        if logical.kind != ResourceKind::Buffer {
            return Err(ResourceError::KindMismatch);
        }
        validate_buffer_spec(&spec)?;
        let mut inner = self.inner.lock();
        inner.ensure_claim_slot(stage, &logical)?;
        let physical = physical.into_iter().collect::<BTreeSet<_>>();
        let previous = inner.bindings.get(&logical).copied();
        if let Some(generation) = previous {
            let (health, identity) = inner
                .generations
                .get(&generation)
                .map(|record| (record.health, record.identity.clone()))
                .ok_or(ResourceError::UnknownGeneration(generation))?;
            if health == GenerationHealth::Live
                && identity == ResourceIdentity::Buffer(spec.clone())
            {
                if !physical.is_empty() {
                    inner.record_cleanup_generation(
                        stage,
                        ResourceIdentity::Buffer(spec),
                        physical,
                    )?;
                    return Err(ResourceError::UnexpectedPhysicalForReuse);
                }
                inner.add_stage_claim(stage, logical, generation)?;
                return Ok(StagedResource {
                    generation,
                    reused: true,
                    buffer_action: Some(BufferStageAction::Reused),
                });
            }
            if policy == BufferShapePolicy::PreserveCompatible {
                if let ResourceIdentity::Buffer(previous_spec) = identity {
                    if previous_spec != spec {
                        if !physical.is_empty() {
                            inner.record_cleanup_generation(
                                stage,
                                ResourceIdentity::Buffer(spec.clone()),
                                physical,
                            )?;
                        }
                        return Err(ResourceError::BufferReplacementPolicyRequired {
                            previous: previous_spec,
                            next: spec,
                        });
                    }
                }
            }
        }
        let action = match policy {
            BufferShapePolicy::PreserveCompatible => BufferStageAction::Created,
            BufferShapePolicy::Clear => BufferStageAction::Cleared,
            BufferShapePolicy::CopyOverlap => BufferStageAction::CopiedOverlap,
        };
        let identity = ResourceIdentity::Buffer(spec);
        let generation = match inner.create_generation(
            identity.clone(),
            BTreeSet::new(),
            physical.clone(),
            false,
        ) {
            Ok(generation) => generation,
            Err(error) => {
                if !physical.is_empty() && !matches!(error, ResourceError::PhysicalResourceInUse(_))
                {
                    inner.record_cleanup_generation(stage, identity, physical)?;
                }
                return Err(error);
            }
        };
        inner.add_stage_claim(stage, logical, generation)?;
        Ok(StagedResource {
            generation,
            reused: false,
            buffer_action: Some(action),
        })
    }

    /// Stages removal of the binding observed when this stage was built. The
    /// committed binding remains authoritative until `commit_stage` validates
    /// that it is still the same generation and removes it atomically.
    pub fn stage_remove(
        &self,
        stage: ResourceStage,
        logical: LogicalResource,
    ) -> Result<ResourceGeneration, ResourceError> {
        let mut inner = self.inner.lock();
        inner.ensure_claim_slot(stage, &logical)?;
        let generation = inner
            .bindings
            .get(&logical)
            .copied()
            .ok_or_else(|| ResourceError::Unbound(logical.clone()))?;
        let record = inner
            .stages
            .get_mut(&stage)
            .ok_or(ResourceError::UnknownStage(stage))?;
        record
            .expected_bindings
            .insert(logical.clone(), Some(generation));
        record.removals.insert(logical, generation);
        Ok(generation)
    }

    /// Records physical allocations made before an SFZ transitive load failed.
    /// They can never be committed or reused and become eligible for exact
    /// cleanup when the failed stage is discarded.
    pub fn record_failed_sfz_stage(
        &self,
        stage: ResourceStage,
        identity: SfzIdentity,
        physical: impl IntoIterator<Item = PhysicalResourceId>,
    ) -> Result<ResourceGeneration, ResourceError> {
        validate_sfz_identity(&identity)?;
        let mut inner = self.inner.lock();
        if !inner.stages.contains_key(&stage) {
            return Err(ResourceError::UnknownStage(stage));
        }
        let generation = inner.create_generation(
            ResourceIdentity::Sfz(identity),
            BTreeSet::new(),
            physical,
            true,
        )?;
        inner
            .stages
            .get_mut(&stage)
            .ok_or(ResourceError::UnknownStage(stage))?
            .cleanup_only
            .insert(generation);
        inner
            .generations
            .get_mut(&generation)
            .expect("failed SFZ generation was inserted")
            .cleanup_stages
            .insert(stage);
        Ok(generation)
    }

    pub fn prepare_commit(&self, stage: ResourceStage) -> Result<(), ResourceError> {
        let inner = self.inner.lock();
        let record = inner
            .stages
            .get(&stage)
            .ok_or(ResourceError::UnknownStage(stage))?;
        inner.validate_stage(record)
    }

    pub fn snapshot_stage(
        &self,
        stage: ResourceStage,
    ) -> Result<ResourceStageSnapshot, ResourceError> {
        let inner = self.inner.lock();
        let record = inner
            .stages
            .get(&stage)
            .ok_or(ResourceError::UnknownStage(stage))?;
        Ok(ResourceStageSnapshot {
            stage,
            digest: inner.stage_digest(stage, record)?,
            claims: record.claims.clone(),
            removals: record.removals.clone(),
            cleanup_generations: inner.stage_cleanup_generations(stage, record)?,
        })
    }

    pub fn prepare_snapshot(&self, snapshot: &ResourceStageSnapshot) -> Result<(), ResourceError> {
        let inner = self.inner.lock();
        let record = inner
            .stages
            .get(&snapshot.stage)
            .ok_or(ResourceError::UnknownStage(snapshot.stage))?;
        let actual = inner.stage_digest(snapshot.stage, record)?;
        if actual != snapshot.digest {
            return Err(ResourceError::StageSnapshotChanged {
                expected: snapshot.digest.clone(),
                actual,
            });
        }
        inner.validate_stage(record)
    }

    pub fn commit_snapshot(
        &self,
        snapshot: &ResourceStageSnapshot,
    ) -> Result<ResourceRetirement, ResourceError> {
        self.commit_stage_checked(snapshot.stage, Some(&snapshot.digest))
    }

    pub fn commit_stage(&self, stage: ResourceStage) -> Result<ResourceRetirement, ResourceError> {
        self.commit_stage_checked(stage, None)
    }

    fn commit_stage_checked(
        &self,
        stage: ResourceStage,
        expected_digest: Option<&str>,
    ) -> Result<ResourceRetirement, ResourceError> {
        let mut inner = self.inner.lock();
        if let Some(expected) = expected_digest {
            let record = inner
                .stages
                .get(&stage)
                .ok_or(ResourceError::UnknownStage(stage))?;
            let actual = inner.stage_digest(stage, record)?;
            if actual != expected {
                return Err(ResourceError::StageSnapshotChanged {
                    expected: expected.to_string(),
                    actual,
                });
            }
        }
        let staged = inner
            .stages
            .remove(&stage)
            .ok_or(ResourceError::UnknownStage(stage))?;
        if let Err(error) = inner.validate_stage(&staged) {
            inner.stages.insert(stage, staged);
            return Err(error);
        }
        let mut retired = BTreeSet::new();
        for (logical, expected) in staged.removals {
            let removed = inner.bindings.remove(&logical);
            debug_assert_eq!(removed, Some(expected));
            inner
                .generations
                .get_mut(&expected)
                .expect("validated removal generation")
                .committed_bindings
                .remove(&logical);
            retired.insert(expected);
        }
        for (logical, generation) in staged.claims {
            if let Some(previous) = inner.bindings.insert(logical.clone(), generation) {
                if previous != generation {
                    inner
                        .generations
                        .get_mut(&previous)
                        .expect("validated previous generation")
                        .committed_bindings
                        .remove(&logical);
                    retired.insert(previous);
                }
            }
            let record = inner
                .generations
                .get_mut(&generation)
                .expect("validated staged generation");
            record.staged_claims.remove(&(stage, logical.clone()));
            record.committed_bindings.insert(logical);
        }
        Ok(ResourceRetirement {
            generations: retired,
        })
    }

    pub fn discard_stage(&self, stage: ResourceStage) -> Result<ResourceRetirement, ResourceError> {
        let mut inner = self.inner.lock();
        let staged = inner
            .stages
            .remove(&stage)
            .ok_or(ResourceError::UnknownStage(stage))?;
        let mut retired = staged.cleanup_only;
        for generation in &retired {
            inner
                .generations
                .get_mut(generation)
                .ok_or(ResourceError::UnknownGeneration(*generation))?
                .cleanup_stages
                .remove(&stage);
        }
        for (logical, generation) in staged.claims {
            let record = inner
                .generations
                .get_mut(&generation)
                .ok_or(ResourceError::UnknownGeneration(generation))?;
            record.staged_claims.remove(&(stage, logical));
            if record.committed_bindings.is_empty() && record.staged_claims.is_empty() {
                retired.insert(generation);
            }
        }
        Ok(ResourceRetirement {
            generations: retired,
        })
    }

    /// Discard a stage whose graph activation became uncertain. Newly owned
    /// generations are quarantined instead of becoming freeable because the
    /// backend may still have readers in the unconfirmed graph. Reused
    /// generations simply lose the provisional claim; their committed claims
    /// continue to provide authority.
    pub(crate) fn quarantine_stage(
        &self,
        stage: ResourceStage,
    ) -> Result<ResourceRetirement, ResourceError> {
        let retirement = self.discard_stage(stage)?;
        let mut inner = self.inner.lock();
        let retired = retirement.generations().collect::<BTreeSet<_>>();
        for generation in retirement.generations() {
            let record = inner
                .generations
                .get(&generation)
                .ok_or(ResourceError::UnknownGeneration(generation))?;
            if !record.committed_bindings.is_empty()
                || !record.staged_claims.is_empty()
                || !record.cleanup_stages.is_empty()
                || !record.readers.is_empty()
                || record
                    .dependents
                    .iter()
                    .any(|dependent| !retired.contains(dependent))
            {
                return Err(ResourceError::GenerationClaimed(generation));
            }
        }
        for generation in retirement.generations() {
            let record = inner
                .generations
                .get_mut(&generation)
                .expect("quarantine generation was validated");
            if record.health == GenerationHealth::Live {
                record.health = GenerationHealth::Quarantined;
            }
        }
        Ok(retirement)
    }

    pub fn acquire(&self, logical: &LogicalResource) -> Result<ReaderLease, ResourceError> {
        let mut inner = self.inner.lock();
        let generation = inner
            .bindings
            .get(logical)
            .copied()
            .ok_or_else(|| ResourceError::Unbound(logical.clone()))?;
        let mut generations = vec![generation];
        let dependencies = inner
            .generations
            .get(&generation)
            .ok_or(ResourceError::UnknownGeneration(generation))?
            .dependencies
            .iter()
            .copied()
            .collect::<Vec<_>>();
        generations.extend(dependencies);
        for pinned in &generations {
            let record = inner
                .generations
                .get(pinned)
                .ok_or(ResourceError::UnknownGeneration(*pinned))?;
            if record.health != GenerationHealth::Live {
                return Err(ResourceError::GenerationNotLive(*pinned));
            }
        }
        inner.next_reader = inner
            .next_reader
            .checked_add(1)
            .ok_or(ResourceError::Exhausted("resource reader"))?;
        let reader = inner.next_reader;
        let mut physical = BTreeSet::new();
        for pinned in &generations {
            let record = inner
                .generations
                .get_mut(pinned)
                .expect("pinned generations were validated");
            record.readers.insert(reader);
            physical.extend(record.physical.iter().copied());
        }
        Ok(ReaderLease {
            manager: self.clone(),
            reader,
            generation,
            generations,
            physical: physical.into_iter().collect(),
        })
    }

    fn release_reader(&self, generations: &[ResourceGeneration], reader: u64) {
        let mut inner = self.inner.lock();
        for generation in generations {
            if let Some(record) = inner.generations.get_mut(generation) {
                record.readers.remove(&reader);
            }
        }
    }

    #[must_use]
    pub fn generation_for(&self, logical: &LogicalResource) -> Option<ResourceGeneration> {
        self.inner.lock().bindings.get(logical).copied()
    }

    #[must_use]
    pub fn stage_exists(&self, stage: ResourceStage) -> bool {
        self.inner.lock().stages.contains_key(&stage)
    }

    pub fn freeable_generations(&self) -> Vec<ResourceGeneration> {
        let inner = self.inner.lock();
        inner
            .generations
            .iter()
            .filter_map(|(generation, record)| {
                ResourceInner::is_freeable(record).then_some(*generation)
            })
            .collect()
    }

    pub(crate) fn freeable_retirement(&self) -> ResourceRetirement {
        ResourceRetirement::from_generations(self.freeable_generations())
    }

    pub(crate) fn freeable_from(&self, retirement: &ResourceRetirement) -> Vec<ResourceGeneration> {
        let inner = self.inner.lock();
        retirement
            .generations()
            .filter(|generation| {
                inner
                    .generations
                    .get(generation)
                    .is_some_and(ResourceInner::is_freeable)
            })
            .collect()
    }

    pub(crate) fn retirement_is_pending(&self, retirement: &ResourceRetirement) -> bool {
        let inner = self.inner.lock();
        retirement.generations().any(|generation| {
            inner.generations.get(&generation).is_some_and(|record| {
                matches!(
                    record.health,
                    GenerationHealth::Live | GenerationHealth::FreePending
                )
            })
        })
    }

    pub(crate) fn begin_free(
        &self,
        generation: ResourceGeneration,
        quiescence: QuiescenceProof,
    ) -> Result<FreeBatch, ResourceError> {
        let mut inner = self.inner.lock();
        inner.next_free_attempt = inner
            .next_free_attempt
            .checked_add(1)
            .ok_or(ResourceError::Exhausted("resource free attempt"))?;
        let attempt = inner.next_free_attempt;
        let record = inner
            .generations
            .get_mut(&generation)
            .ok_or(ResourceError::UnknownGeneration(generation))?;
        if record.health != GenerationHealth::Live {
            return Err(ResourceError::FreeAlreadyAttempted(generation));
        }
        if !record.committed_bindings.is_empty()
            || !record.staged_claims.is_empty()
            || !record.cleanup_stages.is_empty()
            || !record.readers.is_empty()
            || !record.dependents.is_empty()
        {
            return Err(ResourceError::GenerationClaimed(generation));
        }
        record.health = GenerationHealth::FreePending;
        record.free_attempt = Some(attempt);
        Ok(FreeBatch {
            generation,
            attempt,
            physical: record.physical.iter().copied().collect(),
            quiescence,
        })
    }

    pub(crate) fn finish_free(
        &self,
        batch: &FreeBatch,
        confirmations: Vec<PhysicalFreeConfirmation>,
    ) -> Result<(), ResourceError> {
        let mut inner = self.inner.lock();
        let expected = batch.physical.iter().copied().collect::<BTreeSet<_>>();
        let actual = confirmations
            .iter()
            .map(PhysicalFreeConfirmation::physical)
            .collect::<BTreeSet<_>>();
        if expected != actual || confirmations.len() != expected.len() {
            inner.quarantine_free_batch(batch)?;
            return Err(ResourceError::FreeConfirmationSetMismatch(batch.generation));
        }
        if confirmations.iter().any(|confirmation| {
            matches!(
                confirmation,
                PhysicalFreeConfirmation::Confirmed { backend, token, .. }
                    if backend.is_empty() || token.is_empty()
            )
        }) {
            inner.quarantine_free_batch(batch)?;
            return Err(ResourceError::Invalid(
                "confirmed free needs backend and correlation token".into(),
            ));
        }
        let (confirmed, dependencies, quarantined) = {
            let record = inner
                .generations
                .get_mut(&batch.generation)
                .ok_or(ResourceError::UnknownGeneration(batch.generation))?;
            if record.health != GenerationHealth::FreePending
                || record.free_attempt != Some(batch.attempt)
                || record.physical.iter().copied().collect::<Vec<_>>() != batch.physical
            {
                return Err(ResourceError::FreeBatchMismatch(batch.generation));
            }
            let mut confirmed = BTreeSet::new();
            let mut quarantined = false;
            for confirmation in confirmations {
                match confirmation {
                    PhysicalFreeConfirmation::Confirmed {
                        physical,
                        backend,
                        token,
                    } if !backend.is_empty() && !token.is_empty() => {
                        confirmed.insert(physical);
                    }
                    PhysicalFreeConfirmation::Confirmed { .. } => unreachable!(
                        "invalid confirmations were rejected before mutating accounting"
                    ),
                    PhysicalFreeConfirmation::Uncertain { .. } => {
                        quarantined = true;
                    }
                }
            }
            for physical in &confirmed {
                record.physical.remove(physical);
            }
            record.freed_physical += confirmed.len();
            record.free_attempt = None;
            let dependencies = if quarantined {
                record.health = GenerationHealth::Quarantined;
                BTreeSet::new()
            } else {
                record.health = GenerationHealth::Freed;
                std::mem::take(&mut record.dependencies)
            };
            (confirmed, dependencies, quarantined)
        };
        for id in confirmed {
            inner.physical_owner.remove(&id);
        }
        for dependency in dependencies {
            inner
                .generations
                .get_mut(&dependency)
                .ok_or(ResourceError::UnknownGeneration(dependency))?
                .dependents
                .remove(&batch.generation);
        }
        debug_assert_eq!(
            inner
                .generations
                .get(&batch.generation)
                .expect("free generation retained")
                .health
                == GenerationHealth::Quarantined,
            quarantined
        );
        Ok(())
    }

    /// Retains all physical identifiers when quiescence could not be proven.
    /// A quarantined generation is never considered reusable or freeable.
    pub fn quarantine(&self, generation: ResourceGeneration) -> Result<(), ResourceError> {
        let mut inner = self.inner.lock();
        let record = inner
            .generations
            .get_mut(&generation)
            .ok_or(ResourceError::UnknownGeneration(generation))?;
        if record.health != GenerationHealth::Live {
            return Err(ResourceError::GenerationNotLive(generation));
        }
        if !record.committed_bindings.is_empty()
            || !record.staged_claims.is_empty()
            || !record.cleanup_stages.is_empty()
            || !record.readers.is_empty()
            || !record.dependents.is_empty()
        {
            return Err(ResourceError::GenerationClaimed(generation));
        }
        record.health = GenerationHealth::Quarantined;
        Ok(())
    }

    pub(crate) fn quarantine_retirement(
        &self,
        retirement: &ResourceRetirement,
    ) -> Result<(), ResourceError> {
        let mut inner = self.inner.lock();
        let retired = retirement.generations().collect::<BTreeSet<_>>();
        for generation in retirement.generations() {
            let record = inner
                .generations
                .get(&generation)
                .ok_or(ResourceError::UnknownGeneration(generation))?;
            if !record.committed_bindings.is_empty()
                || !record.staged_claims.is_empty()
                || !record.cleanup_stages.is_empty()
                || !record.readers.is_empty()
                || record
                    .dependents
                    .iter()
                    .any(|dependent| !retired.contains(dependent))
            {
                return Err(ResourceError::GenerationClaimed(generation));
            }
        }
        for generation in retirement.generations() {
            let record = inner
                .generations
                .get_mut(&generation)
                .expect("retirement generation was validated");
            if record.health == GenerationHealth::Live {
                record.health = GenerationHealth::Quarantined;
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn health(&self, generation: ResourceGeneration) -> Option<GenerationHealth> {
        self.inner
            .lock()
            .generations
            .get(&generation)
            .map(|record| record.health)
    }

    #[must_use]
    pub fn accounting(&self) -> ResourceAccounting {
        let inner = self.inner.lock();
        let mut accounting = ResourceAccounting {
            logical_bindings: inner.bindings.len(),
            ..ResourceAccounting::default()
        };
        for record in inner.generations.values() {
            accounting.staged_claims += record.staged_claims.len();
            accounting.staged_claims += record.cleanup_stages.len();
            accounting.reader_claims += record.readers.len();
            accounting.dependency_claims += record.dependencies.len();
            match record.health {
                GenerationHealth::Live => {
                    accounting.live_generations += 1;
                    accounting.live_physical += record.physical.len();
                }
                GenerationHealth::FreePending => {
                    accounting.free_pending_generations += 1;
                    accounting.live_physical += record.physical.len();
                }
                GenerationHealth::Quarantined => {
                    accounting.quarantined_generations += 1;
                    accounting.quarantined_physical += record.physical.len();
                }
                GenerationHealth::Freed => accounting.freed_generations += 1,
            }
            accounting.freed_physical += record.freed_physical;
        }
        accounting
    }
}

impl ResourceInner {
    fn validate_stage_dependencies(
        &self,
        stage: ResourceStage,
        dependencies: &BTreeSet<ResourceGeneration>,
    ) -> Result<(), ResourceError> {
        for dependency in dependencies {
            let record = self
                .generations
                .get(dependency)
                .ok_or(ResourceError::UnknownGeneration(*dependency))?;
            if record.health != GenerationHealth::Live {
                return Err(ResourceError::GenerationNotLive(*dependency));
            }
            if record.committed_bindings.is_empty()
                && !record
                    .staged_claims
                    .iter()
                    .any(|(claim_stage, _)| *claim_stage == stage)
            {
                return Err(ResourceError::Invalid(
                    "an SFZ dependency must be committed or claimed by the same stage".into(),
                ));
            }
        }
        Ok(())
    }

    fn stage_cleanup_generations(
        &self,
        stage: ResourceStage,
        record: &StageRecord,
    ) -> Result<BTreeSet<ResourceGeneration>, ResourceError> {
        let mut generations = record.cleanup_only.clone();
        for generation in record.claims.values() {
            let generation_record = self
                .generations
                .get(generation)
                .ok_or(ResourceError::UnknownGeneration(*generation))?;
            if generation_record.committed_bindings.is_empty()
                && generation_record
                    .staged_claims
                    .iter()
                    .all(|(claim_stage, _)| *claim_stage == stage)
            {
                generations.insert(*generation);
            }
        }
        Ok(generations)
    }

    fn quarantine_free_batch(&mut self, batch: &FreeBatch) -> Result<(), ResourceError> {
        let record = self
            .generations
            .get_mut(&batch.generation)
            .ok_or(ResourceError::UnknownGeneration(batch.generation))?;
        if record.health != GenerationHealth::FreePending
            || record.free_attempt != Some(batch.attempt)
            || record.physical.iter().copied().collect::<Vec<_>>() != batch.physical
        {
            return Err(ResourceError::FreeBatchMismatch(batch.generation));
        }
        record.health = GenerationHealth::Quarantined;
        record.free_attempt = None;
        Ok(())
    }

    fn is_freeable(record: &GenerationRecord) -> bool {
        record.health == GenerationHealth::Live
            && record.committed_bindings.is_empty()
            && record.staged_claims.is_empty()
            && record.cleanup_stages.is_empty()
            && record.readers.is_empty()
            && record.dependents.is_empty()
    }

    fn stage_digest(
        &self,
        stage: ResourceStage,
        record: &StageRecord,
    ) -> Result<String, ResourceError> {
        fn text(hasher: &mut Sha256, value: &str) {
            hasher.update(
                u64::try_from(value.len())
                    .expect("resource identity length fits u64")
                    .to_be_bytes(),
            );
            hasher.update(value.as_bytes());
        }
        fn logical(hasher: &mut Sha256, value: &LogicalResource) {
            hasher.update([value.kind as u8]);
            text(hasher, &value.address);
        }
        fn identity(hasher: &mut Sha256, value: &ResourceIdentity) {
            hasher.update([value.kind() as u8]);
            match value {
                ResourceIdentity::Sample(value) => {
                    text(hasher, &value.canonical_source);
                    text(hasher, &value.content_fingerprint);
                    text(hasher, &value.decode_options_digest);
                    text(hasher, &value.loader_version);
                    text(hasher, &value.backend);
                }
                ResourceIdentity::Buffer(value) => {
                    hasher.update(value.frames.to_be_bytes());
                    hasher.update(value.channels.to_be_bytes());
                    text(hasher, &value.sample_format);
                    text(hasher, &value.backend);
                    hasher.update([value.persistence as u8]);
                }
                ResourceIdentity::Sfz(value) => {
                    text(hasher, &value.canonical_root);
                    text(hasher, &value.transitive_fingerprint);
                    text(hasher, &value.load_options_digest);
                    text(hasher, &value.loader_version);
                    text(hasher, &value.backend);
                }
            }
        }
        fn generation(hasher: &mut Sha256, id: ResourceGeneration, record: &GenerationRecord) {
            hasher.update(id.get().to_be_bytes());
            identity(hasher, &record.identity);
            for physical in &record.physical {
                match physical {
                    PhysicalResourceId::Buffer(value) => {
                        hasher.update([0]);
                        hasher.update(value.to_be_bytes());
                    }
                    PhysicalResourceId::Native(value) => {
                        hasher.update([1]);
                        hasher.update(value.to_be_bytes());
                    }
                }
            }
            for dependency in &record.dependencies {
                hasher.update(dependency.get().to_be_bytes());
            }
            hasher.update([u8::from(record.cleanup_only)]);
        }

        let mut hasher = Sha256::new();
        hasher.update(b"vibelang.resource-stage.v1\0");
        hasher.update(stage.get().to_be_bytes());
        for (logical_resource, generation_id) in &record.claims {
            hasher.update([0]);
            logical(&mut hasher, logical_resource);
            let generation_record = self
                .generations
                .get(generation_id)
                .ok_or(ResourceError::UnknownGeneration(*generation_id))?;
            generation(&mut hasher, *generation_id, generation_record);
            hasher.update(
                record
                    .expected_bindings
                    .get(logical_resource)
                    .copied()
                    .flatten()
                    .map_or(0, ResourceGeneration::get)
                    .to_be_bytes(),
            );
        }
        for (logical_resource, generation_id) in &record.removals {
            hasher.update([1]);
            logical(&mut hasher, logical_resource);
            hasher.update(generation_id.get().to_be_bytes());
        }
        for generation_id in &record.cleanup_only {
            hasher.update([2]);
            let generation_record = self
                .generations
                .get(generation_id)
                .ok_or(ResourceError::UnknownGeneration(*generation_id))?;
            generation(&mut hasher, *generation_id, generation_record);
        }
        Ok(format!("sha256:{:x}", hasher.finalize()))
    }

    fn ensure_claim_slot(
        &self,
        stage: ResourceStage,
        logical: &LogicalResource,
    ) -> Result<(), ResourceError> {
        let record = self
            .stages
            .get(&stage)
            .ok_or(ResourceError::UnknownStage(stage))?;
        if record.claims.contains_key(logical) {
            return Err(ResourceError::DuplicateStageClaim(logical.clone()));
        }
        if record.removals.contains_key(logical) {
            return Err(ResourceError::DuplicateStageClaim(logical.clone()));
        }
        Ok(())
    }

    fn find_reusable(
        &self,
        identity: &ResourceIdentity,
        dependencies: &BTreeSet<ResourceGeneration>,
    ) -> Option<ResourceGeneration> {
        self.generations.iter().find_map(|(generation, record)| {
            (record.health == GenerationHealth::Live
                && !record.cleanup_only
                && &record.identity == identity
                && &record.dependencies == dependencies)
                .then_some(*generation)
        })
    }

    fn validate_stage(&self, record: &StageRecord) -> Result<(), ResourceError> {
        if !record.cleanup_only.is_empty() {
            return Err(ResourceError::FailedStageCannotCommit);
        }
        for generation in record.claims.values() {
            if self
                .generations
                .get(generation)
                .is_none_or(|generation| generation.health != GenerationHealth::Live)
            {
                return Err(ResourceError::GenerationNotLive(*generation));
            }
        }
        for (logical, expected) in &record.expected_bindings {
            let actual = self.bindings.get(logical).copied();
            if actual != *expected {
                return Err(ResourceError::BindingChanged {
                    logical: logical.clone(),
                    expected: *expected,
                    actual,
                });
            }
            if let Some(generation) = expected {
                if !self.generations.contains_key(generation) {
                    return Err(ResourceError::UnknownGeneration(*generation));
                }
            }
        }
        Ok(())
    }

    fn create_generation(
        &mut self,
        identity: ResourceIdentity,
        dependencies: BTreeSet<ResourceGeneration>,
        physical: impl IntoIterator<Item = PhysicalResourceId>,
        cleanup_only: bool,
    ) -> Result<ResourceGeneration, ResourceError> {
        let physical = physical.into_iter().collect::<BTreeSet<_>>();
        if physical.is_empty() {
            return Err(ResourceError::Invalid(
                "a physical generation must contain at least one resource".into(),
            ));
        }
        if let Some(id) = physical
            .iter()
            .find(|id| self.physical_owner.contains_key(id))
        {
            return Err(ResourceError::PhysicalResourceInUse(*id));
        }
        for dependency in &dependencies {
            let record = self
                .generations
                .get(dependency)
                .ok_or(ResourceError::UnknownGeneration(*dependency))?;
            if record.health != GenerationHealth::Live {
                return Err(ResourceError::GenerationNotLive(*dependency));
            }
            if record.identity.kind() != ResourceKind::Sample {
                return Err(ResourceError::InvalidDependencyKind(*dependency));
            }
        }
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(ResourceError::Exhausted("resource generation"))?;
        let generation = ResourceGeneration(self.next_generation);
        for id in &physical {
            self.physical_owner.insert(*id, generation);
        }
        self.generations.insert(
            generation,
            GenerationRecord {
                identity,
                physical,
                committed_bindings: BTreeSet::new(),
                staged_claims: BTreeSet::new(),
                cleanup_stages: BTreeSet::new(),
                readers: BTreeSet::new(),
                dependencies: dependencies.clone(),
                dependents: BTreeSet::new(),
                health: GenerationHealth::Live,
                cleanup_only,
                free_attempt: None,
                freed_physical: 0,
            },
        );
        for dependency in dependencies {
            self.generations
                .get_mut(&dependency)
                .expect("dependencies were validated")
                .dependents
                .insert(generation);
        }
        Ok(generation)
    }

    fn record_cleanup_generation(
        &mut self,
        stage: ResourceStage,
        identity: ResourceIdentity,
        physical: BTreeSet<PhysicalResourceId>,
    ) -> Result<ResourceGeneration, ResourceError> {
        let generation = self.create_generation(identity, BTreeSet::new(), physical, true)?;
        self.stages
            .get_mut(&stage)
            .ok_or(ResourceError::UnknownStage(stage))?
            .cleanup_only
            .insert(generation);
        self.generations
            .get_mut(&generation)
            .expect("cleanup generation was inserted")
            .cleanup_stages
            .insert(stage);
        Ok(generation)
    }

    fn add_stage_claim(
        &mut self,
        stage: ResourceStage,
        logical: LogicalResource,
        generation: ResourceGeneration,
    ) -> Result<(), ResourceError> {
        let expected = self.bindings.get(&logical).copied();
        self.stages
            .get_mut(&stage)
            .ok_or(ResourceError::UnknownStage(stage))?
            .claims
            .insert(logical.clone(), generation);
        self.stages
            .get_mut(&stage)
            .expect("stage was validated")
            .expected_bindings
            .insert(logical.clone(), expected);
        self.generations
            .get_mut(&generation)
            .ok_or(ResourceError::UnknownGeneration(generation))?
            .staged_claims
            .insert((stage, logical));
        Ok(())
    }
}

fn validate_nonempty_identity_field(field: &'static str, value: &str) -> Result<(), ResourceError> {
    if value.trim() != value || value.is_empty() {
        return Err(ResourceError::Invalid(format!(
            "resource identity field {field} must be non-empty and canonical"
        )));
    }
    Ok(())
}

fn validate_sample_identity(identity: &SampleIdentity) -> Result<(), ResourceError> {
    validate_nonempty_identity_field("canonical_source", &identity.canonical_source)?;
    validate_nonempty_identity_field("content_fingerprint", &identity.content_fingerprint)?;
    validate_nonempty_identity_field("decode_options_digest", &identity.decode_options_digest)?;
    validate_nonempty_identity_field("loader_version", &identity.loader_version)?;
    validate_nonempty_identity_field("backend", &identity.backend)
}

fn validate_sfz_identity(identity: &SfzIdentity) -> Result<(), ResourceError> {
    validate_nonempty_identity_field("canonical_root", &identity.canonical_root)?;
    validate_nonempty_identity_field("transitive_fingerprint", &identity.transitive_fingerprint)?;
    validate_nonempty_identity_field("load_options_digest", &identity.load_options_digest)?;
    validate_nonempty_identity_field("loader_version", &identity.loader_version)?;
    validate_nonempty_identity_field("backend", &identity.backend)
}

fn validate_buffer_spec(spec: &BufferSpec) -> Result<(), ResourceError> {
    if spec.frames == 0 || spec.channels == 0 {
        return Err(ResourceError::Invalid(
            "buffer frames and channels must be greater than zero".into(),
        ));
    }
    validate_nonempty_identity_field("sample_format", &spec.sample_format)?;
    validate_nonempty_identity_field("backend", &spec.backend)
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum ResourceError {
    #[error("invalid resource state: {0}")]
    Invalid(String),
    #[error("resource counter exhausted: {0}")]
    Exhausted(&'static str),
    #[error("logical resource kind does not match the staging operation")]
    KindMismatch,
    #[error("unknown resource stage {0:?}")]
    UnknownStage(ResourceStage),
    #[error("unknown resource generation {0:?}")]
    UnknownGeneration(ResourceGeneration),
    #[error("duplicate staged claim for {0:?}")]
    DuplicateStageClaim(LogicalResource),
    #[error("physical resource {0:?} is still retained by another generation")]
    PhysicalResourceInUse(PhysicalResourceId),
    #[error("physical allocations must be omitted when an immutable generation is reused")]
    UnexpectedPhysicalForReuse,
    #[error("buffer specification changed from {previous:?} to {next:?} without a clear or copy-overlap policy")]
    BufferReplacementPolicyRequired {
        previous: BufferSpec,
        next: BufferSpec,
    },
    #[error("resource generation {0:?} is not a Sample and cannot back an SFZ generation")]
    InvalidDependencyKind(ResourceGeneration),
    #[error("a stage containing failed transitive allocations cannot commit")]
    FailedStageCannotCommit,
    #[error("resource generation {0:?} is not live")]
    GenerationNotLive(ResourceGeneration),
    #[error("resource stage changed after planning: expected {expected}, actual {actual}")]
    StageSnapshotChanged { expected: String, actual: String },
    #[error("logical resource is unbound: {0:?}")]
    Unbound(LogicalResource),
    #[error("logical resource binding changed after staging for {logical:?}: expected {expected:?}, actual {actual:?}")]
    BindingChanged {
        logical: LogicalResource,
        expected: Option<ResourceGeneration>,
        actual: Option<ResourceGeneration>,
    },
    #[error("resource generation {0:?} is still claimed")]
    GenerationClaimed(ResourceGeneration),
    #[error("resource generation {0:?} already had a free attempt")]
    FreeAlreadyAttempted(ResourceGeneration),
    #[error("resource free requires a correlated quiescence confirmation")]
    QuiescenceNotConfirmed,
    #[error("free completion does not match the retained batch for {0:?}")]
    FreeBatchMismatch(ResourceGeneration),
    #[error("free completion does not account for every physical resource exactly once for {0:?}")]
    FreeConfirmationSetMismatch(ResourceGeneration),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logical(kind: ResourceKind, name: &str) -> LogicalResource {
        LogicalResource::new(kind, name).expect("test logical resource")
    }

    fn sample(path: &str, fingerprint: &str) -> SampleIdentity {
        SampleIdentity {
            canonical_source: path.into(),
            content_fingerprint: fingerprint.into(),
            decode_options_digest: "decode-v1".into(),
            loader_version: "loader-v1".into(),
            backend: "mock".into(),
        }
    }

    fn buffer(frames: u32, channels: u16) -> BufferSpec {
        BufferSpec {
            frames,
            channels,
            sample_format: "f32".into(),
            backend: "mock".into(),
            persistence: BufferPersistence::Ephemeral,
        }
    }

    fn sfz(fingerprint: &str) -> SfzIdentity {
        SfzIdentity {
            canonical_root: "/kit/root.sfz".into(),
            transitive_fingerprint: fingerprint.into(),
            load_options_digest: "options-v1".into(),
            loader_version: "loader-v1".into(),
            backend: "mock".into(),
        }
    }

    fn confirmed() -> QuiescenceProof {
        QuiescenceProof::confirmed("mock", "sync-9").expect("quiescence")
    }

    fn confirmed_free(batch: &FreeBatch) -> Vec<PhysicalFreeConfirmation> {
        batch
            .physical()
            .iter()
            .copied()
            .map(|physical| PhysicalFreeConfirmation::Confirmed {
                physical,
                backend: "mock".into(),
                token: format!("free-{physical:?}"),
            })
            .collect()
    }

    #[test]
    fn sample_same_path_changed_content_gets_new_generation_and_old_binding_survives_stage() {
        let manager = ResourceManager::new();
        let key = logical(ResourceKind::Sample, "sample/kick");
        let first_stage = manager.begin_stage().expect("stage");
        let first = manager
            .stage_sample(
                first_stage,
                key.clone(),
                sample("/audio/kick.wav", "sha256:old"),
                [PhysicalResourceId::Buffer(1)],
            )
            .expect("first sample");
        manager.commit_stage(first_stage).expect("first commit");

        let replacement_stage = manager.begin_stage().expect("stage");
        let replacement = manager
            .stage_sample(
                replacement_stage,
                key.clone(),
                sample("/audio/kick.wav", "sha256:new"),
                [PhysicalResourceId::Buffer(2)],
            )
            .expect("replacement sample");
        assert_ne!(first.generation, replacement.generation);
        assert_eq!(manager.generation_for(&key), Some(first.generation));
        manager
            .commit_stage(replacement_stage)
            .expect("replacement commit");
        assert_eq!(manager.generation_for(&key), Some(replacement.generation));
        assert_eq!(manager.freeable_generations(), vec![first.generation]);
    }

    #[test]
    fn immutable_sample_identity_is_shared_without_duplicate_physical_ownership() {
        let manager = ResourceManager::new();
        let first_stage = manager.begin_stage().expect("stage");
        let first = manager
            .stage_sample(
                first_stage,
                logical(ResourceKind::Sample, "sample/a"),
                sample("/audio/shared.wav", "sha256:same"),
                [PhysicalResourceId::Buffer(1)],
            )
            .expect("sample");
        manager.commit_stage(first_stage).expect("commit");
        let second_stage = manager.begin_stage().expect("stage");
        let second = manager
            .stage_sample(
                second_stage,
                logical(ResourceKind::Sample, "sample/b"),
                sample("/audio/shared.wav", "sha256:same"),
                [],
            )
            .expect("shared sample");
        assert!(second.reused);
        assert_eq!(second.generation, first.generation);
        assert_eq!(manager.accounting().live_physical, 1);
    }

    #[test]
    fn discarding_a_reused_binding_creates_no_cleanup_or_retirement_debt() {
        let manager = ResourceManager::new();
        let first_stage = manager.begin_stage().expect("stage");
        let first = manager
            .stage_sample(
                first_stage,
                logical(ResourceKind::Sample, "sample/authority"),
                sample("/audio/authority.wav", "sha256:same"),
                [PhysicalResourceId::Buffer(2)],
            )
            .expect("sample");
        manager.commit_stage(first_stage).expect("commit");

        let reuse_stage = manager.begin_stage().expect("stage");
        let reused = manager
            .stage_sample(
                reuse_stage,
                logical(ResourceKind::Sample, "sample/reused"),
                sample("/audio/authority.wav", "sha256:same"),
                [],
            )
            .expect("reuse");
        assert_eq!(reused.generation, first.generation);
        let snapshot = manager.snapshot_stage(reuse_stage).expect("snapshot");
        assert!(!snapshot.requires_cleanup(reused.generation));
        assert_eq!(
            manager
                .discard_stage(reuse_stage)
                .expect("discard")
                .generations()
                .count(),
            0
        );
        assert_eq!(
            manager.health(first.generation),
            Some(GenerationHealth::Live)
        );
        assert_eq!(manager.accounting().logical_bindings, 1);
    }

    #[test]
    fn buffer_shape_change_requires_explicit_policy_and_failure_preserves_old_binding() {
        let manager = ResourceManager::new();
        let key = logical(ResourceKind::Buffer, "buffer/recording");
        let first_stage = manager.begin_stage().expect("stage");
        let first = manager
            .stage_buffer(
                first_stage,
                key.clone(),
                buffer(1024, 2),
                BufferShapePolicy::Clear,
                [PhysicalResourceId::Buffer(7)],
            )
            .expect("buffer");
        manager.commit_stage(first_stage).expect("commit");
        let replacement_stage = manager.begin_stage().expect("stage");
        assert!(matches!(
            manager.stage_buffer(
                replacement_stage,
                key.clone(),
                buffer(2048, 1),
                BufferShapePolicy::PreserveCompatible,
                [PhysicalResourceId::Buffer(8)],
            ),
            Err(ResourceError::BufferReplacementPolicyRequired { .. })
        ));
        assert_eq!(manager.generation_for(&key), Some(first.generation));
        let cleanup = manager
            .discard_stage(replacement_stage)
            .expect("discard failed allocation stage");
        let cleanup_generation = cleanup.generations().next().expect("cleanup generation");
        let cleanup_batch = manager
            .begin_free(cleanup_generation, confirmed())
            .expect("cleanup batch");
        manager
            .finish_free(&cleanup_batch, confirmed_free(&cleanup_batch))
            .expect("cleanup physical allocation");
        let replacement_stage = manager.begin_stage().expect("replacement stage");
        let replacement = manager
            .stage_buffer(
                replacement_stage,
                key.clone(),
                buffer(2048, 1),
                BufferShapePolicy::CopyOverlap,
                [PhysicalResourceId::Buffer(8)],
            )
            .expect("explicit replacement");
        assert_eq!(
            replacement.buffer_action,
            Some(BufferStageAction::CopiedOverlap)
        );
        manager.discard_stage(replacement_stage).expect("discard");
        assert_eq!(manager.generation_for(&key), Some(first.generation));
    }

    #[test]
    fn sfz_transitive_failure_cannot_commit_and_all_new_dependencies_are_cleanup_eligible() {
        let manager = ResourceManager::new();
        let stage = manager.begin_stage().expect("stage");
        let failed = manager
            .record_failed_sfz_stage(
                stage,
                sfz("sha256:partial"),
                [
                    PhysicalResourceId::Buffer(10),
                    PhysicalResourceId::Buffer(11),
                ],
            )
            .expect("failed allocation record");
        assert_eq!(
            manager.prepare_commit(stage),
            Err(ResourceError::FailedStageCannotCommit)
        );
        manager.discard_stage(stage).expect("discard");
        assert_eq!(manager.freeable_generations(), vec![failed]);
        let batch = manager.begin_free(failed, confirmed()).expect("free batch");
        assert_eq!(batch.physical.len(), 2);
    }

    #[test]
    fn immutable_reuse_rejects_unaccounted_physical_allocations() {
        let manager = ResourceManager::new();
        let first = manager.begin_stage().expect("stage");
        manager
            .stage_sample(
                first,
                logical(ResourceKind::Sample, "sample/a"),
                sample("/same.wav", "sha256:same"),
                [PhysicalResourceId::Buffer(12)],
            )
            .expect("sample");
        manager.commit_stage(first).expect("commit");
        let second = manager.begin_stage().expect("stage");
        assert_eq!(
            manager.stage_sample(
                second,
                logical(ResourceKind::Sample, "sample/b"),
                sample("/same.wav", "sha256:same"),
                [PhysicalResourceId::Buffer(13)],
            ),
            Err(ResourceError::UnexpectedPhysicalForReuse)
        );
        assert_eq!(manager.accounting().live_physical, 2);
        assert_eq!(manager.accounting().staged_claims, 1);
        let cleanup = manager
            .discard_stage(second)
            .expect("discard orphan allocation");
        assert_eq!(cleanup.generations().count(), 1);
    }

    #[test]
    fn staged_removal_and_snapshot_conflict_preserve_committed_authority() {
        let manager = ResourceManager::new();
        let key = logical(ResourceKind::Sample, "sample/remove");
        let first = manager.begin_stage().expect("stage");
        let original = manager
            .stage_sample(
                first,
                key.clone(),
                sample("/old.wav", "sha256:old"),
                [PhysicalResourceId::Buffer(14)],
            )
            .expect("sample");
        manager.commit_stage(first).expect("commit");

        let removal = manager.begin_stage().expect("stage");
        manager.stage_remove(removal, key.clone()).expect("remove");
        let snapshot = manager.snapshot_stage(removal).expect("snapshot");
        let replacement = manager.begin_stage().expect("stage");
        manager
            .stage_sample(
                replacement,
                key.clone(),
                sample("/new.wav", "sha256:new"),
                [PhysicalResourceId::Buffer(15)],
            )
            .expect("replacement");
        manager
            .commit_stage(replacement)
            .expect("replacement commit");
        assert!(matches!(
            manager.prepare_snapshot(&snapshot),
            Err(ResourceError::BindingChanged { .. })
        ));
        assert_ne!(manager.generation_for(&key), Some(original.generation));

        manager
            .discard_stage(removal)
            .expect("discard stale removal");
        let current_removal = manager.begin_stage().expect("stage");
        let current = manager
            .stage_remove(current_removal, key.clone())
            .expect("remove current");
        let current_snapshot = manager
            .snapshot_stage(current_removal)
            .expect("snapshot current");
        manager
            .commit_snapshot(&current_snapshot)
            .expect("commit removal");
        assert_eq!(manager.generation_for(&key), None);
        assert!(manager.freeable_generations().contains(&current));
    }

    #[test]
    fn sfz_generation_and_reader_pin_transitive_sample_dependencies() {
        let manager = ResourceManager::new();
        let sample_key = logical(ResourceKind::Sample, "sample/sfz-kick");
        let sample_stage = manager.begin_stage().expect("stage");
        let dependency = manager
            .stage_sample(
                sample_stage,
                sample_key.clone(),
                sample("/kick.wav", "sha256:kick"),
                [PhysicalResourceId::Buffer(16)],
            )
            .expect("sample");
        manager.commit_stage(sample_stage).expect("commit sample");

        let sfz_key = logical(ResourceKind::Sfz, "sfz/kit-dependencies");
        let sfz_stage = manager.begin_stage().expect("stage");
        let instrument = manager
            .stage_sfz_with_dependencies(
                sfz_stage,
                sfz_key.clone(),
                sfz("sha256:kit-dependencies"),
                [dependency.generation],
                [PhysicalResourceId::Native(17)],
            )
            .expect("sfz");
        manager.commit_stage(sfz_stage).expect("commit sfz");
        let reader = manager.acquire(&sfz_key).expect("reader");
        assert_eq!(
            reader.physical(),
            &[
                PhysicalResourceId::Buffer(16),
                PhysicalResourceId::Native(17)
            ]
        );

        let removals = manager.begin_stage().expect("stage");
        manager.stage_remove(removals, sfz_key).expect("remove sfz");
        manager
            .stage_remove(removals, sample_key)
            .expect("remove sample");
        manager.commit_stage(removals).expect("commit removals");
        assert!(manager.freeable_generations().is_empty());
        drop(reader);
        assert_eq!(manager.freeable_generations(), vec![instrument.generation]);
        let sfz_batch = manager
            .begin_free(instrument.generation, confirmed())
            .expect("sfz free");
        manager
            .finish_free(&sfz_batch, confirmed_free(&sfz_batch))
            .expect("sfz freed");
        assert_eq!(manager.freeable_generations(), vec![dependency.generation]);
        assert_eq!(manager.accounting().dependency_claims, 0);
    }

    #[test]
    fn sfz_dependencies_must_be_committed_or_owned_by_the_same_stage() {
        let manager = ResourceManager::new();
        let sample_stage = manager.begin_stage().expect("sample stage");
        let dependency = manager
            .stage_sample(
                sample_stage,
                logical(ResourceKind::Sample, "sample/uncommitted"),
                sample("/uncommitted.wav", "sha256:uncommitted"),
                [PhysicalResourceId::Buffer(22)],
            )
            .expect("sample");
        let foreign_stage = manager.begin_stage().expect("foreign stage");
        assert!(matches!(
            manager.stage_sfz_with_dependencies(
                foreign_stage,
                logical(ResourceKind::Sfz, "sfz/foreign"),
                sfz("sha256:foreign"),
                [dependency.generation],
                [PhysicalResourceId::Native(23)],
            ),
            Err(ResourceError::Invalid(message))
                if message.contains("committed or claimed by the same stage")
        ));

        let same_stage = manager
            .stage_sfz_with_dependencies(
                sample_stage,
                logical(ResourceKind::Sfz, "sfz/same-stage"),
                sfz("sha256:same-stage"),
                [dependency.generation],
                [PhysicalResourceId::Native(24)],
            )
            .expect("same-stage dependency");
        manager
            .quarantine_stage(sample_stage)
            .expect("the whole uncertain stage can be quarantined together");
        assert_eq!(
            manager.health(dependency.generation),
            Some(GenerationHealth::Quarantined)
        );
        assert_eq!(
            manager.health(same_stage.generation),
            Some(GenerationHealth::Quarantined)
        );
    }

    #[test]
    fn uncertain_retirement_quarantines_same_stage_sfz_dependencies_atomically() {
        let manager = ResourceManager::new();
        let stage = manager.begin_stage().expect("stage");
        let dependency = manager
            .stage_sample(
                stage,
                logical(ResourceKind::Sample, "sample/uncertain-stage"),
                sample("/uncertain.wav", "sha256:uncertain"),
                [PhysicalResourceId::Buffer(25)],
            )
            .expect("sample");
        let instrument = manager
            .stage_sfz_with_dependencies(
                stage,
                logical(ResourceKind::Sfz, "sfz/uncertain-stage"),
                sfz("sha256:uncertain-stage"),
                [dependency.generation],
                [PhysicalResourceId::Native(26)],
            )
            .expect("sfz");

        let retirement = manager.discard_stage(stage).expect("discard stage");
        manager
            .quarantine_retirement(&retirement)
            .expect("the entire uncertain retirement is one quarantine authority set");
        assert_eq!(
            manager.health(dependency.generation),
            Some(GenerationHealth::Quarantined)
        );
        assert_eq!(
            manager.health(instrument.generation),
            Some(GenerationHealth::Quarantined)
        );
    }

    #[test]
    fn uncertain_cleanup_cannot_quarantine_a_generation_owned_by_a_live_sfz() {
        let manager = ResourceManager::new();
        let sample_key = logical(ResourceKind::Sample, "sample/sfz-owned");
        let sample_stage = manager.begin_stage().expect("stage");
        let sample_generation = manager
            .stage_sample(
                sample_stage,
                sample_key.clone(),
                sample("/owned.wav", "sha256:owned"),
                [PhysicalResourceId::Buffer(18)],
            )
            .expect("sample");
        manager.commit_stage(sample_stage).expect("commit sample");

        let sfz_key = logical(ResourceKind::Sfz, "sfz/live-owner");
        let sfz_stage = manager.begin_stage().expect("stage");
        manager
            .stage_sfz_with_dependencies(
                sfz_stage,
                sfz_key.clone(),
                sfz("sha256:live-owner"),
                [sample_generation.generation],
                [PhysicalResourceId::Native(19)],
            )
            .expect("sfz");
        manager.commit_stage(sfz_stage).expect("commit sfz");

        let removal = manager.begin_stage().expect("stage");
        manager
            .stage_remove(removal, sample_key)
            .expect("remove sample binding");
        let retirement = manager.commit_stage(removal).expect("commit removal");
        assert_eq!(
            manager.quarantine_retirement(&retirement),
            Err(ResourceError::GenerationClaimed(
                sample_generation.generation
            ))
        );
        assert_eq!(
            manager.health(sample_generation.generation),
            Some(GenerationHealth::Live)
        );
        assert!(manager.acquire(&sfz_key).is_ok());
    }

    #[test]
    fn reader_lease_pins_the_exact_replaced_generation_until_drop() {
        let manager = ResourceManager::new();
        let key = logical(ResourceKind::Sample, "sample/hat");
        let first_stage = manager.begin_stage().expect("stage");
        let first = manager
            .stage_sample(
                first_stage,
                key.clone(),
                sample("/hat.wav", "sha256:1"),
                [PhysicalResourceId::Buffer(20)],
            )
            .expect("sample");
        manager.commit_stage(first_stage).expect("commit");
        let reader = manager.acquire(&key).expect("reader");
        let next_stage = manager.begin_stage().expect("stage");
        manager
            .stage_sample(
                next_stage,
                key.clone(),
                sample("/hat.wav", "sha256:2"),
                [PhysicalResourceId::Buffer(21)],
            )
            .expect("replacement");
        manager.commit_stage(next_stage).expect("commit");
        assert!(manager.freeable_generations().is_empty());
        assert_eq!(reader.generation(), first.generation);
        assert_eq!(reader.physical(), &[PhysicalResourceId::Buffer(20)]);
        drop(reader);
        assert_eq!(manager.freeable_generations(), vec![first.generation]);
    }

    #[test]
    fn free_is_exact_once_and_uncertain_completion_quarantines_ids_against_reuse() {
        let manager = ResourceManager::new();
        let stage = manager.begin_stage().expect("stage");
        let staged = manager
            .stage_sfz(
                stage,
                logical(ResourceKind::Sfz, "sfz/kit"),
                sfz("sha256:kit"),
                [PhysicalResourceId::Buffer(30)],
            )
            .expect("sfz");
        manager.discard_stage(stage).expect("discard");
        let batch = manager
            .begin_free(staged.generation, confirmed())
            .expect("free batch");
        assert!(matches!(
            manager.begin_free(staged.generation, confirmed()),
            Err(ResourceError::FreeAlreadyAttempted(_))
        ));
        manager
            .finish_free(
                &batch,
                vec![PhysicalFreeConfirmation::Uncertain {
                    physical: PhysicalResourceId::Buffer(30),
                    detail: "connection lost after send".into(),
                }],
            )
            .expect("quarantine");
        assert_eq!(
            manager.health(staged.generation),
            Some(GenerationHealth::Quarantined)
        );
        let next = manager.begin_stage().expect("stage");
        assert_eq!(
            manager.stage_sfz(
                next,
                logical(ResourceKind::Sfz, "sfz/other"),
                sfz("sha256:other"),
                [PhysicalResourceId::Buffer(30)],
            ),
            Err(ResourceError::PhysicalResourceInUse(
                PhysicalResourceId::Buffer(30)
            ))
        );
    }

    #[test]
    fn confirmed_free_releases_physical_id_but_retains_exact_generation_accounting() {
        let manager = ResourceManager::new();
        let stage = manager.begin_stage().expect("stage");
        let staged = manager
            .stage_sample(
                stage,
                logical(ResourceKind::Sample, "sample/free"),
                sample("/free.wav", "sha256:free"),
                [PhysicalResourceId::Buffer(40)],
            )
            .expect("sample");
        manager.discard_stage(stage).expect("discard");
        let batch = manager
            .begin_free(staged.generation, confirmed())
            .expect("free batch");
        manager
            .finish_free(&batch, confirmed_free(&batch))
            .expect("confirmed free");
        assert_eq!(
            manager.accounting(),
            ResourceAccounting {
                freed_generations: 1,
                freed_physical: 1,
                ..ResourceAccounting::default()
            }
        );
        let next = manager.begin_stage().expect("stage");
        assert!(manager
            .stage_sample(
                next,
                logical(ResourceKind::Sample, "sample/reuse-id"),
                sample("/reuse.wav", "sha256:reuse"),
                [PhysicalResourceId::Buffer(40)],
            )
            .is_ok());
    }

    #[test]
    fn mismatched_free_confirmation_quarantines_the_whole_retained_batch() {
        let manager = ResourceManager::new();
        let stage = manager.begin_stage().expect("stage");
        let staged = manager
            .stage_sfz(
                stage,
                logical(ResourceKind::Sfz, "sfz/mismatch"),
                sfz("sha256:mismatch"),
                [
                    PhysicalResourceId::Buffer(41),
                    PhysicalResourceId::Buffer(42),
                ],
            )
            .expect("sfz");
        manager.discard_stage(stage).expect("discard");
        let batch = manager
            .begin_free(staged.generation, confirmed())
            .expect("free batch");
        assert_eq!(
            manager.finish_free(
                &batch,
                vec![PhysicalFreeConfirmation::Confirmed {
                    physical: PhysicalResourceId::Buffer(41),
                    backend: "mock".into(),
                    token: "free-41".into(),
                }],
            ),
            Err(ResourceError::FreeConfirmationSetMismatch(
                staged.generation
            ))
        );
        assert_eq!(
            manager.health(staged.generation),
            Some(GenerationHealth::Quarantined)
        );
        assert_eq!(manager.accounting().quarantined_physical, 2);
    }

    #[test]
    fn invalid_resource_identities_and_shapes_are_rejected_before_allocation() {
        let manager = ResourceManager::new();
        let sample_stage = manager.begin_stage().expect("stage");
        let mut invalid_sample = sample("/kick.wav", "sha256:kick");
        invalid_sample.backend.clear();
        assert!(matches!(
            manager.stage_sample(
                sample_stage,
                logical(ResourceKind::Sample, "sample/invalid"),
                invalid_sample,
                [PhysicalResourceId::Buffer(43)],
            ),
            Err(ResourceError::Invalid(_))
        ));
        let buffer_stage = manager.begin_stage().expect("stage");
        assert!(matches!(
            manager.stage_buffer(
                buffer_stage,
                logical(ResourceKind::Buffer, "buffer/invalid"),
                buffer(0, 2),
                BufferShapePolicy::Clear,
                [PhysicalResourceId::Buffer(44)],
            ),
            Err(ResourceError::Invalid(_))
        ));
        assert_eq!(manager.accounting().live_generations, 0);
        assert_eq!(manager.accounting().live_physical, 0);
    }
}
