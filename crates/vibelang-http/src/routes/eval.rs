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
    AttemptId, FailurePhase, MutationReceipt, ReceiptState, TerminalOutcome,
};
use vibelang_core::MutationAttempt;

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
    /// Canonical truth for the attempt allocated before evaluation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<MutationReceipt>,
}

/// Internal request sent to the main thread for evaluation
pub struct EvalJob {
    pub code: String,
    pub attempt: MutationAttempt,
    pub latest_receipt: Arc<Mutex<Option<MutationReceipt>>>,
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
    let attempt = match state
        .handle
        .begin_attempt(submission, reply_sink, event_sink)
    {
        Ok(attempt) => attempt,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(EvalResponse {
                    success: false,
                    legacy_success_scope: "evaluation_only",
                    result: None,
                    error: Some(error.to_string()),
                    receipt: None,
                }),
            );
        }
    };
    if !attempt.is_active() {
        let receipt = attempt.receipt().clone();
        return (
            eval_receipt_status(&receipt),
            Json(EvalResponse {
                success: false,
                legacy_success_scope: "evaluation_only",
                result: None,
                error: receipt_error(&receipt),
                receipt: Some(receipt),
            }),
        );
    }
    let job = EvalJob {
        code: req.code,
        attempt,
        latest_receipt: Arc::clone(&latest_receipt),
        response_tx,
    };

    if let Err(error) = eval_tx.send(job) {
        let receipt = state
            .handle
            .finish_attempt_failure(
                error.0.attempt,
                FailurePhase::Admission,
                "eval_dispatch_failed",
                "the HTTP evaluation worker is unavailable",
            )
            .ok();
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(EvalResponse {
                success: false,
                legacy_success_scope: "evaluation_only",
                result: None,
                error: Some("Failed to send eval request".to_string()),
                receipt,
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::path::Path as FsPath;
    use std::sync::mpsc;
    use std::time::Duration;
    use tokio::sync::broadcast;
    use vibelang_core::compat::Instant;
    use vibelang_core::mutation::{ComponentState, RollbackState};
    use vibelang_core::{
        AddAction, Backend, BufferId, BufferInfo, NodeId, ParamMap, ReloadMessage, Runtime,
    };

    #[derive(Debug)]
    struct CarrierBackendError;

    impl std::fmt::Display for CarrierBackendError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("carrier backend error")
        }
    }

    impl std::error::Error for CarrierBackendError {}

    struct CarrierBackend;

    #[async_trait]
    impl Backend for CarrierBackend {
        type Error = CarrierBackendError;

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

    fn app_state(
        runtime: &Runtime<CarrierBackend>,
        eval_tx: mpsc::Sender<EvalJob>,
    ) -> Arc<AppState> {
        let (ws_tx, _) = broadcast::channel(16);
        Arc::new(AppState {
            handle: runtime.handle(),
            state: Arc::clone(runtime.state()),
            ws_tx,
            eval_tx: Some(eval_tx),
        })
    }

    fn recv_job(receiver: &mpsc::Receiver<EvalJob>) -> EvalJob {
        receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("eval route should dispatch one preallocated job")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn http_eval_success_preserves_preallocated_attempt_through_admission() {
        let runtime = Runtime::new(CarrierBackend);
        let (sender, receiver) = mpsc::channel();
        let state = app_state(&runtime, sender);
        let handle = state.handle.clone();
        let request = tokio::spawn(eval_code(
            State(state),
            Json(EvalRequest {
                code: "set_tempo(145);".into(),
            }),
        ));
        let EvalJob {
            attempt,
            response_tx,
            ..
        } = recv_job(&receiver);
        let attempt_id = attempt.receipt().attempt_id;
        assert!(attempt.is_active());
        assert!(attempt.receipt().revision.is_none());
        let accepted = handle
            .submit_attempt(
                ReloadMessage::Apply {
                    state: vibelang_core::reload::ScriptState::default(),
                }
                .into(),
                attempt,
            )
            .await
            .unwrap();
        assert!(response_tx
            .send(EvalResult {
                success: true,
                result: Some("Code evaluated and submitted".into()),
                error: None,
                receipt: Some(accepted.clone()),
            })
            .is_ok());

        let (status, Json(response)) = request.await.unwrap();
        assert_eq!(status, StatusCode::ACCEPTED);
        assert!(response.success);
        assert_eq!(response.receipt.as_ref().unwrap().attempt_id, attempt_id);
        assert_eq!(response.receipt, Some(accepted));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn http_eval_parse_failure_is_effect_free_rejected_attempt() {
        let runtime = Runtime::new(CarrierBackend);
        let (sender, receiver) = mpsc::channel();
        let state = app_state(&runtime, sender);
        let handle = state.handle.clone();
        let request = tokio::spawn(eval_code(
            State(state),
            Json(EvalRequest {
                code: "let = ;".into(),
            }),
        ));
        let EvalJob {
            attempt,
            response_tx,
            ..
        } = recv_job(&receiver);
        let attempt_id = attempt.receipt().attempt_id;
        let receipt = handle
            .finish_attempt_failure(
                attempt,
                FailurePhase::Parse,
                "script_parse_failed",
                "unexpected token",
            )
            .unwrap();
        assert!(response_tx
            .send(EvalResult {
                success: false,
                result: None,
                error: Some("unexpected token".into()),
                receipt: Some(receipt),
            })
            .is_ok());

        let (status, Json(response)) = request.await.unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let receipt = response.receipt.unwrap();
        assert_eq!(receipt.attempt_id, attempt_id);
        assert!(receipt.revision.is_none());
        let ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) = receipt.state else {
            panic!("effect-free parse failure must be rejected");
        };
        assert_eq!(rejected.phase, FailurePhase::Parse);
        assert_eq!(rejected.code, "script_parse_failed");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn http_eval_eager_failure_is_fenced_partial() {
        let runtime = Runtime::new(CarrierBackend);
        let (sender, receiver) = mpsc::channel();
        let state = app_state(&runtime, sender);
        let handle = state.handle.clone();
        let request = tokio::spawn(eval_code(
            State(state),
            Json(EvalRequest {
                code: "write_file(\"out\", \"data\"); throw \"stop\";".into(),
            }),
        ));
        let EvalJob {
            mut attempt,
            response_tx,
            ..
        } = recv_job(&receiver);
        let attempt_id = attempt.receipt().attempt_id;
        attempt
            .record_uncertain_effect(
                "http/eval/evaluation",
                "evaluate",
                "script_evaluation_failed",
                "extension failed after writing output",
            )
            .unwrap();
        let receipt = handle
            .finish_attempt_failure(
                attempt,
                FailurePhase::Evaluate,
                "script_evaluation_failed",
                "extension failed after writing output",
            )
            .unwrap();
        assert!(response_tx
            .send(EvalResult {
                success: false,
                result: None,
                error: Some("extension failed after writing output".into()),
                receipt: Some(receipt),
            })
            .is_ok());

        let (_, Json(response)) = request.await.unwrap();
        let receipt = response.receipt.unwrap();
        assert_eq!(receipt.attempt_id, attempt_id);
        assert!(receipt.revision.is_none());
        let ReceiptState::Terminal(TerminalOutcome::Partial(partial)) = receipt.state else {
            panic!("failure after eager HTTP evaluation must be partial");
        };
        assert!(partial.fenced);
        assert_eq!(partial.phase, FailurePhase::Evaluate);
        assert_eq!(partial.code, "script_evaluation_failed");
        assert_eq!(partial.components[0].state, ComponentState::Uncertain);
    }

    #[tokio::test]
    async fn http_eval_dispatch_failure_terminalizes_allocated_attempt() {
        let runtime = Runtime::new(CarrierBackend);
        let (sender, receiver) = mpsc::channel();
        drop(receiver);
        let state = app_state(&runtime, sender);

        let (status, Json(response)) = eval_code(
            State(state),
            Json(EvalRequest {
                code: "set_tempo(120);".into(),
            }),
        )
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let receipt = response
            .receipt
            .expect("dispatch failure must retain its canonical receipt");
        assert!(receipt.revision.is_none());
        let ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) = receipt.state else {
            panic!("effect-free dispatch failure must be rejected");
        };
        assert_eq!(rejected.phase, FailurePhase::Admission);
        assert_eq!(rejected.code, "eval_dispatch_failed");
        assert_eq!(rejected.rollback, RollbackState::NotNeeded);
    }
}
