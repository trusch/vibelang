use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use std::time::SystemTime;
use uuid::Uuid;

pub const MUTATION_SCHEMA_VERSION: u16 = 1;

macro_rules! uuid_v7_id {
    ($name:ident) => {
        #[doc = "@vibelang-contract-wire"]
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn parse(value: &str) -> Result<Self, String> {
                let uuid = Uuid::parse_str(value).map_err(|error| error.to_string())?;
                if uuid.get_version_num() != 7 {
                    return Err(format!("{} must be UUIDv7", stringify!($name)));
                }
                if uuid.hyphenated().to_string() != value {
                    return Err(format!(
                        "{} must use canonical lowercase hyphenated UUID encoding",
                        stringify!($name)
                    ));
                }
                Ok(Self(uuid))
            }

            #[must_use]
            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.0.hyphenated().to_string())
                    .finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.hyphenated().fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(&value).map_err(de::Error::custom)
            }
        }
    };
}

uuid_v7_id!(AttemptId);
uuid_v7_id!(RuntimeEpoch);

macro_rules! decimal_u64 {
    ($name:ident) => {
        #[doc = "@vibelang-contract-wire"]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u64);

        impl $name {
            pub fn new(value: u64) -> Result<Self, String> {
                if value == 0 {
                    return Err(format!("{} must be greater than zero", stringify!($name)));
                }
                Ok(Self(value))
            }

            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }

            pub fn checked_next(self) -> Result<Self, String> {
                self.0
                    .checked_add(1)
                    .map(Self)
                    .ok_or_else(|| format!("{} exhausted", stringify!($name)))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let parsed = value.parse::<u64>().map_err(|error| error.to_string())?;
                if parsed.to_string() != value {
                    return Err(format!("{} must use canonical decimal", stringify!($name)));
                }
                Self::new(parsed)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::from_str(&value).map_err(de::Error::custom)
            }
        }
    };
}

decimal_u64!(RevisionId);
decimal_u64!(EventSequence);

/// Signed 1/65,536-quarter-note ticks.
#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BeatTicks(i64);

impl BeatTicks {
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl Serialize for BeatTicks {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for BeatTicks {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let parsed = value.parse::<i64>().map_err(de::Error::custom)?;
        if parsed.to_string() != value {
            return Err(de::Error::custom("BeatTicks must use canonical decimal"));
        }
        Ok(Self(parsed))
    }
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Timestamp(String);

impl Timestamp {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if !value.ends_with('Z') {
            return Err("timestamp must use RFC 3339 UTC encoding".into());
        }
        humantime::parse_rfc3339(&value).map_err(|error| error.to_string())?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn from_system_time(value: SystemTime) -> Self {
        Self(humantime::format_rfc3339(value).to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteSeconds(f64);

impl FiniteSeconds {
    pub fn new(value: f64) -> Result<Self, String> {
        if !value.is_finite() {
            return Err("backend seconds must be finite".into());
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl Serialize for FiniteSeconds {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for FiniteSeconds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(f64::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PublicDigest(String);

impl PublicDigest {
    pub(crate) fn sha256(hex: String) -> Self {
        Self(format!("sha256:{hex}"))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err("public digest must use sha256:<hex>".into());
        };
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("public digest must contain 64 lowercase hex digits".into());
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for PublicDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationSubmissionWire {
    pub kind: MutationKind,
    pub source: MutationSource,
    pub submission_digest: Option<PublicDigest>,
    pub idempotency_key_present: bool,
    pub expected_revision: Option<RevisionId>,
    pub atomicity: Atomicity,
    pub supersession: SupersessionPolicy,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationReceipt {
    pub schema_version: u16,
    pub attempt_id: AttemptId,
    pub runtime_epoch: RuntimeEpoch,
    pub revision: Option<RevisionId>,
    pub event_sequence: EventSequence,
    pub request: RequestIdentity,
    pub state: ReceiptState,
    pub previous_confirmed_revision: Option<RevisionId>,
    pub timestamps: ReceiptTimestamps,
    pub diagnostics: Vec<Diagnostic>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestIdentity {
    pub kind: MutationKind,
    pub source: MutationSource,
    pub submission_digest: Option<PublicDigest>,
    pub operation_digest: Option<PublicDigest>,
    pub idempotency_key_present: bool,
    pub expected_revision: Option<RevisionId>,
    pub atomicity: Atomicity,
    pub supersession: SupersessionPolicy,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "details",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ReceiptState {
    Evaluating { phase: PreAcceptancePhase },
    Accepted { queue_position: Option<u32> },
    Planning,
    Staging { completed: u32, total: u32 },
    Committing { phase: CommitPhase },
    Terminal(TerminalOutcome),
}

impl ReceiptState {
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal(_))
    }
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "outcome",
    content = "details",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TerminalOutcome {
    Rejected(Rejected),
    Superseded(Superseded),
    Applied(Applied),
    Partial(Partial),
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreAcceptancePhase {
    Decode,
    Parse,
    Evaluate,
    Validate,
    IdempotencyCheck,
    ExpectedRevisionCheck,
    CapabilityCheck,
    Admission,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitPhase {
    Reconcile,
    Activate,
    BackendBarrier,
    MusicalBoundary,
    ExternalEffects,
    Rollback,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "source",
    content = "details",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MutationSource {
    Cli {
        mode: CliMode,
        source: Option<String>,
    },
    Http {
        method: String,
        path: String,
        request_id: String,
    },
    Rhai {
        engine_id: String,
    },
    Wasm {
        instance_id: String,
    },
    Internal {
        parent_revision: RevisionId,
    },
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliMode {
    Startup,
    Watch,
    EvalServer,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptTimestamps {
    pub submitted_at: Timestamp,
    pub accepted_at: Option<Timestamp>,
    pub last_transition_at: Timestamp,
    pub terminal_at: Option<Timestamp>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub component_path: Option<String>,
    pub source_span: Option<SourceSpan>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub source: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "details",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MutationKind {
    Candidate {
        origin: CandidateOrigin,
    },
    Command {
        domain: MessageDomain,
        operation: String,
    },
    Compensation {
        for_revision: RevisionId,
    },
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateOrigin {
    ScriptFile,
    WatchReload,
    HttpEval,
    RhaiHost,
    WasmRuntime,
    WasmCompiler,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageDomain {
    Transport,
    SynthDef,
    Sample,
    Sfz,
    Recording,
    Group,
    Voice,
    Pattern,
    Melody,
    Sequence,
    Effect,
    Fade,
    Reload,
    Sync,
    Midi,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Atomicity {
    Required,
    BestEffort,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "policy",
    content = "details",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SupersessionPolicy {
    Fifo,
    ReplacePending { key: String },
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rejected {
    pub phase: FailurePhase,
    pub code: String,
    pub message: String,
    pub rollback: RollbackState,
    pub preserved_revision: Option<RevisionId>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Superseded {
    pub reason: SupersessionReason,
    pub by_revision: Option<RevisionId>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupersessionReason {
    Replaced,
    Cancelled,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Applied {
    pub effective_at: EffectiveAt,
    pub confirmations: Vec<Confirmation>,
    pub components: Vec<ComponentOutcome>,
    pub audible_tail_until: Option<EffectiveAt>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Partial {
    pub phase: FailurePhase,
    pub code: String,
    pub components: Vec<ComponentOutcome>,
    pub rollback: RollbackState,
    pub fenced: bool,
    pub last_confirmed_revision: Option<RevisionId>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePhase {
    Decode,
    Parse,
    Evaluate,
    Validate,
    Idempotency,
    ExpectedRevision,
    Capability,
    Admission,
    Planning,
    Staging,
    Reconcile,
    Activate,
    BackendBarrier,
    MusicalBoundary,
    ExternalEffect,
    Rollback,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedComponent {
    pub path: String,
    pub action: String,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentOutcome {
    pub path: String,
    pub action: String,
    pub state: ComponentState,
    pub effective_at: Option<EffectiveAt>,
    pub confirmation: Option<Confirmation>,
    pub diagnostic: Option<Diagnostic>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    Applied,
    Failed,
    Uncertain,
    NotStarted,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveAt {
    pub observed_at: Timestamp,
    pub musical_beat: Option<BeatTicks>,
    pub backend_time_seconds: Option<FiniteSeconds>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackState {
    NotNeeded,
    Confirmed,
    Failed,
    Unavailable,
    Uncertain,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "confirmation",
    content = "details",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Confirmation {
    RuntimeCommit,
    BackendBarrier {
        backend: String,
        token: String,
    },
    MusicalBoundary {
        beat: BeatTicks,
        backend_time: Option<FiniteSeconds>,
    },
    ExternalAcknowledgment {
        system: String,
        token: String,
    },
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeMutationStatus {
    pub schema_version: u16,
    pub runtime_epoch: RuntimeEpoch,
    pub event_sequence: Option<EventSequence>,
    pub accepted_through: Option<RevisionId>,
    pub last_confirmed_revision: Option<RevisionId>,
    pub last_rejected_revision: Option<RevisionId>,
    pub live_state: LiveState,
    pub pending: Vec<PendingRevision>,
    pub receipt_window: ReceiptWindow,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "details",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum LiveState {
    Clean,
    Partial {
        revision: RevisionId,
        fenced: bool,
    },
    PreAdmissionPartial {
        attempt_id: AttemptId,
        fenced: bool,
    },
    Unknown {
        since_revision: RevisionId,
        fenced: bool,
    },
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRevision {
    pub attempt_id: AttemptId,
    pub revision: RevisionId,
    pub state: ReceiptState,
    pub expected_revision: Option<RevisionId>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptWindow {
    pub first_event_sequence: Option<EventSequence>,
    pub last_event_sequence: Option<EventSequence>,
    pub first_revision: Option<RevisionId>,
    pub last_revision: Option<RevisionId>,
    pub expires_before: Option<Timestamp>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptEvent {
    pub schema_version: u16,
    pub runtime_epoch: RuntimeEpoch,
    pub event_sequence: EventSequence,
    pub previous_event_sequence: Option<EventSequence>,
    pub receipt: MutationReceipt,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetReason {
    RuntimeEpochChanged,
    RetentionExpired,
    SequenceAhead,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "result",
    content = "details",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EventQueryResult {
    Events {
        events: Vec<ReceiptEvent>,
    },
    ResetRequired {
        reason: ResetReason,
        status: Box<RuntimeMutationStatus>,
    },
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationError {
    pub schema_version: u16,
    pub code: String,
    pub message: String,
    pub runtime_epoch: RuntimeEpoch,
    pub attempt_id: Option<AttemptId>,
    pub revision: Option<RevisionId>,
    pub retryable: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAvailability {
    Available,
    Degraded,
    Unavailable,
    Unknown,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationCapability {
    pub id: String,
    pub availability: CapabilityAvailability,
    pub reason_ids: Vec<String>,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptRetentionCapability {
    pub minimum_event_count: u32,
    pub minimum_age_seconds: u32,
    pub tombstone_capacity: u32,
    pub persistence: LedgerPersistence,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerPersistence {
    InProcess,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationCapabilities {
    pub schema_version: u16,
    pub runtime_epoch: RuntimeEpoch,
    pub retention: ReceiptRetentionCapability,
    pub expected_revision: MutationCapability,
    pub idempotency: MutationCapability,
    pub cancellation_before_planning: MutationCapability,
    pub backend_barrier: MutationCapability,
    pub musical_boundary: MutationCapability,
    pub atomic_generation_activation: MutationCapability,
}

#[doc = "@vibelang-contract-wire"]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationContextWire {
    pub attempt_id: AttemptId,
    pub runtime_epoch: RuntimeEpoch,
    pub revision: Option<RevisionId>,
    pub component_path: Option<String>,
    pub idempotency_keyed: bool,
}

pub fn validate_transition(from: &ReceiptState, to: &ReceiptState) -> Result<(), String> {
    use ReceiptState::{Accepted, Committing, Evaluating, Planning, Staging, Terminal};

    let legal = match (from, to) {
        (Evaluating { phase: left }, Evaluating { phase: right }) => right >= left,
        (Evaluating { .. }, Accepted { .. }) => true,
        (Evaluating { .. }, Terminal(outcome)) => matches!(
            outcome,
            TerminalOutcome::Rejected(_)
                | TerminalOutcome::Superseded(_)
                | TerminalOutcome::Partial(_)
        ),
        (Accepted { .. }, Planning) => true,
        (Accepted { .. }, Terminal(outcome)) => matches!(
            outcome,
            TerminalOutcome::Rejected(_)
                | TerminalOutcome::Superseded(_)
                | TerminalOutcome::Partial(_)
        ),
        (Planning, Staging { .. } | Committing { .. }) => true,
        (Planning, Terminal(outcome)) => matches!(
            outcome,
            TerminalOutcome::Rejected(_) | TerminalOutcome::Partial(_)
        ),
        (
            Staging {
                completed: left_completed,
                total: left_total,
            },
            Staging {
                completed: right_completed,
                total: right_total,
            },
        ) => right_total == left_total && right_completed >= left_completed,
        (Staging { .. }, Committing { .. }) => true,
        (Staging { .. }, Terminal(outcome)) => matches!(
            outcome,
            TerminalOutcome::Rejected(_) | TerminalOutcome::Partial(_)
        ),
        (Committing { phase: left }, Committing { phase: right }) => right >= left,
        (Committing { .. }, Terminal(outcome)) => matches!(
            outcome,
            TerminalOutcome::Rejected(_)
                | TerminalOutcome::Applied(_)
                | TerminalOutcome::Partial(_)
        ),
        (Terminal(_), _) => false,
        _ => false,
    };
    if legal {
        validate_staging(to)
    } else {
        Err(format!(
            "illegal receipt transition from {from:?} to {to:?}"
        ))
    }
}

fn validate_staging(state: &ReceiptState) -> Result<(), String> {
    if let ReceiptState::Staging { completed, total } = state {
        if *completed > *total {
            return Err("staging completed count exceeds total".into());
        }
    }
    Ok(())
}

pub fn validate_terminal(
    outcome: &TerminalOutcome,
    previous_confirmed_revision: Option<RevisionId>,
    planned: &[PlannedComponent],
) -> Result<(), String> {
    match outcome {
        TerminalOutcome::Rejected(rejected) => {
            if rejected.code.trim().is_empty() || rejected.message.trim().is_empty() {
                return Err("rejected outcome requires a nonempty code and message".into());
            }
            if !matches!(
                rejected.rollback,
                RollbackState::NotNeeded | RollbackState::Confirmed
            ) {
                return Err("rejected outcome requires no rollback or confirmed rollback".into());
            }
            if rejected.preserved_revision != previous_confirmed_revision {
                return Err(
                    "rejected outcome must preserve the previous confirmed revision".into(),
                );
            }
        }
        TerminalOutcome::Superseded(_) => {}
        TerminalOutcome::Applied(applied) => {
            if applied.confirmations.is_empty() {
                return Err("applied outcome requires at least one confirmation".into());
            }
            validate_component_partition(&applied.components, planned)?;
            if applied
                .components
                .iter()
                .any(|component| component.state != ComponentState::Applied)
            {
                return Err("applied outcome may contain only applied components".into());
            }
        }
        TerminalOutcome::Partial(partial) => {
            if partial.code.trim().is_empty() {
                return Err("partial outcome requires a nonempty code".into());
            }
            validate_component_partition(&partial.components, planned)?;
            if partial.last_confirmed_revision != previous_confirmed_revision {
                return Err("partial outcome must retain the previous confirmed revision".into());
            }
            if !partial.components.iter().any(|component| {
                matches!(
                    component.state,
                    ComponentState::Applied | ComponentState::Uncertain
                )
            }) {
                return Err(
                    "partial outcome requires at least one applied or uncertain component".into(),
                );
            }
            if partial
                .components
                .iter()
                .any(|component| component.state == ComponentState::Uncertain)
                && !partial.fenced
            {
                return Err("uncertain partial outcome must fence the runtime".into());
            }
            if matches!(
                partial.rollback,
                RollbackState::Failed | RollbackState::Uncertain
            ) && !partial.fenced
            {
                return Err("failed or uncertain rollback must fence the runtime".into());
            }
        }
    }
    Ok(())
}

fn validate_component_partition(
    outcomes: &[ComponentOutcome],
    planned: &[PlannedComponent],
) -> Result<(), String> {
    let mut expected = planned
        .iter()
        .map(|component| (component.path.as_str(), component.action.as_str()))
        .collect::<Vec<_>>();
    expected.sort_unstable();
    expected.dedup();
    if expected.len() != planned.len() {
        return Err("planned component paths and actions must be unique".into());
    }
    let mut actual = outcomes
        .iter()
        .map(|component| (component.path.as_str(), component.action.as_str()))
        .collect::<Vec<_>>();
    actual.sort_unstable();
    actual.dedup();
    if actual.len() != outcomes.len() || actual != expected {
        return Err("terminal components must exactly partition the planned components".into());
    }
    Ok(())
}
