//! Voices endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::api::context::SourceLocation;
use vibelang_core::state::{StateMessage, VoiceState};

use crate::http_server::{
    models::{ErrorResponse, NoteOffRequest, NoteOnRequest, ParamSet, SourceLocation as ApiSourceLocation, TriggerRequest, Voice, VoiceCreate, VoiceUpdate},
    AppState,
};

/// Convert internal SourceLocation to API model
fn source_location_to_api(sl: &vibelang_core::api::context::SourceLocation) -> Option<ApiSourceLocation> {
    if sl.file.is_some() || sl.line.is_some() {
        Some(ApiSourceLocation {
            file: sl.file.clone(),
            line: sl.line.map(|l| l as usize),
            column: sl.column.map(|c| c as usize),
        })
    } else {
        None
    }
}

/// Convert internal VoiceState to API Voice model
fn voice_to_api(vs: &vibelang_core::state::VoiceState) -> Voice {
    Voice {
        name: vs.name.clone(),
        synth_name: vs.synth_name.clone().unwrap_or_default(),
        polyphony: vs.polyphony as usize,
        gain: vs.gain as f32,
        group_path: vs.group_path.clone(),
        group_name: vs.group_name.clone().unwrap_or_default(),
        output_bus: vs.output_bus.map(|b| b as i32),
        muted: vs.muted,
        soloed: vs.soloed,
        params: vs.params.clone(),
        sfz_instrument: vs.sfz_instrument.clone(),
        vst_instrument: vs.vst_instrument.clone(),
        active_notes: vs.active_notes.keys().copied().collect(),
        sustained_notes: vs.sustained_notes.iter().copied().collect(),
        running: vs.running,
        running_node_id: vs.running_node_id,
        source_location: source_location_to_api(&vs.source_location),
    }
}

/// GET /voices - List all voices
pub async fn list_voices(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Voice>> {
    let voices = state.handle.with_state(|s| {
        s.voices.values().map(voice_to_api).collect::<Vec<_>>()
    });

    Json(voices)
}

/// POST /voices - Create a new voice
pub async fn create_voice(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VoiceCreate>,
) -> Result<(StatusCode, Json<Voice>), (StatusCode, Json<ErrorResponse>)> {
    // Check if voice already exists
    let exists = state.handle.with_state(|s| s.voices.contains_key(&req.name));
    if exists {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::conflict(&format!("Voice '{}' already exists", req.name))),
        ));
    }

    // Get group name from path
    let group_name = req.group_path.split('/').last().unwrap_or("main").to_string();

    log::debug!("Creating voice '{}' with synth '{:?}' in group '{}'", req.name, req.synth_name, req.group_path);

    // Write directly to shared state instead of sending async message
    // This avoids race conditions between message processing and state reads
    let voice = state.handle.with_state_mut(|s| {
        // Create or get the voice entry
        let voice = s.voices.entry(req.name.clone()).or_insert_with(|| {
            VoiceState::new(req.name.clone(), req.group_path.clone())
        });
        voice.group_path = req.group_path.clone();
        voice.group_name = Some(group_name);
        voice.synth_name = req.synth_name.clone();
        voice.polyphony = req.polyphony as i64;
        voice.gain = req.gain as f64;
        voice.muted = false;
        voice.soloed = false;
        voice.output_bus = None;
        voice.params = req.params.clone();
        voice.sfz_instrument = None;
        voice.vst_instrument = None;
        voice.source_location = SourceLocation::new(None, None, None);

        // Convert to API model before releasing the mutable borrow
        let api_voice = voice_to_api(voice);

        // Now bump version (voice borrow is no longer active)
        s.bump_version();

        api_voice
    });

    log::debug!("Voice '{}' created successfully", req.name);
    Ok((StatusCode::CREATED, Json(voice)))
}

/// GET /voices/:name - Get voice by name
pub async fn get_voice(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Voice>, (StatusCode, Json<ErrorResponse>)> {
    let voice = state.handle.with_state(|s| s.voices.get(&name).map(voice_to_api));

    match voice {
        Some(v) => Ok(Json(v)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        )),
    }
}

/// PATCH /voices/:name - Update voice
pub async fn update_voice(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(update): Json<VoiceUpdate>,
) -> Result<Json<Voice>, (StatusCode, Json<ErrorResponse>)> {
    // Check if voice exists
    let exists = state.handle.with_state(|s| s.voices.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        ));
    }

    // Update params
    for (param_name, value) in update.params {
        if let Err(e) = state.handle.send(StateMessage::SetVoiceParam {
            name: name.clone(),
            param: param_name,
            value,
        }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to update param: {}", e))),
            ));
        }
    }

    // Return updated voice
    get_voice(State(state), Path(name)).await
}

/// DELETE /voices/:name - Delete a voice
pub async fn delete_voice(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.voices.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::DeleteVoice { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to delete voice: {}", e))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /voices/:name/trigger - Trigger a voice
pub async fn trigger_voice(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<Option<TriggerRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.voices.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        ));
    }

    let params: Vec<(String, f32)> = req
        .map(|r| r.params.into_iter().collect())
        .unwrap_or_default();

    if let Err(e) = state.handle.send(StateMessage::TriggerVoice {
        name: name.clone(),
        synth_name: None,
        group_path: None,
        params,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to trigger voice: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:name/stop - Stop a running voice
pub async fn stop_voice(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.voices.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::StopVoice { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to stop voice: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:name/note-on - Send note-on to voice
pub async fn note_on(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<NoteOnRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.voices.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::NoteOn {
        voice_name: name.clone(),
        note: req.note,
        velocity: req.velocity,
        duration: None,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to send note-on: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:name/note-off - Send note-off to voice
pub async fn note_off(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<NoteOffRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.voices.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::NoteOff {
        voice_name: name.clone(),
        note: req.note,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to send note-off: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}

/// PUT /voices/:name/params/:param - Set a voice parameter
pub async fn set_voice_param(
    State(state): State<Arc<AppState>>,
    Path((name, param)): Path<(String, String)>,
    Json(req): Json<ParamSet>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Check if voice exists
    let exists = state.handle.with_state(|s| s.voices.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        ));
    }

    // If fade_beats is specified, use a fade; otherwise set immediately
    if let Some(duration_beats) = req.fade_beats {
        let duration_str = format!("{}b", duration_beats);
        if let Err(e) = state.handle.send(StateMessage::FadeVoiceParam {
            name: name.clone(),
            param: param.clone(),
            target: req.value,
            duration: duration_str,
            delay: None,
            quantize: None,
        }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to fade param: {}", e))),
            ));
        }
    } else {
        if let Err(e) = state.handle.send(StateMessage::SetVoiceParam {
            name: name.clone(),
            param: param.clone(),
            value: req.value,
        }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to set param: {}", e))),
            ));
        }
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:name/mute - Mute a voice
pub async fn mute_voice(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.voices.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::MuteVoice { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to mute voice: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /voices/:name/unmute - Unmute a voice
pub async fn unmute_voice(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.voices.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::UnmuteVoice { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to unmute voice: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}
