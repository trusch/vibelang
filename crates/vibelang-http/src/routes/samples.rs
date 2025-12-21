//! Samples endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::state::StateMessage;

use crate::{
    models::{ErrorResponse, Sample, SampleLoad, SampleSlice},
    AppState,
};

/// Convert internal SampleInfo to API Sample model
fn sample_to_api(si: &vibelang_core::state::SampleInfo) -> Sample {
    let slices = si.slices.iter().map(|s| SampleSlice {
        index: s.index,
        start_frame: s.start_frame,
        end_frame: s.end_frame,
        synthdef_name: s.synthdef_name.clone(),
    }).collect();

    Sample {
        id: si.id.clone(),
        path: si.path.clone(),
        buffer_id: si.buffer_id,
        num_channels: si.num_channels,
        num_frames: si.num_frames,
        sample_rate: si.sample_rate,
        synthdef_name: si.synthdef_name.clone(),
        slices,
    }
}

/// GET /samples - List all loaded samples
pub async fn list_samples(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Sample>> {
    let samples = state.handle.with_state(|s| {
        s.samples.values().map(sample_to_api).collect::<Vec<_>>()
    });

    Json(samples)
}

/// POST /samples - Load a new sample
pub async fn load_sample(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SampleLoad>,
) -> Result<(StatusCode, Json<Sample>), (StatusCode, Json<ErrorResponse>)> {
    // Generate ID from filename if not provided
    let id = req.id.unwrap_or_else(|| {
        std::path::Path::new(&req.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "sample".to_string())
    });

    // Check if sample already exists
    let exists = state.handle.with_state(|s| s.samples.contains_key(&id));
    if exists {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::conflict(&format!("Sample '{}' already exists", id))),
        ));
    }

    // Load the sample
    if let Err(e) = state.handle.send(StateMessage::LoadSample {
        id: id.clone(),
        path: req.path.clone(),
        resolved_path: None,
        analyze_bpm: false,
        warp_to_bpm: None,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to load sample: {}", e))),
        ));
    }

    // Wait a bit for the sample to load (sample loading is async)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Return the loaded sample
    let sample = state.handle.with_state(|s| s.samples.get(&id).map(sample_to_api));

    match sample {
        Some(s) => Ok((StatusCode::CREATED, Json(s))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal("Sample load message sent but sample not found in state (may still be loading)")),
        )),
    }
}

/// GET /samples/:id - Get sample by ID
pub async fn get_sample(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Sample>, (StatusCode, Json<ErrorResponse>)> {
    let sample = state.handle.with_state(|s| s.samples.get(&id).map(sample_to_api));

    match sample {
        Some(s) => Ok(Json(s)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Sample '{}' not found", id))),
        )),
    }
}

/// DELETE /samples/:id - Free a sample
pub async fn free_sample(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.samples.contains_key(&id));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Sample '{}' not found", id))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::FreeSample { id: id.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to free sample: {}", e))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
