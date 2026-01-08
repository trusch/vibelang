//! Melodies endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core2::{MelodyId, MelodyMessage};

use crate::{
    models::{ErrorResponse, Melody, MelodyNote, MelodyUpdate, StartRequest, StopRequest},
    AppState,
};

/// Parse a melody ID from a string path parameter.
fn parse_melody_id(id: &str) -> Result<MelodyId, (StatusCode, Json<ErrorResponse>)> {
    id.parse::<u32>()
        .map(MelodyId::new)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request(&format!(
                    "Invalid melody ID '{}': must be a number",
                    id
                ))),
            )
        })
}

/// Convert internal MelodyState to API Melody model
fn melody_to_api(id: &MelodyId, state: &vibelang_core2::MelodyState) -> Melody {
    Melody {
        id: id.raw().to_string(),
        voice_id: state.config.voice.map(|v| v.raw().to_string()).unwrap_or_default(),
        loop_beats: state.config.length.to_f64(),
        notes: state
            .config
            .notes
            .iter()
            .map(|n| MelodyNote {
                beat: n.beat.to_f64(),
                note: n.note,
                velocity: n.velocity,
                gate: n.duration.to_f64(),
                params: std::collections::HashMap::new(),
            })
            .collect(),
        playing: state.playing,
        loop_position: state.loop_position.to_f64(),
    }
}

/// GET /melodies - List all melodies
pub async fn list_melodies(State(state): State<Arc<AppState>>) -> Json<Vec<Melody>> {
    let melodies = state
        .with_state(|s| {
            s.melodies
                .iter()
                .map(|(id, ms)| melody_to_api(id, ms))
                .collect::<Vec<_>>()
        })
        .await;

    Json(melodies)
}

/// GET /melodies/:id - Get melody by ID
pub async fn get_melody(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Melody>, (StatusCode, Json<ErrorResponse>)> {
    let melody_id = parse_melody_id(&id)?;

    let melody = state
        .with_state(|s| {
            s.melodies
                .get(&melody_id)
                .map(|ms| melody_to_api(&melody_id, ms))
        })
        .await;

    match melody {
        Some(m) => Ok(Json(m)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Melody '{}' not found",
                id
            ))),
        )),
    }
}

/// PATCH /melodies/:id - Update melody
///
/// Note: Melody updates (`notes` and `loop_beats`) are not supported via API
/// and require script reload. This endpoint currently returns the current state.
pub async fn update_melody(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<MelodyUpdate>,
) -> Result<Json<Melody>, (StatusCode, Json<ErrorResponse>)> {
    let melody_id = parse_melody_id(&id)?;

    let exists = state
        .with_state(|s| s.melodies.contains_key(&melody_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Melody '{}' not found",
                id
            ))),
        ));
    }

    // Note: notes and loop_beats updates require script reload
    if update.notes.is_some() || update.loop_beats.is_some() {
        tracing::warn!(
            "Melody {} update requested notes or loop_beats change, which requires script reload",
            id
        );
    }

    get_melody(State(state), Path(id)).await
}

/// POST /melodies/:id/start - Start a melody
pub async fn start_melody(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(_req): Json<Option<StartRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let melody_id = parse_melody_id(&id)?;

    let exists = state
        .with_state(|s| s.melodies.contains_key(&melody_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Melody '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(MelodyMessage::Start { id: melody_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to start melody: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /melodies/:id/stop - Stop a melody
pub async fn stop_melody(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(_req): Json<Option<StopRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let melody_id = parse_melody_id(&id)?;

    let exists = state
        .with_state(|s| s.melodies.contains_key(&melody_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Melody '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(MelodyMessage::Stop { id: melody_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to stop melody: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}
