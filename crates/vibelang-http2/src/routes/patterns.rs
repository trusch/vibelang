//! Patterns endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core2::{PatternId, PatternMessage};

use crate::{
    models::{ErrorResponse, Pattern, PatternStep, PatternUpdate, StartRequest, StopRequest},
    AppState,
};

/// Parse a pattern ID from a string path parameter.
fn parse_pattern_id(id: &str) -> Result<PatternId, (StatusCode, Json<ErrorResponse>)> {
    id.parse::<u32>()
        .map(PatternId::new)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request(&format!(
                    "Invalid pattern ID '{}': must be a number",
                    id
                ))),
            )
        })
}

/// Convert internal PatternState to API Pattern model
fn pattern_to_api(id: &PatternId, state: &vibelang_core2::PatternState) -> Pattern {
    Pattern {
        id: id.raw().to_string(),
        voice_id: state.config.voice.map(|v| v.raw().to_string()).unwrap_or_default(),
        loop_beats: state.config.length.to_f64(),
        steps: state
            .config
            .steps
            .iter()
            .map(|s| PatternStep {
                beat: s.beat.to_f64(),
                params: s.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            })
            .collect(),
        playing: state.playing,
        loop_position: state.loop_position.to_f64(),
    }
}

/// GET /patterns - List all patterns
pub async fn list_patterns(State(state): State<Arc<AppState>>) -> Json<Vec<Pattern>> {
    let patterns = state
        .with_state(|s| {
            s.patterns
                .iter()
                .map(|(id, ps)| pattern_to_api(id, ps))
                .collect::<Vec<_>>()
        })
        .await;

    Json(patterns)
}

/// GET /patterns/:id - Get pattern by ID
pub async fn get_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Pattern>, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = parse_pattern_id(&id)?;

    let pattern = state
        .with_state(|s| {
            s.patterns
                .get(&pattern_id)
                .map(|ps| pattern_to_api(&pattern_id, ps))
        })
        .await;

    match pattern {
        Some(p) => Ok(Json(p)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Pattern '{}' not found",
                id
            ))),
        )),
    }
}

/// PATCH /patterns/:id - Update pattern
///
/// Note: `steps` and `loop_beats` updates are not supported via API and require
/// script reload. Only `params` can be updated at runtime, which sets the param
/// value on all pattern steps.
pub async fn update_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<PatternUpdate>,
) -> Result<Json<Pattern>, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = parse_pattern_id(&id)?;

    let exists = state
        .with_state(|s| s.patterns.contains_key(&pattern_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Pattern '{}' not found",
                id
            ))),
        ));
    }

    // Apply param updates (sets param on all steps)
    for (param, value) in update.params {
        if let Err(e) = state
            .send(
                PatternMessage::SetParam {
                    id: pattern_id,
                    param,
                    value,
                }
                .into(),
            )
            .await
        {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!(
                    "Failed to update pattern params: {}",
                    e
                ))),
            ));
        }
    }

    // Note: steps and loop_beats updates require script reload
    if update.steps.is_some() || update.loop_beats.is_some() {
        tracing::warn!(
            "Pattern {} update requested steps or loop_beats change, which requires script reload",
            id
        );
    }

    get_pattern(State(state), Path(id)).await
}

/// POST /patterns/:id/start - Start a pattern
pub async fn start_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(_req): Json<Option<StartRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = parse_pattern_id(&id)?;

    let exists = state
        .with_state(|s| s.patterns.contains_key(&pattern_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Pattern '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(PatternMessage::Start { id: pattern_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to start pattern: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /patterns/:id/stop - Stop a pattern
pub async fn stop_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(_req): Json<Option<StopRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = parse_pattern_id(&id)?;

    let exists = state
        .with_state(|s| s.patterns.contains_key(&pattern_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Pattern '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(PatternMessage::Stop { id: pattern_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to stop pattern: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}
