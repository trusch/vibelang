//! Sequences endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core2::{Clip, SequenceId, SequenceMessage};

use crate::{
    models::{ErrorResponse, Sequence, SequenceClip, SequenceStartRequest, SequenceUpdate},
    AppState,
};

/// Parse a sequence ID from a string path parameter.
fn parse_sequence_id(id: &str) -> Result<SequenceId, (StatusCode, Json<ErrorResponse>)> {
    id.parse::<u32>()
        .map(SequenceId::new)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request(&format!(
                    "Invalid sequence ID '{}': must be a number",
                    id
                ))),
            )
        })
}

/// Convert a Clip enum to API model
fn clip_to_api(clip: &Clip) -> SequenceClip {
    match clip {
        Clip::Pattern { id, start, end } => SequenceClip {
            clip_type: "pattern".to_string(),
            target_id: id.raw().to_string(),
            start_beat: start.to_f64(),
            end_beat: end.to_f64(),
        },
        Clip::Melody { id, start, end } => SequenceClip {
            clip_type: "melody".to_string(),
            target_id: id.raw().to_string(),
            start_beat: start.to_f64(),
            end_beat: end.to_f64(),
        },
        Clip::Fade { start, .. } => SequenceClip {
            clip_type: "fade".to_string(),
            target_id: String::new(),
            start_beat: start.to_f64(),
            end_beat: start.to_f64(),
        },
        Clip::Sequence { id, start } => SequenceClip {
            clip_type: "sequence".to_string(),
            target_id: id.raw().to_string(),
            start_beat: start.to_f64(),
            end_beat: start.to_f64(),
        },
    }
}

/// Convert internal SequenceState to API Sequence model
fn sequence_to_api(id: &SequenceId, state: &vibelang_core2::SequenceState) -> Sequence {
    Sequence {
        id: id.raw().to_string(),
        loop_beats: state.config.length.to_f64(),
        clips: state.config.clips.iter().map(clip_to_api).collect(),
        playing: state.playing,
        paused: state.paused,
        looping: state.looping,
        position: state.position.to_f64(),
    }
}

/// GET /sequences - List all sequences
pub async fn list_sequences(State(state): State<Arc<AppState>>) -> Json<Vec<Sequence>> {
    let sequences = state
        .with_state(|s| {
            s.sequences
                .iter()
                .map(|(id, ss)| sequence_to_api(id, ss))
                .collect::<Vec<_>>()
        })
        .await;

    Json(sequences)
}

/// GET /sequences/:id - Get sequence by ID
pub async fn get_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Sequence>, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = parse_sequence_id(&id)?;

    let sequence = state
        .with_state(|s| {
            s.sequences
                .get(&sequence_id)
                .map(|ss| sequence_to_api(&sequence_id, ss))
        })
        .await;

    match sequence {
        Some(seq) => Ok(Json(seq)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Sequence '{}' not found",
                id
            ))),
        )),
    }
}

/// PATCH /sequences/:id - Update sequence
///
/// Note: Sequence updates (`clips` and `loop_beats`) are not supported via API
/// and require script reload. This endpoint currently returns the current state.
pub async fn update_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<SequenceUpdate>,
) -> Result<Json<Sequence>, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = parse_sequence_id(&id)?;

    let exists = state
        .with_state(|s| s.sequences.contains_key(&sequence_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Sequence '{}' not found",
                id
            ))),
        ));
    }

    // Note: clips and loop_beats updates require script reload
    if update.clips.is_some() || update.loop_beats.is_some() {
        tracing::warn!(
            "Sequence {} update requested clips or loop_beats change, which requires script reload",
            id
        );
    }

    get_sequence(State(state), Path(id)).await
}

/// POST /sequences/:id/start - Start a sequence
pub async fn start_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<Option<SequenceStartRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = parse_sequence_id(&id)?;

    let exists = state
        .with_state(|s| s.sequences.contains_key(&sequence_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Sequence '{}' not found",
                id
            ))),
        ));
    }

    let looping = !req.map(|r| r.play_once).unwrap_or(false);

    if let Err(e) = state
        .send(
            SequenceMessage::Start {
                id: sequence_id,
                looping,
            }
            .into(),
        )
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to start sequence: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /sequences/:id/stop - Stop a sequence
pub async fn stop_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = parse_sequence_id(&id)?;

    let exists = state
        .with_state(|s| s.sequences.contains_key(&sequence_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Sequence '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(SequenceMessage::Stop { id: sequence_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to stop sequence: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /sequences/:id/pause - Pause a sequence
pub async fn pause_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = parse_sequence_id(&id)?;

    let exists = state
        .with_state(|s| s.sequences.contains_key(&sequence_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Sequence '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(SequenceMessage::Pause { id: sequence_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to pause sequence: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}
