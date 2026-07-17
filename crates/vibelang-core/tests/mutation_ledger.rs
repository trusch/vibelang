use rhai::serde::{from_dynamic, to_dynamic};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, SystemTime};
use vibelang_core::mutation::*;

fn now(offset: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + offset)
}

fn submission(
    key: Option<&str>,
    expected_revision: Option<RevisionId>,
    semantic: serde_json::Value,
    redacted: serde_json::Value,
) -> Submission {
    Submission {
        kind: MutationKind::Command {
            domain: MessageDomain::Transport,
            operation: "set_tempo".into(),
        },
        source: MutationSource::Rhai {
            engine_id: "test-engine".into(),
        },
        caller_namespace: "test-caller".into(),
        idempotency_key: key.map(str::to_owned),
        require_idempotency_key: false,
        retry_epoch: None,
        expected_revision,
        atomicity: Atomicity::Required,
        supersession: SupersessionPolicy::Fifo,
        material: RequestMaterial::from_values(semantic, Some(redacted)),
    }
}

fn new_receipt(result: SubmissionResult) -> MutationReceipt {
    match result {
        SubmissionResult::New(receipt) => receipt,
        other => panic!("expected new receipt, got {other:?}"),
    }
}

fn planned(path: &str) -> PlannedComponent {
    PlannedComponent {
        path: path.into(),
        action: "replace".into(),
    }
}

fn component(path: &str, state: ComponentState) -> ComponentOutcome {
    component_action(path, "replace", state)
}

fn component_action(path: &str, action: &str, state: ComponentState) -> ComponentOutcome {
    ComponentOutcome {
        path: path.into(),
        action: action.into(),
        state,
        effective_at: None,
        confirmation: None,
        diagnostic: None,
    }
}

fn assert_wire<T: Clone + Debug + Serialize + DeserializeOwned + PartialEq>(value: &T) {
    let json = serde_json::to_string(value).unwrap();
    assert_eq!(*value, serde_json::from_str::<T>(&json).unwrap());
    let dynamic = to_dynamic(value).unwrap();
    assert_eq!(*value, from_dynamic::<T>(&dynamic).unwrap());
}

fn applied(components: Vec<ComponentOutcome>, at: SystemTime) -> ReceiptState {
    ReceiptState::Terminal(TerminalOutcome::Applied(Applied {
        effective_at: EffectiveAt {
            observed_at: Timestamp::from_system_time(at),
            musical_beat: None,
            backend_time_seconds: None,
        },
        confirmations: vec![Confirmation::RuntimeCommit],
        components,
        audible_tail_until: None,
    }))
}

fn partial(
    phase: FailurePhase,
    code: &str,
    components: Vec<ComponentOutcome>,
    rollback: RollbackState,
    fenced: bool,
    last_confirmed_revision: Option<RevisionId>,
) -> ReceiptState {
    ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
        phase,
        code: code.into(),
        components,
        rollback,
        fenced,
        last_confirmed_revision,
    }))
}

fn apply_one(
    ledger: &MutationLedger,
    expected_revision: Option<RevisionId>,
    offset: u64,
) -> RevisionId {
    let receipt = new_receipt(
        ledger
            .submit(
                submission(
                    None,
                    expected_revision,
                    json!({"tempo": 120 + offset}),
                    json!({"tempo": 120 + offset}),
                ),
                now(offset),
            )
            .unwrap(),
    );
    let accepted = ledger
        .accept(receipt.attempt_id, None, now(offset + 1))
        .unwrap();
    let revision = accepted.revision.unwrap();
    ledger
        .begin_planning(
            receipt.attempt_id,
            vec![planned("transport")],
            now(offset + 2),
        )
        .unwrap();
    ledger
        .transition(
            receipt.attempt_id,
            ReceiptState::Committing {
                phase: CommitPhase::Activate,
            },
            now(offset + 3),
        )
        .unwrap();
    ledger
        .transition(
            receipt.attempt_id,
            applied(
                vec![component("transport", ComponentState::Applied)],
                now(offset + 4),
            ),
            now(offset + 4),
        )
        .unwrap();
    revision
}

#[test]
fn identifiers_and_wire_values_are_lossless_across_json_and_rhai() {
    let attempt_id = AttemptId::new();
    let epoch = RuntimeEpoch::new();
    assert_eq!(attempt_id.as_uuid().get_version_num(), 7);
    assert_eq!(epoch.as_uuid().get_version_num(), 7);
    assert!(AttemptId::parse("00000000-0000-4000-8000-000000000000").is_err());

    let revision = RevisionId::new(u64::MAX).unwrap();
    let sequence = EventSequence::new(u64::MAX).unwrap();
    assert_eq!(
        serde_json::to_string(&revision).unwrap(),
        format!("\"{}\"", u64::MAX)
    );
    assert_eq!(
        serde_json::to_string(&sequence).unwrap(),
        format!("\"{}\"", u64::MAX)
    );
    assert!(serde_json::from_str::<RevisionId>("1").is_err());
    assert!(serde_json::from_str::<RevisionId>("\"01\"").is_err());

    let wire = MutationContextWire {
        attempt_id,
        runtime_epoch: epoch,
        revision: Some(revision),
        component_path: Some("generation.transport".into()),
        idempotency_keyed: true,
    };
    assert_wire(&wire);
    let wasm_value = serde_json::to_value(&wire).unwrap();
    assert!(wasm_value["revision"].is_string());
    assert!(wasm_value["attempt_id"].is_string());
    assert!(Timestamp::parse("2026-07-17T00:00:00+02:00").is_err());
}

#[test]
fn public_digest_is_redacted_while_keyed_semantics_detect_secret_changes() {
    let ledger = MutationLedger::new(LedgerConfig::default()).unwrap();
    let first = new_receipt(
        ledger
            .submit(
                submission(
                    Some("stable-key"),
                    None,
                    json!({"token": "secret-a", "tempo": 120}),
                    json!({"token": "<redacted>", "tempo": 120}),
                ),
                now(0),
            )
            .unwrap(),
    );
    let digest = first.request.submission_digest.clone().unwrap();
    assert!(digest.as_str().starts_with("sha256:"));
    assert_eq!(digest.as_str().len(), 71);

    let replay = ledger
        .submit(
            submission(
                Some("stable-key"),
                None,
                json!({"tempo": 120, "token": "secret-a"}),
                json!({"tempo": 120, "token": "<redacted>"}),
            ),
            now(1),
        )
        .unwrap();
    assert!(matches!(replay, SubmissionResult::Replayed(_)));
    assert_eq!(replay.receipt().attempt_id, first.attempt_id);
    assert_eq!(replay.receipt().event_sequence, first.event_sequence);

    let conflict = ledger
        .submit(
            submission(
                Some("stable-key"),
                None,
                json!({"token": "secret-b", "tempo": 120}),
                json!({"token": "<redacted>", "tempo": 120}),
            ),
            now(2),
        )
        .unwrap();
    assert!(matches!(conflict, SubmissionResult::Rejected(_)));
    assert_eq!(conflict.receipt().request.submission_digest, Some(digest));
    assert!(format!(
        "{:?}",
        submission(
            None,
            None,
            json!({"token": "do-not-log"}),
            json!({"token": "<redacted>"}),
        )
        .material
    )
    .contains("<redacted>"));
    assert!(!format!(
        "{:?}",
        submission(
            None,
            None,
            json!({"token": "do-not-log"}),
            json!({"token": "<redacted>"}),
        )
        .material
    )
    .contains("do-not-log"));

    let policy = MutationLedger::new(LedgerConfig::default()).unwrap();
    policy
        .submit(
            submission(
                Some("policy-key"),
                None,
                json!({"tempo": 120}),
                json!({"tempo": 120}),
            ),
            now(3),
        )
        .unwrap();
    let mut changed_policy = submission(
        Some("policy-key"),
        None,
        json!({"tempo": 120}),
        json!({"tempo": 120}),
    );
    changed_policy.atomicity = Atomicity::BestEffort;
    assert!(matches!(
        policy.submit(changed_policy, now(4)).unwrap().receipt().state,
        ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected { ref code, .. }))
            if code == "idempotency_conflict"
    ));
}

#[test]
fn simultaneous_identical_keys_with_distinct_private_semantics_are_linearizable() {
    const COUNT: usize = 12;
    let ledger = Arc::new(MutationLedger::new(LedgerConfig::default()).unwrap());
    let barrier = Arc::new(Barrier::new(COUNT));
    let handles = (0..COUNT)
        .map(|index| {
            let ledger = Arc::clone(&ledger);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let secret = if index % 2 == 0 { "alpha" } else { "beta" };
                barrier.wait();
                ledger
                    .submit(
                        submission(
                            Some("contended-key"),
                            None,
                            json!({"secret": secret, "tempo": 120}),
                            json!({"secret": "<redacted>", "tempo": 120}),
                        ),
                        now(0),
                    )
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, SubmissionResult::New(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, SubmissionResult::Replayed(_)))
            .count(),
        COUNT / 2 - 1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, SubmissionResult::Rejected(_)))
            .count(),
        COUNT / 2
    );
    let canonical_attempt = results
        .iter()
        .find_map(|result| match result {
            SubmissionResult::New(receipt) => Some(receipt.attempt_id),
            _ => None,
        })
        .unwrap();
    assert!(results.iter().all(|result| match result {
        SubmissionResult::New(receipt) | SubmissionResult::Replayed(receipt) => {
            receipt.attempt_id == canonical_attempt
        }
        SubmissionResult::Rejected(receipt) => matches!(
            receipt.state,
            ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected { ref code, .. }))
                if code == "idempotency_conflict"
        ),
    }));
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.receipt().request.submission_digest.as_ref())
            .map(PublicDigest::as_str)
            .collect::<BTreeSet<_>>()
            .len(),
        1
    );
}

#[test]
fn concurrent_allocation_is_unique_monotonic_and_gap_free() {
    const COUNT: usize = 24;
    let ledger = Arc::new(MutationLedger::new(LedgerConfig::default()).unwrap());
    let attempts = (0..COUNT)
        .map(|index| {
            new_receipt(
                ledger
                    .submit(
                        submission(None, None, json!({"index": index}), json!({"index": index})),
                        now(index as u64),
                    )
                    .unwrap(),
            )
            .attempt_id
        })
        .collect::<Vec<_>>();
    let barrier = Arc::new(Barrier::new(COUNT));
    let handles = attempts
        .into_iter()
        .map(|attempt_id| {
            let ledger = Arc::clone(&ledger);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                ledger
                    .accept(attempt_id, Some(&json!({"operation": "replace"})), now(100))
                    .unwrap()
                    .revision
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let mut revisions = handles
        .into_iter()
        .map(|handle| handle.join().unwrap().get())
        .collect::<Vec<_>>();
    revisions.sort_unstable();
    assert_eq!(revisions, (1..=COUNT as u64).collect::<Vec<_>>());

    let events = match ledger.events_after(ledger.runtime_epoch(), None, now(101)) {
        EventQueryResult::Events { events } => events,
        other => panic!("expected events, got {other:?}"),
    };
    let sequences = events
        .iter()
        .map(|event| event.event_sequence.get())
        .collect::<BTreeSet<_>>();
    assert_eq!(sequences.len(), COUNT * 2);
    assert_eq!(sequences.iter().next(), Some(&1));
    assert_eq!(sequences.iter().next_back(), Some(&(COUNT as u64 * 2)));
}

#[test]
fn transition_graph_and_terminal_component_invariants_are_closed() {
    let evaluating = ReceiptState::Evaluating {
        phase: PreAcceptancePhase::Decode,
    };
    assert!(validate_transition(
        &evaluating,
        &ReceiptState::Evaluating {
            phase: PreAcceptancePhase::Admission,
        }
    )
    .is_ok());
    assert!(validate_transition(
        &ReceiptState::Staging {
            completed: 1,
            total: 2,
        },
        &ReceiptState::Staging {
            completed: 0,
            total: 2,
        }
    )
    .is_err());
    let terminal = ReceiptState::Terminal(TerminalOutcome::Superseded(Superseded {
        reason: SupersessionReason::Cancelled,
        by_revision: None,
    }));
    assert!(validate_transition(&terminal, &evaluating).is_err());

    let states = vec![
        ReceiptState::Evaluating {
            phase: PreAcceptancePhase::Decode,
        },
        ReceiptState::Evaluating {
            phase: PreAcceptancePhase::Admission,
        },
        ReceiptState::Accepted {
            queue_position: None,
        },
        ReceiptState::Planning,
        ReceiptState::Staging {
            completed: 0,
            total: 2,
        },
        ReceiptState::Staging {
            completed: 1,
            total: 2,
        },
        ReceiptState::Committing {
            phase: CommitPhase::Reconcile,
        },
        ReceiptState::Committing {
            phase: CommitPhase::Rollback,
        },
        ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
            phase: FailurePhase::Planning,
            code: "rejected".into(),
            message: "rejected".into(),
            rollback: RollbackState::NotNeeded,
            preserved_revision: None,
        })),
        ReceiptState::Terminal(TerminalOutcome::Superseded(Superseded {
            reason: SupersessionReason::Cancelled,
            by_revision: None,
        })),
        applied(Vec::new(), now(0)),
        ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: FailurePhase::Activate,
            code: "partial".into(),
            components: Vec::new(),
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: None,
        })),
    ];
    let allowed = [
        &[0, 1, 2, 8, 9, 11][..],
        &[1, 2, 8, 9, 11],
        &[3, 8, 9, 11],
        &[4, 5, 6, 7, 8, 11],
        &[4, 5, 6, 7, 8, 11],
        &[5, 6, 7, 8, 11],
        &[6, 7, 8, 10, 11],
        &[7, 8, 10, 11],
        &[],
        &[],
        &[],
        &[],
    ];
    for (from_index, from) in states.iter().enumerate() {
        for (to_index, to) in states.iter().enumerate() {
            assert_eq!(
                validate_transition(from, to).is_ok(),
                allowed[from_index].contains(&to_index),
                "unexpected transition {from_index} -> {to_index}: {from:?} -> {to:?}"
            );
        }
    }

    let previous = Some(RevisionId::new(7).unwrap());
    let planned_components = vec![planned("transport"), planned("synths/kick")];
    let rejected = TerminalOutcome::Rejected(Rejected {
        phase: FailurePhase::Staging,
        code: "staging_failed".into(),
        message: "staging failed".into(),
        rollback: RollbackState::Confirmed,
        preserved_revision: previous,
    });
    assert!(validate_terminal(&rejected, previous, &planned_components).is_ok());

    let valid_partial = TerminalOutcome::Partial(Partial {
        phase: FailurePhase::Activate,
        code: "partial_apply".into(),
        components: vec![
            component("transport", ComponentState::Applied),
            component("synths/kick", ComponentState::Uncertain),
        ],
        rollback: RollbackState::Uncertain,
        fenced: true,
        last_confirmed_revision: previous,
    });
    assert!(validate_terminal(&valid_partial, previous, &planned_components).is_ok());

    let mut not_fenced = valid_partial.clone();
    let TerminalOutcome::Partial(partial) = &mut not_fenced else {
        unreachable!();
    };
    partial.fenced = false;
    assert!(validate_terminal(&not_fenced, previous, &planned_components).is_err());

    let all_failed = TerminalOutcome::Partial(Partial {
        phase: FailurePhase::Staging,
        code: "staging_failed".into(),
        components: vec![
            component("transport", ComponentState::Failed),
            component("synths/kick", ComponentState::NotStarted),
        ],
        rollback: RollbackState::NotNeeded,
        fenced: false,
        last_confirmed_revision: previous,
    });
    assert!(validate_terminal(&all_failed, previous, &planned_components).is_err());

    let invalid_rejected = TerminalOutcome::Rejected(Rejected {
        phase: FailurePhase::Rollback,
        code: "rollback_failed".into(),
        message: "rollback failed".into(),
        rollback: RollbackState::Failed,
        preserved_revision: previous,
    });
    assert!(validate_terminal(&invalid_rejected, previous, &planned_components).is_err());

    let incomplete = TerminalOutcome::Applied(Applied {
        effective_at: EffectiveAt {
            observed_at: Timestamp::from_system_time(now(0)),
            musical_beat: None,
            backend_time_seconds: None,
        },
        confirmations: Vec::new(),
        components: vec![component("transport", ComponentState::Applied)],
        audible_tail_until: None,
    });
    assert!(validate_terminal(&incomplete, previous, &planned_components).is_err());

    let unconfirmed = TerminalOutcome::Applied(Applied {
        effective_at: EffectiveAt {
            observed_at: Timestamp::from_system_time(now(0)),
            musical_beat: None,
            backend_time_seconds: None,
        },
        confirmations: Vec::new(),
        components: vec![
            component("transport", ComponentState::Applied),
            component("synths/kick", ComponentState::Applied),
        ],
        audible_tail_until: None,
    });
    assert!(validate_terminal(&unconfirmed, previous, &planned_components).is_err());
}

#[test]
fn evaluating_partial_records_eager_effect_evidence_without_allocating_a_revision() {
    let ledger = MutationLedger::new(LedgerConfig::default()).unwrap();
    let evaluating = new_receipt(
        ledger
            .submit(
                submission(
                    None,
                    None,
                    json!({"synthdef": "eager"}),
                    json!({"synthdef": "eager"}),
                ),
                now(0),
            )
            .unwrap(),
    );

    assert!(matches!(
        ledger.transition(
            evaluating.attempt_id,
            partial(
                FailurePhase::Evaluate,
                "legacy_eager_effect",
                Vec::new(),
                RollbackState::Unavailable,
                true,
                None,
            ),
            now(1),
        ),
        Err(LedgerError::InvalidReceipt(_))
    ));
    let invalid_component = ComponentOutcome {
        path: String::new(),
        action: "deploy".into(),
        state: ComponentState::Applied,
        effective_at: None,
        confirmation: None,
        diagnostic: None,
    };
    assert!(matches!(
        ledger.transition(
            evaluating.attempt_id,
            partial(
                FailurePhase::Evaluate,
                "legacy_eager_effect",
                vec![invalid_component],
                RollbackState::Unavailable,
                true,
                None,
            ),
            now(2),
        ),
        Err(LedgerError::InvalidReceipt(_))
    ));
    assert!(matches!(
        ledger.transition(
            evaluating.attempt_id,
            partial(
                FailurePhase::Evaluate,
                "legacy_eager_effect",
                vec![component_action(
                    "synthdef/eager",
                    "deploy",
                    ComponentState::Applied,
                )],
                RollbackState::Unavailable,
                false,
                None,
            ),
            now(3),
        ),
        Err(LedgerError::InvalidReceipt(_))
    ));
    let unchanged = ledger.receipt(evaluating.attempt_id).unwrap();
    assert!(matches!(unchanged.state, ReceiptState::Evaluating { .. }));
    assert_eq!(unchanged.revision, None);

    let terminal = ledger
        .transition(
            evaluating.attempt_id,
            partial(
                FailurePhase::Evaluate,
                "legacy_eager_effect",
                vec![component_action(
                    "synthdef/eager",
                    "deploy",
                    ComponentState::Applied,
                )],
                RollbackState::Unavailable,
                true,
                None,
            ),
            now(4),
        )
        .unwrap();
    assert_eq!(terminal.revision, None);
    assert!(matches!(
        terminal.state,
        ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            fenced: true,
            last_confirmed_revision: None,
            ..
        }))
    ));
    let status = ledger.status(now(4));
    assert_eq!(status.accepted_through, None);
    assert_eq!(status.last_confirmed_revision, None);
    assert_eq!(
        status.live_state,
        LiveState::PreAdmissionPartial {
            attempt_id: evaluating.attempt_id,
            fenced: true,
        }
    );
    assert!(matches!(
        ledger.accept(evaluating.attempt_id, None, now(5)),
        Err(LedgerError::InvalidTransition(_))
    ));
}

#[test]
fn accepted_partial_retains_its_revision_and_fences_dispatch_uncertainty() {
    let ledger = MutationLedger::new(LedgerConfig::default()).unwrap();
    let confirmed = apply_one(&ledger, None, 0);
    let evaluating = new_receipt(
        ledger
            .submit(
                submission(
                    None,
                    Some(confirmed),
                    json!({"dispatch": "backend"}),
                    json!({"dispatch": "backend"}),
                ),
                now(10),
            )
            .unwrap(),
    );
    let accepted = ledger.accept(evaluating.attempt_id, None, now(11)).unwrap();
    let assigned_revision = accepted.revision.unwrap();

    assert!(matches!(
        ledger.transition(
            evaluating.attempt_id,
            partial(
                FailurePhase::Admission,
                "dispatch_uncertain",
                vec![component_action(
                    "backend/dispatch",
                    "dispatch",
                    ComponentState::Uncertain,
                )],
                RollbackState::Uncertain,
                true,
                None,
            ),
            now(12),
        ),
        Err(LedgerError::InvalidReceipt(_))
    ));
    let mut duplicate_action =
        component_action("backend/dispatch", "dispatch", ComponentState::NotStarted);
    duplicate_action.action = "retry".into();
    assert!(matches!(
        ledger.transition(
            evaluating.attempt_id,
            partial(
                FailurePhase::Admission,
                "dispatch_uncertain",
                vec![
                    component_action("backend/dispatch", "dispatch", ComponentState::Uncertain,),
                    duplicate_action,
                ],
                RollbackState::Uncertain,
                true,
                Some(confirmed),
            ),
            now(13),
        ),
        Err(LedgerError::InvalidReceipt(_))
    ));
    assert!(matches!(
        ledger.transition(
            evaluating.attempt_id,
            partial(
                FailurePhase::Admission,
                "dispatch_uncertain",
                vec![component_action(
                    "backend/dispatch",
                    "dispatch",
                    ComponentState::Applied,
                )],
                RollbackState::Unavailable,
                false,
                Some(confirmed),
            ),
            now(14),
        ),
        Err(LedgerError::InvalidReceipt(_))
    ));
    let unchanged = ledger.receipt(evaluating.attempt_id).unwrap();
    assert!(matches!(unchanged.state, ReceiptState::Accepted { .. }));
    assert_eq!(unchanged.revision, Some(assigned_revision));

    let terminal = ledger
        .transition(
            evaluating.attempt_id,
            partial(
                FailurePhase::Admission,
                "dispatch_uncertain",
                vec![component_action(
                    "backend/dispatch",
                    "dispatch",
                    ComponentState::Uncertain,
                )],
                RollbackState::Uncertain,
                true,
                Some(confirmed),
            ),
            now(15),
        )
        .unwrap();
    assert_eq!(terminal.revision, Some(assigned_revision));
    assert_eq!(terminal.previous_confirmed_revision, Some(confirmed));
    assert!(matches!(
        terminal.state,
        ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            fenced: true,
            last_confirmed_revision: Some(revision),
            ..
        })) if revision == confirmed
    ));
    let status = ledger.status(now(15));
    assert_eq!(status.accepted_through, Some(assigned_revision));
    assert_eq!(status.last_confirmed_revision, Some(confirmed));
    assert_eq!(
        status.live_state,
        LiveState::Partial {
            revision: assigned_revision,
            fenced: true,
        }
    );
    assert!(matches!(
        ledger.transition(
            evaluating.attempt_id,
            ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
                phase: FailurePhase::Admission,
                code: "rewrite".into(),
                message: "rewrite".into(),
                rollback: RollbackState::Confirmed,
                preserved_revision: Some(confirmed),
            })),
            now(16),
        ),
        Err(LedgerError::InvalidTransition(_))
    ));
}

#[test]
fn idempotency_is_linearizable_and_capacity_is_explicit() {
    const COUNT: usize = 12;
    let ledger = Arc::new(MutationLedger::new(LedgerConfig::default()).unwrap());
    let barrier = Arc::new(Barrier::new(COUNT));
    let handles = (0..COUNT)
        .map(|_| {
            let ledger = Arc::clone(&ledger);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                ledger
                    .submit(
                        submission(
                            Some("same"),
                            None,
                            json!({"tempo": 120}),
                            json!({"tempo": 120}),
                        ),
                        now(0),
                    )
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, SubmissionResult::New(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, SubmissionResult::Replayed(_)))
            .count(),
        COUNT - 1
    );
    assert_eq!(
        results
            .iter()
            .map(|result| result.receipt().attempt_id)
            .collect::<BTreeSet<_>>()
            .len(),
        1
    );

    let capacity = MutationLedger::new(LedgerConfig {
        minimum_event_count: 1,
        minimum_event_age: Duration::ZERO,
        idempotency_capacity: 1,
    })
    .unwrap();
    assert!(matches!(
        capacity
            .submit(
                submission(Some("one"), None, json!({"x": 1}), json!({"x": 1})),
                now(0)
            )
            .unwrap(),
        SubmissionResult::New(_)
    ));
    let rejected = capacity
        .submit(
            submission(Some("two"), None, json!({"x": 2}), json!({"x": 2})),
            now(1),
        )
        .unwrap();
    assert!(matches!(rejected, SubmissionResult::Rejected(_)));
    assert!(matches!(
        rejected.receipt().state,
        ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected { ref code, .. }))
            if code == "idempotency_capacity_exhausted"
    ));
}

#[test]
fn expected_revision_and_cancellation_races_have_one_coherent_winner() {
    let ledger = MutationLedger::new(LedgerConfig::default()).unwrap();
    let first = apply_one(&ledger, None, 0);
    let left = new_receipt(
        ledger
            .submit(
                submission(
                    None,
                    Some(first),
                    json!({"value": "left"}),
                    json!({"value": "left"}),
                ),
                now(10),
            )
            .unwrap(),
    );
    let right = new_receipt(
        ledger
            .submit(
                submission(
                    None,
                    Some(first),
                    json!({"value": "right"}),
                    json!({"value": "right"}),
                ),
                now(11),
            )
            .unwrap(),
    );
    let left = ledger.accept(left.attempt_id, None, now(12)).unwrap();
    let right = ledger.accept(right.attempt_id, None, now(13)).unwrap();
    ledger
        .begin_planning(left.attempt_id, vec![planned("left")], now(14))
        .unwrap();
    ledger
        .transition(
            left.attempt_id,
            ReceiptState::Committing {
                phase: CommitPhase::Activate,
            },
            now(15),
        )
        .unwrap();
    ledger
        .transition(
            left.attempt_id,
            applied(vec![component("left", ComponentState::Applied)], now(16)),
            now(16),
        )
        .unwrap();
    let conflict = ledger
        .begin_planning(right.attempt_id, vec![planned("right")], now(17))
        .unwrap();
    assert!(matches!(
        conflict.state,
        ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
            ref code,
            preserved_revision,
            ..
        })) if code == "revision_conflict" && preserved_revision == left.revision
    ));
    let status = ledger.status(now(17));
    assert_eq!(status.last_confirmed_revision, left.revision);
    assert_eq!(status.last_rejected_revision, right.revision);

    for iteration in 0..24 {
        let candidate = new_receipt(
            ledger
                .submit(
                    submission(
                        None,
                        None,
                        json!({"iteration": iteration}),
                        json!({"iteration": iteration}),
                    ),
                    now(100 + iteration),
                )
                .unwrap(),
        );
        ledger.accept(candidate.attempt_id, None, now(200)).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let planning_ledger = ledger.clone();
        let cancellation_ledger = ledger.clone();
        let planning_barrier = Arc::clone(&barrier);
        let cancellation_barrier = Arc::clone(&barrier);
        let attempt_id = candidate.attempt_id;
        let planner = thread::spawn(move || {
            planning_barrier.wait();
            planning_ledger.begin_planning(attempt_id, vec![planned("race")], now(201))
        });
        let canceller = thread::spawn(move || {
            cancellation_barrier.wait();
            cancellation_ledger.cancel(attempt_id, now(201))
        });
        let planning = planner.join().unwrap();
        let cancellation = canceller.join().unwrap();
        let final_receipt = ledger.receipt(attempt_id).unwrap();
        match final_receipt.state {
            ReceiptState::Planning => {
                assert!(planning.is_ok());
                assert!(matches!(
                    cancellation,
                    CancelResult::Rejected(MutationError { ref code, .. })
                        if code == "too_late_to_cancel"
                ));
                ledger
                    .transition(
                        attempt_id,
                        ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
                            phase: FailurePhase::Planning,
                            code: "test_cleanup".into(),
                            message: "test cleanup".into(),
                            rollback: RollbackState::NotNeeded,
                            preserved_revision: status.last_confirmed_revision,
                        })),
                        now(202),
                    )
                    .unwrap();
            }
            ReceiptState::Terminal(TerminalOutcome::Superseded(_)) => {
                assert!(planning.is_err());
                assert!(matches!(cancellation, CancelResult::Receipt(_)));
            }
            other => panic!("incoherent cancellation race result: {other:?}"),
        }
    }
}

#[test]
fn retention_tombstones_gaps_epoch_reset_and_restart_are_explicit() {
    let ledger = MutationLedger::new(LedgerConfig {
        minimum_event_count: 2,
        minimum_event_age: Duration::ZERO,
        idempotency_capacity: 4,
    })
    .unwrap();
    let old_epoch = ledger.runtime_epoch();
    let keyed = new_receipt(
        ledger
            .submit(
                submission(
                    Some("retained-key"),
                    None,
                    json!({"secret": "a"}),
                    json!({"secret": "<redacted>"}),
                ),
                now(0),
            )
            .unwrap(),
    );
    assert!(matches!(
        ledger.cancel(keyed.attempt_id, now(1)),
        CancelResult::Receipt(_)
    ));
    for offset in 2..6 {
        let receipt = new_receipt(
            ledger
                .submit(
                    submission(
                        None,
                        None,
                        json!({"offset": offset}),
                        json!({"offset": offset}),
                    ),
                    now(offset),
                )
                .unwrap(),
        );
        let _ = ledger.cancel(receipt.attempt_id, now(offset));
    }
    ledger.prune(now(10)).unwrap();
    assert!(ledger.receipt(keyed.attempt_id).is_err());
    let expired = ledger
        .submit(
            submission(
                Some("retained-key"),
                None,
                json!({"secret": "changed-after-reset"}),
                json!({"secret": "<redacted>"}),
            ),
            now(11),
        )
        .unwrap();
    assert!(matches!(
        expired.receipt().state,
        ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected { ref code, .. }))
            if code == "idempotency_key_expired"
    ));

    assert!(matches!(
        ledger.events_after(old_epoch, Some(EventSequence::new(1).unwrap()), now(12)),
        EventQueryResult::ResetRequired {
            reason: ResetReason::RetentionExpired,
            ..
        }
    ));
    let last = ledger.status(now(12)).event_sequence.unwrap();
    assert!(matches!(
        ledger.events_after(old_epoch, Some(last.checked_next().unwrap()), now(12)),
        EventQueryResult::ResetRequired {
            reason: ResetReason::SequenceAhead,
            ..
        }
    ));

    let new_epoch = ledger.reset().unwrap();
    assert_ne!(new_epoch, old_epoch);
    assert!(matches!(
        ledger.events_after(old_epoch, None, now(13)),
        EventQueryResult::ResetRequired {
            reason: ResetReason::RuntimeEpochChanged,
            ..
        }
    ));
    let mut stale_retry = submission(
        Some("retained-key"),
        None,
        json!({"secret": "a"}),
        json!({"secret": "<redacted>"}),
    );
    stale_retry.retry_epoch = Some(old_epoch);
    let rejected = ledger.submit(stale_retry, now(14)).unwrap();
    assert!(matches!(
        rejected.receipt().state,
        ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected { ref code, .. }))
            if code == "runtime_epoch_changed"
    ));

    let omitted_epoch_retry = ledger
        .submit(
            submission(
                Some("retained-key"),
                None,
                json!({"secret": "changed-again-after-reset"}),
                json!({"secret": "<redacted>"}),
            ),
            now(15),
        )
        .unwrap();
    assert!(matches!(
        omitted_epoch_retry.receipt().state,
        ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected { ref code, .. }))
            if code == "runtime_epoch_changed"
    ));

    let fresh = new_receipt(
        ledger
            .submit(
                submission(
                    Some("post-reset-key"),
                    None,
                    json!({"secret": "b"}),
                    json!({"secret": "<redacted>"}),
                ),
                now(16),
            )
            .unwrap(),
    );
    assert_eq!(fresh.runtime_epoch, new_epoch);

    let mut other_caller = submission(
        Some("retained-key"),
        None,
        json!({"secret": "c"}),
        json!({"secret": "<redacted>"}),
    );
    other_caller.caller_namespace = "other-caller".into();
    assert!(matches!(
        ledger.submit(other_caller, now(17)).unwrap(),
        SubmissionResult::New(_)
    ));
}

#[test]
fn reset_and_omitted_epoch_retry_are_linearized_without_new_admission() {
    const ITERATIONS: usize = 32;
    for iteration in 0..ITERATIONS {
        let ledger = Arc::new(MutationLedger::new(LedgerConfig::default()).unwrap());
        let original = new_receipt(
            ledger
                .submit(
                    submission(
                        Some("reset-race-key"),
                        None,
                        json!({"iteration": iteration}),
                        json!({"iteration": iteration}),
                    ),
                    now(iteration as u64),
                )
                .unwrap(),
        );
        let barrier = Arc::new(Barrier::new(2));
        let reset_ledger = Arc::clone(&ledger);
        let retry_ledger = Arc::clone(&ledger);
        let reset_barrier = Arc::clone(&barrier);
        let retry_barrier = Arc::clone(&barrier);
        let resetter = thread::spawn(move || {
            reset_barrier.wait();
            reset_ledger.reset().unwrap()
        });
        let retry = thread::spawn(move || {
            retry_barrier.wait();
            retry_ledger
                .submit(
                    submission(
                        Some("reset-race-key"),
                        None,
                        json!({"iteration": iteration}),
                        json!({"iteration": iteration}),
                    ),
                    now(iteration as u64 + 1),
                )
                .unwrap()
        });
        let new_epoch = resetter.join().unwrap();
        let retry = retry.join().unwrap();
        match retry {
            SubmissionResult::Replayed(receipt) => {
                assert_eq!(receipt.attempt_id, original.attempt_id);
                assert_eq!(receipt.runtime_epoch, original.runtime_epoch);
            }
            SubmissionResult::Rejected(receipt) => assert!(matches!(
                receipt.state,
                ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
                    ref code,
                    ..
                })) if code == "runtime_epoch_changed"
            )),
            SubmissionResult::New(receipt) => {
                panic!("reset race silently readmitted attempt {receipt:?}")
            }
        }
        assert_eq!(ledger.runtime_epoch(), new_epoch);
        let after_boundary = ledger
            .submit(
                submission(
                    Some("reset-race-key"),
                    None,
                    json!({"iteration": iteration}),
                    json!({"iteration": iteration}),
                ),
                now(iteration as u64 + 2),
            )
            .unwrap();
        assert!(matches!(
            after_boundary.receipt().state,
            ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected { ref code, .. }))
                if code == "runtime_epoch_changed"
        ));
    }
}

#[test]
fn cross_epoch_identity_evidence_is_bounded_and_fails_closed() {
    let ledger = MutationLedger::new(LedgerConfig {
        minimum_event_count: 1,
        minimum_event_age: Duration::ZERO,
        idempotency_capacity: 2,
    })
    .unwrap();
    assert!(matches!(
        ledger
            .submit(
                submission(Some("epoch-one"), None, json!({"x": 1}), json!({"x": 1})),
                now(0),
            )
            .unwrap(),
        SubmissionResult::New(_)
    ));
    ledger.reset().unwrap();
    assert!(matches!(
        ledger
            .submit(
                submission(Some("epoch-two"), None, json!({"x": 2}), json!({"x": 2})),
                now(1),
            )
            .unwrap(),
        SubmissionResult::New(_)
    ));
    ledger.reset().unwrap();

    for (offset, key) in ["epoch-one", "epoch-two"].into_iter().enumerate() {
        let retained = ledger
            .submit(
                submission(Some(key), None, json!({"x": offset}), json!({"x": offset})),
                now(offset as u64 + 2),
            )
            .unwrap();
        assert!(matches!(
            retained.receipt().state,
            ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected { ref code, .. }))
                if code == "runtime_epoch_changed"
        ));
    }

    let exhausted = ledger
        .submit(
            submission(Some("epoch-three"), None, json!({"x": 3}), json!({"x": 3})),
            now(4),
        )
        .unwrap();
    assert!(matches!(
        exhausted.receipt().state,
        ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected { ref code, .. }))
            if code == "idempotency_capacity_exhausted"
    ));
}

#[test]
fn mutation_context_preserves_identity_revision_and_sinks_across_children() {
    let ledger = MutationLedger::new(LedgerConfig::default()).unwrap();
    let receipt = new_receipt(
        ledger
            .submit(
                submission(None, None, json!({"x": 1}), json!({"x": 1})),
                now(0),
            )
            .unwrap(),
    );
    let event = match ledger.events_after(ledger.runtime_epoch(), None, now(0)) {
        EventQueryResult::Events { events } => events.into_iter().next().unwrap(),
        other => panic!("expected events, got {other:?}"),
    };
    let replies = Arc::new(AtomicUsize::new(0));
    let events = Arc::new(AtomicUsize::new(0));
    let reply_count = Arc::clone(&replies);
    let event_count = Arc::clone(&events);
    let context = MutationContext::new(
        receipt.attempt_id,
        receipt.runtime_epoch,
        false,
        MutationReplySink::new(move |_| {
            reply_count.fetch_add(1, Ordering::SeqCst);
        }),
        MutationEventSink::new(move |_| {
            event_count.fetch_add(1, Ordering::SeqCst);
        }),
    );
    let revision = RevisionId::new(9).unwrap();
    let child = context
        .with_revision(revision)
        .unwrap()
        .for_component("generation.transport");
    assert_eq!(child.attempt_id(), context.attempt_id());
    assert_eq!(child.runtime_epoch(), context.runtime_epoch());
    assert_eq!(child.revision(), Some(revision));
    assert_eq!(
        child.wire().component_path.as_deref(),
        Some("generation.transport")
    );
    assert!(child.with_revision(RevisionId::new(10).unwrap()).is_err());
    child.reply(receipt);
    child.event(event);
    assert_eq!(replies.load(Ordering::SeqCst), 1);
    assert_eq!(events.load(Ordering::SeqCst), 1);
}

#[test]
fn canonical_receipt_status_error_capability_and_event_wires_round_trip() {
    let ledger = MutationLedger::new(LedgerConfig::default()).unwrap();
    let revision = apply_one(&ledger, None, 0);
    let status = ledger.status(now(5));
    let events = match ledger.events_after(ledger.runtime_epoch(), None, now(5)) {
        EventQueryResult::Events { events } => events,
        other => panic!("expected events, got {other:?}"),
    };
    let receipt = events.last().unwrap().receipt.clone();
    assert_eq!(receipt.revision, Some(revision));
    assert_wire(&receipt);
    assert_wire(&status);
    assert_wire(events.last().unwrap());
    assert_wire(&EventQueryResult::Events { events });
    assert_wire(&ledger.capabilities());

    let error = match ledger.cancel(AttemptId::new(), now(6)) {
        CancelResult::Rejected(error) => error,
        other => panic!("expected receipt_not_found, got {other:?}"),
    };
    assert_wire(&error);

    let unknown = serde_json::json!({
        "state": "planning",
        "unexpected": true
    });
    assert!(serde_json::from_value::<ReceiptState>(unknown).is_err());
}
