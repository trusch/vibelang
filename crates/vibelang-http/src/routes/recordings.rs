//! Recording read/control endpoint handlers.
//!
//! Native-only — recordings require a real audio backend.

#![cfg(not(target_arch = "wasm32"))]

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::message::RecordingMessage;
use vibelang_core::traits::{RecordingInfo as CoreRecordingInfo, RecordingStatus};
use vibelang_core::types::ids::RecordingId;

use crate::{
    models::{ErrorResponse, Recording, RecordingStatus as ApiRecordingStatus},
    AppState,
};

fn parse_recording_id(id: &str) -> Result<RecordingId, (StatusCode, Json<ErrorResponse>)> {
    id.parse::<u32>().map(RecordingId::new).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::bad_request(&format!(
                "Invalid recording ID '{}': must be a number",
                id
            ))),
        )
    })
}

fn status_to_api(status: &RecordingStatus) -> ApiRecordingStatus {
    match status {
        RecordingStatus::Pending => ApiRecordingStatus::Pending,
        RecordingStatus::CountingIn => ApiRecordingStatus::CountingIn,
        RecordingStatus::Recording => ApiRecordingStatus::Recording,
        RecordingStatus::Completed => ApiRecordingStatus::Completed,
        RecordingStatus::Cancelled => ApiRecordingStatus::Cancelled,
    }
}

fn recording_to_api(info: &CoreRecordingInfo, tempo: f64) -> Recording {
    let duration_secs = if info.length_beats.is_some() || info.length_seconds.is_some() {
        Some(info.duration_secs(tempo))
    } else {
        None
    };
    let path = if matches!(info.status, RecordingStatus::Completed) {
        info.file_path.as_ref().map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    };
    Recording {
        id: info.id.raw().to_string(),
        status: status_to_api(&info.status),
        duration_secs,
        path,
    }
}

/// GET /recordings — list all recordings with their status
pub async fn list_recordings(State(state): State<Arc<AppState>>) -> Json<Vec<Recording>> {
    let recordings = state
        .with_state(|s| {
            let tempo = s.tempo;
            s.recordings
                .values()
                .map(|info| recording_to_api(info, tempo))
                .collect::<Vec<_>>()
        })
        .await;

    Json(recordings)
}

/// GET /recordings/:id — get a single recording
pub async fn get_recording(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Recording>, (StatusCode, Json<ErrorResponse>)> {
    let rec_id = parse_recording_id(&id)?;

    let recording = state
        .with_state(|s| {
            let tempo = s.tempo;
            s.recordings.get(&rec_id).map(|info| recording_to_api(info, tempo))
        })
        .await;

    match recording {
        Some(r) => Ok(Json(r)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Recording '{}' not found",
                id
            ))),
        )),
    }
}

/// POST /recordings/:id/stop — stop a recording early (finalize and save)
pub async fn stop_recording(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let rec_id = parse_recording_id(&id)?;

    let exists = state
        .with_state(|s| s.recordings.contains_key(&rec_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Recording '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(RecordingMessage::Stop { id: rec_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to stop recording: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /recordings/:id/cancel — cancel and discard a recording
pub async fn cancel_recording(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let rec_id = parse_recording_id(&id)?;

    let exists = state
        .with_state(|s| s.recordings.contains_key(&rec_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Recording '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(RecordingMessage::Cancel { id: rec_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to cancel recording: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
