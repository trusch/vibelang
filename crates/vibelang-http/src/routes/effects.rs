//! Effects endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::{
    hash_name_to_id,
    traits::{FadeConfig, FadeTarget},
    types::Duration,
    EffectId, EffectMessage, FadeMessage, Message,
};

use crate::{
    models::{Effect, EffectUpdate, ErrorResponse, ParamSet},
    AppState,
};

/// Resolve an effect identifier (numeric ID or `fx("name")` script name).
///
/// Effect ids are the FNV-1a hash of the script name (`hash_name_to_id`),
/// so a non-numeric identifier resolves by hashing it — the same derivation
/// the scripting layer uses. A numeric identifier is tried as a raw id first
/// and falls back to the name hash when no effect carries that id, so an
/// effect declared as `fx("123")` stays reachable. Existence is checked
/// against live state either way, mirroring `resolve_voice_id` in the voices
/// routes.
async fn resolve_effect_id(
    state: &Arc<AppState>,
    id: &str,
) -> Result<EffectId, (StatusCode, Json<ErrorResponse>)> {
    // A numeric identifier wins when it addresses a live effect...
    if let Ok(num_id) = id.parse::<u32>() {
        let numeric = EffectId::new(num_id);
        if state.with_state(|s| s.effects.contains_key(&numeric)).await {
            return Ok(numeric);
        }
        // ...otherwise fall through, so `fx("123")` stays addressable by name.
    }

    let hashed = EffectId::new(hash_name_to_id(id));
    let exists = state.with_state(|s| s.effects.contains_key(&hashed)).await;
    if exists {
        Ok(hashed)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Effect '{}' not found",
                id
            ))),
        ))
    }
}

/// Convert internal EffectState to API Effect model
fn effect_to_api(id: &EffectId, state: &vibelang_core::EffectState) -> Effect {
    Effect {
        id: id.raw().to_string(),
        synthdef_name: state.synthdef.clone(),
        group_path: state.group.raw().to_string(),
        node_id: Some(state.node_id.raw() as i32),
        bus_in: None,  // Not tracked in core
        bus_out: None, // Not tracked in core
        params: state.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        position: None, // Not tracked in core
        vst_plugin: None,
        source_location: None,
    }
}

/// GET /effects - List all effects
pub async fn list_effects(State(state): State<Arc<AppState>>) -> Json<Vec<Effect>> {
    let effects = state
        .with_state(|s| {
            s.effects
                .iter()
                .map(|(id, es)| effect_to_api(id, es))
                .collect::<Vec<_>>()
        })
        .await;

    Json(effects)
}

/// GET /effects/:id - Get effect by ID
pub async fn get_effect(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Effect>, (StatusCode, Json<ErrorResponse>)> {
    let effect_id = resolve_effect_id(&state, &id).await?;

    let effect = state
        .with_state(|s| {
            s.effects
                .get(&effect_id)
                .map(|es| effect_to_api(&effect_id, es))
        })
        .await;

    match effect {
        Some(e) => Ok(Json(e)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Effect '{}' not found",
                id
            ))),
        )),
    }
}

/// PATCH /effects/:id - Update effect
pub async fn update_effect(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<EffectUpdate>,
) -> Result<Json<Effect>, (StatusCode, Json<ErrorResponse>)> {
    let effect_id = resolve_effect_id(&state, &id).await?;

    // Update params
    for (param_name, value) in update.params {
        if let Err(e) = state
            .send(
                EffectMessage::SetParam {
                    id: effect_id,
                    param: param_name,
                    value,
                }
                .into(),
            )
            .await
        {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!(
                    "Failed to update param: {}",
                    e
                ))),
            ));
        }
    }

    get_effect(State(state), Path(id)).await
}

/// DELETE /effects/:id - Delete an effect
pub async fn delete_effect(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let effect_id = resolve_effect_id(&state, &id).await?;

    if let Err(e) = state
        .send(EffectMessage::Remove { id: effect_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to delete effect: {}",
                e
            ))),
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
    let effect_id = resolve_effect_id(&state, &id).await?;

    let message: Message = if let Some(fade_beats) = req.fade_beats {
        FadeMessage::Start {
            config: FadeConfig::new(
                FadeTarget::Effect(effect_id),
                &param,
                req.value,
                Duration::from_beats(fade_beats),
            ),
        }
        .into()
    } else {
        EffectMessage::SetParam {
            id: effect_id,
            param,
            value: req.value,
        }
        .into()
    };
    if let Err(e) = state.send(message).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to set param: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}
