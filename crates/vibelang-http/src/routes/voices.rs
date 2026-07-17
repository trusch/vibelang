//! Voices endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::{GroupId, ParamMap, VoiceConfig, VoiceId, VoiceMessage};

use crate::{
    models::{
        ErrorResponse, NoteOffRequest, NoteOnRequest, ParamSet, TriggerRequest, Voice, VoiceCreate,
        VoiceUpdate,
    },
    AppState,
};

/// Resolve a voice identifier (either numeric ID or string name) to a VoiceId.
/// Returns the VoiceId if found, or an error response if not found.
async fn resolve_voice_id(
    state: &Arc<AppState>,
    identifier: &str,
) -> Result<VoiceId, (StatusCode, Json<ErrorResponse>)> {
    // First, try to parse as a numeric ID
    if let Ok(num_id) = identifier.parse::<u32>() {
        let voice_id = VoiceId::new(num_id);
        let exists = state.with_state(|s| s.voices.contains_key(&voice_id)).await;
        if exists {
            return Ok(voice_id);
        }
        // Fall through to try as name if numeric ID not found
    }

    // Try to find by name
    let found = state
        .with_state(|s| {
            s.voices
                .iter()
                .find(|(_, vs)| vs.config.name == identifier)
                .map(|(id, _)| *id)
        })
        .await;

    match found {
        Some(id) => Ok(id),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Voice '{}' not found",
                identifier
            ))),
        )),
    }
}

/// Convert internal VoiceState to API Voice model
fn voice_to_api(_id: &VoiceId, state: &vibelang_core::VoiceState) -> Voice {
    // Use the actual name from config
    let name = state.config.name.clone();
    let group_path = state.config.group.raw().to_string();

    Voice {
        name,
        synth_name: state.config.synthdef.clone(),
        polyphony: state.config.polyphony,
        gain: state.config.params.get("amp").copied().unwrap_or(1.0),
        group_path: group_path.clone(),
        group_name: Some(group_path),
        output_bus: None, // Not directly tracked
        muted: state.config.muted,
        soloed: false, // Voice-level solo not tracked in core
        params: state
            .config
            .params
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        sfz_instrument: state.config.sfz_instrument.map(|s| s.raw().to_string()),
        vst_instrument: None, // Not implemented
        active_notes: Some(state.note_nodes.keys().copied().collect()),
        sustained_notes: None, // Not tracked
        running: !state.active_nodes.is_empty(),
        running_node_id: state.active_nodes.first().map(|n| n.raw() as i32),
        source_location: None, // Not tracked in core state
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

/// GET /voices/:id - Get voice by ID or name
pub async fn get_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Voice>, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

    let voice = state
        .with_state(|s| {
            s.voices
                .get(&voice_id)
                .map(|vs| voice_to_api(&voice_id, vs))
        })
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

/// PATCH /voices/:id - Update voice by ID or name
pub async fn update_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<VoiceUpdate>,
) -> Result<Json<Voice>, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

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

/// POST /voices/:id/trigger - Trigger a voice by ID or name
pub async fn trigger_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<Option<TriggerRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

    let mut params = ParamMap::new();
    if let Some(r) = req {
        for (k, v) in r.params {
            params.insert(k, v);
        }
    }

    if let Err(e) = state
        .send(
            VoiceMessage::Trigger {
                id: voice_id,
                params,
            }
            .into(),
        )
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

/// POST /voices/:id/stop - Stop a running voice by ID or name
pub async fn stop_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

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

/// POST /voices/:id/note-on - Send note-on to voice by ID or name
pub async fn note_on(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<NoteOnRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

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

/// POST /voices/:id/note-off - Send note-off to voice by ID or name
pub async fn note_off(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<NoteOffRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

    if let Err(e) = state
        .send(
            VoiceMessage::NoteOff {
                voice: voice_id,
                note: req.note,
            }
            .into(),
        )
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

/// PUT /voices/:id/params/:param - Set a voice parameter by ID or name
pub async fn set_voice_param(
    State(state): State<Arc<AppState>>,
    Path((id, param)): Path<(String, String)>,
    Json(req): Json<ParamSet>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

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

/// POST /voices/:id/mute - Mute a voice by ID or name
pub async fn mute_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

    if let Err(e) = state
        .send(
            VoiceMessage::Mute {
                id: voice_id,
                muted: true,
            }
            .into(),
        )
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

/// POST /voices/:id/unmute - Unmute a voice by ID or name
pub async fn unmute_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

    if let Err(e) = state
        .send(
            VoiceMessage::Mute {
                id: voice_id,
                muted: false,
            }
            .into(),
        )
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

/// POST /voices - Create a new voice
pub async fn create_voice(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VoiceCreate>,
) -> Result<(StatusCode, Json<Voice>), (StatusCode, Json<ErrorResponse>)> {
    // Generate a new voice ID
    let voice_id = state
        .with_state(|s| {
            let max_id = s.voices.keys().map(|id| id.raw()).max().unwrap_or(0);
            VoiceId::new(max_id + 1)
        })
        .await;

    // Parse group ID (default to root group if not specified)
    let group_id = if let Some(gid) = &req.group_path {
        gid.parse::<u32>().map(GroupId::new).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request(&format!(
                    "Invalid group path '{}': must be a number",
                    gid
                ))),
            )
        })?
    } else {
        // Use root group (ID 0)
        GroupId::new(0)
    };

    // Build voice config - synth_name is required
    let voice_name = req
        .name
        .clone()
        .unwrap_or_else(|| voice_id.raw().to_string());
    let synth_name = req
        .synth_name
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let mut config = VoiceConfig::new(&voice_name, &synth_name, group_id);
    if let Some(polyphony) = req.polyphony {
        config = config.with_polyphony(polyphony);
    }
    for (param, value) in req.params {
        config = config.with_param(param, value);
    }

    // Send create message
    if let Err(e) = state
        .send(
            VoiceMessage::Create {
                id: voice_id,
                config: Box::new(config),
            }
            .into(),
        )
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to create voice: {}",
                e
            ))),
        ));
    }

    // Preserve the old entity snapshot only when it is already observable.
    let voice = state
        .with_state(|s| {
            s.voices
                .get(&voice_id)
                .map(|vs| voice_to_api(&voice_id, vs))
        })
        .await;

    match voice {
        Some(v) => Ok((StatusCode::CREATED, Json(v))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(
                "Voice created but not found in state",
            )),
        )),
    }
}

/// DELETE /voices/:id - Delete a voice by ID or name
pub async fn delete_voice(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let voice_id = resolve_voice_id(&state, &id).await?;

    if let Err(e) = state
        .send(VoiceMessage::Delete { id: voice_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to delete voice: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
