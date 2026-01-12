//! Samples endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::path::PathBuf;
use std::sync::Arc;
use vibelang_core2::{SampleConfig, SampleId, SampleMessage};

use crate::{
    models::{ErrorResponse, Sample, SampleLoad},
    AppState,
};

/// Parse a sample ID from a string path parameter.
fn parse_sample_id(id: &str) -> Result<SampleId, (StatusCode, Json<ErrorResponse>)> {
    id.parse::<u32>()
        .map(SampleId::new)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request(&format!(
                    "Invalid sample ID '{}': must be a number",
                    id
                ))),
            )
        })
}

/// Convert internal SampleInfo to API Sample model
fn sample_to_api(id: &SampleId, info: &vibelang_core2::SampleInfo) -> Sample {
    // The synthdef name follows the convention "sample_{id}"
    let synthdef_name = format!("sample_{}", id.raw());

    Sample {
        id: id.raw().to_string(),
        path: info.path.to_string_lossy().to_string(),
        buffer_id: info.buffer_id.raw() as i32,
        num_channels: info.channels as i32,
        num_frames: (info.duration_secs * info.sample_rate) as i64,
        sample_rate: info.sample_rate as f32,
        synthdef_name,
        slices: None, // Slicing not currently implemented in core
    }
}

/// GET /samples - List all samples
pub async fn list_samples(State(state): State<Arc<AppState>>) -> Json<Vec<Sample>> {
    let samples = state
        .with_state(|s| {
            s.samples
                .iter()
                .map(|(id, info)| sample_to_api(id, info))
                .collect::<Vec<_>>()
        })
        .await;

    Json(samples)
}

/// GET /samples/:id - Get sample by ID
pub async fn get_sample(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Sample>, (StatusCode, Json<ErrorResponse>)> {
    let sample_id = parse_sample_id(&id)?;

    let sample = state
        .with_state(|s| s.samples.get(&sample_id).map(|info| sample_to_api(&sample_id, info)))
        .await;

    match sample {
        Some(s) => Ok(Json(s)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Sample '{}' not found",
                id
            ))),
        )),
    }
}

/// POST /samples - Load a new sample
pub async fn load_sample(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SampleLoad>,
) -> Result<(StatusCode, Json<Sample>), (StatusCode, Json<ErrorResponse>)> {
    // Parse or generate sample ID
    let sample_id = if let Some(id_str) = &req.id {
        id_str.parse::<u32>()
            .map(SampleId::new)
            .map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::bad_request(&format!(
                        "Invalid sample ID '{}': must be a number",
                        id_str
                    ))),
                )
            })?
    } else {
        // Generate a new ID
        state
            .with_state(|s| {
                let max_id = s.samples.keys().map(|id| id.raw()).max().unwrap_or(0);
                SampleId::new(max_id + 1)
            })
            .await
    };

    // Check if sample already exists
    let exists = state.with_state(|s| s.samples.contains_key(&sample_id)).await;
    if exists {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new("conflict", &format!(
                "Sample '{}' already exists",
                sample_id.raw()
            ))),
        ));
    }

    // Build sample config
    let config = SampleConfig::new(PathBuf::from(&req.path));

    // Send load message
    if let Err(e) = state
        .send(SampleMessage::Load { id: sample_id, config }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to load sample: {}",
                e
            ))),
        ));
    }

    // Wait for state to update, then fetch
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let sample = state
        .with_state(|s| s.samples.get(&sample_id).map(|info| sample_to_api(&sample_id, info)))
        .await;

    match sample {
        Some(s) => Ok((StatusCode::CREATED, Json(s))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal("Sample load sent but not found in state")),
        )),
    }
}

/// DELETE /samples/:id - Delete a sample
pub async fn delete_sample(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let sample_id = parse_sample_id(&id)?;

    let exists = state.with_state(|s| s.samples.contains_key(&sample_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Sample '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(SampleMessage::Free { id: sample_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to delete sample: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
