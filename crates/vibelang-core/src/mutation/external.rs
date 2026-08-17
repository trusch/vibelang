//! Explicit best-effort submission model for external effects (M09).
//!
//! Filesystem, process, network, MIDI, and file effects can never hide
//! inside a Candidate — candidate authoring already rejects
//! `EffectDomain::External` declarations — and they never enter the
//! ledger implicitly either: each effect is accepted only as an explicit
//! caller-issued best-effort submission carrying its own caller-provided
//! idempotency key. A submission that tries to carry external operations
//! inside a Candidate rejects before any ledger work, so no revision is
//! ever allocated for it, and the runtime never derives parent or child
//! receipts from an external effect: every element of an explicitly
//! sequenced external-effect/Candidate pair is its own top-level attempt.

use super::digest::{DigestError, RequestMaterial};
use super::ledger::{LedgerError, MutationLedger, Submission, SubmissionResult};
use super::wire::{
    Atomicity, ExternalDomain, MessageDomain, MutationKind, MutationSource, RuntimeEpoch,
    SupersessionPolicy,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt;
use std::time::SystemTime;
use thiserror::Error;

/// The effect families that may only enter the runtime as explicit
/// caller-issued best-effort submissions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExternalEffectDomain {
    Filesystem,
    Process,
    Network,
    Midi,
    File,
}

impl ExternalEffectDomain {
    pub const ALL: [Self; 5] = [
        Self::Filesystem,
        Self::Process,
        Self::Network,
        Self::Midi,
        Self::File,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Filesystem => "filesystem",
            Self::Process => "process",
            Self::Network => "network",
            Self::Midi => "midi",
            Self::File => "file",
        }
    }
}

impl fmt::Display for ExternalEffectDomain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
pub enum ExternalEffectError {
    #[error(
        "external {domain} effect '{operation}' requires a caller-provided idempotency key; \
         the runtime never issues one implicitly"
    )]
    MissingIdempotencyKey {
        domain: ExternalEffectDomain,
        operation: String,
    },
    #[error(
        "external-effect idempotency key {key:?} must be at most 255 bytes without control bytes"
    )]
    InvalidIdempotencyKey { key: String },
    #[error(
        "external effect operation name must be a non-empty dotted ASCII token of at most \
         128 bytes, got {0:?}"
    )]
    InvalidOperation(String),
    #[error("external-effect submissions require a non-empty caller namespace")]
    MissingCallerNamespace,
    #[error(
        "external {domain} effect '{operation}' cannot ride inside a required-atomic Candidate \
         submission; sequence it as its own explicit best-effort submission"
    )]
    MixedRequiredAtomicSubmission {
        domain: ExternalEffectDomain,
        operation: String,
    },
    #[error(
        "external {domain} effect '{operation}' cannot be embedded in a Candidate submission \
         even under best effort; every external effect is its own keyed submission"
    )]
    EmbeddedExternalEffect {
        domain: ExternalEffectDomain,
        operation: String,
    },
    #[error("a sequenced external-effect plan seats a Candidate submission, got a {0} submission")]
    NotACandidate(String),
    #[error(
        "idempotency key {key:?} repeats across the explicitly sequenced \
         external-effect/Candidate pair; every element carries its own distinct key"
    )]
    DuplicateIdempotencyKey { key: String },
    #[error(
        "external effects are caller-issued top-level submissions; the runtime never creates \
         internal parent/child receipts for them"
    )]
    InternalSourceForbidden,
    #[error(transparent)]
    Digest(#[from] DigestError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
}

fn validate_operation(operation: &str) -> Result<(), ExternalEffectError> {
    let valid = !operation.is_empty()
        && operation.len() <= 128
        && operation
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        && !operation.starts_with('.')
        && !operation.ends_with('.');
    if valid {
        Ok(())
    } else {
        Err(ExternalEffectError::InvalidOperation(operation.into()))
    }
}

/// One concrete external operation: its domain, a stable operation name,
/// and the canonical request material the receipt digest is built from.
#[derive(Clone, Debug)]
pub struct ExternalEffectOperation {
    domain: ExternalEffectDomain,
    operation: String,
    material: RequestMaterial,
}

impl ExternalEffectOperation {
    pub fn new<T, R>(
        domain: ExternalEffectDomain,
        operation: impl Into<String>,
        semantic: &T,
        public_redacted: Option<&R>,
    ) -> Result<Self, ExternalEffectError>
    where
        T: Serialize,
        R: Serialize,
    {
        let operation = operation.into();
        validate_operation(&operation)?;
        Ok(Self {
            domain,
            operation,
            material: RequestMaterial::new(semantic, public_redacted)?,
        })
    }

    #[must_use]
    pub const fn domain(&self) -> ExternalEffectDomain {
        self.domain
    }

    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// The domain-qualified operation spelling used for receipt identity.
    #[must_use]
    pub fn qualified_operation(&self) -> String {
        format!("external.{}.{}", self.domain.as_str(), self.operation)
    }

    /// Project this operation onto the mutation wire vocabulary.
    ///
    /// MIDI has an exact v1 message domain and file effects land on the
    /// Recording domain that owns v1 file I/O. Filesystem, process, and
    /// network effects have no music domain to borrow and address the
    /// wire through the dedicated `MutationKind::External` variant, so
    /// every external operation projects totally — no receipt is ever
    /// mislabeled under an unrelated music domain.
    #[must_use]
    pub fn wire_kind(&self) -> MutationKind {
        let operation = self.qualified_operation();
        match self.domain {
            ExternalEffectDomain::Midi => MutationKind::Command {
                domain: MessageDomain::Midi,
                operation,
            },
            ExternalEffectDomain::File => MutationKind::Command {
                domain: MessageDomain::Recording,
                operation,
            },
            ExternalEffectDomain::Filesystem => MutationKind::External {
                domain: ExternalDomain::Filesystem,
                operation,
            },
            ExternalEffectDomain::Process => MutationKind::External {
                domain: ExternalDomain::Process,
                operation,
            },
            ExternalEffectDomain::Network => MutationKind::External {
                domain: ExternalDomain::Network,
                operation,
            },
        }
    }
}

/// An explicit caller-issued external-effect submission.
///
/// Construction is the acceptance boundary: a usable caller-provided
/// idempotency key is mandatory, the source can never be the internal
/// parent/child spelling, and the produced ledger submission is always
/// best-effort — there is deliberately no way to request required
/// atomicity for an external effect.
#[derive(Clone, Debug)]
pub struct ExternalEffectSubmission {
    operation: ExternalEffectOperation,
    idempotency_key: String,
    source: MutationSource,
    caller_namespace: String,
}

impl ExternalEffectSubmission {
    pub fn new(
        operation: ExternalEffectOperation,
        idempotency_key: impl Into<String>,
        source: MutationSource,
        caller_namespace: impl Into<String>,
    ) -> Result<Self, ExternalEffectError> {
        let idempotency_key = idempotency_key.into();
        if idempotency_key.trim().is_empty() {
            return Err(ExternalEffectError::MissingIdempotencyKey {
                domain: operation.domain,
                operation: operation.operation,
            });
        }
        if idempotency_key.len() > 255 || idempotency_key.bytes().any(|byte| byte < 0x20) {
            return Err(ExternalEffectError::InvalidIdempotencyKey {
                key: idempotency_key,
            });
        }
        if matches!(source, MutationSource::Internal { .. }) {
            return Err(ExternalEffectError::InternalSourceForbidden);
        }
        let caller_namespace = caller_namespace.into();
        if caller_namespace.trim().is_empty() {
            return Err(ExternalEffectError::MissingCallerNamespace);
        }
        Ok(Self {
            operation,
            idempotency_key,
            source,
            caller_namespace,
        })
    }

    #[must_use]
    pub fn operation(&self) -> &ExternalEffectOperation {
        &self.operation
    }

    #[must_use]
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    /// The ledger submission this effect enters as: keyed, top-level, and
    /// best-effort only.
    #[must_use]
    pub fn submission(&self, retry_epoch: RuntimeEpoch) -> Submission {
        Submission {
            kind: self.operation.wire_kind(),
            source: self.source.clone(),
            caller_namespace: self.caller_namespace.clone(),
            idempotency_key: Some(self.idempotency_key.clone()),
            require_idempotency_key: true,
            retry_epoch: Some(retry_epoch),
            expected_revision: None,
            atomicity: Atomicity::BestEffort,
            supersession: SupersessionPolicy::Fifo,
            material: self.operation.material.clone(),
        }
    }
}

/// Submit one standalone external effect as its own top-level attempt.
pub fn submit_external_effect(
    ledger: &MutationLedger,
    effect: &ExternalEffectSubmission,
    now: SystemTime,
) -> Result<SubmissionResult, ExternalEffectError> {
    let submission = effect.submission(ledger.runtime_epoch());
    Ok(ledger.submit(submission, now)?)
}

/// The Candidate seat of a sequenced plan.
///
/// Construction is the mixed-submission boundary: a Candidate submission
/// never carries external-effect operations. The rejection happens here,
/// before any ledger call, so no attempt exists and no revision can have
/// been allocated for a mixed submission.
#[derive(Clone, Debug)]
pub struct CandidateSubmission {
    submission: Submission,
}

impl CandidateSubmission {
    /// Wrap a pure Candidate submission.
    pub fn new(submission: Submission) -> Result<Self, ExternalEffectError> {
        if !matches!(submission.kind, MutationKind::Candidate { .. }) {
            let kind = match &submission.kind {
                MutationKind::Candidate { .. } => unreachable!("candidate kinds pass through"),
                MutationKind::Command { .. } => "command",
                MutationKind::External { .. } => "external",
                MutationKind::Compensation { .. } => "compensation",
            };
            return Err(ExternalEffectError::NotACandidate(kind.into()));
        }
        Ok(Self { submission })
    }

    /// The mixed-submission spelling: explicitly requesting external
    /// operations inside one Candidate submission. This rejects whenever
    /// any operation is present — under `Required` atomicity as a mixed
    /// required-atomic/external-effect submission, and under `BestEffort`
    /// because external effects enter only as their own keyed
    /// submissions — always before any ledger work.
    pub fn with_embedded_external(
        submission: Submission,
        operations: Vec<ExternalEffectOperation>,
    ) -> Result<Self, ExternalEffectError> {
        let seat = Self::new(submission)?;
        if let Some(first) = operations.into_iter().next() {
            return Err(match seat.submission.atomicity {
                Atomicity::Required => ExternalEffectError::MixedRequiredAtomicSubmission {
                    domain: first.domain,
                    operation: first.operation,
                },
                Atomicity::BestEffort => ExternalEffectError::EmbeddedExternalEffect {
                    domain: first.domain,
                    operation: first.operation,
                },
            });
        }
        Ok(seat)
    }

    #[must_use]
    pub fn submission(&self) -> &Submission {
        &self.submission
    }
}

/// An explicitly sequenced external-effect/Candidate pair: external
/// submissions the caller orders before and after one Candidate
/// submission. Sequencing is explicit ordering, never containment — each
/// element is its own top-level attempt with its own receipt.
#[derive(Clone, Debug)]
pub struct SequencedExternalPlan {
    before: Vec<ExternalEffectSubmission>,
    candidate: CandidateSubmission,
    after: Vec<ExternalEffectSubmission>,
}

/// What a sequenced plan actually did: one result per submitted element
/// in caller order, plus the count of elements that were never submitted
/// because an earlier element rejected. The runtime does not invent
/// receipts (child work) for the unsubmitted tail.
#[derive(Debug)]
pub struct SequencedSubmissionOutcome {
    pub results: Vec<SubmissionResult>,
    pub unsubmitted: usize,
}

impl SequencedExternalPlan {
    /// Validate the pair before any ledger work: every external element
    /// carries its own key by construction, and no key repeats across the
    /// pair — including the Candidate's optional key. A repeated key
    /// rejects here, with zero attempts created.
    pub fn new(
        before: Vec<ExternalEffectSubmission>,
        candidate: CandidateSubmission,
        after: Vec<ExternalEffectSubmission>,
    ) -> Result<Self, ExternalEffectError> {
        let mut keys = BTreeSet::new();
        let candidate_key = candidate.submission.idempotency_key.as_deref();
        if let Some(key) = candidate_key {
            keys.insert(key.to_string());
        }
        for effect in before.iter().chain(after.iter()) {
            if !keys.insert(effect.idempotency_key.clone()) {
                return Err(ExternalEffectError::DuplicateIdempotencyKey {
                    key: effect.idempotency_key.clone(),
                });
            }
        }
        Ok(Self {
            before,
            candidate,
            after,
        })
    }

    /// Submit every element in caller order, each as its own top-level
    /// attempt. A ledger-rejected element halts the tail: later elements
    /// are counted as unsubmitted instead of becoming runtime-created
    /// child attempts.
    pub fn submit(
        self,
        ledger: &MutationLedger,
        now: SystemTime,
    ) -> Result<SequencedSubmissionOutcome, ExternalEffectError> {
        let epoch = ledger.runtime_epoch();
        let mut ordered = Vec::new();
        for effect in &self.before {
            ordered.push(effect.submission(epoch));
        }
        ordered.push(self.candidate.submission.clone());
        for effect in &self.after {
            ordered.push(effect.submission(epoch));
        }

        let mut results = Vec::new();
        let mut unsubmitted = 0;
        let mut halted = false;
        for submission in ordered {
            if halted {
                unsubmitted += 1;
                continue;
            }
            let result = ledger.submit(submission, now)?;
            halted = matches!(result, SubmissionResult::Rejected(_));
            results.push(result);
        }
        Ok(SequencedSubmissionOutcome {
            results,
            unsubmitted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::ledger::LedgerConfig;
    use super::super::wire::{CandidateOrigin, EventQueryResult, ReceiptState, TerminalOutcome};
    use super::*;
    use serde_json::json;

    fn new_ledger() -> MutationLedger {
        MutationLedger::new(LedgerConfig::default()).expect("default ledger config is valid")
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_784_000_000)
    }

    fn source() -> MutationSource {
        MutationSource::Rhai {
            engine_id: "v2.test.engine".into(),
        }
    }

    fn operation(
        domain: ExternalEffectDomain,
        name: &str,
        payload: u32,
    ) -> ExternalEffectOperation {
        ExternalEffectOperation::new(
            domain,
            name,
            &json!({ "payload": payload }),
            Some(&json!({ "payload": payload })),
        )
        .expect("test operation is canonical")
    }

    fn midi_effect(key: &str, payload: u32) -> ExternalEffectSubmission {
        ExternalEffectSubmission::new(
            operation(ExternalEffectDomain::Midi, "note_on", payload),
            key,
            source(),
            "v2.test.local",
        )
        .expect("keyed MIDI effect is accepted")
    }

    fn candidate_submission(
        atomicity: Atomicity,
        idempotency_key: Option<&str>,
        retry_epoch: Option<RuntimeEpoch>,
    ) -> Submission {
        Submission {
            kind: MutationKind::Candidate {
                origin: CandidateOrigin::RhaiHost,
            },
            source: source(),
            caller_namespace: "v2.test.local".into(),
            idempotency_key: idempotency_key.map(str::to_string),
            require_idempotency_key: false,
            retry_epoch,
            expected_revision: None,
            atomicity,
            supersession: SupersessionPolicy::Fifo,
            material: RequestMaterial::new(&json!({ "candidate": true }), Some(&json!({})))
                .expect("candidate material is canonical"),
        }
    }

    fn event_count(ledger: &MutationLedger) -> usize {
        match ledger.events_after(ledger.runtime_epoch(), None, now()) {
            EventQueryResult::Events { events } => events.len(),
            EventQueryResult::ResetRequired { .. } => panic!("test ledger never resets"),
        }
    }

    #[test]
    fn v2_external_effects_require_caller_issued_keys_across_domains() {
        for domain in ExternalEffectDomain::ALL {
            for blank in ["", "   ", "\t"] {
                let error = ExternalEffectSubmission::new(
                    operation(domain, "write", 1),
                    blank,
                    source(),
                    "v2.test.local",
                )
                .expect_err("a blank key is never a caller-provided key");
                assert!(
                    matches!(
                        &error,
                        ExternalEffectError::MissingIdempotencyKey { domain: got, .. }
                        if *got == domain
                    ),
                    "unexpected error for {domain}: {error}"
                );
            }
            let keyed = ExternalEffectSubmission::new(
                operation(domain, "write", 1),
                format!("key-{domain}"),
                source(),
                "v2.test.local",
            )
            .expect("a keyed external effect is accepted");
            assert_eq!(keyed.idempotency_key(), format!("key-{domain}"));
        }

        let oversized = "k".repeat(256);
        assert!(matches!(
            ExternalEffectSubmission::new(
                operation(ExternalEffectDomain::Midi, "note_on", 1),
                oversized,
                source(),
                "v2.test.local",
            ),
            Err(ExternalEffectError::InvalidIdempotencyKey { .. })
        ));
        assert!(matches!(
            ExternalEffectOperation::new(
                ExternalEffectDomain::Midi,
                ".note",
                &json!({}),
                None::<&serde_json::Value>,
            ),
            Err(ExternalEffectError::InvalidOperation(_))
        ));
    }

    #[test]
    fn v2_external_submissions_are_best_effort_top_level_only() {
        // Every external domain projects totally onto the wire: MIDI and
        // file effects borrow their exact v1 music domains, while
        // filesystem/process/network address the dedicated External kind.
        let command = |domain: MessageDomain| -> Box<dyn Fn(String) -> MutationKind> {
            Box::new(move |operation| MutationKind::Command { domain, operation })
        };
        let external = |domain: ExternalDomain| -> Box<dyn Fn(String) -> MutationKind> {
            Box::new(move |operation| MutationKind::External { domain, operation })
        };
        let table = [
            (ExternalEffectDomain::Midi, command(MessageDomain::Midi)),
            (
                ExternalEffectDomain::File,
                command(MessageDomain::Recording),
            ),
            (
                ExternalEffectDomain::Filesystem,
                external(ExternalDomain::Filesystem),
            ),
            (
                ExternalEffectDomain::Process,
                external(ExternalDomain::Process),
            ),
            (
                ExternalEffectDomain::Network,
                external(ExternalDomain::Network),
            ),
        ];
        let epoch = RuntimeEpoch::new();
        for (domain, expected_kind) in table {
            let effect = ExternalEffectSubmission::new(
                operation(domain, "op", 7),
                "key-wire",
                source(),
                "v2.test.local",
            )
            .expect("keyed effect is accepted");
            let submission = effect.submission(epoch);
            assert_eq!(submission.atomicity, Atomicity::BestEffort);
            assert!(submission.require_idempotency_key);
            assert_eq!(submission.retry_epoch, Some(epoch));
            assert_eq!(
                submission.kind,
                expected_kind(format!("external.{domain}.op")),
                "wire projection for {domain}"
            );
        }

        // The internal parent/child spelling is unrepresentable.
        assert!(matches!(
            ExternalEffectSubmission::new(
                operation(ExternalEffectDomain::Midi, "note_on", 1),
                "key-internal",
                MutationSource::Internal {
                    parent_revision: super::super::wire::RevisionId::new(1).unwrap(),
                },
                "v2.test.local",
            ),
            Err(ExternalEffectError::InternalSourceForbidden)
        ));
    }

    #[test]
    fn v2_mixed_candidate_submissions_reject_before_revision_allocation() {
        let ledger = new_ledger();
        let table = [
            (
                Atomicity::Required,
                "mixed required-atomic submissions reject",
            ),
            (
                Atomicity::BestEffort,
                "best-effort candidates cannot embed external effects either",
            ),
        ];
        for (atomicity, case) in table {
            let error = CandidateSubmission::with_embedded_external(
                candidate_submission(atomicity, None, None),
                vec![operation(ExternalEffectDomain::Midi, "note_on", 3)],
            )
            .expect_err(case);
            match atomicity {
                Atomicity::Required => assert!(
                    matches!(
                        &error,
                        ExternalEffectError::MixedRequiredAtomicSubmission { .. }
                    ),
                    "{case}: {error}"
                ),
                Atomicity::BestEffort => assert!(
                    matches!(&error, ExternalEffectError::EmbeddedExternalEffect { .. }),
                    "{case}: {error}"
                ),
            }
        }
        assert!(matches!(
            CandidateSubmission::new(Submission {
                kind: MutationKind::Command {
                    domain: MessageDomain::Midi,
                    operation: "external.midi.note_on".into(),
                },
                ..candidate_submission(Atomicity::BestEffort, None, None)
            }),
            Err(ExternalEffectError::NotACandidate(_))
        ));

        // The rejection happened before any ledger work: no attempt exists
        // and no revision was allocated.
        assert_eq!(event_count(&ledger), 0);
        let status = ledger.status(now());
        assert_eq!(status.accepted_through, None);
        assert!(status.pending.is_empty());
    }

    #[test]
    fn v2_repeated_sequenced_keys_reject_without_child_work() {
        let ledger = new_ledger();
        let candidate = CandidateSubmission::new(candidate_submission(
            Atomicity::Required,
            Some("key-shared"),
            None,
        ))
        .expect("a pure required-atomic candidate seat is legal");

        // Effect key colliding with the candidate key.
        assert!(matches!(
            SequencedExternalPlan::new(
                vec![midi_effect("key-shared", 1)],
                candidate.clone(),
                Vec::new(),
            ),
            Err(ExternalEffectError::DuplicateIdempotencyKey { key }) if key == "key-shared"
        ));

        // Two effects sharing one key across the before/after split.
        assert!(matches!(
            SequencedExternalPlan::new(
                vec![midi_effect("key-dup", 1)],
                candidate,
                vec![midi_effect("key-dup", 2)],
            ),
            Err(ExternalEffectError::DuplicateIdempotencyKey { key }) if key == "key-dup"
        ));

        // Zero child work: the ledger never saw an attempt.
        assert_eq!(event_count(&ledger), 0);
        assert_eq!(ledger.status(now()).accepted_through, None);
    }

    #[test]
    fn v2_sequenced_pair_submits_ordered_top_level_receipts_and_halts_after_rejection() {
        let ledger = new_ledger();
        let epoch = ledger.runtime_epoch();

        let plan = SequencedExternalPlan::new(
            vec![midi_effect("key-before", 1)],
            CandidateSubmission::new(candidate_submission(
                Atomicity::Required,
                Some("key-candidate"),
                Some(epoch),
            ))
            .unwrap(),
            vec![midi_effect("key-after", 2)],
        )
        .expect("distinct keys are accepted");
        let outcome = plan.submit(&ledger, now()).expect("plan submits");

        assert_eq!(outcome.results.len(), 3);
        assert_eq!(outcome.unsubmitted, 0);
        for result in &outcome.results {
            let receipt = result.receipt();
            assert!(matches!(result, SubmissionResult::New(_)));
            assert!(matches!(receipt.state, ReceiptState::Evaluating { .. }));
            assert_eq!(receipt.revision, None);
            assert!(
                matches!(&receipt.request.source, MutationSource::Rhai { .. }),
                "every element is a top-level caller submission, never an internal child"
            );
        }
        assert!(matches!(
            outcome.results[1].receipt().request.kind,
            MutationKind::Candidate { .. }
        ));
        // Exactly one attempt per element — the runtime created no
        // parent/child receipts around them.
        assert_eq!(event_count(&ledger), 3);

        // A rejected element halts the tail instead of spawning follow-up
        // attempts for it.
        let halted_ledger = new_ledger();
        let stale_epoch = RuntimeEpoch::new();
        let plan = SequencedExternalPlan::new(
            vec![midi_effect("key-before", 1)],
            CandidateSubmission::new(candidate_submission(
                Atomicity::Required,
                Some("key-candidate"),
                Some(stale_epoch),
            ))
            .unwrap(),
            vec![midi_effect("key-after", 2)],
        )
        .unwrap();
        let outcome = plan.submit(&halted_ledger, now()).expect("plan submits");

        assert_eq!(outcome.results.len(), 2);
        assert_eq!(outcome.unsubmitted, 1);
        assert!(matches!(outcome.results[0], SubmissionResult::New(_)));
        let SubmissionResult::Rejected(rejected) = &outcome.results[1] else {
            panic!("the stale-epoch candidate rejects at submission");
        };
        assert_eq!(
            rejected.revision, None,
            "rejected before revision allocation"
        );
        let ReceiptState::Terminal(TerminalOutcome::Rejected(details)) = &rejected.state else {
            panic!("rejected receipts are terminal");
        };
        assert_eq!(details.code, "runtime_epoch_changed");
        // 1 event for the accepted attempt + 2 for the rejected attempt
        // (creation and terminal transition); nothing for the halted tail.
        assert_eq!(event_count(&halted_ledger), 3);
    }

    #[test]
    fn v2_replayed_keys_return_original_receipts_and_conflicts_reject() {
        let ledger = new_ledger();
        let effect = midi_effect("key-replay", 9);

        let first = submit_external_effect(&ledger, &effect, now()).expect("first submission");
        let SubmissionResult::New(original) = &first else {
            panic!("first keyed submission is new");
        };

        let replay = submit_external_effect(&ledger, &effect, now()).expect("replay");
        let SubmissionResult::Replayed(replayed) = &replay else {
            panic!("same key and payload replays the original attempt");
        };
        assert_eq!(replayed.attempt_id, original.attempt_id);

        let conflicting = midi_effect("key-replay", 10);
        let conflict =
            submit_external_effect(&ledger, &conflicting, now()).expect("conflict submission");
        let SubmissionResult::Rejected(rejected) = &conflict else {
            panic!("same key with different payload rejects");
        };
        assert_eq!(rejected.revision, None, "no revision for the conflict");
        let ReceiptState::Terminal(TerminalOutcome::Rejected(details)) = &rejected.state else {
            panic!("idempotency conflicts are terminal rejections");
        };
        assert_eq!(details.code, "idempotency_conflict");
    }
}
