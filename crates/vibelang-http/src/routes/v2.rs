use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use vibelang_core::mutation::{
    AttemptId, CancelResult, EventQueryResult, EventSequence, RuntimeEpoch,
};

use crate::{
    models::{HttpErrorDetail, HttpErrorEnvelope, HTTP_V2_SCHEMA_VERSION},
    AppState,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptEventsQuery {
    pub runtime_epoch: String,
    pub after_event_sequence: Option<u64>,
}

pub async fn cancel_receipt(
    State(state): State<Arc<AppState>>,
    Path(attempt_id): Path<String>,
) -> Response {
    let attempt_id = match AttemptId::parse(&attempt_id) {
        Ok(attempt_id) => attempt_id,
        Err(message) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "receipt.cancel",
                "invalid_attempt_id",
                message,
                Some("attempt_id"),
            );
        }
    };
    let previous = state.handle.mutation_status().event_sequence;
    match state.handle.cancel_mutation(attempt_id) {
        CancelResult::Receipt(receipt) => {
            publish_events_after(&state, receipt.runtime_epoch, previous);
            crate::record_http_receipt((*receipt).clone());
            Json(*receipt).into_response()
        }
        CancelResult::Rejected(error) => {
            let status = if error.code == "receipt_not_found" {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::CONFLICT
            };
            let envelope = HttpErrorEnvelope {
                schema_version: HTTP_V2_SCHEMA_VERSION,
                operation: "receipt.cancel".into(),
                error: HttpErrorDetail {
                    code: error.code,
                    message: error.message,
                    field: Some("attempt_id".into()),
                    reason: None,
                    supported_values: Vec::new(),
                },
                receipt: None,
            };
            (status, Json(envelope)).into_response()
        }
    }
}

pub async fn mutation_status(State(state): State<Arc<AppState>>) -> Response {
    Json(state.handle.mutation_status()).into_response()
}

pub async fn capabilities(State(state): State<Arc<AppState>>) -> Response {
    Json(state.http_capabilities()).into_response()
}

pub async fn capability_details(State(state): State<Arc<AppState>>) -> Response {
    Json(state.http_capability_details()).into_response()
}

pub async fn receipt_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ReceiptEventsQuery>,
) -> Response {
    let epoch = match RuntimeEpoch::parse(&query.runtime_epoch) {
        Ok(epoch) => epoch,
        Err(message) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "receipt.events",
                "invalid_runtime_epoch",
                message,
                Some("runtime_epoch"),
            );
        }
    };
    let after = match query.after_event_sequence.map(EventSequence::new) {
        Some(Ok(sequence)) => Some(sequence),
        Some(Err(message)) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "receipt.events",
                "invalid_event_sequence",
                message,
                Some("after_event_sequence"),
            );
        }
        None => None,
    };
    let result = match state.handle.mutation_events_after(epoch, after) {
        EventQueryResult::Events { events } => EventQueryResult::Events {
            events: events
                .into_iter()
                .map(|event| state.public_receipt_event(event))
                .collect(),
        },
        reset @ EventQueryResult::ResetRequired { .. } => reset,
    };
    Json(result).into_response()
}

fn publish_events_after(state: &AppState, epoch: RuntimeEpoch, after: Option<EventSequence>) {
    if let EventQueryResult::Events { events } = state.handle.mutation_events_after(epoch, after) {
        for event in events {
            state.publish_receipt_event(event);
        }
    }
}

fn error_response(
    status: StatusCode,
    operation: &str,
    code: &str,
    message: impl Into<String>,
    field: Option<&str>,
) -> Response {
    (
        status,
        Json(HttpErrorEnvelope {
            schema_version: HTTP_V2_SCHEMA_VERSION,
            operation: operation.into(),
            error: HttpErrorDetail {
                code: code.into(),
                message: message.into(),
                field: field.map(str::to_string),
                reason: None,
                supported_values: Vec::new(),
            },
            receipt: None,
        }),
    )
        .into_response()
}
