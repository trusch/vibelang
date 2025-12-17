//! Effects endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::api::context::SourceLocation;
use vibelang_core::state::StateMessage;

use crate::http_server::{
    models::{Effect, EffectCreate, EffectUpdate, ErrorResponse, ParamSet, SourceLocation as ApiSourceLocation},
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

/// Convert internal EffectState to API Effect model
fn effect_to_api(es: &vibelang_core::state::EffectState) -> Effect {
    Effect {
        id: es.id.clone(),
        synthdef_name: es.synthdef_name.clone(),
        group_path: es.group_path.clone(),
        node_id: es.node_id,
        bus_in: Some(es.bus_in),
        bus_out: Some(es.bus_out),
        params: es.params.clone(),
        position: es.position,
        vst_plugin: es.vst_plugin.clone(),
        source_location: source_location_to_api(&es.source_location),
    }
}

/// GET /effects - List all effects
pub async fn list_effects(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Effect>> {
    let effects = state.handle.with_state(|s| {
        s.effects.values().map(effect_to_api).collect::<Vec<_>>()
    });

    Json(effects)
}

/// POST /effects - Create a new effect
pub async fn create_effect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EffectCreate>,
) -> Result<(StatusCode, Json<Effect>), (StatusCode, Json<ErrorResponse>)> {
    // Generate ID if not provided
    let id = req.id.unwrap_or_else(|| {
        format!("{}_{}", req.synthdef_name, uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or(""))
    });

    // Check if effect already exists
    let exists = state.handle.with_state(|s| s.effects.contains_key(&id));
    if exists {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::conflict(&format!("Effect '{}' already exists", id))),
        ));
    }

    // Check if group exists
    let group_exists = state.handle.with_state(|s| s.groups.contains_key(&req.group_path));
    if !group_exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Group '{}' not found", req.group_path))),
        ));
    }

    // Get the group's audio bus for effect routing
    let bus = state.handle.with_state(|s| {
        s.groups.get(&req.group_path).map(|g| g.audio_bus).unwrap_or(0)
    });

    // Add the effect
    if let Err(e) = state.handle.send(StateMessage::AddEffect {
        id: id.clone(),
        synthdef: req.synthdef_name.clone(),
        group_path: req.group_path.clone(),
        params: req.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        bus_in: bus,
        bus_out: bus,
        source_location: SourceLocation::new(None, None, None),
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to create effect: {}", e))),
        ));
    }

    // Return the created effect
    let effect = state.handle.with_state(|s| s.effects.get(&id).map(effect_to_api));

    match effect {
        Some(e) => Ok((StatusCode::CREATED, Json(e))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal("Effect created but not found in state")),
        )),
    }
}

/// GET /effects/:id - Get effect by ID
pub async fn get_effect(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Effect>, (StatusCode, Json<ErrorResponse>)> {
    let effect = state.handle.with_state(|s| s.effects.get(&id).map(effect_to_api));

    match effect {
        Some(e) => Ok(Json(e)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Effect '{}' not found", id))),
        )),
    }
}

/// PATCH /effects/:id - Update effect parameters
pub async fn update_effect(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<EffectUpdate>,
) -> Result<Json<Effect>, (StatusCode, Json<ErrorResponse>)> {
    // Check if effect exists
    let exists = state.handle.with_state(|s| s.effects.contains_key(&id));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Effect '{}' not found", id))),
        ));
    }

    // Update params
    for (param_name, value) in update.params {
        if let Err(e) = state.handle.send(StateMessage::SetEffectParam {
            id: id.clone(),
            param: param_name,
            value,
        }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to update param: {}", e))),
            ));
        }
    }

    get_effect(State(state), Path(id)).await
}

/// DELETE /effects/:id - Remove an effect
pub async fn delete_effect(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.effects.contains_key(&id));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Effect '{}' not found", id))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::RemoveEffect { id: id.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to remove effect: {}", e))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// PUT /effects/:id/params/:param - Set an effect parameter
pub async fn set_effect_param(
    State(state): State<Arc<AppState>>,
    Path((id, param)): Path<(String, String)>,
    Json(req): Json<ParamSet>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Check if effect exists
    let exists = state.handle.with_state(|s| s.effects.contains_key(&id));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Effect '{}' not found", id))),
        ));
    }

    // If fade_beats is specified, use a fade; otherwise set immediately
    if let Some(duration_beats) = req.fade_beats {
        let duration_str = format!("{}b", duration_beats);
        if let Err(e) = state.handle.send(StateMessage::FadeEffectParam {
            id: id.clone(),
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
        if let Err(e) = state.handle.send(StateMessage::SetEffectParam {
            id: id.clone(),
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
