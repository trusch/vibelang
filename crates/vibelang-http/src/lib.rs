//! HTTP REST API server for VibeLang (vibelang-core backend).
//!
//! Provides a REST API and WebSocket endpoint for querying and controlling
//! a running VibeLang session using the vibelang-core runtime.
//!
//! # Features
//!
//! - Full CRUD operations for voices, patterns, melodies, sequences
//! - Transport control (play, stop, seek, tempo)
//! - Effect and sample management
//! - MIDI routing and callbacks
//! - Real-time WebSocket events
//! - Live state queries (active synths, meters)
//!
//! # Usage
//!
//! ```ignore
//! use vibelang_http::{start_server, AppState};
//! use vibelang_core::RuntimeHandle;
//!
//! let handle = runtime.handle();
//! let state = runtime.state();
//! tokio::spawn(async move {
//!     start_server(handle, state, [127, 0, 0, 1].into(), 1606, None).await;
//! });
//! ```

mod models;
mod routes;
mod websocket;

use axum::{
    body::{to_bytes, Body},
    extract::Request,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use vibelang_core::mutation::{
    Atomicity, CandidateOrigin, Diagnostic, ExternalDomain, FailurePhase, MessageDomain,
    MutationEventSink, MutationKind, MutationReceipt, MutationReplySink, MutationSource,
    ReceiptEvent, ReceiptState, RequestMaterial, RevisionId, RuntimeEpoch, RuntimeMutationStatus,
    Submission, SupersessionPolicy, TerminalOutcome,
};
use vibelang_core::{Message, RuntimeHandle, State};

pub use models::*;
pub use routes::eval::{EvalJob, EvalResult};
pub use websocket::WebSocketEvent;

/// Sender type for eval requests.
pub type EvalSender = std::sync::mpsc::Sender<EvalJob>;

const LEGACY_SUCCESS_SCOPE_HEADER: &str = "x-vibelang-legacy-success-scope";
const RUNTIME_EPOCH_HEADER: &str = "x-vibelang-runtime-epoch";
const REVISION_HEADER: &str = "x-vibelang-revision";
const EVENT_SEQUENCE_HEADER: &str = "x-vibelang-event-sequence";
const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
const RETRY_EPOCH_HEADER: &str = "x-vibelang-runtime-epoch";
const EXPECTED_REVISION_HEADER: &str = "x-vibelang-expected-revision";
const UNCONDITIONAL_HEADER: &str = "x-vibelang-unconditional";
const WAIT_TIMEOUT_HEADER: &str = "x-vibelang-wait-timeout-ms";
const PREFER_HEADER: &str = "prefer";
const PREFERENCE_APPLIED_HEADER: &str = "preference-applied";
const INSECURE_REMOTE_ACKNOWLEDGEMENT: &str =
    "I understand this exposes VibeLang without remote-user isolation";
pub(crate) const REDACTED_REJECTION_TEXT: &str =
    "mutation rejected; private diagnostic detail is redacted";

#[derive(Clone)]
pub struct HttpCredential {
    pub subject: String,
    pub bearer_token: String,
    pub privileged_detail: bool,
}

impl std::fmt::Debug for HttpCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpCredential")
            .field("subject", &self.subject)
            .field("bearer_token", &"[REDACTED]")
            .field("privileged_detail", &self.privileged_detail)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum HttpSecurityMode {
    LoopbackLocal {
        allowed_origins: Vec<String>,
    },
    AuthenticatedRemote {
        credentials: Vec<HttpCredential>,
        allowed_origins: Vec<String>,
    },
    InsecureRemote {
        acknowledgement: String,
        allowed_origins: Vec<String>,
    },
}

impl HttpSecurityMode {
    #[must_use]
    pub fn loopback_local() -> Self {
        Self::LoopbackLocal {
            allowed_origins: vec![
                "http://localhost".into(),
                "http://127.0.0.1".into(),
                "http://[::1]".into(),
            ],
        }
    }

    pub fn insecure_remote(
        acknowledgement: impl Into<String>,
        allowed_origins: Vec<String>,
    ) -> Result<Self, HttpServerError> {
        let acknowledgement = acknowledgement.into();
        if acknowledgement != INSECURE_REMOTE_ACKNOWLEDGEMENT {
            return Err(HttpServerError::InsecureRemoteAcknowledgementRequired);
        }
        Ok(Self::InsecureRemote {
            acknowledgement,
            allowed_origins,
        })
    }

    fn mode_id(&self) -> &'static str {
        match self {
            Self::LoopbackLocal { .. } => "security.http.loopback_local",
            Self::AuthenticatedRemote { .. } => "security.http.authenticated_remote",
            Self::InsecureRemote { .. } => "security.http.insecure_remote",
        }
    }

    fn allowed_origins(&self) -> &[String] {
        match self {
            Self::LoopbackLocal { allowed_origins }
            | Self::AuthenticatedRemote {
                allowed_origins, ..
            }
            | Self::InsecureRemote {
                allowed_origins, ..
            } => allowed_origins,
        }
    }

    fn remote(&self) -> bool {
        !matches!(self, Self::LoopbackLocal { .. })
    }
}

#[derive(Clone, Debug)]
pub struct HttpServerOptions {
    pub security: HttpSecurityMode,
    pub max_body_bytes: usize,
    pub rate_limit_per_minute: u32,
    pub max_wait: Duration,
    pub eval_enabled: bool,
    pub audit_enabled: bool,
}

impl Default for HttpServerOptions {
    fn default() -> Self {
        Self {
            security: HttpSecurityMode::loopback_local(),
            max_body_bytes: 1024 * 1024,
            rate_limit_per_minute: 600,
            max_wait: Duration::from_secs(30),
            eval_enabled: true,
            audit_enabled: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HttpServerError {
    #[error("loopback-local HTTP security mode requires a loopback bind address")]
    LoopbackBindRequired,
    #[error(
        "authenticated-remote HTTP security mode requires credentials and an origin allowlist"
    )]
    AuthenticatedRemotePolicyIncomplete,
    #[error("insecure-remote HTTP mode requires the exact high-friction acknowledgement")]
    InsecureRemoteAcknowledgementRequired,
    #[error("HTTP request and wait limits must be nonzero")]
    InvalidLimits,
    #[error("failed to bind HTTP listener: {0}")]
    Bind(#[from] std::io::Error),
}

#[derive(Debug)]
struct RateBucket {
    started: Instant,
    count: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReceiptBroadcastCursor {
    runtime_epoch: RuntimeEpoch,
    event_sequence: Option<vibelang_core::mutation::EventSequence>,
}

#[derive(Debug)]
struct HttpSecurityPolicy {
    mode: HttpSecurityMode,
    max_body_bytes: usize,
    rate_limit_per_minute: u32,
    max_wait: Duration,
    eval_enabled: bool,
    audit_enabled: bool,
    insecure_session_namespace: String,
    rate_buckets: Mutex<HashMap<String, RateBucket>>,
}

impl HttpSecurityPolicy {
    fn new(options: HttpServerOptions) -> Result<Self, HttpServerError> {
        if options.max_body_bytes == 0
            || options.rate_limit_per_minute == 0
            || options.max_wait.is_zero()
        {
            return Err(HttpServerError::InvalidLimits);
        }
        match &options.security {
            HttpSecurityMode::AuthenticatedRemote {
                credentials,
                allowed_origins,
            } if credentials.is_empty() || allowed_origins.is_empty() => {
                return Err(HttpServerError::AuthenticatedRemotePolicyIncomplete);
            }
            HttpSecurityMode::InsecureRemote {
                acknowledgement, ..
            } if acknowledgement != INSECURE_REMOTE_ACKNOWLEDGEMENT => {
                return Err(HttpServerError::InsecureRemoteAcknowledgementRequired);
            }
            _ => {}
        }
        Ok(Self {
            mode: options.security,
            max_body_bytes: options.max_body_bytes,
            rate_limit_per_minute: options.rate_limit_per_minute,
            max_wait: options.max_wait,
            eval_enabled: options.eval_enabled,
            audit_enabled: options.audit_enabled,
            insecure_session_namespace: format!("http.insecure.{}", uuid::Uuid::new_v4()),
            rate_buckets: Mutex::new(HashMap::new()),
        })
    }

    fn authentication_and_privileged_detail(&self) -> (bool, bool) {
        match &self.mode {
            HttpSecurityMode::AuthenticatedRemote { credentials, .. } => (
                true,
                credentials
                    .iter()
                    .any(|credential| credential.privileged_detail),
            ),
            _ => (false, false),
        }
    }
}

#[derive(Clone)]
struct HttpRequestContext {
    method: Method,
    path: String,
    v2: bool,
    semantic_body: Option<Value>,
    caller_namespace: String,
    idempotency_key: Option<String>,
    retry_epoch: Option<RuntimeEpoch>,
    expected_revision: Option<RevisionId>,
    wait_terminal: Option<Duration>,
    read_status: Option<RuntimeMutationStatus>,
    redact_receipts: bool,
    receipts: Arc<Mutex<BTreeMap<String, MutationReceipt>>>,
}

impl HttpRequestContext {
    fn legacy(method: Method, path: String) -> Self {
        Self {
            method,
            path,
            v2: false,
            semantic_body: None,
            caller_namespace: "compat.vibelang.v1.http.local".into(),
            idempotency_key: None,
            retry_epoch: None,
            expected_revision: None,
            wait_terminal: None,
            read_status: None,
            redact_receipts: false,
            receipts: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn record(&self, receipt: MutationReceipt) {
        let receipt = sanitize_mutation_receipt(receipt, self.redact_receipts);
        if let Ok(mut receipts) = self.receipts.lock() {
            let key = receipt.attempt_id.to_string();
            let replace = receipts
                .get(&key)
                .is_none_or(|current| receipt.event_sequence >= current.event_sequence);
            if replace {
                receipts.insert(key, receipt);
            }
        }
    }

    fn current_receipts(&self) -> Vec<MutationReceipt> {
        self.receipts
            .lock()
            .map(|receipts| receipts.values().cloned().collect())
            .unwrap_or_default()
    }
}

tokio::task_local! {
    static HTTP_REQUEST_CONTEXT: HttpRequestContext;
}

fn sanitize_mutation_receipt(
    mut receipt: MutationReceipt,
    redact_private_detail: bool,
) -> MutationReceipt {
    if !redact_private_detail {
        return receipt;
    }
    if let ReceiptState::Terminal(outcome) = &mut receipt.state {
        match outcome {
            TerminalOutcome::Rejected(rejected) => {
                rejected.message = REDACTED_REJECTION_TEXT.into();
            }
            TerminalOutcome::Applied(applied) => {
                for component in &mut applied.components {
                    if let Some(diagnostic) = &mut component.diagnostic {
                        sanitize_diagnostic(diagnostic);
                    }
                }
            }
            TerminalOutcome::Partial(partial) => {
                for component in &mut partial.components {
                    if let Some(diagnostic) = &mut component.diagnostic {
                        sanitize_diagnostic(diagnostic);
                    }
                }
            }
            TerminalOutcome::Superseded(_) => {}
        }
    }
    for diagnostic in &mut receipt.diagnostics {
        sanitize_diagnostic(diagnostic);
    }
    receipt
}

fn sanitize_diagnostic(diagnostic: &mut Diagnostic) {
    diagnostic.message = "private diagnostic detail is redacted".into();
    diagnostic.component_path = None;
    if let Some(span) = &mut diagnostic.source_span {
        span.source = "<redacted>".into();
    }
}

pub(crate) fn publish_receipt_event(
    ws_tx: &broadcast::Sender<WebSocketEvent>,
    cursor: &Mutex<ReceiptBroadcastCursor>,
    mut event: ReceiptEvent,
    redact_private_detail: bool,
) {
    let publish = cursor.lock().is_ok_and(|mut cursor| {
        if cursor.runtime_epoch == event.runtime_epoch {
            if cursor
                .event_sequence
                .is_some_and(|sequence| event.event_sequence <= sequence)
            {
                return false;
            }
            // Publishing past a gap would advance the shared cursor over
            // concurrently emitted lower-sequence events and skip them on the
            // legacy stream forever; the ledger poller replays gaps in order.
            if cursor.event_sequence != event.previous_event_sequence {
                return false;
            }
        }
        cursor.runtime_epoch = event.runtime_epoch;
        cursor.event_sequence = Some(event.event_sequence);
        true
    });
    if publish {
        event.receipt = sanitize_mutation_receipt(event.receipt, redact_private_detail);
        let _ = ws_tx.send(WebSocketEvent::receipt(event));
    }
}

/// Honest v1 carrier wrapped around an existing direct-mutation response.
#[derive(Debug, Serialize)]
pub struct MutationHttpResponse {
    pub receipt: MutationReceipt,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_receipts: Vec<MutationReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_result: Option<Value>,
}

/// Shared application state for HTTP handlers.
pub struct AppState {
    /// Runtime handle for sending messages.
    pub handle: RuntimeHandle,
    /// Shared runtime state for reading.
    pub state: Arc<RwLock<State>>,
    /// Broadcast channel for WebSocket events.
    pub ws_tx: broadcast::Sender<WebSocketEvent>,
    /// Channel to send eval requests to the main thread (optional).
    pub eval_tx: Option<EvalSender>,
    receipt_broadcast_cursor: Arc<Mutex<ReceiptBroadcastCursor>>,
    security: Arc<HttpSecurityPolicy>,
}

impl AppState {
    /// Read state immutably.
    pub async fn with_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&State) -> R,
    {
        let guard = self.state.read().await;
        f(&guard)
    }

    /// Submit a direct HTTP mutation through the canonical receipt ledger.
    pub async fn send(&self, msg: Message) -> Result<(), vibelang_core::Error> {
        let submission = self.http_submission(
            MutationKind::Command {
                domain: msg.domain(),
                operation: msg.operation().to_lowercase(),
            },
            json!({
                "message_type": msg.type_name(),
            }),
        );
        let (latest, reply_sink, event_sink) = self.mutation_sinks();
        let result = self
            .handle
            .submit_with_sinks(msg, submission, reply_sink, event_sink)
            .await;

        match result {
            Ok(receipt) => {
                record_http_receipt(receipt);
                Ok(())
            }
            Err(error) => {
                if let Some(receipt) = latest.lock().ok().and_then(|latest| latest.clone()) {
                    record_http_receipt(receipt);
                }
                Err(error)
            }
        }
    }

    pub(crate) fn eval_submission(&self, code: &str) -> Submission {
        self.http_submission(
            MutationKind::Candidate {
                origin: CandidateOrigin::HttpEval,
            },
            json!({
                "operation": "eval",
                "code": code,
            }),
        )
    }

    pub(crate) fn mutation_sinks(
        &self,
    ) -> (
        Arc<Mutex<Option<MutationReceipt>>>,
        MutationReplySink,
        MutationEventSink,
    ) {
        let latest = Arc::new(Mutex::new(None));
        let reply_sink = http_reply_sink(Arc::clone(&latest));
        let ws_tx = self.ws_tx.clone();
        let redact_receipts = self.security.mode.remote();
        let receipt_broadcast_cursor = Arc::clone(&self.receipt_broadcast_cursor);
        let event_sink = MutationEventSink::new(move |event| {
            publish_receipt_event(&ws_tx, &receipt_broadcast_cursor, event, redact_receipts);
        });
        (latest, reply_sink, event_sink)
    }

    pub(crate) fn public_receipt(&self, receipt: MutationReceipt) -> MutationReceipt {
        sanitize_mutation_receipt(receipt, self.security.mode.remote())
    }

    pub(crate) fn public_receipt_event(
        &self,
        mut event: vibelang_core::mutation::ReceiptEvent,
    ) -> vibelang_core::mutation::ReceiptEvent {
        event.receipt = self.public_receipt(event.receipt);
        event
    }

    pub(crate) fn publish_receipt_event(&self, event: ReceiptEvent) {
        publish_receipt_event(
            &self.ws_tx,
            &self.receipt_broadcast_cursor,
            event,
            self.security.mode.remote(),
        );
    }

    fn http_submission(&self, kind: MutationKind, semantic: Value) -> Submission {
        let context = HTTP_REQUEST_CONTEXT.try_with(Clone::clone).ok();
        let method = context
            .as_ref()
            .map(|context| context.method.to_string())
            .unwrap_or_else(|| "HTTP".into());
        let path = context
            .as_ref()
            .map(|context| context.path.clone())
            .unwrap_or_else(|| "/compat/v1".into());
        let v2 = context.as_ref().is_some_and(|context| context.v2);
        let (kind, material) = if v2 {
            (
                mutation_kind_for_http_path(&path),
                canonical_http_request_material(
                    &method,
                    &path,
                    context
                        .as_ref()
                        .and_then(|context| context.semantic_body.as_ref()),
                ),
            )
        } else {
            let public_material = json!({
                "method": &method,
                "path": &path,
                "operation": &semantic,
            });
            (
                kind,
                RequestMaterial::from_values(semantic, Some(public_material)),
            )
        };
        Submission {
            kind,
            source: MutationSource::Http {
                method,
                path,
                request_id: uuid::Uuid::new_v4().to_string(),
            },
            caller_namespace: context
                .as_ref()
                .map(|context| context.caller_namespace.clone())
                .unwrap_or_else(|| "compat.vibelang.v1.http.local".into()),
            idempotency_key: context
                .as_ref()
                .and_then(|context| context.idempotency_key.clone()),
            require_idempotency_key: v2
                && self.security.mode.remote()
                && http_operation_requires_idempotency(
                    context.as_ref().map(|context| context.path.as_str()),
                ),
            retry_epoch: context
                .as_ref()
                .and_then(|context| context.retry_epoch)
                .or_else(|| Some(self.handle.mutation_status().runtime_epoch)),
            expected_revision: context
                .as_ref()
                .and_then(|context| context.expected_revision),
            atomicity: Atomicity::BestEffort,
            supersession: SupersessionPolicy::Fifo,
            material,
        }
    }

    fn http_capabilities(&self) -> HttpCapabilities {
        let mutation = self.handle.mutation_capabilities();
        let (authenticated, privileged_detail_enabled) =
            self.security.authentication_and_privileged_detail();
        let insecure = matches!(self.security.mode, HttpSecurityMode::InsecureRemote { .. });
        HttpCapabilities {
            schema_version: HTTP_V2_SCHEMA_VERSION,
            runtime_epoch: mutation.runtime_epoch,
            mutation,
            security: HttpSecurityCapabilities {
                mode_id: self.security.mode.mode_id().into(),
                degraded: insecure,
                reason_ids: if insecure {
                    vec!["reason.security_policy_disabled".into()]
                } else {
                    Vec::new()
                },
                authentication_required: authenticated,
                origin_allowlist_required: true,
                request_limits_enabled: true,
                rate_limits_enabled: true,
                audit_enabled: self.security.audit_enabled,
                eval_enabled: self.security.eval_enabled && self.eval_tx.is_some(),
                privileged_detail_enabled,
            },
        }
    }

    fn http_capability_details(&self) -> HttpCapabilityDetails {
        HttpCapabilityDetails {
            schema_version: HTTP_V2_SCHEMA_VERSION,
            runtime_epoch: self.handle.mutation_status().runtime_epoch,
            security: HttpSecurityPolicyDetails {
                mode_id: self.security.mode.mode_id().into(),
                max_body_bytes: self.security.max_body_bytes,
                rate_limit_per_minute: self.security.rate_limit_per_minute,
                max_wait_ms: self.security.max_wait.as_millis().min(u128::from(u64::MAX)) as u64,
                eval_enabled: self.security.eval_enabled && self.eval_tx.is_some(),
                audit_enabled: self.security.audit_enabled,
            },
        }
    }
}

fn http_operation_requires_idempotency(path: Option<&str>) -> bool {
    let Some(path) = path else {
        return false;
    };
    path.contains("/note-on")
        || path.ends_with("/trigger")
        || path.starts_with("/v2/midi/")
        || path.starts_with("/v2/recordings")
}

fn is_http_mutation_method(method: &Method) -> bool {
    method == Method::POST
        || method == Method::PATCH
        || method == Method::PUT
        || method == Method::DELETE
}

fn v2_pre_admission_receipt_eligible(method: &Method, path: &str) -> bool {
    is_http_mutation_method(method) && !path.starts_with("/v2/receipts/")
}

fn mutation_kind_for_http_path(path: &str) -> MutationKind {
    if path == "/v2/eval" {
        return MutationKind::Candidate {
            origin: CandidateOrigin::HttpEval,
        };
    }
    let domain = match path.trim_start_matches("/v2/").split('/').next() {
        Some("transport") => MessageDomain::Transport,
        Some("groups") => MessageDomain::Group,
        Some("voices") => MessageDomain::Voice,
        Some("patterns") => MessageDomain::Pattern,
        Some("melodies") => MessageDomain::Melody,
        Some("sequences") => MessageDomain::Sequence,
        Some("effects") => MessageDomain::Effect,
        Some("samples") => MessageDomain::Sample,
        Some("synthdefs") => MessageDomain::SynthDef,
        Some("recordings") => MessageDomain::Recording,
        Some("fades") => MessageDomain::Fade,
        Some("midi") => MessageDomain::Midi,
        _ => {
            return MutationKind::External {
                domain: ExternalDomain::Network,
                operation: path.trim_start_matches("/v2/").replace('/', "."),
            };
        }
    };
    MutationKind::Command {
        domain,
        operation: path.trim_start_matches("/v2/").replace(['/', '-'], "_"),
    }
}

fn canonical_http_request_material(
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> RequestMaterial {
    // The semantic value feeds the ledger request fingerprint. The method must
    // be part of it: the mutation kind is derived from the path alone, so a
    // body-only fingerprint would let one Idempotency-Key replay a receipt
    // across different methods on the same resource.
    let semantic = http_request_identity(method, path, body);
    let public = semantic.clone();
    RequestMaterial::from_values(semantic, Some(public))
}

fn http_request_identity(method: &str, path: &str, body: Option<&Value>) -> Value {
    json!({
        "method": method,
        "path": path,
        "body": body.cloned().unwrap_or(Value::Null),
    })
}

fn failure_phase_for_http_problem(problem: &HttpRequestProblem) -> FailurePhase {
    match problem.code.as_str() {
        "invalid_json" | "unknown_field" | "invalid_request" => FailurePhase::Decode,
        "parse_error" => FailurePhase::Parse,
        "idempotency_key_required" | "idempotency_key_mismatch" | "invalid_runtime_epoch" => {
            FailurePhase::Idempotency
        }
        "expected_revision_required"
        | "expected_revision_mismatch"
        | "invalid_expected_revision" => FailurePhase::ExpectedRevision,
        "unsupported_field" | "unsupported_value" | "capability_unavailable" => {
            FailurePhase::Capability
        }
        "rate_limit_exceeded"
        | "request_body_too_large"
        | "authentication_required"
        | "authentication_failed"
        | "origin_not_allowed" => FailurePhase::Admission,
        _ => FailurePhase::Validate,
    }
}

fn pre_admission_rejection_receipt(
    state: &AppState,
    authorized: &AuthorizedHttpRequest,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    semantic_body: Option<&Value>,
    problem: &HttpRequestProblem,
) -> Option<(MutationReceipt, bool)> {
    if !v2_pre_admission_receipt_eligible(method, path) {
        return None;
    }
    let idempotency_key = headers
        .get(IDEMPOTENCY_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty() && value.len() <= 255)
        .map(str::to_string);
    let retry_epoch = headers
        .get(RETRY_EPOCH_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| RuntimeEpoch::parse(value).ok())
        .or_else(|| Some(state.handle.mutation_status().runtime_epoch));
    let expected_revision = headers
        .get(EXPECTED_REVISION_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .and_then(|value| RevisionId::new(value).ok());
    let submission = Submission {
        kind: mutation_kind_for_http_path(path),
        source: MutationSource::Http {
            method: method.to_string(),
            path: path.into(),
            request_id: uuid::Uuid::new_v4().to_string(),
        },
        caller_namespace: authorized.caller_namespace.clone(),
        idempotency_key,
        require_idempotency_key: state.security.mode.remote()
            && http_operation_requires_idempotency(Some(path)),
        retry_epoch,
        expected_revision,
        atomicity: Atomicity::BestEffort,
        supersession: SupersessionPolicy::Fifo,
        material: canonical_http_request_material(method.as_str(), path, semantic_body),
    };
    let (latest, reply_sink, event_sink) = state.mutation_sinks();
    let attempt = state
        .handle
        .begin_attempt(submission, reply_sink, event_sink)
        .ok()?;
    let replayed_or_precondition_rejected = !attempt.is_active();
    let receipt = if !replayed_or_precondition_rejected {
        state
            .handle
            .finish_attempt_failure(
                attempt,
                failure_phase_for_http_problem(problem),
                problem.code.clone(),
                problem.message.clone(),
            )
            .ok()
    } else {
        Some(attempt.receipt().clone())
    };
    receipt
        .or_else(|| latest.lock().ok().and_then(|receipt| receipt.clone()))
        .map(|receipt| (receipt, replayed_or_precondition_rejected))
}

fn handler_rejection_receipt(
    state: &AppState,
    context: &HttpRequestContext,
    problem: &HttpRequestProblem,
) -> Option<(MutationReceipt, bool)> {
    if !v2_pre_admission_receipt_eligible(&context.method, &context.path) {
        return None;
    }
    let submission = Submission {
        kind: mutation_kind_for_http_path(&context.path),
        source: MutationSource::Http {
            method: context.method.to_string(),
            path: context.path.clone(),
            request_id: uuid::Uuid::new_v4().to_string(),
        },
        caller_namespace: context.caller_namespace.clone(),
        idempotency_key: context.idempotency_key.clone(),
        require_idempotency_key: state.security.mode.remote()
            && http_operation_requires_idempotency(Some(context.path.as_str())),
        retry_epoch: context
            .retry_epoch
            .or_else(|| Some(state.handle.mutation_status().runtime_epoch)),
        expected_revision: context.expected_revision,
        atomicity: Atomicity::BestEffort,
        supersession: SupersessionPolicy::Fifo,
        material: canonical_http_request_material(
            context.method.as_str(),
            &context.path,
            context.semantic_body.as_ref(),
        ),
    };
    let (latest, reply_sink, event_sink) = state.mutation_sinks();
    let attempt = state
        .handle
        .begin_attempt(submission, reply_sink, event_sink)
        .ok()?;
    let replayed_or_precondition_rejected = !attempt.is_active();
    let receipt = if !replayed_or_precondition_rejected {
        state
            .handle
            .finish_attempt_failure(
                attempt,
                failure_phase_for_http_problem(problem),
                problem.code.clone(),
                problem.message.clone(),
            )
            .ok()
    } else {
        Some(attempt.receipt().clone())
    };
    receipt
        .or_else(|| latest.lock().ok().and_then(|receipt| receipt.clone()))
        .map(|receipt| {
            (
                sanitize_mutation_receipt(receipt, state.security.mode.remote()),
                replayed_or_precondition_rejected,
            )
        })
}

fn record_http_receipt(receipt: MutationReceipt) {
    let _ = HTTP_REQUEST_CONTEXT.try_with(|context| context.record(receipt));
}

fn http_reply_sink(latest: Arc<Mutex<Option<MutationReceipt>>>) -> MutationReplySink {
    let request_context = HTTP_REQUEST_CONTEXT.try_with(Clone::clone).ok();
    MutationReplySink::new(move |receipt| {
        let mut publish = false;
        if let Ok(mut latest) = latest.lock() {
            let replace = latest.as_ref().is_none_or(|current| {
                current.attempt_id == receipt.attempt_id
                    && current.runtime_epoch == receipt.runtime_epoch
                    && receipt.event_sequence > current.event_sequence
            });
            if replace {
                *latest = Some(receipt.clone());
                publish = true;
            }
        }
        if publish {
            if let Some(context) = &request_context {
                context.record(receipt);
            }
        }
    })
}

pub(crate) fn canonical_latest_receipt(
    returned: Option<MutationReceipt>,
    observed: Option<MutationReceipt>,
) -> Option<MutationReceipt> {
    match (returned, observed) {
        (Some(returned), Some(observed))
            if returned.attempt_id == observed.attempt_id
                && observed.event_sequence >= returned.event_sequence =>
        {
            Some(observed)
        }
        (Some(returned), _) => Some(returned),
        (None, observed) => observed,
    }
}

#[derive(Debug)]
struct AuthorizedHttpRequest {
    caller_namespace: String,
    origin: Option<String>,
}

#[derive(Debug)]
struct HttpRequestProblem {
    status: StatusCode,
    code: String,
    message: String,
    field: Option<String>,
    reason: Option<String>,
    supported_values: Vec<String>,
}

impl HttpRequestProblem {
    fn bad_request(code: &str, message: impl Into<String>, field: Option<&str>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: code.into(),
            message: message.into(),
            field: field.map(str::to_string),
            reason: None,
            supported_values: Vec::new(),
        }
    }

    fn unsupported(field: &str, reason: impl Into<String>, supported_values: Vec<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "unsupported_field".into(),
            message: format!("field `{field}` is not supported for this v2 operation"),
            field: Some(field.into()),
            reason: Some(reason.into()),
            supported_values,
        }
    }
}

fn request_problem_response(
    state: &AppState,
    authorized: &AuthorizedHttpRequest,
    method: &Method,
    path: &str,
    v2: bool,
    headers: &HeaderMap,
    semantic_body: Option<&Value>,
    mut problem: HttpRequestProblem,
) -> Response {
    let pre_admission = v2
        .then(|| {
            pre_admission_rejection_receipt(
                state,
                authorized,
                method,
                path,
                headers,
                semantic_body,
                &problem,
            )
        })
        .flatten();
    let receipt = pre_admission.map(|(receipt, overrides_problem)| {
        (
            sanitize_mutation_receipt(receipt, state.security.mode.remote()),
            overrides_problem,
        )
    });
    if let Some((receipt, true)) = &receipt {
        if let ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) = &receipt.state {
            if rejected.code == problem.code {
                problem.message = rejected.message.clone();
            } else {
                problem = problem_from_receipt_rejection(rejected);
            }
        } else {
            let mut response = v2_receipt_response(receipt.clone(), false);
            apply_cors_headers(
                response.headers_mut(),
                authorized.origin.as_deref(),
                &state.security.mode,
            );
            return response;
        }
    }
    sanitize_http_problem(&mut problem, state.security.mode.remote());
    let receipt = receipt.map(|(receipt, _)| receipt);
    let mut response = http_problem_response(method, path, v2, problem, receipt);
    apply_cors_headers(
        response.headers_mut(),
        authorized.origin.as_deref(),
        &state.security.mode,
    );
    response
}

fn problem_from_receipt_rejection(
    rejected: &vibelang_core::mutation::Rejected,
) -> HttpRequestProblem {
    let (field, reason) = match rejected.code.as_str() {
        "idempotency_key_required"
        | "idempotency_key_expired"
        | "idempotency_conflict"
        | "idempotency_capacity_exhausted" => (
            Some(IDEMPOTENCY_KEY_HEADER.into()),
            Some("mutation.idempotency".into()),
        ),
        "runtime_epoch_changed" => (
            Some(RETRY_EPOCH_HEADER.into()),
            Some("mutation.idempotency_epoch".into()),
        ),
        "revision_conflict" => (
            Some(EXPECTED_REVISION_HEADER.into()),
            Some("mutation.expected_revision".into()),
        ),
        _ => (None, Some("mutation.receipt".into())),
    };
    HttpRequestProblem {
        status: rejected_http_status(&rejected.code),
        code: rejected.code.clone(),
        message: rejected.message.clone(),
        field,
        reason,
        supported_values: Vec::new(),
    }
}

fn sanitize_http_problem(problem: &mut HttpRequestProblem, redact_private_detail: bool) {
    if redact_private_detail {
        problem.message = REDACTED_REJECTION_TEXT.into();
    }
}

async fn project_http_mutation_response(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let v2 = path == "/v2" || path.starts_with("/v2/");
    let authorized =
        match authorize_http_request(&state.security, request.headers(), &method, &path) {
            Ok(authorized) => authorized,
            Err(problem) => {
                let origin = request
                    .headers()
                    .get(header::ORIGIN)
                    .and_then(|origin| origin.to_str().ok())
                    .filter(|origin| http_origin_allowed(&state.security.mode, origin));
                let mut response = http_problem_response(&method, &path, v2, problem, None);
                apply_cors_headers(response.headers_mut(), origin, &state.security.mode);
                return response;
            }
        };
    if method == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        apply_cors_headers(
            response.headers_mut(),
            authorized.origin.as_deref(),
            &state.security.mode,
        );
        return response;
    }
    if !v2 {
        if let Some(problem) = v1_remote_policy_problem(&state.security.mode, &method, &path) {
            return request_problem_response(
                &state,
                &authorized,
                &method,
                &path,
                false,
                request.headers(),
                None,
                problem,
            );
        }
    }
    if v2
        && request.uri().query().is_some_and(|query| !query.is_empty())
        && path != "/v2/receipt-events"
    {
        return request_problem_response(
            &state,
            &authorized,
            &method,
            &path,
            true,
            request.headers(),
            None,
            HttpRequestProblem {
                status: StatusCode::BAD_REQUEST,
                code: "unknown_query".into(),
                message: "this v2 operation does not accept query parameters".into(),
                field: Some("query".into()),
                reason: Some("schema.operation_scoped".into()),
                supported_values: Vec::new(),
            },
        );
    }

    let headers = request.headers().clone();
    let (mut parts, body) = request.into_parts();
    let bytes = match to_bytes(body, state.security.max_body_bytes).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return request_problem_response(
                &state,
                &authorized,
                &method,
                &path,
                v2,
                &headers,
                None,
                HttpRequestProblem {
                    status: StatusCode::PAYLOAD_TOO_LARGE,
                    code: "request_body_too_large".into(),
                    message: format!(
                        "request body exceeds the {} byte limit",
                        state.security.max_body_bytes
                    ),
                    field: None,
                    reason: Some("security.request_body_limit".into()),
                    supported_values: Vec::new(),
                },
            );
        }
    };
    let mut semantic_body = if bytes.is_empty() {
        None
    } else {
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => Some(value),
            Err(error) if v2 => {
                return request_problem_response(
                    &state,
                    &authorized,
                    &method,
                    &path,
                    true,
                    &headers,
                    None,
                    HttpRequestProblem::bad_request(
                        "invalid_json",
                        format!("request body is not valid JSON: {error}"),
                        None,
                    ),
                );
            }
            Err(_) => None,
        }
    };
    let request_identity_body = semantic_body.clone();
    if v2 {
        if let Err(problem) =
            validate_and_normalize_v2_request(&state, &method, &path, &mut semantic_body).await
        {
            return request_problem_response(
                &state,
                &authorized,
                &method,
                &path,
                true,
                &headers,
                request_identity_body.as_ref(),
                problem,
            );
        }
    }
    let request_bytes = semantic_body
        .as_ref()
        .and_then(|body| serde_json::to_vec(body).ok())
        .unwrap_or_else(|| bytes.to_vec());
    if v2 && semantic_body.is_some() {
        parts.headers.remove(header::CONTENT_LENGTH);
        parts.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }
    let request = Request::from_parts(parts, Body::from(request_bytes));

    let idempotency_key = match optional_header(&headers, IDEMPOTENCY_KEY_HEADER) {
        Ok(value) => value,
        Err(problem) => {
            return request_problem_response(
                &state,
                &authorized,
                &method,
                &path,
                v2,
                &headers,
                request_identity_body.as_ref(),
                problem,
            );
        }
    };
    let retry_epoch = match parse_epoch_header(&headers) {
        Ok(value) => value,
        Err(problem) => {
            return request_problem_response(
                &state,
                &authorized,
                &method,
                &path,
                v2,
                &headers,
                request_identity_body.as_ref(),
                problem,
            );
        }
    };
    let expected_revision = match parse_revision_header(&headers) {
        Ok(value) => value,
        Err(problem) => {
            return request_problem_response(
                &state,
                &authorized,
                &method,
                &path,
                v2,
                &headers,
                request_identity_body.as_ref(),
                problem,
            );
        }
    };
    let unconditional_best_effort = match parse_unconditional_header(&headers) {
        Ok(value) => value,
        Err(problem) => {
            return request_problem_response(
                &state,
                &authorized,
                &method,
                &path,
                v2,
                &headers,
                request_identity_body.as_ref(),
                problem,
            );
        }
    };
    let revision_precondition = operation_requires_expected_revision(&method, &path);
    if v2
        && !is_http_mutation_method(&method)
        && (idempotency_key.is_some()
            || retry_epoch.is_some()
            || expected_revision.is_some()
            || unconditional_best_effort
            || headers.contains_key(WAIT_TIMEOUT_HEADER)
            || prefer_wait_terminal(&headers))
    {
        return request_problem_response(
            &state,
            &authorized,
            &method,
            &path,
            true,
            &headers,
            request_identity_body.as_ref(),
            HttpRequestProblem::unsupported(
                "mutation_headers",
                "idempotency, revision, and bounded-wait headers apply only to mutations",
                Vec::new(),
            ),
        );
    }
    if v2 && unconditional_best_effort && !revision_precondition {
        return request_problem_response(
            &state,
            &authorized,
            &method,
            &path,
            true,
            &headers,
            request_identity_body.as_ref(),
            HttpRequestProblem::unsupported(
                UNCONDITIONAL_HEADER,
                "this operation has no required revision precondition to bypass",
                Vec::new(),
            ),
        );
    }
    if expected_revision.is_some() && unconditional_best_effort {
        return request_problem_response(
            &state,
            &authorized,
            &method,
            &path,
            true,
            &headers,
            request_identity_body.as_ref(),
            representation_conflict(EXPECTED_REVISION_HEADER, UNCONDITIONAL_HEADER),
        );
    }
    if v2 && expected_revision.is_none() && !unconditional_best_effort && revision_precondition {
        return request_problem_response(
            &state,
            &authorized,
            &method,
            &path,
            true,
            &headers,
            request_identity_body.as_ref(),
            HttpRequestProblem {
                status: StatusCode::PRECONDITION_REQUIRED,
                code: "expected_revision_required".into(),
                message: format!(
                    "this operation requires `{EXPECTED_REVISION_HEADER}` or explicit unconditional best effort"
                ),
                field: Some(EXPECTED_REVISION_HEADER.into()),
                reason: Some("mutation.expected_revision".into()),
                supported_values: vec!["positive decimal revision".into(), "x-vibelang-unconditional: best-effort".into()],
            },
        );
    }
    let wait_terminal = match parse_wait_preference(&headers, state.security.max_wait) {
        Ok(value) => value,
        Err(problem) => {
            return request_problem_response(
                &state,
                &authorized,
                &method,
                &path,
                v2,
                &headers,
                request_identity_body.as_ref(),
                problem,
            );
        }
    };
    let eval_response = path == "/eval" || path == "/v2/eval";
    let context = if v2 {
        HttpRequestContext {
            method: method.clone(),
            path: path.clone(),
            v2: true,
            semantic_body: request_identity_body,
            caller_namespace: authorized.caller_namespace.clone(),
            idempotency_key,
            retry_epoch,
            expected_revision,
            wait_terminal,
            read_status: (method == Method::GET).then(|| state.handle.mutation_status()),
            redact_receipts: state.security.mode.remote(),
            receipts: Arc::new(Mutex::new(BTreeMap::new())),
        }
    } else {
        let mut context = HttpRequestContext::legacy(method.clone(), path.clone());
        context.redact_receipts = state.security.mode.remote();
        context
    };
    let projection_context = context.clone();
    let mut receipt_events = state.ws_tx.subscribe();
    let projection_state = state.clone();
    HTTP_REQUEST_CONTEXT
        .scope(context, async move {
            let response = next.run(request).await;
            if let Some(wait) = projection_context.wait_terminal {
                wait_for_terminal_receipt(
                    &projection_state,
                    &projection_context,
                    &mut receipt_events,
                    wait,
                )
                .await;
            }
            let mut response = if projection_context.v2 {
                project_v2_response(response, &projection_context, &projection_state).await
            } else {
                let response = project_response(response, &projection_context, eval_response).await;
                sanitize_v1_remote_response(response, &projection_context, &projection_state).await
            };
            apply_cors_headers(
                response.headers_mut(),
                authorized.origin.as_deref(),
                &projection_state.security.mode,
            );
            response
        })
        .await
}

fn optional_header(
    headers: &HeaderMap,
    name: &'static str,
) -> Result<Option<String>, HttpRequestProblem> {
    let Some(value) = headers.get(name) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| {
        HttpRequestProblem::bad_request(
            "invalid_header",
            format!("`{name}` is not UTF-8"),
            Some(name),
        )
    })?;
    if value.trim().is_empty() || value.len() > 255 || value.bytes().any(|byte| byte < 0x20) {
        return Err(HttpRequestProblem::bad_request(
            "invalid_header",
            format!("`{name}` must contain 1..=255 visible bytes"),
            Some(name),
        ));
    }
    Ok(Some(value.to_string()))
}

fn parse_epoch_header(headers: &HeaderMap) -> Result<Option<RuntimeEpoch>, HttpRequestProblem> {
    optional_header(headers, RETRY_EPOCH_HEADER)?
        .map(|value| {
            RuntimeEpoch::parse(&value).map_err(|message| {
                HttpRequestProblem::bad_request(
                    "invalid_runtime_epoch",
                    message,
                    Some(RETRY_EPOCH_HEADER),
                )
            })
        })
        .transpose()
}

fn parse_revision_header(headers: &HeaderMap) -> Result<Option<RevisionId>, HttpRequestProblem> {
    optional_header(headers, EXPECTED_REVISION_HEADER)?
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| {
                    HttpRequestProblem::bad_request(
                        "invalid_expected_revision",
                        "expected revision must be a positive decimal integer",
                        Some(EXPECTED_REVISION_HEADER),
                    )
                })
                .and_then(|value| {
                    RevisionId::new(value).map_err(|message| {
                        HttpRequestProblem::bad_request(
                            "invalid_expected_revision",
                            message,
                            Some(EXPECTED_REVISION_HEADER),
                        )
                    })
                })
        })
        .transpose()
}

fn parse_unconditional_header(headers: &HeaderMap) -> Result<bool, HttpRequestProblem> {
    match optional_header(headers, UNCONDITIONAL_HEADER)? {
        Some(value) if value == "best-effort" => Ok(true),
        Some(_) => Err(HttpRequestProblem {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "unsupported_value".into(),
            message: format!(
                "`{UNCONDITIONAL_HEADER}` supports only the exact value `best-effort`"
            ),
            field: Some(UNCONDITIONAL_HEADER.into()),
            reason: Some("mutation.expected_revision".into()),
            supported_values: vec!["best-effort".into()],
        }),
        None => Ok(false),
    }
}

fn prefer_wait_terminal(headers: &HeaderMap) -> bool {
    headers
        .get(PREFER_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|item| item.trim().eq_ignore_ascii_case("wait=terminal"))
        })
}

fn parse_wait_preference(
    headers: &HeaderMap,
    maximum: Duration,
) -> Result<Option<Duration>, HttpRequestProblem> {
    if !prefer_wait_terminal(headers) {
        if headers.contains_key(WAIT_TIMEOUT_HEADER) {
            return Err(HttpRequestProblem::bad_request(
                "wait_preference_required",
                format!(
                    "`{WAIT_TIMEOUT_HEADER}` requires `Prefer: wait=terminal` on the same request"
                ),
                Some(WAIT_TIMEOUT_HEADER),
            ));
        }
        return Ok(None);
    }
    let requested = optional_header(headers, WAIT_TIMEOUT_HEADER)?
        .map(|value| {
            value.parse::<u64>().map_err(|_| {
                HttpRequestProblem::bad_request(
                    "invalid_wait_timeout",
                    "wait timeout must be a positive number of milliseconds",
                    Some(WAIT_TIMEOUT_HEADER),
                )
            })
        })
        .transpose()?
        .unwrap_or_else(|| maximum.as_millis().min(u128::from(u64::MAX)) as u64);
    if requested == 0 {
        return Err(HttpRequestProblem::bad_request(
            "invalid_wait_timeout",
            "wait timeout must be greater than zero",
            Some(WAIT_TIMEOUT_HEADER),
        ));
    }
    Ok(Some(Duration::from_millis(requested).min(maximum)))
}

fn operation_requires_expected_revision(method: &Method, path: &str) -> bool {
    if path.starts_with("/v2/receipts/") {
        return false;
    }
    method == Method::PATCH
        || method == Method::PUT
        || method == Method::DELETE
        || (method == Method::POST && path == "/v2/eval")
}

async fn wait_for_terminal_receipt(
    state: &AppState,
    context: &HttpRequestContext,
    events: &mut broadcast::Receiver<WebSocketEvent>,
    wait: Duration,
) {
    let Some(initial) = canonical_http_receipt(&context.current_receipts()).cloned() else {
        return;
    };
    if initial.state.is_terminal() {
        return;
    }
    let attempt_id = initial.attempt_id;
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match state.handle.mutation_receipt(attempt_id) {
            Ok(receipt) => {
                context.record(receipt.clone());
                if receipt.state.is_terminal() {
                    return;
                }
            }
            Err(_) => return,
        }
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Ok(event))
                if event.receipt.as_ref().is_some_and(|receipt| {
                    receipt.attempt_id == attempt_id && receipt.state.is_terminal()
                }) =>
            {
                if let Some(receipt) = event.receipt {
                    context.record(receipt);
                }
                return;
            }
            Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(broadcast::error::RecvError::Closed)) | Err(_) => return,
        }
    }
}

fn authorize_http_request(
    policy: &HttpSecurityPolicy,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
) -> Result<AuthorizedHttpRequest, HttpRequestProblem> {
    let result = authorize_http_request_unaudited(policy, headers, method, path);
    if policy.audit_enabled {
        match &result {
            Ok(authorized) => tracing::info!(
                target: "vibelang_http::audit",
                method = %method,
                path,
                caller_namespace = %authorized.caller_namespace,
                "HTTP request admitted"
            ),
            Err(problem) => tracing::warn!(
                target: "vibelang_http::audit",
                method = %method,
                path,
                code = %problem.code,
                status = %problem.status,
                "HTTP request rejected"
            ),
        }
    }
    result
}

fn authorize_http_request_unaudited(
    policy: &HttpSecurityPolicy,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
) -> Result<AuthorizedHttpRequest, HttpRequestProblem> {
    let origin = headers
        .get(header::ORIGIN)
        .map(|value| {
            value.to_str().map(str::to_string).map_err(|_| {
                HttpRequestProblem::bad_request(
                    "invalid_origin",
                    "Origin must be valid UTF-8",
                    Some("origin"),
                )
            })
        })
        .transpose()?;
    if let Some(origin) = &origin {
        if !http_origin_allowed(&policy.mode, origin) {
            return Err(HttpRequestProblem {
                status: StatusCode::FORBIDDEN,
                code: "origin_not_allowed".into(),
                message: "request Origin is not allowed by the active HTTP security mode".into(),
                field: Some("origin".into()),
                reason: Some("security.origin_allowlist".into()),
                supported_values: Vec::new(),
            });
        }
    }

    if path.ends_with("/eval") && !policy.eval_enabled {
        return Err(HttpRequestProblem {
            status: StatusCode::FORBIDDEN,
            code: "capability_unavailable".into(),
            message: "HTTP evaluation is disabled by operator policy".into(),
            field: None,
            reason: Some("capability.http.eval".into()),
            supported_values: Vec::new(),
        });
    }

    let (caller_namespace, privileged_detail) = match &policy.mode {
        HttpSecurityMode::LoopbackLocal { .. } => ("http.loopback.runtime_local".into(), false),
        HttpSecurityMode::AuthenticatedRemote { .. } if method == Method::OPTIONS => {
            ("http.authenticated.preflight".into(), false)
        }
        HttpSecurityMode::AuthenticatedRemote { credentials, .. } => {
            let authorization = headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .ok_or_else(|| HttpRequestProblem {
                    status: StatusCode::UNAUTHORIZED,
                    code: "authentication_required".into(),
                    message: "a valid Bearer credential is required".into(),
                    field: Some("authorization".into()),
                    reason: Some("security.authentication".into()),
                    supported_values: Vec::new(),
                })?;
            let credential = credentials
                .iter()
                .find(|credential| constant_time_eq(&credential.bearer_token, authorization))
                .ok_or_else(|| HttpRequestProblem {
                    status: StatusCode::UNAUTHORIZED,
                    code: "authentication_failed".into(),
                    message: "the supplied Bearer credential is invalid".into(),
                    field: Some("authorization".into()),
                    reason: Some("security.authentication".into()),
                    supported_values: Vec::new(),
                })?;
            (
                format!("http.authenticated.{}", credential.subject),
                credential.privileged_detail,
            )
        }
        HttpSecurityMode::InsecureRemote { .. } => {
            (policy.insecure_session_namespace.clone(), false)
        }
    };

    if method != Method::OPTIONS && path == "/v2/capabilities/details" && !privileged_detail {
        return Err(HttpRequestProblem {
            status: StatusCode::FORBIDDEN,
            code: "privileged_detail_required".into(),
            message: "privileged capability details require an authenticated privileged credential"
                .into(),
            field: None,
            reason: Some("security.privileged_detail".into()),
            supported_values: Vec::new(),
        });
    }

    if method != Method::OPTIONS {
        let mut buckets = policy.rate_buckets.lock().map_err(|_| HttpRequestProblem {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "rate_limit_unavailable".into(),
            message: "the request rate limiter is unavailable".into(),
            field: None,
            reason: Some("security.rate_limit".into()),
            supported_values: Vec::new(),
        })?;
        let now = Instant::now();
        let bucket = buckets
            .entry(caller_namespace.clone())
            .or_insert(RateBucket {
                started: now,
                count: 0,
            });
        if now.duration_since(bucket.started) >= Duration::from_secs(60) {
            bucket.started = now;
            bucket.count = 0;
        }
        if bucket.count >= policy.rate_limit_per_minute {
            return Err(HttpRequestProblem {
                status: StatusCode::TOO_MANY_REQUESTS,
                code: "rate_limit_exceeded".into(),
                message: "the HTTP request rate limit has been exceeded".into(),
                field: None,
                reason: Some("security.rate_limit".into()),
                supported_values: Vec::new(),
            });
        }
        bucket.count += 1;
    }

    Ok(AuthorizedHttpRequest {
        caller_namespace,
        origin,
    })
}

fn constant_time_eq(expected: &str, supplied: &str) -> bool {
    let expected = expected.as_bytes();
    let supplied = supplied.as_bytes();
    let mut difference = expected.len() ^ supplied.len();
    let length = expected.len().max(supplied.len());
    for index in 0..length {
        difference |= usize::from(
            expected.get(index).copied().unwrap_or(0) ^ supplied.get(index).copied().unwrap_or(0),
        );
    }
    difference == 0
}

fn is_loopback_origin(origin: &str) -> bool {
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let host = authority.split('/').next().unwrap_or_default();
    host == "localhost"
        || host.starts_with("localhost:")
        || host == "127.0.0.1"
        || host.starts_with("127.0.0.1:")
        || host == "[::1]"
        || host.starts_with("[::1]:")
}

fn http_origin_allowed(mode: &HttpSecurityMode, origin: &str) -> bool {
    match mode {
        HttpSecurityMode::LoopbackLocal { .. } => {
            is_loopback_origin(origin)
                && mode.allowed_origins().iter().any(|allowed| {
                    origin == allowed
                        || origin
                            .strip_prefix(allowed)
                            .is_some_and(|suffix| suffix.starts_with(':'))
                })
        }
        HttpSecurityMode::AuthenticatedRemote { .. } | HttpSecurityMode::InsecureRemote { .. } => {
            mode.allowed_origins()
                .iter()
                .any(|allowed| allowed == origin)
        }
    }
}

fn apply_cors_headers(headers: &mut HeaderMap, origin: Option<&str>, mode: &HttpSecurityMode) {
    if let Some(origin) = origin.and_then(|origin| HeaderValue::from_str(origin).ok()) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PATCH, PUT, DELETE, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(
            "authorization, content-type, idempotency-key, prefer, x-vibelang-runtime-epoch, x-vibelang-expected-revision, x-vibelang-unconditional, x-vibelang-wait-timeout-ms",
        ),
    );
    headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static(
            "location, preference-applied, x-vibelang-runtime-epoch, x-vibelang-revision, x-vibelang-event-sequence, x-vibelang-legacy-success-scope",
        ),
    );
    if matches!(mode, HttpSecurityMode::AuthenticatedRemote { .. }) {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        );
    }
}

async fn project_v2_response(
    response: Response,
    context: &HttpRequestContext,
    state: &AppState,
) -> Response {
    let receipts = context.current_receipts();
    if let Some(receipt) = canonical_http_receipt(&receipts).cloned() {
        let mut response = v2_receipt_response(receipt.clone(), context.wait_terminal.is_some());
        if context.method == Method::DELETE && context.path.starts_with("/v2/receipts/") {
            *response.status_mut() = cancel_receipt_http_status(&receipt);
        }
        return response;
    }

    let status = response.status();
    let control_read = context.path.starts_with("/v2/receipts/")
        || context.path == "/v2/mutation-status"
        || context.path == "/v2/capabilities"
        || context.path == "/v2/capabilities/details"
        || context.path == "/v2/receipt-events";
    if status == StatusCode::SWITCHING_PROTOCOLS || context.path == "/v2/ws" {
        return response;
    }
    if status.is_success() && context.method == Method::GET {
        let runtime_status = context
            .read_status
            .clone()
            .unwrap_or_else(|| state.handle.mutation_status());
        if control_read {
            let mut response = response;
            insert_status_headers(response.headers_mut(), &state.handle.mutation_status());
            return response;
        }
        let (mut parts, body) = response.into_parts();
        let data = to_bytes(body, usize::MAX)
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .unwrap_or(Value::Null);
        let group_names = state
            .with_state(|runtime| {
                runtime
                    .groups
                    .iter()
                    .map(|(id, group)| (id.raw().to_string(), group.name.clone()))
                    .collect::<HashMap<_, _>>()
            })
            .await;
        let data = sanitize_v2_state(
            &context.path,
            data,
            state.security.mode.remote(),
            &group_names,
        );
        let revisioned = Revisioned {
            schema_version: HTTP_V2_SCHEMA_VERSION,
            runtime_epoch: runtime_status.runtime_epoch,
            event_sequence: runtime_status.event_sequence,
            last_confirmed_revision: runtime_status.last_confirmed_revision,
            data,
        };
        parts.headers.remove(header::CONTENT_LENGTH);
        parts.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        insert_status_headers(&mut parts.headers, &runtime_status);
        return Response::from_parts(
            parts,
            Body::from(serde_json::to_vec(&revisioned).unwrap_or_default()),
        );
    }

    if status.is_success() && context.method != Method::GET {
        let problem = HttpRequestProblem {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "mutation_receipt_missing".into(),
            message: "the mutation completed without allocating a canonical receipt".into(),
            field: None,
            reason: Some("mutation.receipt_required".into()),
            supported_values: Vec::new(),
        };
        return project_v2_problem_response(context, state, problem);
    }

    let (mut parts, body) = response.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.unwrap_or_default();
    let body = serde_json::from_slice::<Value>(&bytes).ok();
    if body.as_ref().is_some_and(is_v2_error_envelope) {
        parts.headers.remove(header::CONTENT_LENGTH);
        return Response::from_parts(parts, Body::from(bytes));
    }
    let code = body
        .as_ref()
        .and_then(|body| body.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("request_failed");
    let message = body
        .as_ref()
        .and_then(|body| body.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("the HTTP operation failed");
    let problem = HttpRequestProblem {
        status: parts.status,
        code: code.into(),
        message: message.into(),
        field: None,
        reason: None,
        supported_values: Vec::new(),
    };
    project_v2_problem_response(context, state, problem)
}

fn project_v2_problem_response(
    context: &HttpRequestContext,
    state: &AppState,
    mut problem: HttpRequestProblem,
) -> Response {
    sanitize_http_problem(&mut problem, state.security.mode.remote());
    let receipt = handler_rejection_receipt(state, context, &problem);
    if let Some((receipt, true)) = &receipt {
        if let ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) = &receipt.state {
            if rejected.code == problem.code {
                problem.message = rejected.message.clone();
            } else {
                problem = problem_from_receipt_rejection(rejected);
                sanitize_http_problem(&mut problem, state.security.mode.remote());
            }
        } else {
            return v2_receipt_response(receipt.clone(), context.wait_terminal.is_some());
        }
    }
    http_problem_response(
        &context.method,
        &context.path,
        true,
        problem,
        receipt.map(|(receipt, _)| receipt),
    )
}

fn v2_receipt_response(receipt: MutationReceipt, wait_terminal: bool) -> Response {
    let mut response = Json(receipt.clone()).into_response();
    *response.status_mut() = receipt_http_status(&receipt);
    response.headers_mut().insert(
        header::LOCATION,
        header_value(&format!("/v2/receipts/{}", receipt.attempt_id)),
    );
    insert_freshness_headers(response.headers_mut(), &receipt);
    // `Preference-Applied` may only assert a preference that was honored: a
    // bounded wait that timed out returns a non-terminal receipt and must not
    // claim wait=terminal.
    if wait_terminal && receipt.state.is_terminal() {
        response.headers_mut().insert(
            PREFERENCE_APPLIED_HEADER,
            HeaderValue::from_static("wait=terminal"),
        );
    }
    response
}

/// Cancel is its own operation: a successful cancellation leaves the target
/// receipt Superseded (200), while any other terminal state means the cancel
/// could not take effect (409).
fn cancel_receipt_http_status(receipt: &MutationReceipt) -> StatusCode {
    match &receipt.state {
        ReceiptState::Terminal(TerminalOutcome::Superseded(_)) => StatusCode::OK,
        ReceiptState::Terminal(_) => StatusCode::CONFLICT,
        _ => StatusCode::ACCEPTED,
    }
}

fn is_v2_error_envelope(value: &Value) -> bool {
    value.get("schema_version").and_then(Value::as_u64) == Some(u64::from(HTTP_V2_SCHEMA_VERSION))
        && value.get("operation").and_then(Value::as_str).is_some()
        && value.get("error").and_then(Value::as_object).is_some()
}

fn receipt_http_status(receipt: &MutationReceipt) -> StatusCode {
    match &receipt.state {
        ReceiptState::Evaluating { .. }
        | ReceiptState::Accepted { .. }
        | ReceiptState::Planning
        | ReceiptState::Staging { .. }
        | ReceiptState::Committing { .. } => StatusCode::ACCEPTED,
        ReceiptState::Terminal(TerminalOutcome::Applied(_)) => StatusCode::OK,
        ReceiptState::Terminal(TerminalOutcome::Superseded(_)) => StatusCode::CONFLICT,
        ReceiptState::Terminal(TerminalOutcome::Partial(_)) => StatusCode::CONFLICT,
        ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) => {
            rejected_http_status(&rejected.code)
        }
    }
}

fn insert_freshness_headers(headers: &mut HeaderMap, receipt: &MutationReceipt) {
    headers.insert(
        RUNTIME_EPOCH_HEADER,
        header_value(&receipt.runtime_epoch.to_string()),
    );
    headers.insert(
        EVENT_SEQUENCE_HEADER,
        header_value(&receipt.event_sequence.to_string()),
    );
    if let Some(revision) = receipt.revision {
        headers.insert(REVISION_HEADER, header_value(&revision.to_string()));
    }
}

fn insert_status_headers(
    headers: &mut HeaderMap,
    status: &vibelang_core::mutation::RuntimeMutationStatus,
) {
    headers.insert(
        RUNTIME_EPOCH_HEADER,
        header_value(&status.runtime_epoch.to_string()),
    );
    if let Some(sequence) = status.event_sequence {
        headers.insert(EVENT_SEQUENCE_HEADER, header_value(&sequence.to_string()));
    }
    if let Some(revision) = status.last_confirmed_revision {
        headers.insert(REVISION_HEADER, header_value(&revision.to_string()));
    }
}

fn http_problem_response(
    method: &Method,
    path: &str,
    v2: bool,
    problem: HttpRequestProblem,
    receipt: Option<MutationReceipt>,
) -> Response {
    if !v2 {
        return (
            problem.status,
            Json(ErrorResponse::new(&problem.code, &problem.message)),
        )
            .into_response();
    }
    let operation = format!(
        "{} {}",
        method.as_str().to_ascii_lowercase(),
        path.strip_prefix("/v2").unwrap_or(path)
    );
    let receipt_headers = receipt.clone();
    let mut response = (
        problem.status,
        Json(HttpErrorEnvelope {
            schema_version: HTTP_V2_SCHEMA_VERSION,
            operation,
            error: HttpErrorDetail {
                code: problem.code,
                message: problem.message,
                field: problem.field,
                reason: problem.reason,
                supported_values: problem.supported_values,
            },
            receipt,
        }),
    )
        .into_response();
    if response.status() == StatusCode::UNAUTHORIZED {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"vibelang-http\""),
        );
    }
    if let Some(receipt) = receipt_headers {
        response.headers_mut().insert(
            header::LOCATION,
            header_value(&format!("/v2/receipts/{}", receipt.attempt_id)),
        );
        insert_freshness_headers(response.headers_mut(), &receipt);
    }
    response
}

/// Remote security modes forbid arbitrary server-filesystem access on the v1
/// surface exactly as the v2 policy already does.
fn v1_remote_policy_problem(
    mode: &HttpSecurityMode,
    method: &Method,
    path: &str,
) -> Option<HttpRequestProblem> {
    if !mode.remote() {
        return None;
    }
    (method == Method::POST && path == "/samples").then(|| HttpRequestProblem {
        status: StatusCode::FORBIDDEN,
        code: "capability_unavailable".into(),
        message: "remote filesystem sample loading is disabled by HTTP policy".into(),
        field: Some("path".into()),
        reason: Some("capability.extension.filesystem".into()),
        supported_values: Vec::new(),
    })
}

fn v1_remote_read_redaction_applies(path: &str) -> bool {
    path == "/samples"
        || path.starts_with("/samples/")
        || path == "/recordings"
        || path.starts_with("/recordings/")
}

/// Remote privacy redaction for v1 reads: strip server filesystem paths from
/// sample and recording projections, mirroring the v2 revisioned-read rule.
async fn sanitize_v1_remote_response(
    response: Response,
    context: &HttpRequestContext,
    state: &AppState,
) -> Response {
    if !state.security.mode.remote()
        || context.method != Method::GET
        || !response.status().is_success()
        || !v1_remote_read_redaction_applies(&context.path)
    {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, usize::MAX).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(mut data) = serde_json::from_slice::<Value>(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    for_each_resource_object_mut(&mut data, |object| {
        object.remove("path");
    });
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(
        parts,
        Body::from(serde_json::to_vec(&data).unwrap_or_default()),
    )
}

fn sanitize_v2_state(
    path: &str,
    mut data: Value,
    remote: bool,
    group_names: &HashMap<String, String>,
) -> Value {
    for_each_resource_object_mut(&mut data, |object| {
        if path.ends_with("/transport") {
            remove_object_fields(object, &["quantization_beats"]);
        } else if path.contains("/groups") {
            remove_object_fields(object, &["synth_node_ids", "source_location"]);
        } else if path.contains("/voices") {
            remove_object_fields(
                object,
                &[
                    "group_name",
                    "output_bus",
                    "soloed",
                    "sfz_instrument",
                    "vst_instrument",
                    "sustained_notes",
                    "source_location",
                ],
            );
            normalize_group_path(object, group_names);
        } else if path.contains("/patterns") {
            remove_object_fields(
                object,
                &["params", "is_looping", "source_location", "step_pattern"],
            );
            remove_status_schedule_fields(object);
            normalize_group_path(object, group_names);
        } else if path.contains("/melodies") {
            remove_object_fields(
                object,
                &["params", "is_looping", "source_location", "notes_patterns"],
            );
            remove_status_schedule_fields(object);
            normalize_group_path(object, group_names);
        } else if path.contains("/sequences") {
            remove_object_fields(object, &["source_location"]);
            if let Some(clips) = object.get_mut("clips").and_then(Value::as_array_mut) {
                for clip in clips {
                    if let Some(clip) = clip.as_object_mut() {
                        remove_object_fields(clip, &["once", "duration_beats"]);
                    }
                }
            }
        } else if path.contains("/effects") {
            remove_object_fields(
                object,
                &[
                    "bus_in",
                    "bus_out",
                    "position",
                    "vst_plugin",
                    "source_location",
                ],
            );
        } else if path.contains("/samples") {
            remove_object_fields(object, &["slices"]);
        }
        if remote && (path.contains("/samples") || path.contains("/recordings")) {
            object.remove("path");
        }
    });
    data
}

fn for_each_resource_object_mut(
    value: &mut Value,
    mut visit: impl FnMut(&mut serde_json::Map<String, Value>),
) {
    match value {
        Value::Object(object) => visit(object),
        Value::Array(values) => {
            for value in values {
                if let Some(object) = value.as_object_mut() {
                    visit(object);
                }
            }
        }
        _ => {}
    }
}

fn remove_object_fields(object: &mut serde_json::Map<String, Value>, fields: &[&str]) {
    for field in fields {
        object.remove(*field);
    }
}

fn remove_status_schedule_fields(object: &mut serde_json::Map<String, Value>) {
    if let Some(status) = object.get_mut("status").and_then(Value::as_object_mut) {
        remove_object_fields(status, &["start_beat", "stop_beat"]);
    }
}

fn normalize_group_path(
    object: &mut serde_json::Map<String, Value>,
    group_names: &HashMap<String, String>,
) {
    let Some(group_path) = object.get("group_path").and_then(Value::as_str) else {
        return;
    };
    if let Some(group_name) = group_names.get(group_path) {
        object.insert("group_path".into(), Value::String(group_name.clone()));
    }
}

async fn validate_and_normalize_v2_request(
    state: &AppState,
    method: &Method,
    path: &str,
    body: &mut Option<Value>,
) -> Result<(), HttpRequestProblem> {
    let operation_path = path.strip_prefix("/v2").unwrap_or(path);
    let segments = operation_path
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    match (method, segments.as_slice()) {
        (&Method::PATCH, ["transport"]) => {
            let request: V2TransportUpdate = decode_v2_body(body, operation_path)?;
            if request.quantization_beats.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "quantization_beats",
                    "quantized transport mutation has no scheduler contract",
                    vec!["omit for immediate mutation".into()],
                ));
            }
            let selected =
                usize::from(request.bpm.is_some()) + usize::from(request.time_signature.is_some());
            if selected != 1 {
                return Err(HttpRequestProblem {
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    code: "unsupported_combination".into(),
                    message: "PATCH /transport accepts exactly one operation field per revision"
                        .into(),
                    field: None,
                    reason: Some("mutation.single_operation".into()),
                    supported_values: vec!["bpm".into(), "time_signature".into()],
                });
            }
            if request
                .bpm
                .is_some_and(|bpm| !bpm.is_finite() || !(1.0..=999.0).contains(&bpm))
            {
                return Err(invalid_value("bpm", "must be finite and in 1..=999"));
            }
            if request.time_signature.is_some_and(|time_signature| {
                !(1..=32).contains(&time_signature.numerator)
                    || !(1..=32).contains(&time_signature.denominator)
                    || !time_signature.denominator.is_power_of_two()
            }) {
                return Err(invalid_value(
                    "time_signature",
                    "numerator must be 1..=32 and denominator a power of two in 1..=32",
                ));
            }
        }
        (&Method::POST, ["transport", "seek"]) => {
            let request: V2SeekRequest = decode_v2_body(body, operation_path)?;
            if !request.beat.is_finite() || request.beat < 0.0 {
                return Err(invalid_value("beat", "must be finite and non-negative"));
            }
        }
        (&Method::PATCH, ["groups", _]) => {
            let request: V2GroupUpdate = decode_v2_body(body, operation_path)?;
            require_exactly_one_param(&request.params)?;
        }
        (&Method::PUT, [family, _, "params", _])
            if matches!(*family, "groups" | "voices" | "patterns" | "effects") =>
        {
            let request: V2ParamSet = decode_v2_body(body, operation_path)?;
            if !request.value.is_finite() {
                return Err(invalid_value("value", "must be finite"));
            }
            if request
                .fade_beats
                .is_some_and(|duration| !duration.is_finite() || duration <= 0.0)
            {
                return Err(invalid_value(
                    "fade_beats",
                    "must be finite and greater than zero",
                ));
            }
        }
        (&Method::POST, ["voices"]) => {
            let request: V2VoiceCreate = decode_v2_body(body, operation_path)?;
            if request.name.as_deref().is_some_and(str::is_empty) {
                return Err(invalid_value("name", "must not be empty"));
            }
            if request.synth_name.as_deref().is_some_and(str::is_empty) {
                return Err(invalid_value("synth_name", "must not be empty"));
            }
            if request
                .polyphony
                .is_some_and(|polyphony| !(1..=128).contains(&polyphony))
            {
                return Err(invalid_value("polyphony", "must be in 1..=128"));
            }
            if request.params.values().any(|value| !value.is_finite()) {
                return Err(invalid_value("params", "values must be finite"));
            }
            if request.sample.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "sample",
                    "voice-source creation is not available through HTTP v2",
                    vec!["POST /v2/samples".into()],
                ));
            }
            if request.sfz.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "sfz",
                    "voice-source creation is not available through HTTP v2",
                    Vec::new(),
                ));
            }
            normalize_voice_gain(body, request.gain, request.params.get("amp").copied())?;
            normalize_voice_group(state, body, request.group_path.as_deref()).await?;
        }
        (&Method::PATCH, ["voices", _]) => {
            let request: V2VoiceUpdate = decode_v2_body(body, operation_path)?;
            if request.synth_name.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "synth_name",
                    "live synth replacement requires explicit node-recreation semantics",
                    Vec::new(),
                ));
            }
            if request.polyphony.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "polyphony",
                    "live polyphony replacement requires explicit node-recreation semantics",
                    Vec::new(),
                ));
            }
            normalize_voice_gain(body, request.gain, request.params.get("amp").copied())?;
            let normalized: V2VoiceUpdate = decode_v2_body(body, operation_path)?;
            require_exactly_one_param(&normalized.params)?;
        }
        (&Method::POST, ["voices", _, "trigger"]) => {
            let request: V2TriggerRequest = decode_optional_v2_body(body, operation_path)?;
            if request.params.values().any(|value| !value.is_finite()) {
                return Err(invalid_value("params", "values must be finite"));
            }
        }
        (&Method::POST, ["voices", _, "note-on"]) => {
            let request: V2NoteOnRequest = decode_v2_body(body, operation_path)?;
            if request.note > 127 {
                return Err(invalid_value("note", "must be in 0..=127"));
            }
            if !request.velocity.is_finite() || !(0.0..=1.0).contains(&request.velocity) {
                return Err(invalid_value(
                    "velocity",
                    "must use normalized velocity in 0.0..=1.0",
                ));
            }
        }
        (&Method::POST, ["voices", _, "note-off"]) => {
            let request: V2NoteOffRequest = decode_v2_body(body, operation_path)?;
            if request.note > 127 {
                return Err(invalid_value("note", "must be in 0..=127"));
            }
        }
        (&Method::POST, ["patterns"]) => {
            validate_v2_pattern_create(body, operation_path)?;
        }
        (&Method::PATCH, ["patterns", _]) => {
            let request: V2PatternUpdate = decode_v2_body(body, operation_path)?;
            if request.events.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "events",
                    "HTTP content replacement is not yet correlated to a musical swap boundary",
                    vec!["replace through a v2 Candidate".into()],
                ));
            }
            if request.pattern_string.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "pattern_string",
                    "HTTP content replacement is not yet correlated to a musical swap boundary",
                    vec!["replace through a v2 Candidate".into()],
                ));
            }
            if request.loop_beats.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "loop_beats",
                    "HTTP content replacement is not yet correlated to a musical swap boundary",
                    vec!["replace through a v2 Candidate".into()],
                ));
            }
            require_exactly_one_param(&request.params)?;
        }
        (&Method::POST, ["patterns", _, action]) if matches!(*action, "start" | "stop") => {
            let request: V2LoopControlRequest = decode_optional_v2_body(body, operation_path)?;
            if request.quantize_beats.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "quantize_beats",
                    "pattern scheduling does not expose an HTTP quantization boundary",
                    vec!["omit for immediate control".into()],
                ));
            }
        }
        (&Method::POST, ["melodies"]) => {
            let request: V2MelodyCreate = decode_v2_body(body, operation_path)?;
            if request.name.is_empty() {
                return Err(invalid_value("name", "must not be empty"));
            }
            if request.voice_name.is_empty() {
                return Err(invalid_value("voice_name", "must not be empty"));
            }
            validate_loop_length(request.loop_beats)?;
            if !request.params.is_empty() {
                return Err(HttpRequestProblem::unsupported(
                    "params",
                    "top-level Melody parameter precedence is not defined",
                    vec!["events[].params".into()],
                ));
            }
            if request.melody_string.is_some() && !request.events.is_empty() {
                return Err(representation_conflict("events", "melody_string"));
            }
            validate_melody_events(&request.events, request.loop_beats)?;
            if let Some(melody) = request.melody_string {
                let (events, length) = parse_melody_string(&melody)?;
                let explicit_loop_beats = body
                    .as_ref()
                    .and_then(Value::as_object)
                    .is_some_and(|object| object.contains_key("loop_beats"));
                let normalized_loop_beats = if explicit_loop_beats {
                    request.loop_beats
                } else {
                    length
                };
                validate_melody_events(&events, normalized_loop_beats)?;
                let value = body_object_mut(body)?;
                value.insert(
                    "events".into(),
                    serde_json::to_value(events).unwrap_or_default(),
                );
                if !explicit_loop_beats {
                    value.insert("loop_beats".into(), Value::from(length));
                }
            }
        }
        (&Method::PATCH, ["melodies", _]) => {
            let request: V2MelodyUpdate = decode_v2_body(body, operation_path)?;
            let field = if request.events.is_some() {
                Some("events")
            } else if request.melody_string.is_some() {
                Some("melody_string")
            } else if request.lanes.is_some() {
                Some("lanes")
            } else if request.loop_beats.is_some() {
                Some("loop_beats")
            } else if !request.params.is_empty() {
                Some("params")
            } else {
                None
            };
            return Err(match field {
                Some(field) => HttpRequestProblem::unsupported(
                    field,
                    "HTTP Melody replacement is not yet correlated to a musical swap boundary",
                    vec!["replace through a v2 Candidate".into()],
                ),
                None => HttpRequestProblem {
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    code: "no_effect".into(),
                    message: "the request does not select a Melody mutation".into(),
                    field: None,
                    reason: Some("mutation.no_effect".into()),
                    supported_values: Vec::new(),
                },
            });
        }
        (&Method::POST, ["melodies", _, action]) if matches!(*action, "start" | "stop") => {
            let request: V2LoopControlRequest = decode_optional_v2_body(body, operation_path)?;
            if request.quantize_beats.is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "quantize_beats",
                    "melody scheduling does not expose an HTTP quantization boundary",
                    vec!["omit for immediate control".into()],
                ));
            }
        }
        (&Method::POST, ["sequences"]) => {
            let request: V2SequenceCreate = decode_v2_body(body, operation_path)?;
            if request.name.is_empty() {
                return Err(invalid_value("name", "must not be empty"));
            }
            validate_loop_length(request.loop_beats)?;
            normalize_sequence_clips(state, body, &request.clips, request.loop_beats).await?;
        }
        (&Method::PATCH, ["sequences", _]) => {
            let request: V2SequenceUpdate = decode_v2_body(body, operation_path)?;
            let field = if request.clips.is_some() {
                Some("clips")
            } else if request.loop_beats.is_some() {
                Some("loop_beats")
            } else {
                None
            };
            return Err(match field {
                Some(field) => HttpRequestProblem::unsupported(
                    field,
                    "HTTP Sequence replacement is not yet available as one atomic runtime command",
                    vec!["replace through a v2 Candidate".into()],
                ),
                None => HttpRequestProblem {
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    code: "no_effect".into(),
                    message: "the request does not select a Sequence mutation".into(),
                    field: None,
                    reason: Some("mutation.no_effect".into()),
                    supported_values: Vec::new(),
                },
            });
        }
        (&Method::POST, ["sequences", _, "start"]) => {
            let _: V2SequenceStartRequest = decode_optional_v2_body(body, operation_path)?;
        }
        (&Method::PATCH, ["effects", _]) => {
            let request: V2EffectUpdate = decode_v2_body(body, operation_path)?;
            require_exactly_one_param(&request.params)?;
        }
        (&Method::POST, ["samples"]) => {
            let request: V2SampleLoad = decode_v2_body(body, operation_path)?;
            if request.path.is_empty() {
                return Err(invalid_value("path", "must not be empty"));
            }
            if let Some(id) = request.id {
                id.parse::<u32>()
                    .map_err(|_| invalid_value("id", "must be a decimal u32"))?;
            }
            if state.security.mode.remote() {
                return Err(HttpRequestProblem {
                    status: StatusCode::FORBIDDEN,
                    code: "capability_unavailable".into(),
                    message: "remote filesystem sample loading is disabled by HTTP policy".into(),
                    field: Some("path".into()),
                    reason: Some("capability.extension.filesystem".into()),
                    supported_values: Vec::new(),
                });
            }
        }
        (&Method::POST, ["fades"]) => {
            let request: V2FadeCreate = decode_v2_body(body, operation_path)?;
            if request.param_name.is_empty() {
                return Err(invalid_value("param_name", "must not be empty"));
            }
            if !request.target_value.is_finite()
                || request.start_value.is_some_and(|value| !value.is_finite())
            {
                return Err(invalid_value(
                    "start_value/target_value",
                    "must contain finite values",
                ));
            }
            validate_fade_duration(request.duration_beats)?;
            normalize_fade_target(state, body, &request.target_type, &request.target_name).await?;
        }
        (&Method::DELETE, ["fades"]) => {
            validate_object_fields(
                required_body(body, operation_path)?,
                &["target_type", "target_name", "param"],
            )?;
            normalize_named_fade_target(state, body).await?;
        }
        (&Method::POST, ["fades", family, _])
            if matches!(*family, "voice" | "group" | "effect" | "pattern" | "melody") =>
        {
            let value = required_body(body, operation_path)?;
            validate_object_fields(value, &["param", "to", "duration_beats", "from", "curve"])?;
            if value
                .get("param")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(invalid_value("param", "must be a non-empty string"));
            }
            let target = value
                .get("to")
                .and_then(Value::as_f64)
                .ok_or_else(|| invalid_value("to", "must be a number"))?;
            if !target.is_finite()
                || value
                    .get("from")
                    .is_some_and(|from| from.as_f64().is_none_or(|from| !from.is_finite()))
            {
                return Err(invalid_value("to/from", "must contain finite values"));
            }
            let duration = value
                .get("duration_beats")
                .and_then(Value::as_f64)
                .ok_or_else(|| invalid_value("duration_beats", "must be a number"))?;
            validate_fade_duration(duration)?;
            if let Some(curve) = value.get("curve") {
                validate_curve(curve)?;
            }
        }
        (&Method::POST, ["eval"]) => {
            validate_object_fields(required_body(body, operation_path)?, &["code"])?;
        }
        (&Method::POST, ["midi", "note", "off"]) => {
            let value = required_body(body, operation_path)?;
            validate_object_fields(value, &["device_id", "channel", "note", "velocity"])?;
            if value.get("velocity").is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "velocity",
                    "MIDI note-off velocity is not implemented by the runtime message",
                    Vec::new(),
                ));
            }
            validate_integer_field(value, "device_id", 0, i64::from(u32::MAX), true)?;
            validate_integer_field(value, "note", 0, 127, true)?;
            normalize_midi_channel(body, true)?;
        }
        (&Method::POST, ["midi", "record", "stop"]) => {
            let value = required_body(body, operation_path)?;
            validate_object_fields(value, &["device_id", "quantize"])?;
            if value.get("quantize").is_some() {
                return Err(HttpRequestProblem::unsupported(
                    "quantize",
                    "MIDI recording stop quantization is not implemented",
                    vec!["omit for immediate stop".into()],
                ));
            }
            validate_integer_field(value, "device_id", 0, i64::from(u32::MAX), true)?;
        }
        (&Method::POST, ["midi", "note", "on"]) => {
            let value = required_body(body, operation_path)?;
            validate_object_fields(value, &["device_id", "channel", "note", "velocity"])?;
            validate_integer_field(value, "device_id", 0, i64::from(u32::MAX), true)?;
            validate_integer_field(value, "note", 0, 127, true)?;
            validate_integer_field(value, "velocity", 0, 127, false)?;
            normalize_midi_channel(body, true)?;
        }
        (&Method::POST, ["midi", "cc"]) => {
            let value = required_body(body, operation_path)?;
            validate_object_fields(value, &["device_id", "channel", "cc", "value"])?;
            validate_integer_field(value, "device_id", 0, i64::from(u32::MAX), true)?;
            validate_integer_field(value, "cc", 0, 127, true)?;
            validate_integer_field(value, "value", 0, 127, true)?;
            normalize_midi_channel(body, true)?;
        }
        (&Method::POST, ["midi", "record", "start"]) => {
            let value = required_body(body, operation_path)?;
            validate_object_fields(value, &["device_id", "channel"])?;
            validate_integer_field(value, "device_id", 0, i64::from(u32::MAX), true)?;
            normalize_midi_channel(body, false)?;
        }
        (&Method::POST, ["midi", "route", "keyboard"]) => {
            let value = required_body(body, operation_path)?;
            validate_object_fields(
                value,
                &[
                    "device_id",
                    "voice_id",
                    "channel",
                    "note_min",
                    "note_max",
                    "transpose",
                ],
            )?;
            validate_integer_field(value, "device_id", 0, i64::from(u32::MAX), true)?;
            validate_integer_field(value, "voice_id", 0, i64::from(u32::MAX), true)?;
            let note_min = validate_integer_field(value, "note_min", 0, 127, false)?;
            let note_max = validate_integer_field(value, "note_max", 0, 127, false)?;
            validate_integer_field(
                value,
                "transpose",
                i64::from(i8::MIN),
                i64::from(i8::MAX),
                false,
            )?;
            if note_min.zip(note_max).is_some_and(|(min, max)| min > max) {
                return Err(invalid_value(
                    "note_min/note_max",
                    "must be ordered from low to high",
                ));
            }
            normalize_midi_channel(body, false)?;
        }
        (&Method::POST, ["midi", category, action])
            if matches!(*category, "input" | "output" | "clock" | "transport")
                && matches!(
                    *action,
                    "open" | "enable" | "disable" | "start" | "stop" | "continue"
                ) =>
        {
            let value = required_body(body, operation_path)?;
            validate_object_fields(value, &["device_id"])?;
            validate_integer_field(value, "device_id", 0, i64::from(u32::MAX), true)?;
        }
        (&Method::POST, ["midi", "close"]) => {
            let value = required_body(body, operation_path)?;
            validate_object_fields(value, &["device_id"])?;
            validate_integer_field(value, "device_id", 0, i64::from(u32::MAX), true)?;
        }
        (&Method::GET, ["midi", "routes"]) => {
            return Err(HttpRequestProblem {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "capability_unavailable".into(),
                message: "MIDI route observation is not available from the runtime".into(),
                field: None,
                reason: Some("capability.midi.route_observation".into()),
                supported_values: Vec::new(),
            });
        }
        _ if method == Method::POST || method == Method::PATCH || method == Method::PUT => {
            if let Some(value) = body.as_ref() {
                validate_object_fields(value, &[])?;
            }
        }
        _ if body.is_some() => {
            return Err(HttpRequestProblem::unsupported(
                "request_body",
                "this operation does not define a request body",
                Vec::new(),
            ));
        }
        _ => {}
    }
    Ok(())
}

fn decode_v2_body<T: serde::de::DeserializeOwned>(
    body: &Option<Value>,
    operation: &str,
) -> Result<T, HttpRequestProblem> {
    let value = required_body(body, operation)?;
    serde_json::from_value(value.clone()).map_err(|error| {
        HttpRequestProblem::bad_request(
            "invalid_request",
            format!("{operation} request does not match its operation-scoped DTO: {error}"),
            serde_field_from_error(&error.to_string()).as_deref(),
        )
    })
}

fn decode_optional_v2_body<T: serde::de::DeserializeOwned>(
    body: &mut Option<Value>,
    operation: &str,
) -> Result<T, HttpRequestProblem> {
    // Normalize an absent body to `{}` so the declared-optional body is also
    // optional at the legacy handler boundary, whose Json extractor would
    // otherwise reject the empty payload with an untyped envelope.
    if body.is_none() {
        *body = Some(json!({}));
    }
    decode_v2_body(body, operation)
}

fn required_body<'a>(
    body: &'a Option<Value>,
    operation: &str,
) -> Result<&'a Value, HttpRequestProblem> {
    body.as_ref().ok_or_else(|| {
        HttpRequestProblem::bad_request(
            "request_body_required",
            format!("{operation} requires a JSON request body"),
            None,
        )
    })
}

fn body_object_mut(
    body: &mut Option<Value>,
) -> Result<&mut serde_json::Map<String, Value>, HttpRequestProblem> {
    body.as_mut().and_then(Value::as_object_mut).ok_or_else(|| {
        HttpRequestProblem::bad_request(
            "invalid_request",
            "request body must be a JSON object",
            None,
        )
    })
}

fn serde_field_from_error(message: &str) -> Option<String> {
    message
        .strip_prefix("unknown field `")
        .and_then(|rest| rest.split_once('`'))
        .map(|(field, _)| field.to_string())
}

fn invalid_value(field: &str, requirement: &str) -> HttpRequestProblem {
    HttpRequestProblem {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "invalid_value".into(),
        message: format!("field `{field}` {requirement}"),
        field: Some(field.into()),
        reason: Some("validation.range".into()),
        supported_values: Vec::new(),
    }
}

fn representation_conflict(left: &str, right: &str) -> HttpRequestProblem {
    HttpRequestProblem {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "representation_conflict".into(),
        message: format!("fields `{left}` and `{right}` are mutually exclusive"),
        field: None,
        reason: Some("content.exclusive_representation".into()),
        supported_values: vec![left.into(), right.into()],
    }
}

fn require_exactly_one_param(params: &HashMap<String, f32>) -> Result<(), HttpRequestProblem> {
    if params.values().any(|value| !value.is_finite()) {
        return Err(invalid_value("params", "values must be finite"));
    }
    if params.is_empty() {
        return Err(HttpRequestProblem {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "no_effect".into(),
            message: "the request does not select a parameter mutation".into(),
            field: Some("params".into()),
            reason: Some("mutation.no_effect".into()),
            supported_values: vec!["exactly one parameter per request".into()],
        });
    }
    if params.len() > 1 {
        return Err(HttpRequestProblem {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "unsupported_combination".into(),
            message: "this runtime boundary cannot apply multiple parameters as one revision"
                .into(),
            field: Some("params".into()),
            reason: Some("mutation.atomic_parameter_set_unavailable".into()),
            supported_values: vec!["one parameter per request".into()],
        });
    }
    Ok(())
}

fn validate_loop_length(length: f64) -> Result<(), HttpRequestProblem> {
    if !length.is_finite() || length <= 0.0 {
        return Err(invalid_value(
            "loop_beats",
            "must be finite and greater than zero",
        ));
    }
    Ok(())
}

fn validate_swing(swing: f32) -> Result<(), HttpRequestProblem> {
    if !swing.is_finite() || !(0.0..=1.0).contains(&swing) {
        return Err(invalid_value("swing", "must be in 0.0..=1.0"));
    }
    Ok(())
}

fn validate_v2_pattern_create(
    body: &mut Option<Value>,
    operation_path: &str,
) -> Result<(), HttpRequestProblem> {
    let request: V2PatternCreate = decode_v2_body(body, operation_path)?;
    if request.name.is_empty() {
        return Err(invalid_value("name", "must not be empty"));
    }
    if request.voice_name.is_empty() {
        return Err(invalid_value("voice_name", "must not be empty"));
    }
    if request.params.values().any(|value| !value.is_finite()) {
        return Err(invalid_value("params", "values must be finite"));
    }
    validate_loop_length(request.loop_beats)?;
    validate_swing(request.swing)?;
    // Swing is only consumed by the pattern_string grid, where it is baked
    // into event beats below; the runtime scheduler never reads the stored
    // swing value, so accepting it alongside explicit events would be an
    // accepted-but-inert field.
    let swing_supplied = body
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|object| object.contains_key("swing"));
    if swing_supplied && request.pattern_string.is_none() {
        return Err(HttpRequestProblem::unsupported(
            "swing",
            "swing is applied only when authoring through pattern_string; supply pre-swung event beats instead",
            vec!["pattern_string".into()],
        ));
    }
    validate_pattern_events(&request.events, request.loop_beats)?;
    if request.pattern_string.is_some() && !request.events.is_empty() {
        return Err(representation_conflict("events", "pattern_string"));
    }
    if let Some(pattern) = request.pattern_string {
        let events = parse_pattern_string(&pattern, request.swing)?;
        let inferred_loop_beats = pattern_bar_count(&pattern) as f64 * 4.0;
        let explicit_loop_beats = body
            .as_ref()
            .and_then(Value::as_object)
            .is_some_and(|object| object.contains_key("loop_beats"));
        let normalized_loop_beats = if explicit_loop_beats {
            request.loop_beats
        } else {
            inferred_loop_beats
        };
        validate_pattern_events(&events, normalized_loop_beats)?;
        let value = body_object_mut(body)?;
        value.insert(
            "events".into(),
            serde_json::to_value(events).unwrap_or_default(),
        );
        // The swing timing now lives in the baked beats; zero the stored
        // field so the runtime state does not carry it a second time.
        value.insert("swing".into(), Value::from(0.0));
        if !explicit_loop_beats {
            value.insert("loop_beats".into(), Value::from(inferred_loop_beats));
        }
    }
    Ok(())
}

fn validate_pattern_events(
    events: &[V2PatternEvent],
    length: f64,
) -> Result<(), HttpRequestProblem> {
    for (index, event) in events.iter().enumerate() {
        if !event.beat.is_finite() || event.beat < 0.0 || event.beat >= length {
            return Err(invalid_value(
                &format!("events/{index}/beat"),
                "must be finite and inside the loop",
            ));
        }
        if event
            .params
            .as_ref()
            .is_some_and(|params| params.values().any(|value| !value.is_finite()))
        {
            return Err(invalid_value(
                &format!("events/{index}/params"),
                "values must be finite",
            ));
        }
    }
    Ok(())
}

fn pattern_bar_count(pattern: &str) -> usize {
    pattern.split('|').count()
}

fn parse_pattern_string(
    pattern: &str,
    swing: f32,
) -> Result<Vec<V2PatternEvent>, HttpRequestProblem> {
    let bars = pattern.split('|').map(str::trim).collect::<Vec<_>>();
    if bars.is_empty() || bars.iter().any(|bar| bar.is_empty()) {
        return Err(invalid_value(
            "pattern_string",
            "must not be empty or contain an empty bar",
        ));
    }
    let mut events = Vec::new();
    let mut step_index = 0usize;
    let mut position = 0usize;
    for (bar_index, bar) in bars.into_iter().enumerate() {
        let tokens = bar
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            return Err(invalid_value(
                "pattern_string",
                "must not contain an empty bar",
            ));
        }
        let step = 4.0 / tokens.len() as f64;
        for (index, token) in tokens.into_iter().enumerate() {
            let beat = bar_index as f64 * 4.0 + index as f64 * step;
            let beat = if step_index % 2 == 1 {
                beat + f64::from(swing) * step * 0.5
            } else {
                beat
            };
            let velocity = match token {
                'x' => Some(0.7),
                'X' => Some(1.0),
                'o' | 'O' => Some(0.3),
                '1'..='9' => Some(f32::from(token as u8 - b'0') / 9.0),
                '.' | '_' | '0' | '-' => None,
                _ => {
                    return Err(HttpRequestProblem {
                        status: StatusCode::UNPROCESSABLE_ENTITY,
                        code: "parse_error".into(),
                        message: format!("invalid pattern token `{token}` at position {position}"),
                        field: Some("pattern_string".into()),
                        reason: Some("parser.pattern.v2".into()),
                        supported_values: vec!["x X o O 1-9 . _ 0 - |".into()],
                    });
                }
            };
            if let Some(velocity) = velocity {
                events.push(V2PatternEvent {
                    beat,
                    params: Some(HashMap::from([("amp".into(), velocity)])),
                });
            }
            step_index += 1;
            position += 1;
        }
        position += 1;
    }
    Ok(events)
}

fn validate_melody_events(events: &[V2MelodyEvent], length: f64) -> Result<(), HttpRequestProblem> {
    for (index, event) in events.iter().enumerate() {
        if !event.beat.is_finite() || event.beat < 0.0 || event.beat >= length {
            return Err(invalid_value(
                &format!("events/{index}/beat"),
                "must be finite and inside the loop",
            ));
        }
        if event.frequency.is_some() {
            return Err(HttpRequestProblem::unsupported(
                &format!("events/{index}/frequency"),
                "note/frequency precedence is not defined",
                vec!["note".into()],
            ));
        }
        parse_midi_note(&event.note).map_err(|message| HttpRequestProblem {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "parse_error".into(),
            message,
            field: Some(format!("events/{index}/note")),
            reason: Some("parser.note_name.strict".into()),
            supported_values: Vec::new(),
        })?;
        if event
            .duration
            .is_some_and(|duration| !duration.is_finite() || duration <= 0.0)
        {
            return Err(invalid_value(
                &format!("events/{index}/duration"),
                "must be finite and greater than zero",
            ));
        }
        if event
            .velocity
            .is_some_and(|velocity| !velocity.is_finite() || !(0.0..=1.0).contains(&velocity))
        {
            return Err(invalid_value(
                &format!("events/{index}/velocity"),
                "must use normalized velocity in 0.0..=1.0",
            ));
        }
        if event
            .params
            .as_ref()
            .is_some_and(|params| params.values().any(|value| !value.is_finite()))
        {
            return Err(invalid_value(
                &format!("events/{index}/params"),
                "values must be finite",
            ));
        }
    }
    Ok(())
}

fn parse_midi_note(note: &str) -> Result<u8, String> {
    let parsed = note.parse::<u8>().or_else(|_| {
        vibelang_dsp::notes::parse_note_name_strict(note)
            .map_err(|error| format!("invalid note `{note}`: {error}"))
    })?;
    (parsed <= 127)
        .then_some(parsed)
        .ok_or_else(|| format!("invalid note `{note}`: MIDI notes must be in 0..=127"))
}

fn parse_melody_string(melody: &str) -> Result<(Vec<V2MelodyEvent>, f64), HttpRequestProblem> {
    if melody.trim().is_empty() {
        return Err(invalid_value("melody_string", "must not be empty"));
    }
    let bars = melody.split('|').collect::<Vec<_>>();
    let mut events = Vec::<V2MelodyEvent>::new();
    for (bar_index, bar) in bars.iter().enumerate() {
        let tokens = bar.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            return Err(invalid_value(
                "melody_string",
                "must not contain an empty bar",
            ));
        }
        let step = 4.0 / tokens.len() as f64;
        for (token_index, token) in tokens.into_iter().enumerate() {
            let beat = bar_index as f64 * 4.0 + token_index as f64 * step;
            match token {
                "." | "_" => {}
                "-" => {
                    let previous = events.last_mut().ok_or_else(|| HttpRequestProblem {
                        status: StatusCode::UNPROCESSABLE_ENTITY,
                        code: "parse_error".into(),
                        message: "melody tie requires a preceding note".into(),
                        field: Some("melody_string".into()),
                        reason: Some("parser.melody.v2".into()),
                        supported_values: Vec::new(),
                    })?;
                    previous.duration = Some(previous.duration.unwrap_or(step) + step);
                }
                note => {
                    let midi = parse_midi_note(note).map_err(|message| HttpRequestProblem {
                        status: StatusCode::UNPROCESSABLE_ENTITY,
                        code: "parse_error".into(),
                        message,
                        field: Some("melody_string".into()),
                        reason: Some("parser.melody.v2".into()),
                        supported_values: vec!["notes, rests (`.`/`_`), and ties (`-`)".into()],
                    })?;
                    events.push(V2MelodyEvent {
                        beat,
                        note: midi.to_string(),
                        frequency: None,
                        duration: Some(step),
                        velocity: Some(0.8),
                        params: None,
                    });
                }
            }
        }
    }
    Ok((events, bars.len() as f64 * 4.0))
}

fn validate_fade_duration(duration: f64) -> Result<(), HttpRequestProblem> {
    if !duration.is_finite() || duration <= 0.0 {
        return Err(invalid_value(
            "duration_beats",
            "must be finite and greater than zero",
        ));
    }
    Ok(())
}

fn validate_curve(curve: &Value) -> Result<(), HttpRequestProblem> {
    const NAMES: &[&str] = &[
        "linear",
        "ease_in",
        "ease_out",
        "ease_in_out",
        "sine_in",
        "sine_out",
        "sine_in_out",
        "cubic_in",
        "cubic_out",
        "cubic_in_out",
        "exponential",
        "logarithmic",
        "step",
    ];
    if let Some(name) = curve.as_str() {
        if NAMES.contains(&name) {
            return Ok(());
        }
        return Err(HttpRequestProblem {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "unsupported_value".into(),
            message: format!("unknown fade curve `{name}`"),
            field: Some("curve".into()),
            reason: Some("fade.curve".into()),
            supported_values: NAMES.iter().map(|name| (*name).into()).collect(),
        });
    }
    let object = curve.as_object().ok_or_else(|| {
        invalid_value(
            "curve",
            "must be a supported name, {\"exp\": number}, or {\"spline\": [[t,value],...]}",
        )
    })?;
    validate_object_fields(curve, &["exp", "spline"])?;
    if let Some(exponent) = object.get("exp") {
        let exponent = exponent
            .as_f64()
            .ok_or_else(|| invalid_value("curve.exp", "must be a number"))?;
        if !exponent.is_finite() || exponent <= 0.0 {
            return Err(invalid_value(
                "curve.exp",
                "must be finite and greater than zero",
            ));
        }
        if object.len() != 1 {
            return Err(representation_conflict("curve.exp", "curve.spline"));
        }
        return Ok(());
    }
    if let Some(points) = object.get("spline").and_then(Value::as_array) {
        if object.len() != 1 || points.len() < 2 || points.len() > 64 {
            return Err(invalid_value(
                "curve.spline",
                "must contain 2..=64 control points and no exp member",
            ));
        }
        let mut previous = -1.0;
        for (index, point) in points.iter().enumerate() {
            let point = point.as_array().ok_or_else(|| {
                invalid_value(&format!("curve.spline/{index}"), "must be [time, value]")
            })?;
            if point.len() != 2 {
                return Err(invalid_value(
                    &format!("curve.spline/{index}"),
                    "must be [time, value]",
                ));
            }
            let time = point[0].as_f64().ok_or_else(|| {
                invalid_value(&format!("curve.spline/{index}/0"), "must be a number")
            })?;
            let value = point[1].as_f64().ok_or_else(|| {
                invalid_value(&format!("curve.spline/{index}/1"), "must be a number")
            })?;
            if !time.is_finite()
                || !value.is_finite()
                || !(0.0..=1.0).contains(&time)
                || time <= previous
            {
                return Err(invalid_value(
                    &format!("curve.spline/{index}"),
                    "must contain finite values with strictly increasing time in 0.0..=1.0",
                ));
            }
            previous = time;
        }
        return Ok(());
    }
    Err(invalid_value(
        "curve",
        "must contain exactly one of exp or spline",
    ))
}

fn validate_object_fields(value: &Value, allowed: &[&str]) -> Result<(), HttpRequestProblem> {
    let object = value.as_object().ok_or_else(|| {
        HttpRequestProblem::bad_request(
            "invalid_request",
            "request body must be a JSON object",
            None,
        )
    })?;
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(HttpRequestProblem {
            status: StatusCode::BAD_REQUEST,
            code: "unknown_field".into(),
            message: format!("field `{field}` is not part of this operation-scoped DTO"),
            field: Some(field.clone()),
            reason: Some("schema.operation_scoped".into()),
            supported_values: allowed.iter().map(|field| (*field).into()).collect(),
        });
    }
    Ok(())
}

fn validate_integer_field(
    value: &Value,
    field: &str,
    minimum: i64,
    maximum: i64,
    required: bool,
) -> Result<Option<i64>, HttpRequestProblem> {
    let object = value.as_object().ok_or_else(|| {
        HttpRequestProblem::bad_request(
            "invalid_request",
            "request body must be a JSON object",
            None,
        )
    })?;
    let Some(value) = object.get(field) else {
        return if required {
            Err(invalid_value(field, "is required"))
        } else {
            Ok(None)
        };
    };
    let integer = value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .ok_or_else(|| invalid_value(field, "must be an integer"))?;
    if !(minimum..=maximum).contains(&integer) {
        return Err(invalid_value(
            field,
            &format!("must be in {minimum}..={maximum}"),
        ));
    }
    Ok(Some(integer))
}

fn normalize_midi_channel(
    body: &mut Option<Value>,
    required: bool,
) -> Result<(), HttpRequestProblem> {
    let value = required_body(body, "MIDI operation")?;
    let Some(channel) = validate_integer_field(value, "channel", 1, 16, required)? else {
        return Ok(());
    };
    body_object_mut(body)?.insert("channel".into(), Value::from(channel - 1));
    Ok(())
}

fn normalize_voice_gain(
    body: &mut Option<Value>,
    gain: Option<f32>,
    amp: Option<f32>,
) -> Result<(), HttpRequestProblem> {
    let Some(gain) = gain else {
        return Ok(());
    };
    if !gain.is_finite() || gain < 0.0 {
        return Err(invalid_value("gain", "must be finite and non-negative"));
    }
    let object = body_object_mut(body)?;
    let params = object
        .entry("params")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| invalid_value("params", "must be an object"))?;
    if amp.is_some_and(|amp| amp != gain) {
        return Err(representation_conflict("gain", "params.amp"));
    }
    params.insert("amp".into(), Value::from(gain));
    object.remove("gain");
    Ok(())
}

async fn normalize_voice_group(
    state: &AppState,
    body: &mut Option<Value>,
    group: Option<&str>,
) -> Result<(), HttpRequestProblem> {
    let Some(group) = group else {
        return Ok(());
    };
    let resolved = state
        .with_state(|runtime| {
            if let Ok(raw) = group.parse::<u32>() {
                let id = vibelang_core::GroupId::new(raw);
                return (runtime.groups.contains_key(&id) || raw == 0).then_some(raw);
            }
            let matches = runtime
                .groups
                .iter()
                .filter(|(_, candidate)| candidate.name == group)
                .map(|(id, _)| id.raw())
                .collect::<Vec<_>>();
            (matches.len() == 1).then_some(matches[0])
        })
        .await
        .ok_or_else(|| HttpRequestProblem {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "group_not_found".into(),
            message: format!("group `{group}` does not resolve uniquely"),
            field: Some("group_path".into()),
            reason: Some("identity.group".into()),
            supported_values: Vec::new(),
        })?;
    let object = body_object_mut(body)?;
    object.remove("group_id");
    object.insert("group_path".into(), Value::String(resolved.to_string()));
    Ok(())
}

async fn normalize_sequence_clips(
    state: &AppState,
    body: &mut Option<Value>,
    clips: &[V2SequenceClip],
    sequence_length: f64,
) -> Result<(), HttpRequestProblem> {
    if clips.is_empty() {
        return Ok(());
    }
    let names = state
        .with_state(|runtime| {
            (
                runtime.patterns.iter().fold(
                    HashMap::<String, Vec<u32>>::new(),
                    |mut names, (id, pattern)| {
                        names
                            .entry(pattern.content.name.clone())
                            .or_default()
                            .push(id.raw());
                        names
                    },
                ),
                runtime.melodies.iter().fold(
                    HashMap::<String, Vec<u32>>::new(),
                    |mut names, (id, melody)| {
                        names
                            .entry(melody.content.name.clone())
                            .or_default()
                            .push(id.raw());
                        names
                    },
                ),
                runtime.sequences.iter().fold(
                    HashMap::<String, Vec<u32>>::new(),
                    |mut names, (id, sequence)| {
                        names
                            .entry(sequence.config.name.clone())
                            .or_default()
                            .push(id.raw());
                        names
                    },
                ),
            )
        })
        .await;
    let value = body_object_mut(body)?;
    let wire_clips = value
        .get_mut("clips")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid_value("clips", "must be an array"))?;
    for (index, (clip, wire)) in clips.iter().zip(wire_clips.iter_mut()).enumerate() {
        if clip.once.is_some() {
            return Err(HttpRequestProblem::unsupported(
                &format!("clips/{index}/once"),
                "per-clip one-shot semantics are not implemented",
                Vec::new(),
            ));
        }
        if clip.end_beat.is_some() && clip.duration_beats.is_some() {
            return Err(HttpRequestProblem {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "representation_conflict".into(),
                message: format!(
                    "clip {index} must select exactly one of end_beat or duration_beats"
                ),
                field: Some(format!("clips/{index}")),
                reason: Some("content.exclusive_representation".into()),
                supported_values: vec!["end_beat".into(), "duration_beats".into()],
            });
        }
        if !clip.start_beat.is_finite()
            || clip.start_beat < 0.0
            || clip.start_beat >= sequence_length
        {
            return Err(invalid_value(
                &format!("clips/{index}/start_beat"),
                "must be finite, non-negative, and inside the sequence loop",
            ));
        }
        if clip
            .duration_beats
            .is_some_and(|duration| !duration.is_finite() || duration <= 0.0)
            || clip
                .end_beat
                .is_some_and(|end| !end.is_finite() || end <= clip.start_beat)
        {
            return Err(invalid_value(
                &format!("clips/{index}"),
                "end/duration must be finite and after the clip start",
            ));
        }
        let (kind, directory) = match clip.clip_type.to_ascii_lowercase().as_str() {
            "pattern" => ("pattern", &names.0),
            "melody" => ("melody", &names.1),
            "sequence" => {
                if clip.end_beat.is_some() || clip.duration_beats.is_some() {
                    let field = if clip.end_beat.is_some() {
                        "end_beat"
                    } else {
                        "duration_beats"
                    };
                    return Err(HttpRequestProblem::unsupported(
                        &format!("clips/{index}/{field}"),
                        "nested Sequence clips do not define an end or duration",
                        vec!["omit end_beat and duration_beats".into()],
                    ));
                }
                ("sequence", &names.2)
            }
            "fade" => {
                return Err(HttpRequestProblem::unsupported(
                    &format!("clips/{index}/type"),
                    "the legacy six-field clip shape cannot encode a Fade target and curve",
                    vec!["pattern".into(), "melody".into(), "sequence".into()],
                ));
            }
            other => {
                return Err(HttpRequestProblem {
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    code: "unsupported_value".into(),
                    message: format!("unsupported sequence clip type `{other}`"),
                    field: Some(format!("clips/{index}/type")),
                    reason: Some("sequence.clip_type".into()),
                    supported_values: vec!["pattern".into(), "melody".into(), "sequence".into()],
                });
            }
        };
        if matches!(kind, "pattern" | "melody") {
            let effective_end = clip
                .end_beat
                .or_else(|| {
                    clip.duration_beats
                        .map(|duration| clip.start_beat + duration)
                })
                .unwrap_or(clip.start_beat + 4.0);
            if effective_end > sequence_length {
                return Err(invalid_value(
                    &format!("clips/{index}"),
                    "must end inside the sequence loop",
                ));
            }
        }
        let numeric = clip.name.parse::<u32>().ok().filter(|id| {
            directory
                .values()
                .flatten()
                .any(|candidate| candidate == id)
        });
        let resolved = if let Some(id) = numeric {
            Some(id)
        } else {
            match directory.get(&clip.name).map(Vec::as_slice) {
                Some([id]) => Some(*id),
                Some(_) => {
                    return Err(HttpRequestProblem {
                        status: StatusCode::UNPROCESSABLE_ENTITY,
                        code: "dependency_ambiguous".into(),
                        message: format!(
                            "{kind} dependency `{}` does not resolve uniquely",
                            clip.name
                        ),
                        field: Some(format!("clips/{index}/name")),
                        reason: Some("sequence.dependency".into()),
                        supported_values: Vec::new(),
                    });
                }
                None => None,
            }
        }
        .ok_or_else(|| HttpRequestProblem {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "dependency_not_found".into(),
            message: format!("{kind} dependency `{}` does not exist", clip.name),
            field: Some(format!("clips/{index}/name")),
            reason: Some("sequence.dependency".into()),
            supported_values: Vec::new(),
        })?;
        if let Some(object) = wire.as_object_mut() {
            object.insert("type".into(), Value::String(kind.into()));
            object.insert("name".into(), Value::String(resolved.to_string()));
        }
    }
    Ok(())
}

async fn normalize_fade_target(
    state: &AppState,
    body: &mut Option<Value>,
    target_type: &FadeTargetType,
    target_name: &str,
) -> Result<(), HttpRequestProblem> {
    let resolved = resolve_fade_target(state, target_type, target_name).await?;
    body_object_mut(body)?.insert("target_name".into(), Value::String(resolved.to_string()));
    Ok(())
}

async fn normalize_named_fade_target(
    state: &AppState,
    body: &mut Option<Value>,
) -> Result<(), HttpRequestProblem> {
    let value = required_body(body, "DELETE /fades")?;
    let target_type: FadeTargetType = serde_json::from_value(
        value
            .get("target_type")
            .cloned()
            .ok_or_else(|| invalid_value("target_type", "is required"))?,
    )
    .map_err(|error| invalid_value("target_type", &error.to_string()))?;
    let target_name = value
        .get("target_name")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_value("target_name", "must be a string"))?
        .to_string();
    if value
        .get("param")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(invalid_value("param", "must be a non-empty string"));
    }
    normalize_fade_target(state, body, &target_type, &target_name).await
}

async fn resolve_fade_target(
    state: &AppState,
    target_type: &FadeTargetType,
    target_name: &str,
) -> Result<u32, HttpRequestProblem> {
    state
        .with_state(|runtime| {
            let numeric = target_name.parse::<u32>().ok();
            match target_type {
                FadeTargetType::Group => numeric
                    .filter(|raw| {
                        runtime
                            .groups
                            .contains_key(&vibelang_core::GroupId::new(*raw))
                    })
                    .or_else(|| {
                        runtime
                            .groups
                            .iter()
                            .find(|(_, group)| group.name == target_name)
                            .map(|(id, _)| id.raw())
                    }),
                FadeTargetType::Voice => numeric
                    .filter(|raw| {
                        runtime
                            .voices
                            .contains_key(&vibelang_core::VoiceId::new(*raw))
                    })
                    .or_else(|| {
                        runtime
                            .voices
                            .iter()
                            .find(|(_, voice)| voice.config.name == target_name)
                            .map(|(id, _)| id.raw())
                    }),
                FadeTargetType::Effect => numeric.filter(|raw| {
                    runtime
                        .effects
                        .contains_key(&vibelang_core::EffectId::new(*raw))
                }),
            }
        })
        .await
        .ok_or_else(|| HttpRequestProblem {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "fade_target_not_found".into(),
            message: format!("fade target `{target_name}` does not exist"),
            field: Some("target_name".into()),
            reason: Some("fade.target".into()),
            supported_values: Vec::new(),
        })
}

async fn project_response(
    mut response: Response,
    context: &HttpRequestContext,
    eval_response: bool,
) -> Response {
    let receipts = context.current_receipts();
    let Some(receipt) = canonical_http_receipt(&receipts).cloned() else {
        return response;
    };

    response
        .headers_mut()
        .insert(header::LOCATION, receipt_location(&receipt));
    response.headers_mut().insert(
        RUNTIME_EPOCH_HEADER,
        header_value(&receipt.runtime_epoch.to_string()),
    );
    response.headers_mut().insert(
        EVENT_SEQUENCE_HEADER,
        header_value(&receipt.event_sequence.to_string()),
    );
    if let Some(revision) = receipt.revision {
        response
            .headers_mut()
            .insert(REVISION_HEADER, header_value(&revision.to_string()));
    }
    if matches!(receipt.state, ReceiptState::Accepted { .. }) {
        response.headers_mut().insert(
            LEGACY_SUCCESS_SCOPE_HEADER,
            HeaderValue::from_static(if eval_response {
                "evaluation_only"
            } else {
                "queue_admitted"
            }),
        );
    }
    response = with_receipt_status(response, &receipt);

    if eval_response {
        return response;
    }

    let related_receipts = receipts
        .into_iter()
        .filter(|candidate| candidate.attempt_id != receipt.attempt_id)
        .collect();
    let (mut parts, body) = response.into_parts();
    let legacy_result = match to_bytes(body, 64 * 1024 * 1024).await {
        Ok(bytes) if bytes.is_empty() => None,
        Ok(bytes) => serde_json::from_slice(&bytes)
            .ok()
            .or_else(|| Some(Value::String(String::from_utf8_lossy(&bytes).into_owned()))),
        Err(error) => Some(json!({
            "error": "legacy_response_unavailable",
            "message": error.to_string(),
        })),
    };
    let carrier = MutationHttpResponse {
        receipt,
        related_receipts,
        legacy_result,
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let body = serde_json::to_vec(&carrier).unwrap_or_else(|error| {
        serde_json::to_vec(&json!({
            "error": "receipt_serialization_failed",
            "message": error.to_string(),
        }))
        .unwrap_or_default()
    });
    Response::from_parts(parts, Body::from(body))
}

fn canonical_http_receipt(receipts: &[MutationReceipt]) -> Option<&MutationReceipt> {
    receipts.iter().max_by_key(|receipt| {
        let precedence = match &receipt.state {
            ReceiptState::Terminal(TerminalOutcome::Partial(_)) => 4,
            ReceiptState::Terminal(TerminalOutcome::Rejected(_))
            | ReceiptState::Terminal(TerminalOutcome::Superseded(_)) => 3,
            ReceiptState::Evaluating { .. }
            | ReceiptState::Accepted { .. }
            | ReceiptState::Planning
            | ReceiptState::Staging { .. }
            | ReceiptState::Committing { .. } => 2,
            ReceiptState::Terminal(TerminalOutcome::Applied(_)) => 1,
        };
        (precedence, receipt.event_sequence)
    })
}

fn with_receipt_status(mut response: Response, receipt: &MutationReceipt) -> Response {
    let original = response.status();
    *response.status_mut() = match &receipt.state {
        ReceiptState::Evaluating { .. }
        | ReceiptState::Accepted { .. }
        | ReceiptState::Planning
        | ReceiptState::Staging { .. }
        | ReceiptState::Committing { .. } => StatusCode::ACCEPTED,
        ReceiptState::Terminal(TerminalOutcome::Applied(_)) => {
            if original.is_success() {
                original
            } else {
                StatusCode::OK
            }
        }
        ReceiptState::Terminal(TerminalOutcome::Superseded(_))
        | ReceiptState::Terminal(TerminalOutcome::Partial(_)) => StatusCode::CONFLICT,
        ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) => {
            rejected_http_status(&rejected.code)
        }
    };
    response
}

fn rejected_http_status(code: &str) -> StatusCode {
    if matches!(
        code,
        "runtime_fenced"
            | "revision_conflict"
            | "idempotency_conflict"
            | "idempotency_key_expired"
            | "runtime_epoch_changed"
    ) {
        StatusCode::CONFLICT
    } else if code == "idempotency_key_required" || code == "expected_revision_required" {
        StatusCode::PRECONDITION_REQUIRED
    } else if code.contains("capability")
        || code.starts_with("queue_")
        || code == "idempotency_capacity_exhausted"
        || code == "backend_unavailable"
    {
        StatusCode::SERVICE_UNAVAILABLE
    } else if code.ends_with("_not_found") || code == "receipt_not_found" {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::BAD_REQUEST
    }
}

fn receipt_location(receipt: &MutationReceipt) -> HeaderValue {
    header_value(&format!("/receipts/{}", receipt.attempt_id))
}

fn header_value(value: &str) -> HeaderValue {
    HeaderValue::from_str(value).unwrap_or_else(|_| HeaderValue::from_static("invalid"))
}

/// Start the HTTP server on the specified address and port.
///
/// # Arguments
///
/// * `handle` - The VibeLang runtime handle for sending messages
/// * `state` - The shared runtime state for reading
/// * `bind_addr` - The address to bind (use `127.0.0.1` unless remote access is intended)
/// * `port` - The port to listen on
/// * `eval_tx` - Optional channel to send code evaluation requests to the main thread
///
/// # Example
///
/// ```ignore
/// let handle = runtime.handle();
/// let state = runtime.state();
/// let (eval_tx, eval_rx) = std::sync::mpsc::channel();
/// tokio::spawn(async move {
///     start_server(handle, state, [127, 0, 0, 1].into(), 1606, Some(eval_tx)).await;
/// });
/// ```
pub async fn start_server(
    handle: RuntimeHandle,
    state: Arc<RwLock<State>>,
    bind_addr: IpAddr,
    port: u16,
    eval_tx: Option<EvalSender>,
) {
    if let Err(error) = start_server_with_options(
        handle,
        state,
        bind_addr,
        port,
        eval_tx,
        HttpServerOptions::default(),
    )
    .await
    {
        tracing::error!("HTTP API server failed: {error}");
    }
}

pub async fn start_server_with_options(
    handle: RuntimeHandle,
    state: Arc<RwLock<State>>,
    bind_addr: IpAddr,
    port: u16,
    eval_tx: Option<EvalSender>,
    options: HttpServerOptions,
) -> Result<(), HttpServerError> {
    if matches!(options.security, HttpSecurityMode::LoopbackLocal { .. })
        && !bind_addr.is_loopback()
    {
        return Err(HttpServerError::LoopbackBindRequired);
    }
    let security = Arc::new(HttpSecurityPolicy::new(options)?);

    // Create broadcast channel for WebSocket events
    let (ws_tx, _) = broadcast::channel::<WebSocketEvent>(1024);
    let mutation_status = handle.mutation_status();
    let receipt_broadcast_cursor = Arc::new(Mutex::new(ReceiptBroadcastCursor {
        runtime_epoch: mutation_status.runtime_epoch,
        event_sequence: mutation_status.event_sequence,
    }));

    let app_state = Arc::new(AppState {
        handle: handle.clone(),
        state: state.clone(),
        ws_tx: ws_tx.clone(),
        eval_tx,
        receipt_broadcast_cursor: Arc::clone(&receipt_broadcast_cursor),
        security,
    });

    // Start the event broadcaster in the background
    let broadcast_state = state.clone();
    let broadcast_handle = handle.clone();
    let broadcast_tx = ws_tx.clone();
    let broadcast_receipt_cursor = receipt_broadcast_cursor;
    let redact_receipts = app_state.security.mode.remote();
    tokio::spawn(async move {
        websocket::run_event_broadcaster(
            broadcast_state,
            broadcast_handle,
            broadcast_tx,
            broadcast_receipt_cursor,
            redact_receipts,
        )
        .await;
    });

    // Build the router with all routes
    let app = Router::new()
        // Transport
        .route("/transport", get(routes::transport::get_transport))
        .route("/transport", patch(routes::transport::update_transport))
        .route("/transport/start", post(routes::transport::start_transport))
        .route("/transport/stop", post(routes::transport::stop_transport))
        .route("/transport/seek", post(routes::transport::seek_transport))
        // Groups
        .route("/groups", get(routes::groups::list_groups))
        .route("/groups/{id}", get(routes::groups::get_group))
        .route("/groups/{id}", patch(routes::groups::update_group))
        .route("/groups/{id}/mute", post(routes::groups::mute_group))
        .route("/groups/{id}/unmute", post(routes::groups::unmute_group))
        .route("/groups/{id}/solo", post(routes::groups::solo_group))
        .route("/groups/{id}/unsolo", post(routes::groups::unsolo_group))
        .route(
            "/groups/{id}/params/{param}",
            put(routes::groups::set_group_param),
        )
        // Voices
        .route("/voices", get(routes::voices::list_voices))
        .route("/voices", post(routes::voices::create_voice))
        .route("/voices/{id}", get(routes::voices::get_voice))
        .route("/voices/{id}", patch(routes::voices::update_voice))
        .route("/voices/{id}", delete(routes::voices::delete_voice))
        .route("/voices/{id}/trigger", post(routes::voices::trigger_voice))
        .route("/voices/{id}/stop", post(routes::voices::stop_voice))
        .route("/voices/{id}/note-on", post(routes::voices::note_on))
        .route("/voices/{id}/note-off", post(routes::voices::note_off))
        .route(
            "/voices/{id}/params/{param}",
            put(routes::voices::set_voice_param),
        )
        .route("/voices/{id}/mute", post(routes::voices::mute_voice))
        .route("/voices/{id}/unmute", post(routes::voices::unmute_voice))
        // Patterns
        .route("/patterns", get(routes::patterns::list_patterns))
        .route("/patterns", post(routes::patterns::create_pattern))
        .route("/patterns/{id}", get(routes::patterns::get_pattern))
        .route("/patterns/{id}", patch(routes::patterns::update_pattern))
        .route("/patterns/{id}", delete(routes::patterns::delete_pattern))
        .route(
            "/patterns/{id}/start",
            post(routes::patterns::start_pattern),
        )
        .route("/patterns/{id}/stop", post(routes::patterns::stop_pattern))
        .route(
            "/patterns/{id}/params/{param}",
            put(routes::patterns::set_pattern_param),
        )
        // Melodies
        .route("/melodies", get(routes::melodies::list_melodies))
        .route("/melodies", post(routes::melodies::create_melody))
        .route("/melodies/{id}", get(routes::melodies::get_melody))
        .route("/melodies/{id}", patch(routes::melodies::update_melody))
        .route("/melodies/{id}", delete(routes::melodies::delete_melody))
        .route("/melodies/{id}/start", post(routes::melodies::start_melody))
        .route("/melodies/{id}/stop", post(routes::melodies::stop_melody))
        // Sequences
        .route("/sequences", get(routes::sequences::list_sequences))
        .route("/sequences", post(routes::sequences::create_sequence))
        .route("/sequences/{id}", get(routes::sequences::get_sequence))
        .route("/sequences/{id}", patch(routes::sequences::update_sequence))
        .route(
            "/sequences/{id}",
            delete(routes::sequences::delete_sequence),
        )
        .route(
            "/sequences/{id}/start",
            post(routes::sequences::start_sequence),
        )
        .route(
            "/sequences/{id}/stop",
            post(routes::sequences::stop_sequence),
        )
        .route(
            "/sequences/{id}/pause",
            post(routes::sequences::pause_sequence),
        )
        .route(
            "/sequences/{id}/resume",
            post(routes::sequences::resume_sequence),
        )
        // Effects
        .route("/effects", get(routes::effects::list_effects))
        .route("/effects/{id}", get(routes::effects::get_effect))
        .route("/effects/{id}", patch(routes::effects::update_effect))
        .route("/effects/{id}", delete(routes::effects::delete_effect))
        .route(
            "/effects/{id}/params/{param}",
            put(routes::effects::set_effect_param),
        )
        // Samples
        .route("/samples", get(routes::samples::list_samples))
        .route("/samples", post(routes::samples::load_sample))
        .route("/samples/{id}", get(routes::samples::get_sample))
        .route("/samples/{id}", delete(routes::samples::delete_sample))
        // SynthDefs
        .route("/synthdefs", get(routes::synthdefs::list_synthdefs))
        .route("/synthdefs/{name}", get(routes::synthdefs::get_synthdef))
        // Eval
        .route("/eval", post(routes::eval::eval_code))
        // Canonical receipt observation for v1 mutation carriers
        .route("/receipts/{attempt_id}", get(routes::eval::get_receipt))
        // Live state
        .route("/live", get(routes::live::get_live_state))
        .route("/live/transport", get(routes::live::get_transport_state))
        .route("/live/fades", get(routes::live::get_active_fades))
        .route("/live/meters", get(routes::live::get_meters))
        // Fades (alias for /live/fades for compatibility)
        .route("/fades", get(routes::live::get_active_fades))
        // Fade control
        .route("/fades", post(routes::fades::start_fade))
        .route("/fades", delete(routes::fades::cancel_fade))
        .route("/fades/voice/{name}", post(routes::fades::fade_voice))
        .route("/fades/group/{path}", post(routes::fades::fade_group))
        .route("/fades/effect/{id}", post(routes::fades::fade_effect))
        .route("/fades/pattern/{name}", post(routes::fades::fade_pattern))
        .route("/fades/melody/{name}", post(routes::fades::fade_melody))
        // WebSocket
        .route("/ws", get(websocket::ws_handler));

    // Add recording routes (native only)
    #[cfg(not(target_arch = "wasm32"))]
    let app = app
        .route("/recordings", get(routes::recordings::list_recordings))
        .route("/recordings/{id}", get(routes::recordings::get_recording))
        .route(
            "/recordings/{id}/stop",
            post(routes::recordings::stop_recording),
        )
        .route(
            "/recordings/{id}/cancel",
            post(routes::recordings::cancel_recording),
        );

    // Add MIDI routes (feature-gated)
    #[cfg(feature = "midi")]
    let app = app
        .route("/midi/devices", get(routes::midi::list_devices))
        .route("/midi/input/open", post(routes::midi::open_input))
        .route("/midi/output/open", post(routes::midi::open_output))
        .route("/midi/close", post(routes::midi::close_device))
        .route("/midi/note/on", post(routes::midi::send_note_on))
        .route("/midi/note/off", post(routes::midi::send_note_off))
        .route("/midi/cc", post(routes::midi::send_cc))
        .route("/midi/record/start", post(routes::midi::start_recording))
        .route("/midi/record/stop", post(routes::midi::stop_recording))
        .route(
            "/midi/clock/enable",
            post(routes::midi::enable_clock_output),
        )
        .route(
            "/midi/clock/disable",
            post(routes::midi::disable_clock_output),
        )
        .route("/midi/transport/start", post(routes::midi::send_midi_start))
        .route("/midi/transport/stop", post(routes::midi::send_midi_stop))
        .route(
            "/midi/transport/continue",
            post(routes::midi::send_midi_continue),
        )
        // Route management
        .route("/midi/routes", get(routes::midi::list_routes))
        .route("/midi/routes", delete(routes::midi::clear_routes))
        .route(
            "/midi/route/keyboard",
            post(routes::midi::add_keyboard_route),
        )
        .route(
            "/midi/route/{index}",
            delete(routes::midi::remove_keyboard_route),
        );

    let v2 = app
        .clone()
        .route("/receipts/{attempt_id}", delete(routes::v2::cancel_receipt))
        .route("/mutation-status", get(routes::v2::mutation_status))
        .route("/capabilities", get(routes::v2::capabilities))
        .route("/capabilities/details", get(routes::v2::capability_details))
        .route("/receipt-events", get(routes::v2::receipt_events));

    let app = Router::new().merge(app).nest("/v2", v2);

    // Add shared state and protocol/security middleware.
    let app = app
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            project_http_mutation_response,
        ))
        .with_state(app_state);

    let addr = SocketAddr::new(bind_addr, port);
    tracing::info!(
        "HTTP API server starting on http://{}:{}",
        addr.ip(),
        addr.port()
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .await
        .map_err(HttpServerError::Bind)
}

#[cfg(test)]
mod receipt_projection_tests {
    use super::*;
    use axum::response::IntoResponse;
    use axum::Json;
    use vibelang_core::mutation::{
        Applied, AttemptId, ComponentOutcome, ComponentState, Confirmation, Diagnostic,
        DiagnosticSeverity, EffectiveAt, EventSequence, FailurePhase, Partial, ReceiptTimestamps,
        Rejected, RequestIdentity, RevisionId, RollbackState, RuntimeEpoch, SourceSpan, Timestamp,
        MUTATION_SCHEMA_VERSION,
    };

    pub(super) fn receipt(state: ReceiptState, event_sequence: u64) -> MutationReceipt {
        let now = Timestamp::parse("2026-07-17T08:00:00Z").unwrap();
        MutationReceipt {
            schema_version: MUTATION_SCHEMA_VERSION,
            attempt_id: AttemptId::new(),
            runtime_epoch: RuntimeEpoch::new(),
            revision: Some(RevisionId::new(1).unwrap()),
            event_sequence: EventSequence::new(event_sequence).unwrap(),
            request: RequestIdentity {
                kind: MutationKind::Command {
                    domain: vibelang_core::mutation::MessageDomain::Transport,
                    operation: "start".into(),
                },
                source: MutationSource::Http {
                    method: "POST".into(),
                    path: "/transport/start".into(),
                    request_id: "request-1".into(),
                },
                submission_digest: None,
                operation_digest: None,
                idempotency_key_present: false,
                expected_revision: None,
                atomicity: Atomicity::BestEffort,
                supersession: SupersessionPolicy::Fifo,
            },
            state,
            previous_confirmed_revision: None,
            timestamps: ReceiptTimestamps {
                submitted_at: now.clone(),
                accepted_at: Some(now.clone()),
                last_transition_at: now,
                terminal_at: None,
            },
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn accepted_direct_mutation_is_202_with_nested_legacy_result() {
        let context = HttpRequestContext::legacy(Method::POST, "/transport/start".into());
        let accepted = receipt(
            ReceiptState::Accepted {
                queue_position: Some(1),
            },
            1,
        );
        context.record(accepted.clone());
        let response = (StatusCode::OK, Json(json!({ "running": false }))).into_response();
        let response = futures::executor::block_on(project_response(response, &context, false));

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(
            response
                .headers()
                .get(LEGACY_SUCCESS_SCOPE_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("queue_admitted")
        );
        let expected_location = format!("/receipts/{}", accepted.attempt_id);
        assert_eq!(
            response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok()),
            Some(expected_location.as_str())
        );
        let body = futures::executor::block_on(to_bytes(response.into_body(), usize::MAX)).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["receipt"]["state"]["state"], "accepted");
        assert_eq!(body["legacy_result"]["running"], false);
        assert!(body.get("success").is_none());
    }

    #[test]
    fn partial_receipt_outranks_pending_and_never_returns_success() {
        let context = HttpRequestContext::legacy(Method::PATCH, "/transport".into());
        context.record(receipt(
            ReceiptState::Accepted {
                queue_position: Some(1),
            },
            1,
        ));
        let partial = receipt(
            ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
                phase: FailurePhase::BackendBarrier,
                code: "backend_sync_failed".into(),
                components: Vec::new(),
                rollback: RollbackState::Uncertain,
                fenced: true,
                last_confirmed_revision: None,
            })),
            2,
        );
        context.record(partial.clone());
        let response = StatusCode::NO_CONTENT.into_response();
        let response = futures::executor::block_on(project_response(response, &context, false));

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = futures::executor::block_on(to_bytes(response.into_body(), usize::MAX)).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["receipt"]["attempt_id"],
            partial.attempt_id.to_string()
        );
        assert_eq!(body["receipt"]["state"]["state"], "terminal");
        assert_eq!(body["receipt"]["state"]["details"]["outcome"], "partial");
        assert_eq!(
            body["receipt"]["state"]["details"]["details"]["fenced"],
            true
        );
        assert!(body.get("success").is_none());
    }

    #[test]
    fn accepted_eval_marks_legacy_success_as_evaluation_only() {
        let context = HttpRequestContext::legacy(Method::POST, "/eval".into());
        context.record(receipt(
            ReceiptState::Accepted {
                queue_position: Some(1),
            },
            1,
        ));
        let response = Json(json!({
            "success": true,
            "legacy_success_scope": "evaluation_only",
        }))
        .into_response();
        let response = futures::executor::block_on(project_response(response, &context, true));

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(
            response
                .headers()
                .get(LEGACY_SUCCESS_SCOPE_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("evaluation_only")
        );
        let body = futures::executor::block_on(to_bytes(response.into_body(), usize::MAX)).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["legacy_success_scope"], "evaluation_only");
        assert!(body.get("legacy_result").is_none());
    }

    #[test]
    fn applied_receipt_preserves_the_legacy_family_status() {
        let applied = receipt(
            ReceiptState::Terminal(TerminalOutcome::Applied(Applied {
                effective_at: EffectiveAt {
                    observed_at: Timestamp::parse("2026-07-17T08:00:01Z").unwrap(),
                    musical_beat: None,
                    backend_time_seconds: None,
                },
                confirmations: vec![Confirmation::RuntimeCommit],
                components: Vec::new(),
                audible_tail_until: None,
            })),
            2,
        );
        let response = with_receipt_status(StatusCode::CREATED.into_response(), &applied);
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[test]
    fn remote_receipts_redact_diagnostic_paths_and_messages() {
        let private_diagnostic = Diagnostic {
            severity: DiagnosticSeverity::Error,
            code: "private_failure".into(),
            message: "failed below /home/user/private/song.vibe".into(),
            component_path: Some("/home/user/private".into()),
            source_span: Some(SourceSpan {
                source: "/home/user/private/song.vibe".into(),
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 2,
            }),
        };
        let mut receipt = receipt(
            ReceiptState::Terminal(TerminalOutcome::Applied(Applied {
                effective_at: EffectiveAt {
                    observed_at: Timestamp::parse("2026-07-17T08:00:01Z").unwrap(),
                    musical_beat: None,
                    backend_time_seconds: None,
                },
                confirmations: vec![Confirmation::RuntimeCommit],
                components: vec![ComponentOutcome {
                    path: "voice.private".into(),
                    action: "update".into(),
                    state: ComponentState::Applied,
                    effective_at: None,
                    confirmation: None,
                    diagnostic: Some(private_diagnostic.clone()),
                }],
                audible_tail_until: None,
            })),
            1,
        );
        receipt.diagnostics.push(private_diagnostic);

        let receipt = sanitize_mutation_receipt(receipt, true);
        assert_eq!(
            receipt.diagnostics[0].message,
            "private diagnostic detail is redacted"
        );
        assert!(receipt.diagnostics[0].component_path.is_none());
        assert_eq!(
            receipt.diagnostics[0]
                .source_span
                .as_ref()
                .map(|span| span.source.as_str()),
            Some("<redacted>")
        );
        let ReceiptState::Terminal(TerminalOutcome::Applied(applied)) = receipt.state else {
            panic!("expected applied receipt");
        };
        let diagnostic = applied.components[0]
            .diagnostic
            .as_ref()
            .expect("component diagnostic");
        assert_eq!(diagnostic.message, "private diagnostic detail is redacted");
        assert!(diagnostic.component_path.is_none());
        assert_eq!(
            diagnostic
                .source_span
                .as_ref()
                .map(|span| span.source.as_str()),
            Some("<redacted>")
        );
    }

    #[test]
    fn canonical_rejection_problem_preserves_idempotency_identity_without_private_text() {
        let rejected = Rejected {
            phase: FailurePhase::Idempotency,
            code: "idempotency_conflict".into(),
            message: "the idempotency key is bound to /home/user/private/song.vibe".into(),
            rollback: RollbackState::NotNeeded,
            preserved_revision: None,
        };
        let mut problem = problem_from_receipt_rejection(&rejected);
        assert_eq!(problem.status, StatusCode::CONFLICT);
        assert_eq!(problem.code, "idempotency_conflict");
        assert_eq!(problem.field.as_deref(), Some(IDEMPOTENCY_KEY_HEADER));
        assert_eq!(problem.reason.as_deref(), Some("mutation.idempotency"));

        sanitize_http_problem(&mut problem, true);
        assert_eq!(
            problem.message,
            "mutation rejected; private diagnostic detail is redacted"
        );
    }

    #[test]
    fn canonical_v2_receipt_projection_carries_location_and_freshness() {
        let receipt = receipt(
            ReceiptState::Accepted {
                queue_position: Some(1),
            },
            1,
        );
        let attempt_id = receipt.attempt_id;
        let response = v2_receipt_response(receipt, true);
        let expected_location = format!("/v2/receipts/{attempt_id}");
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(
            response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok()),
            Some(expected_location.as_str())
        );
        assert!(response.headers().contains_key(RUNTIME_EPOCH_HEADER));
        assert!(response.headers().contains_key(EVENT_SEQUENCE_HEADER));
        // The bounded wait ended without a terminal receipt, so the
        // wait=terminal preference was not honored and must not be asserted.
        assert!(response.headers().get(PREFERENCE_APPLIED_HEADER).is_none());
    }

    #[tokio::test]
    async fn reply_sink_retains_request_context_for_late_terminal_precedence() {
        let context = HttpRequestContext::legacy(Method::PATCH, "/transport".into());
        let (latest, reply_sink) = HTTP_REQUEST_CONTEXT
            .scope(context.clone(), async {
                let latest = Arc::new(Mutex::new(None));
                let reply_sink = http_reply_sink(Arc::clone(&latest));
                (latest, reply_sink)
            })
            .await;
        let accepted = receipt(
            ReceiptState::Accepted {
                queue_position: Some(1),
            },
            1,
        );
        reply_sink.publish(accepted.clone());
        let mut partial = accepted;
        partial.event_sequence = EventSequence::new(2).unwrap();
        partial.state = ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: FailurePhase::BackendBarrier,
            code: "backend_acknowledgement_lost".into(),
            components: Vec::new(),
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: None,
        }));
        reply_sink.publish(partial.clone());

        assert_eq!(
            latest
                .lock()
                .unwrap()
                .as_ref()
                .map(|receipt| &receipt.state),
            Some(&partial.state)
        );
        assert_eq!(
            canonical_http_receipt(&context.current_receipts()),
            Some(&partial)
        );
    }

    #[test]
    fn latest_receipt_prefers_a_known_terminal_transition() {
        let accepted = receipt(
            ReceiptState::Accepted {
                queue_position: Some(1),
            },
            1,
        );
        let mut partial = accepted.clone();
        partial.event_sequence = EventSequence::new(2).unwrap();
        partial.state = ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: FailurePhase::BackendBarrier,
            code: "backend_acknowledgement_lost".into(),
            components: Vec::new(),
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: None,
        }));

        assert_eq!(
            canonical_latest_receipt(Some(accepted), Some(partial.clone())),
            Some(partial)
        );
    }

    #[test]
    fn mutation_routes_do_not_bypass_the_receipt_aware_sender() {
        let routes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
        for entry in std::fs::read_dir(routes).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let compact = source
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();
            assert!(
                !compact.contains("state.handle.send("),
                "{} bypasses AppState::send",
                path.display()
            );
        }
    }

    #[test]
    fn loopback_security_accepts_only_loopback_origins() {
        let policy = HttpSecurityPolicy::new(HttpServerOptions::default()).unwrap();
        let mut allowed = HeaderMap::new();
        allowed.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:5173"),
        );
        let admitted =
            authorize_http_request(&policy, &allowed, &Method::GET, "/v2/capabilities").unwrap();
        assert_eq!(admitted.caller_namespace, "http.loopback.runtime_local");

        let mut rejected = HeaderMap::new();
        rejected.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://remote.example"),
        );
        let error = authorize_http_request(&policy, &rejected, &Method::GET, "/v2/capabilities")
            .unwrap_err();
        assert_eq!(error.code, "origin_not_allowed");
    }

    #[test]
    fn cors_headers_echo_only_origins_allowed_by_the_active_mode() {
        let mode = HttpSecurityMode::AuthenticatedRemote {
            credentials: vec![HttpCredential {
                subject: "performer".into(),
                bearer_token: "secret-token".into(),
                privileged_detail: false,
            }],
            allowed_origins: vec!["https://studio.example".into()],
        };
        assert!(http_origin_allowed(&mode, "https://studio.example"));
        assert!(!http_origin_allowed(&mode, "https://attacker.example"));

        let mut headers = HeaderMap::new();
        apply_cors_headers(&mut headers, Some("https://studio.example"), &mode);
        assert_eq!(
            headers
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|value| value.to_str().ok()),
            Some("https://studio.example")
        );
        assert_eq!(
            headers
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
        let exposed = headers
            .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .and_then(|value| value.to_str().ok())
            .expect("exposed response headers");
        assert!(exposed.contains("location"));
        assert!(exposed.contains("preference-applied"));
        assert!(exposed.contains("x-vibelang-runtime-epoch"));
        assert!(exposed.contains("x-vibelang-revision"));
        assert!(exposed.contains("x-vibelang-event-sequence"));
    }

    #[test]
    fn authenticated_remote_requires_allowlisted_origin_and_bearer() {
        let credential = HttpCredential {
            subject: "performer".into(),
            bearer_token: "secret-token".into(),
            privileged_detail: true,
        };
        assert!(!format!("{credential:?}").contains("secret-token"));
        let policy = HttpSecurityPolicy::new(HttpServerOptions {
            security: HttpSecurityMode::AuthenticatedRemote {
                credentials: vec![credential],
                allowed_origins: vec!["https://studio.example".into()],
            },
            ..HttpServerOptions::default()
        })
        .unwrap();
        assert_eq!(policy.authentication_and_privileged_detail(), (true, true));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://studio.example"),
        );
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-token"),
        );
        let admitted =
            authorize_http_request(&policy, &headers, &Method::GET, "/v2/capabilities").unwrap();
        assert_eq!(admitted.caller_namespace, "http.authenticated.performer");
        assert!(!format!("{admitted:?}").contains("secret-token"));
        authorize_http_request(&policy, &headers, &Method::GET, "/v2/capabilities/details")
            .unwrap();

        headers.remove(header::AUTHORIZATION);
        let error = authorize_http_request(&policy, &headers, &Method::GET, "/v2/capabilities")
            .unwrap_err();
        assert_eq!(error.code, "authentication_required");
    }

    #[test]
    fn privileged_capability_details_reject_non_privileged_security_modes() {
        let loopback = HttpSecurityPolicy::new(HttpServerOptions::default()).unwrap();
        let error = authorize_http_request(
            &loopback,
            &HeaderMap::new(),
            &Method::GET,
            "/v2/capabilities/details",
        )
        .unwrap_err();
        assert_eq!(error.code, "privileged_detail_required");

        let policy = HttpSecurityPolicy::new(HttpServerOptions {
            security: HttpSecurityMode::AuthenticatedRemote {
                credentials: vec![HttpCredential {
                    subject: "performer".into(),
                    bearer_token: "non-privileged-token".into(),
                    privileged_detail: false,
                }],
                allowed_origins: vec!["https://studio.example".into()],
            },
            ..HttpServerOptions::default()
        })
        .unwrap();
        assert_eq!(policy.authentication_and_privileged_detail(), (true, false));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://studio.example"),
        );
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer non-privileged-token"),
        );
        let error =
            authorize_http_request(&policy, &headers, &Method::GET, "/v2/capabilities/details")
                .unwrap_err();
        assert_eq!(error.code, "privileged_detail_required");
    }

    #[test]
    fn insecure_remote_requires_acknowledgement_and_remains_rate_limited() {
        assert!(matches!(
            HttpSecurityMode::insecure_remote(
                "I understand",
                vec!["https://studio.example".into()]
            ),
            Err(HttpServerError::InsecureRemoteAcknowledgementRequired)
        ));
        let security = HttpSecurityMode::insecure_remote(
            INSECURE_REMOTE_ACKNOWLEDGEMENT,
            vec!["https://studio.example".into()],
        )
        .unwrap();
        let policy = HttpSecurityPolicy::new(HttpServerOptions {
            security,
            rate_limit_per_minute: 1,
            ..HttpServerOptions::default()
        })
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://studio.example"),
        );
        let admitted =
            authorize_http_request(&policy, &headers, &Method::GET, "/v2/capabilities").unwrap();
        assert!(admitted.caller_namespace.starts_with("http.insecure."));
        let error = authorize_http_request(&policy, &headers, &Method::GET, "/v2/capabilities")
            .unwrap_err();
        assert_eq!(error.code, "rate_limit_exceeded");
    }

    #[test]
    fn revisioned_reads_remove_private_runtime_fields() {
        let group_names = HashMap::from([("1".into(), "leads".into())]);
        let sanitized = sanitize_v2_state(
            "/v2/voices",
            json!({
                "id": 1,
                "name": "lead",
                "group_path": "1",
                "group_name": "1",
                "output_bus": 16,
                "source_location": "/home/user/project/song.vibe",
                "sustained_notes": [60],
            }),
            false,
            &group_names,
        );
        assert_eq!(sanitized["name"], "lead");
        assert_eq!(sanitized["group_path"], "leads");
        assert!(sanitized.get("group_name").is_none());
        assert!(sanitized.get("output_bus").is_none());
        assert!(sanitized.get("source_location").is_none());
        assert!(sanitized.get("sustained_notes").is_none());
    }

    #[test]
    fn revisioned_pattern_reads_preserve_effectful_event_params() {
        let sanitized = sanitize_v2_state(
            "/v2/patterns/kick",
            json!({
                "name": "kick",
                "params": null,
                "status": {
                    "state": "playing",
                    "start_beat": null,
                    "stop_beat": null
                },
                "events": [{
                    "beat": 0.0,
                    "params": {"amp": 0.8}
                }]
            }),
            false,
            &HashMap::new(),
        );
        assert!(sanitized.get("params").is_none());
        assert_eq!(sanitized["events"][0]["params"]["amp"], 0.8);
        assert!(sanitized["status"].get("start_beat").is_none());
        assert!(sanitized["status"].get("stop_beat").is_none());
    }

    #[test]
    fn revisioned_sequence_reads_use_one_end_representation() {
        let sanitized = sanitize_v2_state(
            "/v2/sequences/song",
            json!({
                "name": "song",
                "clips": [{
                    "type": "pattern",
                    "name": "1",
                    "start_beat": 0.0,
                    "end_beat": 4.0,
                    "duration_beats": 4.0,
                    "once": null
                }]
            }),
            false,
            &HashMap::new(),
        );
        assert_eq!(sanitized["clips"][0]["end_beat"], 4.0);
        assert!(sanitized["clips"][0].get("duration_beats").is_none());
        assert!(sanitized["clips"][0].get("once").is_none());
    }

    #[test]
    fn remote_revisioned_reads_redact_filesystem_paths() {
        let sanitized = sanitize_v2_state(
            "/v2/samples",
            json!([{
                "id": "1",
                "path": "/home/user/project/kick.wav",
                "buffer_id": 2
            }]),
            true,
            &HashMap::new(),
        );
        assert!(sanitized[0].get("path").is_none());
        assert_eq!(sanitized[0]["buffer_id"], 2);
    }

    #[test]
    fn v2_midi_channels_use_human_facing_one_based_values() {
        let mut first = Some(json!({ "channel": 1 }));
        normalize_midi_channel(&mut first, true).unwrap();
        assert_eq!(first.unwrap()["channel"], 0);

        let mut last = Some(json!({ "channel": 16 }));
        normalize_midi_channel(&mut last, true).unwrap();
        assert_eq!(last.unwrap()["channel"], 15);

        let mut below = Some(json!({ "channel": 0 }));
        let error = normalize_midi_channel(&mut below, true).unwrap_err();
        assert_eq!(error.code, "invalid_value");
        assert_eq!(error.field.as_deref(), Some("channel"));

        let mut above = Some(json!({ "channel": 17 }));
        assert!(normalize_midi_channel(&mut above, true).is_err());
    }

    #[test]
    fn bounded_wait_is_clamped_and_requires_explicit_preference() {
        let maximum = Duration::from_millis(250);
        let mut headers = HeaderMap::new();
        headers.insert(PREFER_HEADER, HeaderValue::from_static("wait=terminal"));
        headers.insert(WAIT_TIMEOUT_HEADER, HeaderValue::from_static("1000"));
        assert_eq!(
            parse_wait_preference(&headers, maximum).unwrap(),
            Some(maximum)
        );

        headers.remove(PREFER_HEADER);
        assert_eq!(
            parse_wait_preference(&headers, maximum).unwrap_err().code,
            "wait_preference_required"
        );
        headers.remove(WAIT_TIMEOUT_HEADER);
        assert_eq!(parse_wait_preference(&headers, maximum).unwrap(), None);
    }

    #[test]
    fn unconditional_revision_bypass_is_exactly_typed() {
        let mut headers = HeaderMap::new();
        headers.insert(
            UNCONDITIONAL_HEADER,
            HeaderValue::from_static("best-effort"),
        );
        assert!(parse_unconditional_header(&headers).unwrap());

        headers.insert(UNCONDITIONAL_HEADER, HeaderValue::from_static("true"));
        let error = parse_unconditional_header(&headers).unwrap_err();
        assert_eq!(error.code, "unsupported_value");
        assert_eq!(error.field.as_deref(), Some(UNCONDITIONAL_HEADER));
    }

    #[test]
    fn strict_operation_helpers_reject_no_effect_and_out_of_range_notes() {
        let error = require_exactly_one_param(&HashMap::new()).unwrap_err();
        assert_eq!(error.code, "no_effect");
        assert_eq!(parse_midi_note("127").unwrap(), 127);
        assert!(parse_midi_note("128").is_err());
        assert!(parse_pattern_string("x||x", 0.0).is_err());
    }

    #[test]
    fn equivalent_gain_representations_normalize_without_float_drift() {
        let mut body = Some(json!({
            "gain": 0.1,
            "params": {"amp": 0.1}
        }));
        let request_identity_body = body.clone();
        normalize_voice_gain(&mut body, Some(0.1), Some(0.1)).unwrap();
        let body = body.unwrap();
        assert!(body.get("gain").is_none());
        assert_eq!(
            body["params"]["amp"].as_f64().map(|value| value as f32),
            Some(0.1)
        );
        assert_eq!(
            request_identity_body
                .as_ref()
                .and_then(|body| body.get("gain"))
                .and_then(Value::as_f64)
                .map(|value| value as f32),
            Some(0.1)
        );
    }

    #[test]
    fn canonical_http_idempotency_material_uses_raw_operation_input() {
        let body = json!({
            "gain": 0.1,
            "group_path": "main"
        });
        let material = canonical_http_request_material("POST", "/v2/voices", Some(&body));
        let debug = format!("{material:?}");
        assert!(debug.contains("POST"));
        assert!(debug.contains("/v2/voices"));
        assert!(debug.contains("group_path"));
        assert!(debug.contains("main"));
        assert!(!debug.contains("authorization"));
        assert!(!debug.contains("bearer"));
    }

    #[test]
    fn v2_error_envelopes_are_recognized_before_legacy_error_projection() {
        assert!(is_v2_error_envelope(&json!({
            "schema_version": HTTP_V2_SCHEMA_VERSION,
            "operation": "receipt.cancel",
            "error": {
                "code": "receipt_not_found",
                "message": "not found"
            }
        })));
        assert!(!is_v2_error_envelope(&json!({
            "error": "not_found",
            "message": "not found"
        })));
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use async_trait::async_trait;
    use std::path::Path as FsPath;
    use vibelang_core::compat::Instant;
    use vibelang_core::{AddAction, Backend, BufferId, BufferInfo, NodeId, ParamMap, Runtime};

    #[derive(Debug)]
    pub(crate) struct NoopBackendError;

    impl std::fmt::Display for NoopBackendError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("noop backend error")
        }
    }

    impl std::error::Error for NoopBackendError {}

    pub(crate) struct NoopBackend;

    #[async_trait]
    impl Backend for NoopBackend {
        type Error = NoopBackendError;

        async fn load_synthdef(
            &self,
            _name: &str,
            _data: &[u8],
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn create_synth(
            &self,
            _def: &str,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
            _params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn create_group(
            &self,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_node(&self, _node: NodeId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn run_node(
            &self,
            _node: NodeId,
            _running: bool,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn set_param(
            &self,
            _node: NodeId,
            _param: &str,
            _value: f32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn map_param_to_bus(
            &self,
            _node: NodeId,
            _param: &str,
            _bus: u32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn load_buffer(
            &self,
            _id: BufferId,
            _path: &FsPath,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames: 0,
                channels: 1,
                sample_rate: 44_100.0,
            })
        }

        async fn alloc_buffer(
            &self,
            _id: BufferId,
            frames: u32,
            channels: u16,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames,
                channels,
                sample_rate: 44_100.0,
            })
        }

        async fn write_buffer(
            &self,
            _id: BufferId,
            _path: &FsPath,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_buffer(&self, _id: BufferId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        fn current_time(&self) -> Instant {
            Instant::now()
        }
    }

    pub(crate) fn app_state(security: HttpSecurityMode) -> (Runtime<NoopBackend>, Arc<AppState>) {
        let runtime = Runtime::new(NoopBackend);
        let (ws_tx, _) = broadcast::channel(16);
        let status = runtime.handle().mutation_status();
        let state = Arc::new(AppState {
            handle: runtime.handle(),
            state: Arc::clone(runtime.state()),
            ws_tx,
            eval_tx: None,
            receipt_broadcast_cursor: Arc::new(Mutex::new(ReceiptBroadcastCursor {
                runtime_epoch: status.runtime_epoch,
                event_sequence: status.event_sequence,
            })),
            security: Arc::new(
                HttpSecurityPolicy::new(HttpServerOptions {
                    security,
                    ..HttpServerOptions::default()
                })
                .unwrap(),
            ),
        });
        (runtime, state)
    }

    pub(crate) fn insecure_remote_mode() -> HttpSecurityMode {
        HttpSecurityMode::insecure_remote(
            INSECURE_REMOTE_ACKNOWLEDGEMENT,
            vec!["https://studio.example".into()],
        )
        .unwrap()
    }
}

#[cfg(test)]
mod gate_remediation_tests {
    use super::receipt_projection_tests::receipt;
    use super::test_support::{app_state, insecure_remote_mode};
    use super::*;
    use axum::response::IntoResponse;
    use vibelang_core::mutation::{
        Applied, AttemptId, Confirmation, EffectiveAt, EventSequence, FailurePhase, Superseded,
        SupersessionReason, Timestamp,
    };

    fn accepted_receipt(sequence: u64) -> MutationReceipt {
        receipt(
            ReceiptState::Accepted {
                queue_position: Some(1),
            },
            sequence,
        )
    }

    fn applied_receipt(sequence: u64) -> MutationReceipt {
        receipt(
            ReceiptState::Terminal(TerminalOutcome::Applied(Applied {
                effective_at: EffectiveAt {
                    observed_at: Timestamp::parse("2026-07-17T08:00:01Z").unwrap(),
                    musical_beat: None,
                    backend_time_seconds: None,
                },
                confirmations: vec![Confirmation::RuntimeCommit],
                components: Vec::new(),
                audible_tail_until: None,
            })),
            sequence,
        )
    }

    fn superseded_receipt(sequence: u64) -> MutationReceipt {
        receipt(
            ReceiptState::Terminal(TerminalOutcome::Superseded(Superseded {
                reason: SupersessionReason::Cancelled,
                by_revision: None,
            })),
            sequence,
        )
    }

    // F1: the remote security modes must gate the v1 surface exactly like v2.
    #[test]
    fn v1_remote_policy_gates_filesystem_sample_load() {
        let remote = insecure_remote_mode();
        let problem = v1_remote_policy_problem(&remote, &Method::POST, "/samples")
            .expect("remote v1 sample load must be rejected");
        assert_eq!(problem.status, StatusCode::FORBIDDEN);
        assert_eq!(problem.code, "capability_unavailable");
        assert_eq!(problem.field.as_deref(), Some("path"));

        let authenticated = HttpSecurityMode::AuthenticatedRemote {
            credentials: vec![HttpCredential {
                subject: "performer".into(),
                bearer_token: "token".into(),
                privileged_detail: false,
            }],
            allowed_origins: vec!["https://studio.example".into()],
        };
        assert!(v1_remote_policy_problem(&authenticated, &Method::POST, "/samples").is_some());

        assert!(v1_remote_policy_problem(&remote, &Method::GET, "/samples").is_none());
        assert!(v1_remote_policy_problem(&remote, &Method::POST, "/voices").is_none());
        let local = HttpSecurityMode::loopback_local();
        assert!(v1_remote_policy_problem(&local, &Method::POST, "/samples").is_none());
    }

    // F1: v1 sample and recording reads must not leak server filesystem paths
    // in remote modes.
    #[tokio::test]
    async fn v1_remote_reads_redact_sample_and_recording_paths() {
        let (_runtime, state) = app_state(insecure_remote_mode());

        let context = HttpRequestContext::legacy(Method::GET, "/samples".into());
        let response = Json(json!([{
            "id": "1",
            "path": "/home/user/project/kick.wav",
            "buffer_id": 2
        }]))
        .into_response();
        let response = sanitize_v1_remote_response(response, &context, &state).await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert!(body[0].get("path").is_none());
        assert_eq!(body[0]["buffer_id"], 2);

        let context = HttpRequestContext::legacy(Method::GET, "/recordings/1".into());
        let response = Json(json!({
            "id": "1",
            "status": "completed",
            "path": "/tmp/session/recording.wav"
        }))
        .into_response();
        let response = sanitize_v1_remote_response(response, &context, &state).await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert!(body.get("path").is_none());
        assert_eq!(body["status"], "completed");

        // Routes without filesystem projections pass through untouched.
        let context = HttpRequestContext::legacy(Method::GET, "/voices".into());
        let response =
            Json(json!([{ "name": "lead", "path": "not-a-file-field" }])).into_response();
        let response = sanitize_v1_remote_response(response, &context, &state).await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body[0]["path"], "not-a-file-field");

        // Loopback mode keeps the full projection.
        let (_local_runtime, local_state) = app_state(HttpSecurityMode::loopback_local());
        let context = HttpRequestContext::legacy(Method::GET, "/samples".into());
        let response = Json(json!([{ "id": "1", "path": "/home/user/kick.wav" }])).into_response();
        let response = sanitize_v1_remote_response(response, &context, &local_state).await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body[0]["path"], "/home/user/kick.wav");
    }

    // F2: the fingerprinted request identity must include the HTTP method.
    #[test]
    fn http_request_identity_distinguishes_methods() {
        let body = json!({ "params": { "cutoff": 0.5 } });
        let patch = http_request_identity("PATCH", "/v2/voices/1", Some(&body));
        let delete = http_request_identity("DELETE", "/v2/voices/1", Some(&body));
        assert_ne!(patch, delete);
        assert_eq!(
            patch,
            http_request_identity("PATCH", "/v2/voices/1", Some(&body))
        );
        assert_eq!(patch["method"], "PATCH");
        assert_eq!(patch["path"], "/v2/voices/1");
    }

    // F2: one Idempotency-Key must not replay a receipt across HTTP methods.
    #[tokio::test]
    async fn idempotency_keys_do_not_replay_receipts_across_methods() {
        let (_runtime, state) = app_state(HttpSecurityMode::loopback_local());
        let submission = |method: &str| Submission {
            kind: mutation_kind_for_http_path("/v2/voices/1"),
            source: MutationSource::Http {
                method: method.into(),
                path: "/v2/voices/1".into(),
                request_id: uuid::Uuid::new_v4().to_string(),
            },
            caller_namespace: "http.test.local".into(),
            idempotency_key: Some("shared-key".into()),
            require_idempotency_key: false,
            retry_epoch: Some(state.handle.mutation_status().runtime_epoch),
            expected_revision: None,
            atomicity: Atomicity::BestEffort,
            supersession: SupersessionPolicy::Fifo,
            material: canonical_http_request_material(method, "/v2/voices/1", None),
        };

        let (_, reply_sink, event_sink) = state.mutation_sinks();
        let attempt = state
            .handle
            .begin_attempt(submission("DELETE"), reply_sink, event_sink)
            .unwrap();
        assert!(attempt.is_active());
        let delete_receipt = state
            .handle
            .finish_attempt_failure(
                attempt,
                FailurePhase::Validate,
                "voice_not_found",
                "no voice 1",
            )
            .unwrap();

        // Same method and key: the canonical receipt replays.
        let (_, reply_sink, event_sink) = state.mutation_sinks();
        let replay = state
            .handle
            .begin_attempt(submission("DELETE"), reply_sink, event_sink)
            .unwrap();
        assert!(!replay.is_active());
        assert_eq!(replay.receipt().attempt_id, delete_receipt.attempt_id);

        // Different method, same key and body: never the DELETE receipt.
        let (_, reply_sink, event_sink) = state.mutation_sinks();
        match state
            .handle
            .begin_attempt(submission("PATCH"), reply_sink, event_sink)
        {
            Ok(attempt) => {
                let receipt = attempt.receipt().clone();
                assert_ne!(
                    receipt.attempt_id, delete_receipt.attempt_id,
                    "a PATCH must not replay the DELETE receipt"
                );
                if !attempt.is_active() {
                    let ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) =
                        &receipt.state
                    else {
                        panic!("cross-method key reuse must reject, not replay");
                    };
                    assert_eq!(rejected.code, "idempotency_conflict");
                }
            }
            Err(_) => {}
        }
    }

    // F3: swing must be effective or rejected on every accepted input shape.
    #[test]
    fn v2_pattern_swing_is_effective_or_rejected() {
        let mut body = Some(json!({
            "name": "kick",
            "voice_name": "drums",
            "swing": 0.3,
            "events": [{ "beat": 0.0 }]
        }));
        let error = validate_v2_pattern_create(&mut body, "/patterns").unwrap_err();
        assert_eq!(error.code, "unsupported_field");
        assert_eq!(error.field.as_deref(), Some("swing"));

        // The scheduler never reads the stored value, so even swing 0.0 is
        // inert next to explicit events and stays rejected.
        let mut body = Some(json!({
            "name": "kick",
            "voice_name": "drums",
            "swing": 0.0,
            "events": [{ "beat": 0.0 }]
        }));
        assert!(validate_v2_pattern_create(&mut body, "/patterns").is_err());

        // Through pattern_string the swing is consumed: baked into beats and
        // zeroed in the stored configuration.
        let mut body = Some(json!({
            "name": "kick",
            "voice_name": "drums",
            "swing": 0.5,
            "pattern_string": "xx.."
        }));
        validate_v2_pattern_create(&mut body, "/patterns").unwrap();
        let body = body.unwrap();
        assert_eq!(body["swing"], 0.0);
        let events = body["events"].as_array().unwrap();
        assert_eq!(events[0]["beat"], 0.0);
        assert_eq!(events[1]["beat"], 1.25);

        // Explicit events without swing stay accepted.
        let mut body = Some(json!({
            "name": "kick",
            "voice_name": "drums",
            "events": [{ "beat": 0.0 }]
        }));
        validate_v2_pattern_create(&mut body, "/patterns").unwrap();
    }

    // F5: Preference-Applied asserts wait=terminal only when the receipt is
    // actually terminal.
    #[test]
    fn wait_preference_is_applied_only_for_terminal_receipts() {
        let response = v2_receipt_response(accepted_receipt(1), true);
        assert!(response.headers().get(PREFERENCE_APPLIED_HEADER).is_none());

        let response = v2_receipt_response(applied_receipt(2), true);
        assert_eq!(
            response
                .headers()
                .get(PREFERENCE_APPLIED_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("wait=terminal")
        );
    }

    // F6: DELETE /v2/receipts projects cancellation success as 200 and an
    // uncancellable terminal receipt as 409.
    #[test]
    fn cancel_projection_reports_success_200_and_uncancellable_409() {
        assert_eq!(
            cancel_receipt_http_status(&superseded_receipt(2)),
            StatusCode::OK
        );
        assert_eq!(
            cancel_receipt_http_status(&applied_receipt(2)),
            StatusCode::CONFLICT
        );
        assert_eq!(
            cancel_receipt_http_status(&accepted_receipt(1)),
            StatusCode::ACCEPTED
        );
    }

    // F7: a concurrently published higher-sequence event must not advance the
    // shared broadcast cursor past an unpublished lower-sequence event.
    #[test]
    fn out_of_order_receipt_publishes_defer_to_the_ledger_poller() {
        let (tx, mut rx) = broadcast::channel(16);
        let base = accepted_receipt(1);
        let epoch = base.runtime_epoch;
        let cursor = Mutex::new(ReceiptBroadcastCursor {
            runtime_epoch: epoch,
            event_sequence: Some(EventSequence::new(1).unwrap()),
        });
        let make_event = |sequence: u64, previous: u64| {
            let mut receipt = base.clone();
            receipt.event_sequence = EventSequence::new(sequence).unwrap();
            ReceiptEvent {
                schema_version: receipt.schema_version,
                runtime_epoch: epoch,
                event_sequence: receipt.event_sequence,
                previous_event_sequence: Some(EventSequence::new(previous).unwrap()),
                receipt,
            }
        };

        // Gapped publish: deferred, cursor unchanged.
        publish_receipt_event(&tx, &cursor, make_event(3, 2), false);
        assert!(rx.try_recv().is_err());

        // The poller replays the chain in order.
        publish_receipt_event(&tx, &cursor, make_event(2, 1), false);
        publish_receipt_event(&tx, &cursor, make_event(3, 2), false);
        let first = rx.try_recv().unwrap();
        let second = rx.try_recv().unwrap();
        assert_eq!(first.event_sequence, Some(EventSequence::new(2).unwrap()));
        assert_eq!(second.event_sequence, Some(EventSequence::new(3).unwrap()));

        // Republishing below the cursor stays deduplicated.
        publish_receipt_event(&tx, &cursor, make_event(3, 2), false);
        assert!(rx.try_recv().is_err());
    }

    // F9: operations that declare their body optional accept the absent body.
    #[test]
    fn optional_v2_bodies_normalize_to_empty_object() {
        let mut body = None;
        let request: V2TriggerRequest =
            decode_optional_v2_body(&mut body, "/voices/1/trigger").unwrap();
        assert!(request.params.is_empty());
        assert_eq!(body, Some(json!({})));

        let mut body = None;
        let _: V2LoopControlRequest =
            decode_optional_v2_body(&mut body, "/patterns/1/start").unwrap();
        assert_eq!(body, Some(json!({})));
    }

    // F11: cancel rejections must obey remote privacy redaction.
    #[tokio::test]
    async fn remote_cancel_rejections_redact_private_detail() {
        let (_runtime, state) = app_state(insecure_remote_mode());
        let response = routes::v2::cancel_receipt(
            axum::extract::State(state),
            axum::extract::Path(AttemptId::new().to_string()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["error"]["code"], "receipt_not_found");
        assert_eq!(body["error"]["message"], REDACTED_REJECTION_TEXT);

        let (_local_runtime, local_state) = app_state(HttpSecurityMode::loopback_local());
        let response = routes::v2::cancel_receipt(
            axum::extract::State(local_state),
            axum::extract::Path(AttemptId::new().to_string()),
        )
        .await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_ne!(body["error"]["message"], REDACTED_REJECTION_TEXT);
    }

    struct CapturedAuditEvent {
        level: tracing::Level,
        fields: BTreeMap<String, String>,
    }

    struct AuditFieldVisitor<'a>(&'a mut BTreeMap<String, String>);

    impl tracing::field::Visit for AuditFieldVisitor<'_> {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0.insert(field.name().into(), value.into());
        }

        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.insert(field.name().into(), format!("{value:?}"));
        }
    }

    struct AuditEventCapture {
        events: Arc<Mutex<Vec<CapturedAuditEvent>>>,
    }

    impl tracing::Subscriber for AuditEventCapture {
        fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
            metadata.target() == "vibelang_http::audit"
        }

        fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut fields = BTreeMap::new();
            event.record(&mut AuditFieldVisitor(&mut fields));
            if let Ok(mut events) = self.events.lock() {
                events.push(CapturedAuditEvent {
                    level: *event.metadata().level(),
                    fields,
                });
            }
        }

        fn enter(&self, _span: &tracing::span::Id) {}

        fn exit(&self, _span: &tracing::span::Id) {}
    }

    fn capture_audit_events(scope: impl FnOnce()) -> Vec<CapturedAuditEvent> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let capture = AuditEventCapture {
            events: Arc::clone(&events),
        };
        tracing::subscriber::with_default(capture, scope);
        Arc::try_unwrap(events)
            .ok()
            .and_then(|events| events.into_inner().ok())
            .unwrap_or_default()
    }

    // F8: the security audit must record rejected requests, not only admitted
    // ones, or auth/origin/rate-limit probes leave no trail.
    #[test]
    fn audit_records_rejected_and_admitted_requests() {
        let authenticated = HttpSecurityPolicy::new(HttpServerOptions {
            security: HttpSecurityMode::AuthenticatedRemote {
                credentials: vec![HttpCredential {
                    subject: "performer".into(),
                    bearer_token: "token".into(),
                    privileged_detail: false,
                }],
                allowed_origins: vec!["https://studio.example".into()],
            },
            ..HttpServerOptions::default()
        })
        .unwrap();
        let events = capture_audit_events(|| {
            let result =
                authorize_http_request(&authenticated, &HeaderMap::new(), &Method::POST, "/voices");
            assert!(result.is_err());
        });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].level, tracing::Level::WARN);
        assert_eq!(
            events[0].fields.get("code").map(String::as_str),
            Some("authentication_required")
        );
        assert_eq!(
            events[0].fields.get("path").map(String::as_str),
            Some("/voices")
        );

        let loopback = HttpSecurityPolicy::new(HttpServerOptions::default()).unwrap();
        let events = capture_audit_events(|| {
            let result =
                authorize_http_request(&loopback, &HeaderMap::new(), &Method::POST, "/voices");
            assert!(result.is_ok());
        });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].level, tracing::Level::INFO);
        assert!(events[0].fields.contains_key("caller_namespace"));

        let unaudited = HttpSecurityPolicy::new(HttpServerOptions {
            audit_enabled: false,
            ..HttpServerOptions::default()
        })
        .unwrap();
        let events = capture_audit_events(|| {
            let result =
                authorize_http_request(&unaudited, &HeaderMap::new(), &Method::GET, "/voices");
            assert!(result.is_ok());
        });
        assert!(events.is_empty());
    }
}
