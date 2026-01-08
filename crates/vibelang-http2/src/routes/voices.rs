//! Voices endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core2::{ParamMap, VoiceId, VoiceMessage};

use crate::{
    models::{ErrorResponse, NoteOffRequest, NoteOnRequest, ParamSet, TriggerRequest, Voice, VoiceUpdate},
    AppState,
};

/// Parse a voice ID from a string path parameter.
fn parse_voice_id(id: &str) -> Result<VoiceId, (StatusCode, Json<ErrorResponse>)> {
    id.parse::<u32>()
        .map(VoiceId::new)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request(&format!(
                    "Invalid voice ID '{}': must be a number",
                    id
                ))),
            )
        })
}

/// Convert internal VoiceState to API Voice model
fn voice_to_api(id: &VoiceId, state: &vibelang_core2::VoiceState) -> Voice {
    Voice {
        id: id.raw().to_string(),
        synthdef: Some(state.config.synthdef.clone()),
        group_id: state.config.group.raw().to_string(),
        polyphony: state.config.polyphony,
        gain: state.config.params.get("amp").copied().unwrap_or(1.0),
        muted: state.config.muted,
        params: state.config.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        sfz_instrument: state.config.sfz_instrument.map(|s| s.raw().to_string()),
        active_notes: state.note_nodes.keys().copied().collect(),
    }
}

/// GET /voices - List all voices
pub async fn list_voices(State(state): State<Arc<AppState>>) -> Json<Vec<Voice>> {
    let voices = state
        .with_state(|s| {
            s.voices
                .iter()
                .map(|(id, vs)| voice_to_api(id, vs))
                .collect::<Vec<_>>()
        })
        .await;

    Json(voices)
}

/// GET /voices/:id - Get voice by ID
pub async fn get_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Voice>, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = parse_voice_id(&id)?;

    let voice = state
        .with_state(|s| s.voices.get(&voice_id).map(|vs| voice_to_api(&voice_id, vs)))
        .await;

    match voice {
        Some(v) => Ok(Json(v)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                id
            ))),
        )),
    }
}

/// PATCH /voices/:id - Update voice
pub async fn update_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<VoiceUpdate>,
) -> Result<Json<Voice>, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = parse_voice_id(&id)?;

    // Check if voice exists
    let exists = state.with_state(|s| s.voices.contains_key(&voice_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                id
            ))),
        ));
    }

    // Apply param updates
    for (param, value) in update.params {
        if let Err(e) = state
            .send(
                VoiceMessage::SetParam {
                    id: voice_id,
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
                    "Failed to update voice params: {}",
                    e
                ))),
            ));
        }
    }

    get_voice(State(state), Path(id)).await
}

/// POST /voices/:id/trigger - Trigger a voice
pub async fn trigger_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<Option<TriggerRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = parse_voice_id(&id)?;

    let exists = state.with_state(|s| s.voices.contains_key(&voice_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                id
            ))),
        ));
    }

    let mut params = ParamMap::new();
    if let Some(r) = req {
        for (k, v) in r.params {
            params.insert(k, v);
        }
    }

    if let Err(e) = state
        .send(VoiceMessage::Trigger { id: voice_id, params }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to trigger voice: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:id/stop - Stop a running voice
pub async fn stop_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = parse_voice_id(&id)?;

    let exists = state.with_state(|s| s.voices.contains_key(&voice_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state.send(VoiceMessage::Stop { id: voice_id }.into()).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to stop voice: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:id/note-on - Send note-on to voice
pub async fn note_on(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<NoteOnRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = parse_voice_id(&id)?;

    let exists = state.with_state(|s| s.voices.contains_key(&voice_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(
            VoiceMessage::NoteOn {
                voice: voice_id,
                note: req.note,
                velocity: req.velocity,
            }
            .into(),
        )
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to send note-on: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:id/note-off - Send note-off to voice
pub async fn note_off(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<NoteOffRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = parse_voice_id(&id)?;

    let exists = state.with_state(|s| s.voices.contains_key(&voice_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(VoiceMessage::NoteOff { voice: voice_id, note: req.note }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to send note-off: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// PUT /voices/:id/params/:param - Set a voice parameter
pub async fn set_voice_param(
    State(state): State<Arc<AppState>>,
    Path((id, param)): Path<(String, String)>,
    Json(req): Json<ParamSet>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = parse_voice_id(&id)?;

    let exists = state.with_state(|s| s.voices.contains_key(&voice_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(
            VoiceMessage::SetParam {
                id: voice_id,
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
                "Failed to set voice param: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:id/mute - Mute a voice
pub async fn mute_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = parse_voice_id(&id)?;

    let exists = state.with_state(|s| s.voices.contains_key(&voice_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(VoiceMessage::Mute { id: voice_id, muted: true }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to mute voice: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:id/unmute - Unmute a voice
pub async fn unmute_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = parse_voice_id(&id)?;

    let exists = state.with_state(|s| s.voices.contains_key(&voice_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(VoiceMessage::Mute { id: voice_id, muted: false }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to unmute voice: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}
