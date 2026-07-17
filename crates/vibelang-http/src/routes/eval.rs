//! Eval endpoint handler for executing Rhai code dynamically.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use vibelang_core::mutation::{
    AttemptId, MutationEventSink, MutationReceipt, MutationReplySink, ReceiptState, Submission,
    TerminalOutcome,
};

use crate::{AppState, ErrorResponse};

/// Request body for code evaluation
#[derive(Debug, Deserialize)]
pub struct EvalRequest {
    /// The Rhai code to evaluate
    pub code: String,
}

/// Response from code evaluation
#[derive(Debug, Serialize)]
pub struct EvalResponse {
    /// Whether parsing/evaluation succeeded and no delivery failure was known before return.
    pub success: bool,
    /// Fixed compatibility scope for the legacy boolean. This is never an applied claim.
    pub legacy_success_scope: &'static str,
    /// Result value (if any)
    pub result: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Canonical queue/runtime truth when evaluation reached mutation submission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<MutationReceipt>,
}

/// Internal request sent to the main thread for evaluation
pub struct EvalJob {
    pub code: String,
    pub submission: Submission,
    pub latest_receipt: Arc<Mutex<Option<MutationReceipt>>>,
    pub reply_sink: MutationReplySink,
    pub event_sink: MutationEventSink,
    pub response_tx: oneshot::Sender<EvalResult>,
}

/// Result of code evaluation
pub struct EvalResult {
    /// Evaluation-only truth; the handler combines this with the canonical receipt.
    pub success: bool,
    pub result: Option<String>,
    pub error: Option<String>,
    pub receipt: Option<MutationReceipt>,
}

/// GET /receipts/{attempt_id} - Read the latest canonical receipt.
pub async fn get_receipt(
    State(state): State<Arc<AppState>>,
    Path(attempt_id): Path<String>,
) -> Result<Json<MutationReceipt>, (StatusCode, Json<ErrorResponse>)> {
    let attempt_id = AttemptId::parse(&attempt_id).map_err(|message| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("invalid_attempt_id", &message)),
        )
    })?;
    state
        .handle
        .mutation_receipt(attempt_id)
        .map(Json)
        .map_err(|error| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("receipt_not_found", &error.to_string())),
            )
        })
}

/// POST /eval - Evaluate Rhai code
pub async fn eval_code(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EvalRequest>,
) -> (StatusCode, Json<EvalResponse>) {
    // Check if eval channel is available
    let eval_tx = match &state.eval_tx {
        Some(tx) => tx,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(EvalResponse {
                    success: false,
                    legacy_success_scope: "evaluation_only",
                    result: None,
                    error: Some("Eval not available in this mode".to_string()),
                    receipt: None,
                }),
            );
        }
    };

    // Create a oneshot channel for the response
    let (response_tx, response_rx) = oneshot::channel();

    // Send the eval job to the main thread
    let submission = state.eval_submission(&req.code);
    let (latest_receipt, reply_sink, event_sink) = state.mutation_sinks();
    let job = EvalJob {
        code: req.code,
        submission,
        latest_receipt: Arc::clone(&latest_receipt),
        reply_sink,
        event_sink,
        response_tx,
    };

    if eval_tx.send(job).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(EvalResponse {
                success: false,
                legacy_success_scope: "evaluation_only",
                result: None,
                error: Some("Failed to send eval request".to_string()),
                receipt: None,
            }),
        );
    }

    // Wait for the result
    match response_rx.await {
        Ok(result) => {
            let observed = latest_receipt.lock().ok().and_then(|latest| latest.clone());
            let receipt = crate::canonical_latest_receipt(result.receipt, observed);
            let receipt_failed = receipt.as_ref().is_some_and(|receipt| {
                matches!(
                    receipt.state,
                    ReceiptState::Terminal(TerminalOutcome::Rejected(_))
                        | ReceiptState::Terminal(TerminalOutcome::Superseded(_))
                        | ReceiptState::Terminal(TerminalOutcome::Partial(_))
                )
            });
            let delivery_missing = result.success && receipt.is_none();
            let success = result.success && !receipt_failed && !delivery_missing;
            let status = if !result.success {
                StatusCode::BAD_REQUEST
            } else if let Some(receipt) = &receipt {
                eval_receipt_status(receipt)
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(EvalResponse {
                    success,
                    legacy_success_scope: "evaluation_only",
                    result: success.then_some(result.result).flatten(),
                    error: if receipt_failed && result.error.is_none() {
                        receipt.as_ref().and_then(receipt_error)
                    } else {
                        result.error
                    },
                    receipt,
                }),
            )
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(EvalResponse {
                success: false,
                legacy_success_scope: "evaluation_only",
                result: None,
                error: Some("Eval request cancelled".to_string()),
                receipt: latest_receipt.lock().ok().and_then(|latest| latest.clone()),
            }),
        ),
    }
}

fn eval_receipt_status(receipt: &MutationReceipt) -> StatusCode {
    match &receipt.state {
        ReceiptState::Evaluating { .. }
        | ReceiptState::Accepted { .. }
        | ReceiptState::Planning
        | ReceiptState::Staging { .. }
        | ReceiptState::Committing { .. } => StatusCode::ACCEPTED,
        ReceiptState::Terminal(TerminalOutcome::Applied(_)) => StatusCode::OK,
        ReceiptState::Terminal(TerminalOutcome::Superseded(_))
        | ReceiptState::Terminal(TerminalOutcome::Partial(_)) => StatusCode::CONFLICT,
        ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) => {
            crate::rejected_http_status(&rejected.code)
        }
    }
}

fn receipt_error(receipt: &MutationReceipt) -> Option<String> {
    match &receipt.state {
        ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) => {
            Some(format!("{}: {}", rejected.code, rejected.message))
        }
        ReceiptState::Terminal(TerminalOutcome::Superseded(superseded)) => {
            Some(format!("superseded: {:?}", superseded.reason))
        }
        ReceiptState::Terminal(TerminalOutcome::Partial(partial)) => {
            Some(format!("{}: mutation is partial", partial.code))
        }
        _ => None,
    }
}
