//! Patterns endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::{
    traits::{PatternConfig, Step},
    types::Beat,
    PatternId, PatternMessage, PatternOwner, VoiceId,
};

use crate::{
    models::{
        ErrorResponse, LoopState, LoopStatus, Pattern, PatternCreate, PatternEvent, PatternUpdate,
        StartRequest, StopRequest,
    },
    AppState,
};

/// Resolve a pattern identifier (either numeric ID or string name) to a PatternId.
async fn resolve_pattern_id(
    state: &Arc<AppState>,
    identifier: &str,
) -> Result<PatternId, (StatusCode, Json<ErrorResponse>)> {
    // First, try to parse as a numeric ID
    if let Ok(num_id) = identifier.parse::<u32>() {
        let pattern_id = PatternId::new(num_id);
        let exists = state
            .with_state(|s| s.patterns.contains_key(&pattern_id))
            .await;
        if exists {
            return Ok(pattern_id);
        }
        // Fall through to try as name if numeric ID not found
    }

    // Try to find by name
    let found = state
        .with_state(|s| {
            s.patterns
                .iter()
                .find(|(_, ps)| ps.content.name == identifier)
                .map(|(id, _)| *id)
        })
        .await;

    match found {
        Some(id) => Ok(id),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Pattern '{}' not found",
                identifier
            ))),
        )),
    }
}

/// Convert internal PatternState to API Pattern model
fn pattern_to_api(
    _id: &PatternId,
    state: &vibelang_core::PatternState,
    voices: &std::collections::HashMap<vibelang_core::VoiceId, vibelang_core::VoiceState>,
) -> Pattern {
    // Use the actual name from content
    let name = state.content.name.clone();
    // Get voice name from the voice state
    let voice_name = state
        .content
        .voice
        .and_then(|vid| voices.get(&vid))
        .map(|vs| vs.config.name.clone())
        .unwrap_or_default();

    // Get group path from the voice if available
    let group_path = state
        .content
        .voice
        .and_then(|vid| voices.get(&vid))
        .map(|vs| vs.config.group.raw().to_string())
        .unwrap_or_else(|| "0".to_string());

    // Convert playing state to LoopStatus
    let status = LoopStatus {
        state: if state.playing {
            LoopState::Playing
        } else {
            LoopState::Stopped
        },
        start_beat: None,
        stop_beat: None,
    };

    Pattern {
        name,
        voice_name,
        group_path,
        loop_beats: state.content.length.to_f64(),
        events: state
            .content
            .steps
            .iter()
            .map(|s| PatternEvent {
                beat: s.beat.to_f64(),
                params: if s.params.is_empty() {
                    None
                } else {
                    Some(s.params.iter().map(|(k, v)| (k.clone(), *v)).collect())
                },
            })
            .collect(),
        params: None,
        status,
        is_looping: true, // Patterns are always looping
        source_location: None,
        step_pattern: None,
    }
}

/// GET /patterns - List all patterns
pub async fn list_patterns(State(state): State<Arc<AppState>>) -> Json<Vec<Pattern>> {
    let patterns = state
        .with_state(|s| {
            s.patterns
                .iter()
                .map(|(id, ps)| pattern_to_api(id, ps, &s.voices))
                .collect::<Vec<_>>()
        })
        .await;

    Json(patterns)
}

/// GET /patterns/:id - Get pattern by ID or name
pub async fn get_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Pattern>, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = resolve_pattern_id(&state, &id).await?;

    let pattern = state
        .with_state(|s| {
            s.patterns
                .get(&pattern_id)
                .map(|ps| pattern_to_api(&pattern_id, ps, &s.voices))
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

/// PATCH /patterns/:id - Update pattern by ID or name
///
/// Note: `steps` and `loop_beats` updates are not supported via API and require
/// script reload. Only `params` can be updated at runtime, which sets the param
/// value on all pattern steps.
pub async fn update_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<PatternUpdate>,
) -> Result<Json<Pattern>, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = resolve_pattern_id(&state, &id).await?;

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

    // Note: events and loop_beats updates require script reload
    if update.events.is_some() || update.loop_beats.is_some() {
        tracing::warn!(
            "Pattern {} update requested events or loop_beats change, which requires script reload",
            id
        );
    }

    get_pattern(State(state), Path(id)).await
}

/// POST /patterns/:id/start - Start a pattern by ID or name
pub async fn start_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(_req): Json<Option<StartRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = resolve_pattern_id(&state, &id).await?;

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

/// POST /patterns/:id/stop - Stop a pattern by ID or name
pub async fn stop_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(_req): Json<Option<StopRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = resolve_pattern_id(&state, &id).await?;

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

/// PUT /patterns/:id/params/:param - Set a single pattern parameter by ID or name
pub async fn set_pattern_param(
    State(state): State<Arc<AppState>>,
    Path((id, param)): Path<(String, String)>,
    Json(req): Json<crate::models::ParamSet>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = resolve_pattern_id(&state, &id).await?;

    if let Err(e) = state
        .send(
            PatternMessage::SetParam {
                id: pattern_id,
                param,
                value: req.value,
            }
            .into(),
        )
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to set pattern param: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /patterns - Create a new pattern
pub async fn create_pattern(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PatternCreate>,
) -> Result<(StatusCode, Json<Pattern>), (StatusCode, Json<ErrorResponse>)> {
    // Find voice by name
    let voice_id = state
        .with_state(|s| {
            s.voices
                .iter()
                .find(|(_, v)| v.config.name == req.voice_name)
                .map(|(id, _)| *id)
        })
        .await;

    let voice_id = match voice_id {
        Some(id) => id,
        None => {
            // Try parsing as numeric ID
            match req.voice_name.parse::<u32>() {
                Ok(n) => VoiceId::new(n),
                Err(_) => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::bad_request(&format!(
                            "Voice '{}' not found",
                            req.voice_name
                        ))),
                    ));
                }
            }
        }
    };

    // Generate pattern ID from name hash or next available
    let pattern_id = state
        .with_state(|s| {
            // Use a simple hash of the name for ID
            let id = req
                .name
                .bytes()
                .fold(1u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
            // Make sure it doesn't conflict
            let mut id = id % 10000 + 1;
            while s.patterns.contains_key(&PatternId::new(id)) {
                id += 1;
            }
            PatternId::new(id)
        })
        .await;

    // Convert events to Steps
    let steps: Vec<Step> = req
        .events
        .iter()
        .map(|e| {
            let mut step = Step::new(Beat::from_f64(e.beat));
            if let Some(params) = &e.params {
                for (k, v) in params {
                    step.params.insert(k.clone(), *v);
                }
            }
            // Also add default params
            for (k, v) in &req.params {
                if !step.params.contains_key(k) {
                    step.params.insert(k.clone(), *v);
                }
            }
            step
        })
        .collect();

    let config = PatternConfig {
        name: req.name.clone(),
        voice: Some(voice_id),
        steps,
        length: Beat::from_f64(req.loop_beats),
        swing: req.swing,
    };

    if let Err(e) = state
        .send(
            PatternMessage::Create {
                id: pattern_id,
                config,
                owner: PatternOwner::Script,
            }
            .into(),
        )
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to create pattern: {}",
                e
            ))),
        ));
    }

    // Return the created pattern
    let pattern = state
        .with_state(|s| {
            s.patterns
                .get(&pattern_id)
                .map(|ps| pattern_to_api(&pattern_id, ps, &s.voices))
        })
        .await;

    match pattern {
        Some(p) => Ok((StatusCode::CREATED, Json(p))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(
                "Pattern created but not found in state",
            )),
        )),
    }
}

/// DELETE /patterns/:id - Delete a pattern by ID or name
pub async fn delete_pattern(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = resolve_pattern_id(&state, &id).await?;

    if let Err(e) = state
        .send(PatternMessage::Delete { id: pattern_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to delete pattern: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
