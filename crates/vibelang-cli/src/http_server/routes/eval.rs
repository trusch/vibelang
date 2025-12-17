//! Eval endpoint handler for executing Rhai code dynamically.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::http_server::AppState;

/// Request body for code evaluation
#[derive(Debug, Deserialize)]
pub struct EvalRequest {
    /// The Rhai code to evaluate
    pub code: String,
}

/// Response from code evaluation
#[derive(Debug, Serialize)]
pub struct EvalResponse {
    /// Whether evaluation succeeded
    pub success: bool,
    /// Result value (if any)
    pub result: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Internal request sent to the main thread for evaluation
pub struct EvalJob {
    pub code: String,
    pub response_tx: oneshot::Sender<EvalResult>,
}

/// Result of code evaluation
pub struct EvalResult {
    pub success: bool,
    pub result: Option<String>,
    pub error: Option<String>,
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
                    result: None,
                    error: Some("Eval not available in this mode".to_string()),
                }),
            );
        }
    };

    // Create a oneshot channel for the response
    let (response_tx, response_rx) = oneshot::channel();

    // Send the eval job to the main thread
    let job = EvalJob {
        code: req.code,
        response_tx,
    };

    if eval_tx.send(job).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(EvalResponse {
                success: false,
                result: None,
                error: Some("Failed to send eval request".to_string()),
            }),
        );
    }

    // Wait for the result
    match response_rx.await {
        Ok(result) => (
            if result.success { StatusCode::OK } else { StatusCode::BAD_REQUEST },
            Json(EvalResponse {
                success: result.success,
                result: result.result,
                error: result.error,
            }),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(EvalResponse {
                success: false,
                result: None,
                error: Some("Eval request cancelled".to_string()),
            }),
        ),
    }
}
