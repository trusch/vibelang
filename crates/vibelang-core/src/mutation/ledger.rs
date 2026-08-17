use super::digest::{
    operation_digest, DigestError, EpochFingerprintKey, IdempotencyKeyFingerprint,
    RequestFingerprint, RetainedIdentityFingerprint, RetainedIdentityKey,
};
use super::wire::validate_pre_planning_partial;
use super::*;
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug)]
pub struct LedgerConfig {
    pub minimum_event_count: usize,
    pub minimum_event_age: Duration,
    pub idempotency_capacity: usize,
}

impl LedgerConfig {
    pub fn validate(&self) -> Result<(), LedgerError> {
        if self.minimum_event_count == 0 {
            return Err(LedgerError::InvalidConfig(
                "minimum_event_count must be greater than zero".into(),
            ));
        }
        if self.idempotency_capacity == 0 {
            return Err(LedgerError::InvalidConfig(
                "idempotency_capacity must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

impl Default for LedgerConfig {
    fn default() -> Self {
        Self {
            minimum_event_count: 10_000,
            minimum_event_age: Duration::from_secs(15 * 60),
            idempotency_capacity: 50_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Submission {
    pub kind: MutationKind,
    pub source: MutationSource,
    pub caller_namespace: String,
    pub idempotency_key: Option<String>,
    pub require_idempotency_key: bool,
    pub retry_epoch: Option<RuntimeEpoch>,
    pub expected_revision: Option<RevisionId>,
    pub atomicity: Atomicity,
    pub supersession: SupersessionPolicy,
    pub material: RequestMaterial,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SubmissionResult {
    New(MutationReceipt),
    Replayed(MutationReceipt),
    Rejected(MutationReceipt),
}

impl SubmissionResult {
    #[must_use]
    pub fn receipt(&self) -> &MutationReceipt {
        match self {
            Self::New(receipt) | Self::Replayed(receipt) | Self::Rejected(receipt) => receipt,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CancelResult {
    Receipt(Box<MutationReceipt>),
    Rejected(MutationError),
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("invalid ledger configuration: {0}")]
    InvalidConfig(String),
    #[error("receipt not found")]
    ReceiptNotFound,
    #[error("receipt transition is invalid: {0}")]
    InvalidTransition(String),
    #[error("receipt invariant is invalid: {0}")]
    InvalidReceipt(String),
    #[error("revision order violation: {0}")]
    RevisionOrder(String),
    #[error("counter exhausted: {0}")]
    CounterExhausted(String),
    #[error(transparent)]
    Digest(#[from] DigestError),
}

#[derive(Clone)]
pub struct MutationLedger {
    inner: Arc<Mutex<LedgerInner>>,
}

impl std::fmt::Debug for MutationLedger {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.lock();
        formatter
            .debug_struct("MutationLedger")
            .field("runtime_epoch", &inner.epoch)
            .field("receipt_count", &inner.receipts.len())
            .field("event_count", &inner.events.len())
            .finish()
    }
}

#[derive(Debug)]
struct LedgerInner {
    config: LedgerConfig,
    epoch: RuntimeEpoch,
    fingerprint_key: EpochFingerprintKey,
    retained_identity_key: RetainedIdentityKey,
    retained_identities: Vec<RetainedIdentityFingerprint>,
    next_revision: u64,
    next_event_sequence: u64,
    accepted_through: Option<RevisionId>,
    last_confirmed_revision: Option<RevisionId>,
    last_rejected_revision: Option<RevisionId>,
    live_state: LiveState,
    receipts: BTreeMap<AttemptId, ReceiptRecord>,
    revisions: BTreeMap<RevisionId, AttemptId>,
    events: VecDeque<StoredEvent>,
    idempotency: HashMap<IdempotencyKeyFingerprint, IdempotencyRecord>,
    tombstones: HashMap<IdempotencyKeyFingerprint, IdempotencyTombstone>,
}

#[derive(Clone, Debug)]
struct ReceiptRecord {
    receipt: MutationReceipt,
    planned: Vec<PlannedComponent>,
    idempotency_key: Option<IdempotencyKeyFingerprint>,
    last_event_sequence: EventSequence,
}

#[derive(Debug)]
struct StoredEvent {
    event: ReceiptEvent,
    recorded_at: SystemTime,
}

#[derive(Debug)]
struct IdempotencyRecord {
    attempt_id: AttemptId,
    request_fingerprint: RequestFingerprint,
}

#[derive(Debug)]
struct IdempotencyTombstone {
    request_fingerprint: RequestFingerprint,
    _expired_at: SystemTime,
}

#[derive(Serialize)]
struct RequestFingerprintPolicy<'a> {
    kind: &'a MutationKind,
    expected_revision: Option<RevisionId>,
    atomicity: Atomicity,
    supersession: &'a SupersessionPolicy,
}

impl MutationLedger {
    pub fn new(config: LedgerConfig) -> Result<Self, LedgerError> {
        config.validate()?;
        Ok(Self {
            inner: Arc::new(Mutex::new(LedgerInner::new(config)?)),
        })
    }

    #[must_use]
    pub fn runtime_epoch(&self) -> RuntimeEpoch {
        self.inner.lock().epoch
    }

    #[doc = "@vibelang-contract-operation request=MutationSubmissionWire response=MutationReceipt error=MutationError"]
    pub fn submit(
        &self,
        submission: Submission,
        now: SystemTime,
    ) -> Result<SubmissionResult, LedgerError> {
        let mut inner = self.inner.lock();
        inner.prune(now)?;
        let public_digest = submission.material.public_digest()?;
        let request_identity = RequestIdentity {
            kind: submission.kind.clone(),
            source: submission.source.clone(),
            submission_digest: public_digest,
            operation_digest: None,
            idempotency_key_present: submission.idempotency_key.is_some(),
            expected_revision: submission.expected_revision,
            atomicity: submission.atomicity,
            supersession: submission.supersession.clone(),
        };

        if submission
            .retry_epoch
            .is_some_and(|retry_epoch| retry_epoch != inner.epoch)
        {
            return inner.reject_new_attempt(
                request_identity,
                None,
                FailurePhase::Idempotency,
                "runtime_epoch_changed",
                "the retry belongs to a previous runtime epoch",
                now,
            );
        }

        let Some(idempotency_key) = submission.idempotency_key.as_deref() else {
            if submission.require_idempotency_key {
                return inner.reject_new_attempt(
                    request_identity,
                    None,
                    FailurePhase::Idempotency,
                    "idempotency_key_required",
                    "this operation requires an idempotency key",
                    now,
                );
            }
            let receipt = inner.create_attempt(request_identity, None, now)?;
            return Ok(SubmissionResult::New(receipt));
        };

        let key_fingerprint = inner
            .fingerprint_key
            .key_fingerprint(&submission.caller_namespace, idempotency_key)?;
        let request_fingerprint = submission.material.request_fingerprint(
            &inner.fingerprint_key,
            &RequestFingerprintPolicy {
                kind: &submission.kind,
                expected_revision: submission.expected_revision,
                atomicity: submission.atomicity,
                supersession: &submission.supersession,
            },
        )?;

        if let Some(tombstone) = inner.tombstones.get(&key_fingerprint) {
            let message = if tombstone
                .request_fingerprint
                .constant_time_eq(&request_fingerprint)
            {
                "the idempotency key expired with its retained receipt"
            } else {
                "the expired idempotency key cannot be reused for another request"
            };
            return inner.reject_new_attempt(
                request_identity,
                None,
                FailurePhase::Idempotency,
                "idempotency_key_expired",
                message,
                now,
            );
        }

        if let Some(existing) = inner.idempotency.get(&key_fingerprint) {
            if existing
                .request_fingerprint
                .constant_time_eq(&request_fingerprint)
            {
                let receipt = inner
                    .receipts
                    .get(&existing.attempt_id)
                    .ok_or(LedgerError::ReceiptNotFound)?
                    .receipt
                    .clone();
                return Ok(SubmissionResult::Replayed(receipt));
            }
            return inner.reject_new_attempt(
                request_identity,
                None,
                FailurePhase::Idempotency,
                "idempotency_conflict",
                "the idempotency key is already bound to different semantic input",
                now,
            );
        }

        let retained_identity = inner
            .retained_identity_key
            .key_fingerprint(&submission.caller_namespace, idempotency_key)?;
        if inner.retained_identity_exists(&retained_identity) {
            return inner.reject_new_attempt(
                request_identity,
                None,
                FailurePhase::Idempotency,
                "runtime_epoch_changed",
                "the idempotency key belongs to a previous runtime epoch",
                now,
            );
        }

        if inner.retained_identities.len() >= inner.config.idempotency_capacity {
            return inner.reject_new_attempt(
                request_identity,
                None,
                FailurePhase::Idempotency,
                "idempotency_capacity_exhausted",
                "the runtime cannot retain another idempotency identity across reset epochs",
                now,
            );
        }

        let receipt = inner.create_attempt(request_identity, Some(key_fingerprint.clone()), now)?;
        inner.idempotency.insert(
            key_fingerprint,
            IdempotencyRecord {
                attempt_id: receipt.attempt_id,
                request_fingerprint,
            },
        );
        inner.retained_identities.push(retained_identity);
        Ok(SubmissionResult::New(receipt))
    }

    pub fn accept(
        &self,
        attempt_id: AttemptId,
        operation_redacted: Option<&Value>,
        now: SystemTime,
    ) -> Result<MutationReceipt, LedgerError> {
        let mut inner = self.inner.lock();
        inner.prune(now)?;
        let current = inner.receipt(attempt_id)?.clone();
        if !matches!(current.state, ReceiptState::Evaluating { .. }) {
            return Err(LedgerError::InvalidTransition(
                "only an evaluating attempt can be accepted".into(),
            ));
        }
        let confirmed_revision = inner.last_confirmed_revision;
        if current
            .request
            .expected_revision
            .is_some_and(|expected| Some(expected) != confirmed_revision)
        {
            inner
                .receipts
                .get_mut(&attempt_id)
                .ok_or(LedgerError::ReceiptNotFound)?
                .receipt
                .previous_confirmed_revision = confirmed_revision;
            return inner.transition(
                attempt_id,
                ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
                    phase: FailurePhase::ExpectedRevision,
                    code: "revision_conflict".into(),
                    message: "expected revision does not match the confirmed revision".into(),
                    rollback: RollbackState::NotNeeded,
                    preserved_revision: confirmed_revision,
                })),
                now,
            );
        }
        let operation_digest = operation_redacted.map(operation_digest).transpose()?;
        let revision = inner.allocate_revision()?;
        let replacement_key = match &current.request.supersession {
            SupersessionPolicy::ReplacePending { key } => Some(key.clone()),
            SupersessionPolicy::Fifo => None,
        };
        if let Some(key) = replacement_key {
            let replaced = inner
                .receipts
                .iter()
                .filter_map(|(id, record)| {
                    (*id != attempt_id
                        && matches!(record.receipt.state, ReceiptState::Accepted { .. })
                        && record.receipt.request.supersession
                            == (SupersessionPolicy::ReplacePending { key: key.clone() }))
                    .then_some(*id)
                })
                .collect::<Vec<_>>();
            for replaced_id in replaced {
                inner.transition(
                    replaced_id,
                    ReceiptState::Terminal(TerminalOutcome::Superseded(Superseded {
                        reason: SupersessionReason::Replaced,
                        by_revision: Some(revision),
                    })),
                    now,
                )?;
            }
        }
        let queue_position = u32::try_from(
            inner
                .receipts
                .values()
                .filter(|record| {
                    record.receipt.revision.is_some() && !record.receipt.state.is_terminal()
                })
                .count(),
        )
        .ok();
        let record = inner
            .receipts
            .get_mut(&attempt_id)
            .ok_or(LedgerError::ReceiptNotFound)?;
        record.receipt.previous_confirmed_revision = confirmed_revision;
        record.receipt.revision = Some(revision);
        record.receipt.request.operation_digest = operation_digest;
        inner.revisions.insert(revision, attempt_id);
        inner.accepted_through = Some(revision);
        inner.transition(attempt_id, ReceiptState::Accepted { queue_position }, now)
    }

    pub fn begin_planning(
        &self,
        attempt_id: AttemptId,
        planned: Vec<PlannedComponent>,
        now: SystemTime,
    ) -> Result<MutationReceipt, LedgerError> {
        self.begin_planning_inner(attempt_id, planned, now, true)
    }

    pub(crate) fn begin_concurrent_planning(
        &self,
        attempt_id: AttemptId,
        planned: Vec<PlannedComponent>,
        now: SystemTime,
    ) -> Result<MutationReceipt, LedgerError> {
        self.begin_planning_inner(attempt_id, planned, now, false)
    }

    fn begin_planning_inner(
        &self,
        attempt_id: AttemptId,
        planned: Vec<PlannedComponent>,
        now: SystemTime,
        require_previous_terminal: bool,
    ) -> Result<MutationReceipt, LedgerError> {
        let mut inner = self.inner.lock();
        inner.prune(now)?;
        let current = inner.receipt(attempt_id)?.clone();
        let revision = current.revision.ok_or_else(|| {
            LedgerError::InvalidTransition("planning requires an allocated revision".into())
        })?;
        if !matches!(current.state, ReceiptState::Accepted { .. }) {
            return Err(LedgerError::InvalidTransition(
                "planning can begin only from accepted".into(),
            ));
        }
        if require_previous_terminal {
            if let Some(blocking) = inner
                .revisions
                .range(..revision)
                .find_map(|(revision, id)| {
                    inner
                        .receipts
                        .get(id)
                        .filter(|record| !record.receipt.state.is_terminal())
                        .map(|_| *revision)
                })
            {
                return Err(LedgerError::RevisionOrder(format!(
                    "revision {revision} is blocked by non-terminal revision {blocking}"
                )));
            }
        }
        let confirmed_revision = inner.last_confirmed_revision;
        if current
            .request
            .expected_revision
            .is_some_and(|expected| Some(expected) != confirmed_revision)
        {
            inner
                .receipts
                .get_mut(&attempt_id)
                .ok_or(LedgerError::ReceiptNotFound)?
                .receipt
                .previous_confirmed_revision = confirmed_revision;
            return inner.transition(
                attempt_id,
                ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
                    phase: FailurePhase::ExpectedRevision,
                    code: "revision_conflict".into(),
                    message: "confirmed revision changed before planning".into(),
                    rollback: RollbackState::NotNeeded,
                    preserved_revision: confirmed_revision,
                })),
                now,
            );
        }
        validate_planned(&planned)?;
        inner
            .receipts
            .get_mut(&attempt_id)
            .ok_or(LedgerError::ReceiptNotFound)?
            .planned = planned;
        inner.transition(attempt_id, ReceiptState::Planning, now)
    }

    pub fn transition(
        &self,
        attempt_id: AttemptId,
        state: ReceiptState,
        now: SystemTime,
    ) -> Result<MutationReceipt, LedgerError> {
        if matches!(state, ReceiptState::Planning) {
            return Err(LedgerError::InvalidTransition(
                "use begin_planning so expected revision and component planning are atomic".into(),
            ));
        }
        let mut inner = self.inner.lock();
        inner.prune(now)?;
        inner.transition(attempt_id, state, now)
    }

    pub(crate) fn transition_with_diagnostics(
        &self,
        attempt_id: AttemptId,
        state: ReceiptState,
        diagnostics: Vec<Diagnostic>,
        now: SystemTime,
    ) -> Result<MutationReceipt, LedgerError> {
        if matches!(state, ReceiptState::Planning) {
            return Err(LedgerError::InvalidTransition(
                "use begin_planning so expected revision and component planning are atomic".into(),
            ));
        }
        let mut inner = self.inner.lock();
        inner.prune(now)?;
        inner.transition_with_diagnostics(attempt_id, state, diagnostics, now)
    }

    pub fn cancel(&self, attempt_id: AttemptId, now: SystemTime) -> CancelResult {
        let mut inner = self.inner.lock();
        if inner.prune(now).is_err() {
            return CancelResult::Rejected(inner.error(
                "ledger_error",
                "receipt retention failed",
                Some(attempt_id),
                None,
                false,
            ));
        }
        let Some(current) = inner
            .receipts
            .get(&attempt_id)
            .map(|record| record.receipt.clone())
        else {
            return CancelResult::Rejected(inner.error(
                "receipt_not_found",
                "the receipt is unknown or expired",
                Some(attempt_id),
                None,
                false,
            ));
        };
        match current.state {
            ReceiptState::Evaluating { .. } | ReceiptState::Accepted { .. } => {
                match inner.transition(
                    attempt_id,
                    ReceiptState::Terminal(TerminalOutcome::Superseded(Superseded {
                        reason: SupersessionReason::Cancelled,
                        by_revision: None,
                    })),
                    now,
                ) {
                    Ok(receipt) => CancelResult::Receipt(Box::new(receipt)),
                    Err(error) => CancelResult::Rejected(inner.error(
                        "invalid_transition",
                        &error.to_string(),
                        Some(attempt_id),
                        current.revision,
                        false,
                    )),
                }
            }
            ReceiptState::Planning
            | ReceiptState::Staging { .. }
            | ReceiptState::Committing { .. } => CancelResult::Rejected(inner.error(
                "too_late_to_cancel",
                "cancellation is unavailable after planning begins",
                Some(attempt_id),
                current.revision,
                false,
            )),
            ReceiptState::Terminal(_) => CancelResult::Receipt(Box::new(current)),
        }
    }

    pub fn receipt(&self, attempt_id: AttemptId) -> Result<MutationReceipt, LedgerError> {
        Ok(self.inner.lock().receipt(attempt_id)?.clone())
    }

    #[must_use]
    pub fn status(&self, now: SystemTime) -> RuntimeMutationStatus {
        self.inner.lock().status(now)
    }

    #[must_use]
    pub fn capabilities(&self) -> MutationCapabilities {
        self.inner.lock().capabilities()
    }

    #[must_use]
    pub fn events_after(
        &self,
        epoch: RuntimeEpoch,
        after: Option<EventSequence>,
        now: SystemTime,
    ) -> EventQueryResult {
        let mut inner = self.inner.lock();
        if inner.prune(now).is_err() {
            return EventQueryResult::ResetRequired {
                reason: ResetReason::RetentionExpired,
                status: Box::new(inner.status(now)),
            };
        }
        inner.events_after(epoch, after, now)
    }

    pub fn reset(&self) -> Result<RuntimeEpoch, LedgerError> {
        self.inner.lock().reset_epoch()
    }

    pub fn prune(&self, now: SystemTime) -> Result<(), LedgerError> {
        self.inner.lock().prune(now)
    }
}

impl LedgerInner {
    fn new(config: LedgerConfig) -> Result<Self, LedgerError> {
        Ok(Self {
            config,
            epoch: RuntimeEpoch::new(),
            fingerprint_key: EpochFingerprintKey::generate()?,
            retained_identity_key: RetainedIdentityKey::generate()?,
            retained_identities: Vec::new(),
            next_revision: 1,
            next_event_sequence: 1,
            accepted_through: None,
            last_confirmed_revision: None,
            last_rejected_revision: None,
            live_state: LiveState::Clean,
            receipts: BTreeMap::new(),
            revisions: BTreeMap::new(),
            events: VecDeque::new(),
            idempotency: HashMap::new(),
            tombstones: HashMap::new(),
        })
    }

    fn reset_epoch(&mut self) -> Result<RuntimeEpoch, LedgerError> {
        let fingerprint_key = EpochFingerprintKey::generate()?;
        let epoch = RuntimeEpoch::new();
        self.epoch = epoch;
        self.fingerprint_key = fingerprint_key;
        self.next_revision = 1;
        self.next_event_sequence = 1;
        self.accepted_through = None;
        self.last_confirmed_revision = None;
        self.last_rejected_revision = None;
        self.live_state = LiveState::Clean;
        self.receipts.clear();
        self.revisions.clear();
        self.events.clear();
        self.idempotency.clear();
        self.tombstones.clear();
        Ok(epoch)
    }

    fn retained_identity_exists(&self, candidate: &RetainedIdentityFingerprint) -> bool {
        bool::from(
            self.retained_identities
                .iter()
                .fold(subtle::Choice::from(0), |found, identity| {
                    found | identity.constant_time_eq(candidate)
                }),
        )
    }

    fn create_attempt(
        &mut self,
        request: RequestIdentity,
        idempotency_key: Option<IdempotencyKeyFingerprint>,
        now: SystemTime,
    ) -> Result<MutationReceipt, LedgerError> {
        let attempt_id = AttemptId::new();
        let sequence = self.allocate_event_sequence()?;
        let timestamp = Timestamp::from_system_time(now);
        let receipt = MutationReceipt {
            schema_version: MUTATION_SCHEMA_VERSION,
            attempt_id,
            runtime_epoch: self.epoch,
            revision: None,
            event_sequence: sequence,
            request,
            state: ReceiptState::Evaluating {
                phase: PreAcceptancePhase::Decode,
            },
            previous_confirmed_revision: self.last_confirmed_revision,
            timestamps: ReceiptTimestamps {
                submitted_at: timestamp.clone(),
                accepted_at: None,
                last_transition_at: timestamp,
                terminal_at: None,
            },
            diagnostics: Vec::new(),
        };
        let previous_event_sequence = self.events.back().map(|event| event.event.event_sequence);
        self.events.push_back(StoredEvent {
            event: ReceiptEvent {
                schema_version: MUTATION_SCHEMA_VERSION,
                runtime_epoch: self.epoch,
                event_sequence: sequence,
                previous_event_sequence,
                receipt: receipt.clone(),
            },
            recorded_at: now,
        });
        self.receipts.insert(
            attempt_id,
            ReceiptRecord {
                receipt: receipt.clone(),
                planned: Vec::new(),
                idempotency_key,
                last_event_sequence: sequence,
            },
        );
        Ok(receipt)
    }

    fn reject_new_attempt(
        &mut self,
        request: RequestIdentity,
        idempotency_key: Option<IdempotencyKeyFingerprint>,
        phase: FailurePhase,
        code: &str,
        message: &str,
        now: SystemTime,
    ) -> Result<SubmissionResult, LedgerError> {
        let receipt = self.create_attempt(request, idempotency_key, now)?;
        let terminal = self.transition(
            receipt.attempt_id,
            ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
                phase,
                code: code.into(),
                message: message.into(),
                rollback: RollbackState::NotNeeded,
                preserved_revision: self.last_confirmed_revision,
            })),
            now,
        )?;
        Ok(SubmissionResult::Rejected(terminal))
    }

    fn transition(
        &mut self,
        attempt_id: AttemptId,
        state: ReceiptState,
        now: SystemTime,
    ) -> Result<MutationReceipt, LedgerError> {
        self.transition_inner(attempt_id, state, Vec::new(), now)
    }

    fn transition_with_diagnostics(
        &mut self,
        attempt_id: AttemptId,
        state: ReceiptState,
        diagnostics: Vec<Diagnostic>,
        now: SystemTime,
    ) -> Result<MutationReceipt, LedgerError> {
        self.transition_inner(attempt_id, state, diagnostics, now)
    }

    fn transition_inner(
        &mut self,
        attempt_id: AttemptId,
        state: ReceiptState,
        diagnostics: Vec<Diagnostic>,
        now: SystemTime,
    ) -> Result<MutationReceipt, LedgerError> {
        let record = self
            .receipts
            .get(&attempt_id)
            .ok_or(LedgerError::ReceiptNotFound)?
            .clone();
        validate_transition(&record.receipt.state, &state)
            .map_err(LedgerError::InvalidTransition)?;
        if let ReceiptState::Terminal(outcome) = &state {
            if let TerminalOutcome::Partial(partial) = outcome {
                match &record.receipt.state {
                    ReceiptState::Evaluating { .. } => {
                        if record.receipt.revision.is_some() {
                            return Err(LedgerError::InvalidReceipt(
                                "an evaluating partial outcome cannot have a revision".into(),
                            ));
                        }
                        validate_pre_planning_partial(
                            partial,
                            record.receipt.previous_confirmed_revision,
                        )
                        .map_err(LedgerError::InvalidReceipt)?;
                    }
                    ReceiptState::Accepted { .. } => {
                        if record.receipt.revision.is_none() {
                            return Err(LedgerError::InvalidReceipt(
                                "an accepted partial outcome requires an allocated revision".into(),
                            ));
                        }
                        validate_pre_planning_partial(
                            partial,
                            record.receipt.previous_confirmed_revision,
                        )
                        .map_err(LedgerError::InvalidReceipt)?;
                    }
                    _ => validate_terminal(
                        outcome,
                        record.receipt.previous_confirmed_revision,
                        &record.planned,
                    )
                    .map_err(LedgerError::InvalidReceipt)?,
                }
            } else {
                validate_terminal(
                    outcome,
                    record.receipt.previous_confirmed_revision,
                    &record.planned,
                )
                .map_err(LedgerError::InvalidReceipt)?;
            }
            if matches!(outcome, TerminalOutcome::Superseded(_))
                && !matches!(
                    record.receipt.state,
                    ReceiptState::Evaluating { .. } | ReceiptState::Accepted { .. }
                )
            {
                return Err(LedgerError::InvalidReceipt(
                    "supersession is available only before planning".into(),
                ));
            }
            if matches!(outcome, TerminalOutcome::Applied(_)) && record.receipt.revision.is_none() {
                return Err(LedgerError::InvalidReceipt(
                    "an applied outcome requires an allocated revision".into(),
                ));
            }
        }

        let sequence = self.allocate_event_sequence()?;
        let previous_event_sequence = self.events.back().map(|event| event.event.event_sequence);
        let timestamp = Timestamp::from_system_time(now);
        let mut receipt = record.receipt.clone();
        receipt.state = state;
        receipt.diagnostics.extend(diagnostics);
        receipt.event_sequence = sequence;
        receipt.timestamps.last_transition_at = timestamp.clone();
        if matches!(receipt.state, ReceiptState::Accepted { .. }) {
            receipt.timestamps.accepted_at = Some(timestamp.clone());
        }
        if receipt.state.is_terminal() {
            receipt.timestamps.terminal_at = Some(timestamp);
        }

        if let ReceiptState::Terminal(outcome) = &receipt.state {
            match outcome {
                TerminalOutcome::Applied(_) => {
                    let revision = receipt.revision.ok_or_else(|| {
                        LedgerError::InvalidReceipt(
                            "an applied outcome requires an allocated revision".into(),
                        )
                    })?;
                    self.last_confirmed_revision = Some(
                        self.last_confirmed_revision
                            .map_or(revision, |confirmed| confirmed.max(revision)),
                    );
                    let restores_clean = match &self.live_state {
                        LiveState::Clean => true,
                        LiveState::Partial {
                            revision: partial_revision,
                            ..
                        } => revision > *partial_revision,
                        LiveState::PreAdmissionPartial { .. } => false,
                        LiveState::Unknown { since_revision, .. } => revision > *since_revision,
                    };
                    if restores_clean {
                        self.live_state = LiveState::Clean;
                    }
                }
                TerminalOutcome::Rejected(_) => {
                    if let Some(revision) = receipt.revision {
                        self.last_rejected_revision = Some(revision);
                    }
                }
                TerminalOutcome::Partial(partial) => match receipt.revision {
                    Some(revision) => {
                        let updates_live_state = match &self.live_state {
                            LiveState::Clean => self
                                .last_confirmed_revision
                                .is_none_or(|confirmed| revision > confirmed),
                            LiveState::Partial {
                                revision: partial_revision,
                                ..
                            } => revision > *partial_revision,
                            LiveState::PreAdmissionPartial { .. } => false,
                            LiveState::Unknown { since_revision, .. } => revision > *since_revision,
                        };
                        if updates_live_state {
                            self.live_state = LiveState::Partial {
                                revision,
                                fenced: partial.fenced,
                            };
                        }
                    }
                    None => {
                        self.live_state = LiveState::PreAdmissionPartial {
                            attempt_id,
                            fenced: partial.fenced,
                        };
                    }
                },
                TerminalOutcome::Superseded(_) => {}
            }
        }

        let record = self
            .receipts
            .get_mut(&attempt_id)
            .ok_or(LedgerError::ReceiptNotFound)?;
        record.receipt = receipt.clone();
        record.last_event_sequence = sequence;
        self.events.push_back(StoredEvent {
            event: ReceiptEvent {
                schema_version: MUTATION_SCHEMA_VERSION,
                runtime_epoch: self.epoch,
                event_sequence: sequence,
                previous_event_sequence,
                receipt: receipt.clone(),
            },
            recorded_at: now,
        });
        Ok(receipt)
    }

    fn allocate_revision(&mut self) -> Result<RevisionId, LedgerError> {
        let revision =
            RevisionId::new(self.next_revision).map_err(LedgerError::CounterExhausted)?;
        self.next_revision = self
            .next_revision
            .checked_add(1)
            .ok_or_else(|| LedgerError::CounterExhausted("RevisionId exhausted".into()))?;
        Ok(revision)
    }

    fn allocate_event_sequence(&mut self) -> Result<EventSequence, LedgerError> {
        let sequence =
            EventSequence::new(self.next_event_sequence).map_err(LedgerError::CounterExhausted)?;
        self.next_event_sequence = self
            .next_event_sequence
            .checked_add(1)
            .ok_or_else(|| LedgerError::CounterExhausted("EventSequence exhausted".into()))?;
        Ok(sequence)
    }

    fn receipt(&self, attempt_id: AttemptId) -> Result<&MutationReceipt, LedgerError> {
        self.receipts
            .get(&attempt_id)
            .map(|record| &record.receipt)
            .ok_or(LedgerError::ReceiptNotFound)
    }

    fn status(&self, now: SystemTime) -> RuntimeMutationStatus {
        let mut pending = self
            .receipts
            .values()
            .filter_map(|record| {
                let revision = record.receipt.revision?;
                (!record.receipt.state.is_terminal()).then(|| PendingRevision {
                    attempt_id: record.receipt.attempt_id,
                    revision,
                    state: record.receipt.state.clone(),
                    expected_revision: record.receipt.request.expected_revision,
                })
            })
            .collect::<Vec<_>>();
        pending.sort_by_key(|pending| pending.revision);
        let first_revision = self
            .events
            .iter()
            .filter_map(|event| event.event.receipt.revision)
            .min();
        let last_revision = self
            .events
            .iter()
            .filter_map(|event| event.event.receipt.revision)
            .max();
        RuntimeMutationStatus {
            schema_version: MUTATION_SCHEMA_VERSION,
            runtime_epoch: self.epoch,
            event_sequence: self.events.back().map(|event| event.event.event_sequence),
            accepted_through: self.accepted_through,
            last_confirmed_revision: self.last_confirmed_revision,
            last_rejected_revision: self.last_rejected_revision,
            live_state: self.live_state.clone(),
            pending,
            receipt_window: ReceiptWindow {
                first_event_sequence: self.events.front().map(|event| event.event.event_sequence),
                last_event_sequence: self.events.back().map(|event| event.event.event_sequence),
                first_revision,
                last_revision,
                expires_before: now
                    .checked_sub(self.config.minimum_event_age)
                    .map(Timestamp::from_system_time),
            },
        }
    }

    fn capabilities(&self) -> MutationCapabilities {
        let available = |id: &str| MutationCapability {
            id: id.into(),
            availability: CapabilityAvailability::Available,
            reason_ids: Vec::new(),
        };
        let unavailable = |id: &str, reason: &str| MutationCapability {
            id: id.into(),
            availability: CapabilityAvailability::Unavailable,
            reason_ids: vec![reason.into()],
        };
        MutationCapabilities {
            schema_version: MUTATION_SCHEMA_VERSION,
            runtime_epoch: self.epoch,
            retention: ReceiptRetentionCapability {
                minimum_event_count: u32::try_from(self.config.minimum_event_count)
                    .unwrap_or(u32::MAX),
                minimum_age_seconds: u32::try_from(self.config.minimum_event_age.as_secs())
                    .unwrap_or(u32::MAX),
                tombstone_capacity: u32::try_from(self.config.idempotency_capacity)
                    .unwrap_or(u32::MAX),
                persistence: LedgerPersistence::InProcess,
            },
            expected_revision: available("mutation.expected_revision"),
            idempotency: available("mutation.idempotency"),
            cancellation_before_planning: available("mutation.cancel_before_planning"),
            backend_barrier: unavailable("mutation.backend_barrier", "m07_not_landed"),
            musical_boundary: unavailable("mutation.musical_boundary", "m04_not_instrumented"),
            atomic_generation_activation: unavailable(
                "mutation.atomic_generation_activation",
                "m07_not_landed",
            ),
        }
    }

    fn events_after(
        &self,
        epoch: RuntimeEpoch,
        after: Option<EventSequence>,
        now: SystemTime,
    ) -> EventQueryResult {
        if epoch != self.epoch {
            return EventQueryResult::ResetRequired {
                reason: ResetReason::RuntimeEpochChanged,
                status: Box::new(self.status(now)),
            };
        }
        let Some(first) = self.events.front().map(|event| event.event.event_sequence) else {
            return if after.is_some() {
                EventQueryResult::ResetRequired {
                    reason: ResetReason::SequenceAhead,
                    status: Box::new(self.status(now)),
                }
            } else {
                EventQueryResult::Events { events: Vec::new() }
            };
        };
        let last = self
            .events
            .back()
            .map(|event| event.event.event_sequence)
            .unwrap_or(first);
        if let Some(after) = after {
            if after > last {
                return EventQueryResult::ResetRequired {
                    reason: ResetReason::SequenceAhead,
                    status: Box::new(self.status(now)),
                };
            }
            if after.get().saturating_add(1) < first.get() {
                return EventQueryResult::ResetRequired {
                    reason: ResetReason::RetentionExpired,
                    status: Box::new(self.status(now)),
                };
            }
        }
        EventQueryResult::Events {
            events: self
                .events
                .iter()
                .filter(|event| after.is_none_or(|after| event.event.event_sequence > after))
                .map(|event| event.event.clone())
                .collect(),
        }
    }

    fn prune(&mut self, now: SystemTime) -> Result<(), LedgerError> {
        while self.events.len() > self.config.minimum_event_count {
            let Some(front) = self.events.front() else {
                break;
            };
            let age = now
                .duration_since(front.recorded_at)
                .unwrap_or(Duration::ZERO);
            if age < self.config.minimum_event_age {
                break;
            }
            let attempt_id = front.event.receipt.attempt_id;
            let protected = self.receipts.get(&attempt_id).is_some_and(|record| {
                !record.receipt.state.is_terminal()
                    && record.last_event_sequence == front.event.event_sequence
            });
            if protected {
                break;
            }
            self.events.pop_front();
        }

        let first_retained = self.events.front().map(|event| event.event.event_sequence);
        let expired_attempts = self
            .receipts
            .iter()
            .filter_map(|(attempt_id, record)| {
                (record.receipt.state.is_terminal()
                    && first_retained.is_none_or(|first| record.last_event_sequence < first))
                .then_some(*attempt_id)
            })
            .collect::<Vec<_>>();
        for attempt_id in expired_attempts {
            let Some(record) = self.receipts.remove(&attempt_id) else {
                continue;
            };
            if let Some(revision) = record.receipt.revision {
                self.revisions.remove(&revision);
            }
            if let Some(key) = record.idempotency_key {
                if let Some(idempotency) = self.idempotency.remove(&key) {
                    self.tombstones.insert(
                        key,
                        IdempotencyTombstone {
                            request_fingerprint: idempotency.request_fingerprint,
                            _expired_at: now,
                        },
                    );
                }
            }
        }
        Ok(())
    }

    fn error(
        &self,
        code: &str,
        message: &str,
        attempt_id: Option<AttemptId>,
        revision: Option<RevisionId>,
        retryable: bool,
    ) -> MutationError {
        MutationError {
            schema_version: MUTATION_SCHEMA_VERSION,
            code: code.into(),
            message: message.into(),
            runtime_epoch: self.epoch,
            attempt_id,
            revision,
            retryable,
            diagnostics: Vec::new(),
        }
    }
}

fn validate_planned(planned: &[PlannedComponent]) -> Result<(), LedgerError> {
    let mut paths = planned
        .iter()
        .map(|component| component.path.as_str())
        .collect::<Vec<_>>();
    if paths.iter().any(|path| path.is_empty())
        || planned.iter().any(|component| component.action.is_empty())
    {
        return Err(LedgerError::InvalidReceipt(
            "planned component paths and actions must be nonempty".into(),
        ));
    }
    paths.sort_unstable();
    paths.dedup();
    if paths.len() != planned.len() {
        return Err(LedgerError::InvalidReceipt(
            "planned component paths must be unique".into(),
        ));
    }
    Ok(())
}
