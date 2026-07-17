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
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, patch, post, put},
    Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};
use vibelang_core::mutation::{
    Atomicity, CandidateOrigin, MutationEventSink, MutationKind, MutationReceipt,
    MutationReplySink, MutationSource, ReceiptState, RequestMaterial, Submission,
    SupersessionPolicy, TerminalOutcome,
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

#[derive(Clone)]
struct HttpRequestContext {
    method: Method,
    path: String,
    receipts: Arc<Mutex<BTreeMap<String, MutationReceipt>>>,
}

impl HttpRequestContext {
    fn new(method: Method, path: String) -> Self {
        Self {
            method,
            path,
            receipts: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn record(&self, receipt: MutationReceipt) {
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
        let event_sink = MutationEventSink::new(move |event| {
            let _ = ws_tx.send(WebSocketEvent::receipt(event));
        });
        (latest, reply_sink, event_sink)
    }

    fn http_submission(&self, kind: MutationKind, semantic: Value) -> Submission {
        let (method, path) = HTTP_REQUEST_CONTEXT
            .try_with(|context| (context.method.to_string(), context.path.clone()))
            .unwrap_or_else(|_| ("HTTP".into(), "/compat/v1".into()));
        let public_material = json!({
            "method": &method,
            "path": &path,
            "operation": &semantic,
        });
        Submission {
            kind,
            source: MutationSource::Http {
                method,
                path,
                request_id: uuid::Uuid::new_v4().to_string(),
            },
            caller_namespace: "compat.vibelang.v1.http.local".into(),
            idempotency_key: None,
            require_idempotency_key: false,
            retry_epoch: Some(self.handle.mutation_status().runtime_epoch),
            expected_revision: None,
            atomicity: Atomicity::BestEffort,
            supersession: SupersessionPolicy::Fifo,
            material: RequestMaterial::from_values(semantic, Some(public_material)),
        }
    }
}

fn record_http_receipt(receipt: MutationReceipt) {
    let _ = HTTP_REQUEST_CONTEXT.try_with(|context| context.record(receipt));
}

fn http_reply_sink(latest: Arc<Mutex<Option<MutationReceipt>>>) -> MutationReplySink {
    let request_context = HTTP_REQUEST_CONTEXT.try_with(Clone::clone).ok();
    MutationReplySink::new(move |receipt| {
        if let Ok(mut latest) = latest.lock() {
            *latest = Some(receipt.clone());
        }
        if let Some(context) = &request_context {
            context.record(receipt);
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

async fn project_http_mutation_response(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let eval_response = path == "/eval";
    let context = HttpRequestContext::new(method, path);
    let projection_context = context.clone();
    HTTP_REQUEST_CONTEXT
        .scope(context, async move {
            let response = next.run(request).await;
            project_response(response, &projection_context, eval_response).await
        })
        .await
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
    if code == "runtime_fenced" {
        StatusCode::CONFLICT
    } else if code.contains("capability")
        || code.starts_with("queue_")
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
    // Create broadcast channel for WebSocket events
    let (ws_tx, _) = broadcast::channel::<WebSocketEvent>(1024);

    let app_state = Arc::new(AppState {
        handle: handle.clone(),
        state: state.clone(),
        ws_tx: ws_tx.clone(),
        eval_tx,
    });

    // Start the event broadcaster in the background
    let broadcast_state = state.clone();
    let broadcast_handle = handle.clone();
    let broadcast_tx = ws_tx.clone();
    tokio::spawn(async move {
        websocket::run_event_broadcaster(broadcast_state, broadcast_handle, broadcast_tx).await;
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

    // Add shared state and CORS middleware
    let app = app
        .with_state(app_state)
        .layer(middleware::from_fn(project_http_mutation_response))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let addr = SocketAddr::new(bind_addr, port);
    tracing::info!(
        "HTTP API server starting on http://{}:{}",
        addr.ip(),
        addr.port()
    );

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod receipt_projection_tests {
    use super::*;
    use axum::response::IntoResponse;
    use axum::Json;
    use vibelang_core::mutation::{
        Applied, AttemptId, Confirmation, EffectiveAt, EventSequence, FailurePhase, Partial,
        ReceiptTimestamps, RequestIdentity, RevisionId, RollbackState, RuntimeEpoch, Timestamp,
        MUTATION_SCHEMA_VERSION,
    };

    fn receipt(state: ReceiptState, event_sequence: u64) -> MutationReceipt {
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
        let context = HttpRequestContext::new(Method::POST, "/transport/start".into());
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
        let context = HttpRequestContext::new(Method::PATCH, "/transport".into());
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
        let context = HttpRequestContext::new(Method::POST, "/eval".into());
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

    #[tokio::test]
    async fn reply_sink_retains_request_context_for_late_terminal_precedence() {
        let context = HttpRequestContext::new(Method::PATCH, "/transport".into());
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
}
