use super::*;
use crate::candidate::{
    Candidate, CandidateDraft, ContractDigest, EngineInstanceId, EvaluationIdentity,
    LanguageContract, ReferenceCatalog,
};
use crate::capabilities::{
    AvailabilityGate, CapabilityCatalog, CapabilityEvidenceSource, CapabilityMatrix,
    CapabilitySnapshot, CapabilitySnapshotAssembler, CapabilitySnapshotFacts, CapabilitySubject,
    GateEvidence,
};
use crate::mutation::{
    EventSequence, LiveState, ReceiptWindow, RuntimeMutationStatus, MUTATION_SCHEMA_VERSION,
};
use crate::resource_manager::{
    BufferPersistence, BufferShapePolicy, BufferSpec, BufferStageAction, ResourceError, SfzIdentity,
};
use std::path::PathBuf;
use std::sync::OnceLock;
use vibelang_api_manifest::conventions::ConventionsMetadata;

fn sample(path: &str, fingerprint: &str) -> SampleIdentity {
    SampleIdentity {
        canonical_source: path.into(),
        content_fingerprint: fingerprint.into(),
        decode_options_digest: "decode-v1".into(),
        loader_version: "loader-v1".into(),
        backend: "mock".into(),
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

fn buffer(frames: u32, channels: u16) -> BufferSpec {
    BufferSpec {
        frames,
        channels,
        sample_format: "f32".into(),
        backend: "mock".into(),
        persistence: BufferPersistence::Ephemeral,
    }
}

fn logical(kind: ResourceKind, address: &str) -> LogicalResource {
    LogicalResource::new(kind, address).expect("valid logical resource")
}

fn catalog() -> &'static CapabilityCatalog {
    static CATALOG: OnceLock<CapabilityCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../api/effective-metadata-v1.json");
        let source = std::fs::read_to_string(path).expect("accepted metadata must exist");
        let metadata: ConventionsMetadata =
            serde_json::from_str(&source).expect("accepted metadata must decode");
        CapabilityCatalog::from_conventions(&metadata).expect("accepted metadata must validate")
    })
}

const fn evidence_source(gate: AvailabilityGate) -> CapabilityEvidenceSource {
    match gate {
        AvailabilityGate::Declaration => CapabilityEvidenceSource::ContractCatalog,
        AvailabilityGate::Target => CapabilityEvidenceSource::BuildTarget,
        AvailabilityGate::BuildFeature => CapabilityEvidenceSource::BuildFeature,
        AvailabilityGate::OperatorPolicy => CapabilityEvidenceSource::OperatorPolicy,
        AvailabilityGate::RuntimeProbe => CapabilityEvidenceSource::RuntimeProbe,
        AvailabilityGate::BackendSemantic => CapabilityEvidenceSource::BackendProbe,
        AvailabilityGate::ConsumerProjection => CapabilityEvidenceSource::ConsumerProjection,
    }
}

fn capability_matrix(atomic_backend_proven: bool) -> CapabilityMatrix {
    let catalog = catalog();
    let mut matrix = CapabilityMatrix::new(catalog, "scope.runtime").expect("runtime matrix");
    let gates = catalog
        .definitions()
        .flat_map(|definition| {
            definition
                .required_gates
                .iter()
                .copied()
                .map(move |gate| (definition.capability_id.clone(), gate))
        })
        .collect::<Vec<_>>();
    for (capability, gate) in gates {
        let evidence = if capability == ATOMIC_GENERATION_CAPABILITY
            && gate == AvailabilityGate::BackendSemantic
            && !atomic_backend_proven
        {
            GateEvidence::unavailable(
                "reason.backend_semantics_missing",
                CapabilityEvidenceSource::BackendProbe,
            )
        } else {
            GateEvidence::available(evidence_source(gate))
        };
        matrix
            .set_gate(catalog, &capability, gate, evidence)
            .expect("catalog-derived gate evidence");
    }
    matrix
}

fn fixed_status(epoch: RuntimeEpoch, confirmed: RevisionId) -> RuntimeMutationStatus {
    RuntimeMutationStatus {
        schema_version: MUTATION_SCHEMA_VERSION,
        runtime_epoch: epoch,
        event_sequence: Some(EventSequence::new(9).expect("event sequence")),
        accepted_through: Some(confirmed),
        last_confirmed_revision: Some(confirmed),
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

fn snapshot(
    epoch: RuntimeEpoch,
    confirmed: RevisionId,
    runtime_id: &str,
    atomic_backend_proven: bool,
) -> CapabilitySnapshot {
    let status = fixed_status(epoch, confirmed);
    let matrix = capability_matrix(atomic_backend_proven);
    let subject = CapabilitySubject::new(
        runtime_id,
        "target.native.linux",
        format!("sha256:{}", "a".repeat(64)),
    )
    .expect("privacy-safe subject");
    let facts = CapabilitySnapshotFacts::new(
        format!("sha256:{}", "b".repeat(64)),
        subject,
        &status,
        &matrix,
        "security.http.loopback_local",
        false,
    )
    .expect("source facts");
    CapabilitySnapshotAssembler::new()
        .assemble(
            catalog(),
            facts,
            Timestamp::parse("2026-07-18T06:00:00Z").expect("timestamp"),
        )
        .expect("source-derived snapshot")
}

fn observation(runtime_id: &str) -> AtomicProbeObservation {
    AtomicProbeObservation {
        backend: "semantic-test-backend".into(),
        runtime_instance: runtime_id.into(),
        inactive_stage_token: "inactive-1".into(),
        activation_token: "activation-2".into(),
        restoration_token: "restoration-3".into(),
        cleanup_token: "cleanup-4".into(),
        inactive_graph_was_silent: true,
        activation_was_one_bundle_or_link: true,
        restoration_was_confirmed: true,
        exact_free_was_confirmed: true,
    }
}

fn candidate(epoch: RuntimeEpoch) -> Candidate {
    CandidateDraft::new(
        EvaluationIdentity::new(
            LanguageContract::v2(ContractDigest::from_bytes(b"m07 integration contract")),
            EngineInstanceId::new(),
            epoch,
        ),
        CandidateOrigin::ScriptFile,
    )
    .finish(&ReferenceCatalog::default())
    .expect("empty integration candidate")
}

#[test]
fn source_derived_atomic_truth_is_subject_bound_and_scsynth_semantics_fail_closed() {
    let epoch =
        RuntimeEpoch::parse("01890f3c-7b5a-7cc0-98c4-dc0c0c0c0c0c").expect("fixed runtime epoch");
    let confirmed = RevisionId::new(7).expect("confirmed revision");
    let scsynth_truth = snapshot(epoch, confirmed, "runtime.local", false);
    assert_eq!(
        AtomicGenerationEvidence::confirm(
            &scsynth_truth,
            Some(confirmed),
            observation("runtime.local")
        ),
        Err(GenerationError::AtomicCapabilityUnavailable)
    );

    let proven = snapshot(epoch, confirmed, "runtime.local", true);
    assert_eq!(
        AtomicGenerationEvidence::confirm(&proven, Some(confirmed), observation("runtime.remote")),
        Err(GenerationError::AtomicProbeIncomplete)
    );
    let evidence =
        AtomicGenerationEvidence::confirm(&proven, Some(confirmed), observation("runtime.local"))
            .expect("complete subject-bound semantic probe");
    let resources = ResourceManager::new();
    let stage = resources.begin_stage().expect("resource stage");
    let plan = NativeGenerationPlanner
        .plan(
            NativePlanRequest {
                candidate: &candidate(epoch),
                target_revision: RevisionId::new(8).expect("target revision"),
                confirmed_revision: Some(confirmed),
                capability_snapshot: &proven,
                atomicity: AtomicAdmission::Required,
                atomic_evidence: Some(&evidence),
                allocation: GenerationAllocation {
                    generation: GraphGeneration::new(8).expect("generation"),
                    root: NodeId::new(1800),
                    parent: NodeId::new(1),
                },
                resource_stage: resources.snapshot_stage(stage).expect("stage snapshot"),
                components: Vec::new(),
                boundary: boundary(),
            },
            48_000.0,
            64,
        )
        .expect("fully evidenced backend may plan required atomicity");
    assert_eq!(plan.atomicity, AtomicAdmission::Required);

    let other_subject = snapshot(epoch, confirmed, "runtime.remote", true);
    let other_stage = resources.begin_stage().expect("other resource stage");
    assert_eq!(
        NativeGenerationPlanner.plan(
            NativePlanRequest {
                candidate: &candidate(epoch),
                target_revision: RevisionId::new(8).expect("target revision"),
                confirmed_revision: Some(confirmed),
                capability_snapshot: &other_subject,
                atomicity: AtomicAdmission::Required,
                atomic_evidence: Some(&evidence),
                allocation: GenerationAllocation {
                    generation: GraphGeneration::new(8).expect("generation"),
                    root: NodeId::new(1801),
                    parent: NodeId::new(1),
                },
                resource_stage: resources
                    .snapshot_stage(other_stage)
                    .expect("other stage snapshot"),
                components: Vec::new(),
                boundary: boundary(),
            },
            48_000.0,
            64,
        ),
        Err(GenerationError::AtomicCapabilityUnavailable)
    );
}

#[tokio::test]
async fn every_precommit_boundary_has_an_exact_rejected_receipt_and_preserves_authority() {
    let base = RevisionId::new(1).expect("base revision");
    for (fault, phases, phase, code) in [
        (
            Fault::Root,
            vec![],
            FailurePhase::Staging,
            "inactive_root_stage_failed",
        ),
        (
            Fault::Create,
            vec![StagePhase::Create],
            FailurePhase::Staging,
            "create_stage_failed",
        ),
        (
            Fault::Update,
            vec![StagePhase::Update],
            FailurePhase::Staging,
            "update_stage_failed",
        ),
        (
            Fault::Route,
            vec![StagePhase::Route],
            FailurePhase::Staging,
            "route_stage_failed",
        ),
        (
            Fault::Effect,
            vec![StagePhase::Effect],
            FailurePhase::Staging,
            "effect_stage_failed",
        ),
        (
            Fault::Barrier,
            vec![],
            FailurePhase::BackendBarrier,
            "staging_barrier_failed",
        ),
        (
            Fault::Activation,
            vec![],
            FailurePhase::Activate,
            "activation_failed",
        ),
        (
            Fault::Commit,
            vec![],
            FailurePhase::Reconcile,
            "runtime_commit_failed",
        ),
    ] {
        let resources = ResourceManager::new();
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(
                plan(&resources, &phases, Some(base)),
                &resources,
                &FaultDriver::with_fault(fault),
            )
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Rejected {
                receipt: Rejected {
                    phase: actual_phase,
                    code: ref actual_code,
                    rollback,
                    preserved_revision: Some(revision),
                    ..
                },
                cleanup: CleanupHealth::Clean,
            } if actual_phase == phase
                && actual_code == code
                && revision == base
                && rollback == if matches!(fault, Fault::Activation | Fault::Commit) {
                    RollbackState::Confirmed
                } else {
                    RollbackState::NotNeeded
                }
        ));
        assert_eq!(
            coordinator.active().map(|active| active.revision),
            Some(base)
        );
        assert!(!coordinator.is_fenced());
    }
}

#[tokio::test]
async fn mismatch_timeout_exhaustion_and_send_failure_never_claim_uncorrelated_success() {
    let base = RevisionId::new(1).expect("base revision");
    for (fault, phases, phase, code) in [
        (
            Fault::Root,
            vec![],
            FailurePhase::Staging,
            "inactive_root_stage_failed",
        ),
        (
            Fault::Create,
            vec![StagePhase::Create],
            FailurePhase::Staging,
            "create_stage_failed",
        ),
        (
            Fault::Update,
            vec![StagePhase::Update],
            FailurePhase::Staging,
            "update_stage_failed",
        ),
        (
            Fault::Route,
            vec![StagePhase::Route],
            FailurePhase::Staging,
            "route_stage_failed",
        ),
        (
            Fault::Effect,
            vec![StagePhase::Effect],
            FailurePhase::Staging,
            "effect_stage_failed",
        ),
        (
            Fault::Barrier,
            vec![],
            FailurePhase::BackendBarrier,
            "barrier_correlation_mismatch",
        ),
        (
            Fault::Activation,
            vec![],
            FailurePhase::Activate,
            "activation_correlation_mismatch",
        ),
    ] {
        let resources = ResourceManager::new();
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(
                plan(&resources, &phases, Some(base)),
                &resources,
                &FaultDriver::with_wrong_ack(fault),
            )
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Rejected {
                receipt: Rejected {
                    phase: actual_phase,
                    code: ref actual_code,
                    preserved_revision: Some(revision),
                    ..
                },
                ..
            } if actual_phase == phase && actual_code == code && revision == base
        ));
    }

    let resources = ResourceManager::new();
    let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
    let duplicate = coordinator
        .execute(
            plan(&resources, &[], Some(base)),
            &resources,
            &FaultDriver::with_duplicate_correlations(),
        )
        .await;
    assert!(matches!(
        duplicate,
        GenerationOutcome::Partial {
            receipt: Partial {
                phase: FailurePhase::Rollback,
                rollback: RollbackState::Uncertain,
                fenced: true,
                last_confirmed_revision: Some(revision),
                ..
            },
            cleanup: CleanupHealth::Degraded(_),
        } if revision == base
    ));

    for (transport_fault, phase, code, rollback) in [
        (
            Fault::CorrelationExhaustion,
            FailurePhase::Staging,
            "inactive_root_correlation_unavailable",
            RollbackState::NotNeeded,
        ),
        (
            Fault::BarrierTimeout,
            FailurePhase::BackendBarrier,
            "staging_barrier_failed",
            RollbackState::NotNeeded,
        ),
        (
            Fault::ActivationSendFailure,
            FailurePhase::Activate,
            "activation_failed",
            RollbackState::Confirmed,
        ),
    ] {
        let resources = ResourceManager::new();
        let driver = FaultDriver::with_fault(transport_fault);
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        let outcome = coordinator
            .execute(plan(&resources, &[], Some(base)), &resources, &driver)
            .await;
        assert!(matches!(
            outcome,
            GenerationOutcome::Rejected {
                receipt: Rejected {
                    phase: actual_phase,
                    code: ref actual_code,
                    rollback: actual_rollback,
                    preserved_revision: Some(revision),
                    ..
                },
                cleanup: CleanupHealth::Clean,
            } if actual_phase == phase
                && actual_code == code
                && actual_rollback == rollback
                && revision == base
        ));
        assert_eq!(
            coordinator.active().map(|active| active.revision),
            Some(base)
        );
    }
}

#[tokio::test]
async fn restoration_cleanup_and_commit_enforce_rejected_partial_and_applied_terminality() {
    let base = RevisionId::new(1).expect("base revision");
    let resources = ResourceManager::new();
    let driver = FaultDriver::with_fault(Fault::Activation);
    driver.faults.lock().insert(Fault::Restoration);
    let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
    let uncertain = coordinator
        .execute(plan(&resources, &[], Some(base)), &resources, &driver)
        .await;
    assert!(matches!(
        uncertain,
        GenerationOutcome::Partial {
            receipt: Partial {
                phase: FailurePhase::Rollback,
                ref code,
                rollback: RollbackState::Uncertain,
                fenced: true,
                last_confirmed_revision: Some(revision),
                ..
            },
            cleanup: CleanupHealth::Degraded(_),
        } if code == "restoration_unconfirmed" && revision == base
    ));
    assert_eq!(
        coordinator.active().map(|active| active.revision),
        Some(base)
    );
    assert!(coordinator.is_fenced());

    let resources = ResourceManager::new();
    let driver = FaultDriver::with_fault(Fault::Root);
    driver.faults.lock().insert(Fault::Cleanup);
    let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
    let cleanup_uncertain = coordinator
        .execute(plan(&resources, &[], Some(base)), &resources, &driver)
        .await;
    assert!(matches!(
        cleanup_uncertain,
        GenerationOutcome::Partial {
            receipt: Partial {
                rollback: RollbackState::Uncertain,
                fenced: true,
                ..
            },
            cleanup: CleanupHealth::Degraded(_),
        }
    ));

    let resources = ResourceManager::new();
    let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
    let applied = coordinator
        .execute(
            plan(&resources, &[StagePhase::Create], Some(base)),
            &resources,
            &FaultDriver::with_fault(Fault::Cleanup),
        )
        .await;
    assert!(matches!(
        applied,
        GenerationOutcome::Applied {
            receipt: Applied {
                ref confirmations,
                ref components,
                ..
            },
            ref generation,
            cleanup: CleanupHealth::Degraded(_),
        } if confirmations.len() == 3
            && components.iter().all(|component| component.state == ComponentState::Applied)
            && generation.revision.get() == 2
    ));
    assert_eq!(
        coordinator.active().map(|active| active.revision.get()),
        Some(2)
    );
    assert!(!coordinator.is_fenced());
}

#[test]
fn quantized_timing_and_exact_resource_stage_reject_nonfinite_overflow_and_drift() {
    let quantized = quantize_boundary(boundary(), 48_000.0, 64).expect("quantized boundary");
    assert_eq!(
        quantized.backend_time_seconds.get().to_bits(),
        1.0013333333333334_f64.to_bits()
    );
    let mut nonfinite = boundary();
    nonfinite.requested_backend_seconds = f64::INFINITY;
    assert_eq!(
        quantize_boundary(nonfinite, 48_000.0, 64),
        Err(GenerationError::InvalidBoundary)
    );
    let mut overflow = boundary();
    overflow.requested_backend_seconds = f64::MAX;
    assert_eq!(
        quantize_boundary(overflow, f64::MIN_POSITIVE, 1),
        Err(GenerationError::InvalidBoundary)
    );

    let resources = ResourceManager::new();
    let stage = resources.begin_stage().expect("resource stage");
    let captured = resources.snapshot_stage(stage).expect("captured stage");
    resources
        .stage_sample(
            stage,
            logical(ResourceKind::Sample, "sample/drift"),
            sample("/drift.wav", "sha256:drift"),
            [PhysicalResourceId::Buffer(90)],
        )
        .expect("stage mutation");
    assert!(matches!(
        resources.prepare_snapshot(&captured),
        Err(ResourceError::StageSnapshotChanged { .. })
    ));
    assert_eq!(resources.accounting().logical_bindings, 0);
}

#[tokio::test]
async fn resource_policy_lifetime_and_free_accounting_are_one_transaction_authority() {
    let resources = ResourceManager::new();
    let sample_key = logical(ResourceKind::Sample, "sample/same-path");
    let first_stage = resources.begin_stage().expect("first stage");
    let first = resources
        .stage_sample(
            first_stage,
            sample_key.clone(),
            sample("/same.wav", "sha256:old"),
            [PhysicalResourceId::Buffer(100)],
        )
        .expect("first sample");
    resources.commit_stage(first_stage).expect("first commit");
    let reader = resources.acquire(&sample_key).expect("old reader");

    let stage = resources.begin_stage().expect("replacement stage");
    let replacement = resources
        .stage_sample(
            stage,
            sample_key.clone(),
            sample("/same.wav", "sha256:new"),
            [PhysicalResourceId::Buffer(101)],
        )
        .expect("same-path replacement");
    let sfz_key = logical(ResourceKind::Sfz, "sfz/same-stage");
    let instrument = resources
        .stage_sfz_with_dependencies(
            stage,
            sfz_key.clone(),
            sfz("sha256:transitive"),
            [replacement.generation],
            [PhysicalResourceId::Native(102)],
        )
        .expect("same-stage SFZ dependency");
    assert_ne!(first.generation, replacement.generation);
    assert_eq!(
        resources.generation_for(&sample_key),
        Some(first.generation)
    );
    assert_eq!(reader.physical(), &[PhysicalResourceId::Buffer(100)]);
    resources
        .commit_stage(stage)
        .expect("atomic resource commit");
    assert_eq!(
        resources.generation_for(&sample_key),
        Some(replacement.generation)
    );
    assert!(resources.freeable_generations().is_empty());
    drop(reader);
    assert_eq!(resources.freeable_generations(), vec![first.generation]);
    assert_eq!(resources.accounting().dependency_claims, 1);
    assert_eq!(
        resources.health(instrument.generation),
        Some(GenerationHealth::Live)
    );

    let buffer_key = logical(ResourceKind::Buffer, "buffer/shape");
    let buffer_stage = resources.begin_stage().expect("buffer stage");
    resources
        .stage_buffer(
            buffer_stage,
            buffer_key.clone(),
            buffer(64, 2),
            BufferShapePolicy::Clear,
            [PhysicalResourceId::Buffer(103)],
        )
        .expect("buffer");
    resources.commit_stage(buffer_stage).expect("buffer commit");
    let incompatible = resources.begin_stage().expect("incompatible stage");
    assert!(matches!(
        resources.stage_buffer(
            incompatible,
            buffer_key.clone(),
            buffer(128, 1),
            BufferShapePolicy::PreserveCompatible,
            [PhysicalResourceId::Buffer(104)],
        ),
        Err(ResourceError::BufferReplacementPolicyRequired { .. })
    ));
    resources
        .discard_stage(incompatible)
        .expect("discard buffer failure");
    let copy_stage = resources.begin_stage().expect("copy stage");
    let copied = resources
        .stage_buffer(
            copy_stage,
            buffer_key,
            buffer(128, 1),
            BufferShapePolicy::CopyOverlap,
            [PhysicalResourceId::Buffer(105)],
        )
        .expect("explicit shape policy");
    assert_eq!(copied.buffer_action, Some(BufferStageAction::CopiedOverlap));

    let failed_stage = resources.begin_stage().expect("failed SFZ stage");
    let failed = resources
        .record_failed_sfz_stage(
            failed_stage,
            sfz("sha256:partial"),
            [PhysicalResourceId::Native(106)],
        )
        .expect("failed transitive allocation census");
    assert_eq!(
        resources.prepare_commit(failed_stage),
        Err(ResourceError::FailedStageCannotCommit)
    );
    resources
        .discard_stage(failed_stage)
        .expect("discard failed SFZ");

    let batch = resources
        .begin_free(
            failed,
            QuiescenceProof::confirmed("mock", "barrier-1").expect("quiescence"),
        )
        .expect("one free attempt");
    assert!(matches!(
        resources.begin_free(
            failed,
            QuiescenceProof::confirmed("mock", "barrier-2").expect("quiescence")
        ),
        Err(ResourceError::FreeAlreadyAttempted(generation)) if generation == failed
    ));
    resources
        .finish_free(
            &batch,
            vec![PhysicalFreeConfirmation::Uncertain {
                physical: PhysicalResourceId::Native(106),
                detail: "timeout after free send".into(),
            }],
        )
        .expect("uncertain free is retained as quarantine");
    assert_eq!(
        resources.health(failed),
        Some(GenerationHealth::Quarantined)
    );
    let accounting = resources.accounting();
    assert_eq!(accounting.quarantined_generations, 1);
    assert_eq!(accounting.quarantined_physical, 1);
    assert_eq!(accounting.freed_physical, 0);
}

#[test]
fn remediation_add_actions_are_contained_and_root_sibling_attacks_are_rejected() {
    let allocation = GenerationAllocation {
        generation: GraphGeneration::new(8).expect("generation"),
        root: NodeId::new(1800),
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
                name: "voice__g8".into(),
                bytes: Arc::from([1_u8]),
            },
        },
        NativePlanComponent {
            declaration: "voice/tail".into(),
            path: "voice/tail".into(),
            operation: NativeStageOperation::CreateSynth {
                definition: "voice__g8".into(),
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
                definition: "voice__g8".into(),
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
                name: "effect__g8".into(),
                bytes: Arc::from([2_u8]),
            },
        },
        NativePlanComponent {
            declaration: "effect/replace".into(),
            path: "effect/replace".into(),
            operation: NativeStageOperation::CreateEffect {
                definition: "effect__g8".into(),
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
                    definition: "escape__g8".into(),
                    node: NodeId::new(5000),
                    target,
                    action,
                    params: ParamMap::new(),
                },
                PlannedNodeKind::Effect => NativeStageOperation::CreateEffect {
                    definition: "escape__g8".into(),
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
                        name: "escape__g8".into(),
                        bytes: Arc::from([3_u8]),
                    },
                });
            }
            attack.push(NativePlanComponent {
                declaration: "escape/node".into(),
                path: "escape/node".into(),
                operation,
            });
            assert!(
                matches!(
                    validate_inactive_operations(&allocation, &attack),
                    Err(GenerationError::InvalidPlan(message))
                        if message.contains("inactive generation root")
                            || message.contains("inactive root")
                ),
                "{kind:?} {action:?}"
            );
        }
    }
}

#[tokio::test]
async fn remediation_stage_authority_is_exact_before_and_during_execution() {
    let base = RevisionId::new(1).expect("revision");

    let resources = ResourceManager::new();
    let planned = plan(&resources, &[], Some(base));
    let stage = planned.resource_stage.stage();
    resources
        .commit_stage(stage)
        .expect("external owner wins before execute");
    let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
    let outcome = coordinator
        .execute(planned, &resources, &FaultDriver::default())
        .await;
    assert!(matches!(
        outcome,
        GenerationOutcome::Partial {
            receipt: Partial {
                phase: FailurePhase::Planning,
                ref code,
                rollback: RollbackState::NotNeeded,
                fenced: true,
                last_confirmed_revision: Some(revision),
                ref components,
            },
            cleanup: CleanupHealth::Degraded(_),
        } if code == "resource_stage_capture_failed"
            && revision == base
            && components.len() == 2
            && components[0].path == "generation/root"
            && components[0].state == ComponentState::NotStarted
            && components[1].path == "generation/cleanup"
            && components[1].state == ComponentState::Uncertain
    ));
    assert_eq!(
        resources.stage_state(stage),
        Some(ResourceStageState::Committed(ResourceStageOwner::External))
    );
    assert_eq!(coordinator.active().map(|value| value.revision), Some(base));
    assert!(coordinator.is_fenced());

    for point in [
        StageRacePoint::AfterPrepare,
        StageRacePoint::AfterActivation,
        StageRacePoint::AtCommit,
    ] {
        let resources = ResourceManager::new();
        let mut planned = plan(&resources, &[], Some(base));
        let stage = planned.resource_stage.stage();
        planned.resource_stage.capture().expect("capture");
        let driver = FaultDriver::with_stage_race(point, resources.clone(), stage);
        let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
        assert!(matches!(
            coordinator.execute(planned, &resources, &driver).await,
            GenerationOutcome::Applied {
                cleanup: CleanupHealth::Clean,
                ..
            }
        ));
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
            coordinator.active().map(|value| value.revision.get()),
            Some(2)
        );
        assert!(!coordinator.is_fenced());
    }
}

#[tokio::test]
async fn remediation_real_resource_commit_failure_reports_split_authority() {
    let resources = ResourceManager::new();
    let key = logical(ResourceKind::Sample, "sample/real-commit-failure");
    let old_stage = resources.begin_stage().expect("old stage");
    let old = resources
        .stage_sample(
            old_stage,
            key.clone(),
            sample("/commit.wav", "sha256:old"),
            [PhysicalResourceId::Buffer(500)],
        )
        .expect("old sample");
    resources.commit_stage(old_stage).expect("old commit");

    let base = RevisionId::new(1).expect("revision");
    let mut planned = plan(&resources, &[], Some(base));
    let planned_stage = planned.resource_stage.stage();
    let replacement = resources
        .stage_sample(
            planned_stage,
            key.clone(),
            sample("/commit.wav", "sha256:planned"),
            [PhysicalResourceId::Buffer(501)],
        )
        .expect("planned replacement");
    planned.components.push(NativePlanComponent {
        declaration: "resource/planned".into(),
        path: "resource/planned".into(),
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
            sample("/commit.wav", "sha256:competing"),
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
    assert_eq!(coordinator.active().map(|value| value.revision), Some(base));
    assert!(coordinator.is_fenced());
}

#[tokio::test]
async fn remediation_timing_uses_only_correlated_acknowledgement_truth() {
    let base = RevisionId::new(1).expect("revision");

    let resources = ResourceManager::new();
    let mut immediate = plan(&resources, &[], Some(base));
    immediate.boundary.deadline = None;
    let mut coordinator = NativeGenerationCoordinator::new(Some(active(base)));
    assert!(matches!(
        coordinator
            .execute(immediate, &resources, &FaultDriver::default())
            .await,
        GenerationOutcome::Applied {
            receipt: Applied {
                effective_at: EffectiveAt {
                    backend_time_seconds: None,
                    musical_beat: None,
                    ..
                },
                audible_tail_until: None,
                ..
            },
            ..
        }
    ));

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
    assert!(matches!(
        coordinator
            .execute(
                future,
                &resources,
                &FaultDriver::with_timing_proof(timing_proof(
                    ActivationTimingKind::Scheduled,
                    expected.get(),
                    Some(BeatTicks::new(65_536)),
                )),
            )
            .await,
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
    assert!(matches!(
        coordinator
            .execute(
                future,
                &resources,
                &FaultDriver::with_timing_proof(timing_proof(
                    ActivationTimingKind::Scheduled,
                    expected + 1.0,
                    Some(BeatTicks::new(65_536)),
                )),
            )
            .await,
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
    assert!(matches!(
        coordinator
            .execute(
                late,
                &resources,
                &FaultDriver::with_timing_proof(timing_proof(
                    ActivationTimingKind::Executed,
                    2.5,
                    Some(BeatTicks::new(80_000)),
                )),
            )
            .await,
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
    assert!(matches!(
        coordinator
            .execute(
                plan(&resources, &[], Some(base)),
                &resources,
                &FaultDriver::with_timing_proof(timing_proof(
                    ActivationTimingKind::Executed,
                    1.0,
                    Some(BeatTicks::new(i64::MAX)),
                )),
            )
            .await,
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
fn remediation_sfz_one_of_many_failure_preserves_old_bindings_and_accounts_cleanup() {
    let resources = ResourceManager::new();
    let sample_a = logical(ResourceKind::Sample, "sample/old-a");
    let sample_b = logical(ResourceKind::Sample, "sample/old-b");
    let sfz_key = logical(ResourceKind::Sfz, "sfz/old-binding");
    let old_stage = resources.begin_stage().expect("old stage");
    let old_a = resources
        .stage_sample(
            old_stage,
            sample_a.clone(),
            sample("/old-a.wav", "sha256:old-a"),
            [PhysicalResourceId::Buffer(600)],
        )
        .expect("old dependency a");
    let old_b = resources
        .stage_sample(
            old_stage,
            sample_b.clone(),
            sample("/old-b.wav", "sha256:old-b"),
            [PhysicalResourceId::Buffer(601)],
        )
        .expect("old dependency b");
    let old_sfz = resources
        .stage_sfz_with_dependencies(
            old_stage,
            sfz_key.clone(),
            sfz("sha256:old-sfz"),
            [old_a.generation, old_b.generation],
            [PhysicalResourceId::Native(602)],
        )
        .expect("old sfz");
    resources.commit_stage(old_stage).expect("old commit");
    let reader = resources.acquire(&sfz_key).expect("old sfz reader");

    let failed_stage = resources.begin_stage().expect("failed replacement stage");
    let replacement_a = resources
        .stage_sample(
            failed_stage,
            sample_a.clone(),
            sample("/old-a.wav", "sha256:new-a"),
            [PhysicalResourceId::Buffer(603)],
        )
        .expect("first replacement dependency succeeds");
    let failed_sfz = resources
        .record_failed_sfz_stage(
            failed_stage,
            sfz("sha256:failed-one-of-many"),
            [
                PhysicalResourceId::Buffer(604),
                PhysicalResourceId::Native(605),
            ],
        )
        .expect("second dependency and sfz allocation failure census");
    assert_eq!(
        resources.prepare_commit(failed_stage),
        Err(ResourceError::FailedStageCannotCommit)
    );
    assert_eq!(resources.generation_for(&sample_a), Some(old_a.generation));
    assert_eq!(resources.generation_for(&sample_b), Some(old_b.generation));
    assert_eq!(resources.generation_for(&sfz_key), Some(old_sfz.generation));
    assert_eq!(
        reader.physical(),
        &[
            PhysicalResourceId::Buffer(600),
            PhysicalResourceId::Buffer(601),
            PhysicalResourceId::Native(602),
        ]
    );

    let retirement = resources
        .discard_stage(failed_stage)
        .expect("discard failed replacement");
    assert!(retirement
        .generations()
        .any(|generation| generation == replacement_a.generation));
    assert!(retirement
        .generations()
        .any(|generation| generation == failed_sfz));
    assert_eq!(resources.generation_for(&sample_a), Some(old_a.generation));
    assert_eq!(resources.generation_for(&sfz_key), Some(old_sfz.generation));

    let confirmed_batch = resources
        .begin_free(
            replacement_a.generation,
            QuiescenceProof::confirmed("mock", "sfz-cleanup-confirmed").expect("quiescence"),
        )
        .expect("confirmed dependency cleanup");
    resources
        .finish_free(
            &confirmed_batch,
            vec![PhysicalFreeConfirmation::Confirmed {
                physical: PhysicalResourceId::Buffer(603),
                backend: "mock".into(),
                token: "free-603".into(),
            }],
        )
        .expect("confirmed dependency free");
    assert!(matches!(
        resources.begin_free(
            replacement_a.generation,
            QuiescenceProof::confirmed("mock", "sfz-cleanup-duplicate").expect("quiescence")
        ),
        Err(ResourceError::FreeAlreadyAttempted(generation))
            if generation == replacement_a.generation
    ));

    let uncertain_batch = resources
        .begin_free(
            failed_sfz,
            QuiescenceProof::confirmed("mock", "sfz-cleanup-uncertain").expect("quiescence"),
        )
        .expect("failed sfz cleanup");
    resources
        .finish_free(
            &uncertain_batch,
            vec![
                PhysicalFreeConfirmation::Confirmed {
                    physical: PhysicalResourceId::Buffer(604),
                    backend: "mock".into(),
                    token: "free-604".into(),
                },
                PhysicalFreeConfirmation::Uncertain {
                    physical: PhysicalResourceId::Native(605),
                    detail: "acknowledgement lost".into(),
                },
            ],
        )
        .expect("mixed cleanup becomes quarantine");
    assert_eq!(
        resources.health(replacement_a.generation),
        Some(GenerationHealth::Freed)
    );
    assert_eq!(
        resources.health(failed_sfz),
        Some(GenerationHealth::Quarantined)
    );
    let accounting = resources.accounting();
    assert_eq!(accounting.logical_bindings, 3);
    assert_eq!(accounting.freed_physical, 2);
    assert_eq!(accounting.quarantined_physical, 1);
    drop(reader);
    assert_eq!(resources.generation_for(&sfz_key), Some(old_sfz.generation));
}

#[test]
fn remediation_buffer_preservation_and_copy_clear_failures_are_exact() {
    let resources = ResourceManager::new();
    let key = logical(ResourceKind::Buffer, "buffer/compatible");
    let stage = resources.begin_stage().expect("initial buffer stage");
    let original = resources
        .stage_buffer(
            stage,
            key.clone(),
            buffer(64, 2),
            BufferShapePolicy::Clear,
            [PhysicalResourceId::Buffer(700)],
        )
        .expect("initial buffer");
    resources.commit_stage(stage).expect("initial commit");
    let compatible = resources.begin_stage().expect("compatible stage");
    let reused = resources
        .stage_buffer(
            compatible,
            key.clone(),
            buffer(64, 2),
            BufferShapePolicy::PreserveCompatible,
            [],
        )
        .expect("compatible buffer reuse");
    assert_eq!(reused.generation, original.generation);
    assert!(reused.reused);
    assert_eq!(reused.buffer_action, Some(BufferStageAction::Reused));
    let retirement = resources
        .commit_stage(compatible)
        .expect("compatible preservation commit");
    assert_eq!(retirement.generations().len(), 0);
    assert_eq!(resources.generation_for(&key), Some(original.generation));
    assert_eq!(
        resources.health(original.generation),
        Some(GenerationHealth::Live)
    );

    for (offset, policy, expected_action) in [
        (0_u32, BufferShapePolicy::Clear, BufferStageAction::Cleared),
        (
            10_u32,
            BufferShapePolicy::CopyOverlap,
            BufferStageAction::CopiedOverlap,
        ),
    ] {
        let resources = ResourceManager::new();
        let key = logical(ResourceKind::Buffer, &format!("buffer/policy-{offset}"));
        let original_physical = PhysicalResourceId::Buffer(710 + offset);
        let replacement_physical = PhysicalResourceId::Buffer(711 + offset);
        let initial = resources.begin_stage().expect("policy initial stage");
        let original = resources
            .stage_buffer(
                initial,
                key.clone(),
                buffer(64, 2),
                BufferShapePolicy::Clear,
                [original_physical],
            )
            .expect("policy initial buffer");
        resources
            .commit_stage(initial)
            .expect("policy initial commit");

        let failed = resources.begin_stage().expect("policy failure stage");
        assert_eq!(
            resources.stage_buffer(
                failed,
                key.clone(),
                buffer(128, 1),
                policy,
                [original_physical],
            ),
            Err(ResourceError::PhysicalResourceInUse(original_physical))
        );
        resources
            .discard_stage(failed)
            .expect("discard deterministic policy failure");
        assert_eq!(resources.generation_for(&key), Some(original.generation));
        assert_eq!(
            resources.health(original.generation),
            Some(GenerationHealth::Live)
        );

        let success = resources.begin_stage().expect("policy success stage");
        let replacement = resources
            .stage_buffer(
                success,
                key.clone(),
                buffer(128, 1),
                policy,
                [replacement_physical],
            )
            .expect("explicit incompatible policy succeeds");
        assert_eq!(replacement.buffer_action, Some(expected_action));
        resources
            .commit_stage(success)
            .expect("explicit policy commit");
        assert_eq!(resources.generation_for(&key), Some(replacement.generation));
        assert_eq!(
            resources.health(replacement.generation),
            Some(GenerationHealth::Live)
        );
        assert!(resources
            .freeable_generations()
            .contains(&original.generation));
    }
}
