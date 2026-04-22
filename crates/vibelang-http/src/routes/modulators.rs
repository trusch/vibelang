//! Modulator endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::{message::ModulatorMessage, types::ids::ModulatorId};

use crate::{
    models::{ErrorResponse, ParamSet},
    AppState,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct ModulatorResponse {
    pub id: String,
    pub name: String,
    pub synthdef: String,
    pub params: HashMap<String, f32>,
    pub control_bus: u32,
}

#[derive(Debug, Deserialize)]
pub struct ModulatorParamUpdate {
    #[serde(default)]
    pub params: HashMap<String, f32>,
}

fn parse_modulator_id(id: &str) -> Result<ModulatorId, (StatusCode, Json<ErrorResponse>)> {
    id.parse::<u32>().map(ModulatorId::new).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::bad_request(&format!(
                "Invalid modulator ID '{}': must be a number",
                id
            ))),
        )
    })
}

fn modulator_to_api(id: &ModulatorId, state: &vibelang_core::ModulatorState) -> ModulatorResponse {
    ModulatorResponse {
        id: id.raw().to_string(),
        name: state.config.name.clone(),
        synthdef: state.config.synthdef.clone(),
        params: state.config.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        control_bus: state.control_bus.raw(),
    }
}

/// GET /modulators - List all modulators
pub async fn list_modulators(State(state): State<Arc<AppState>>) -> Json<Vec<ModulatorResponse>> {
    let modulators = state
        .with_state(|s| {
            s.modulators
                .iter()
                .map(|(id, ms)| modulator_to_api(id, ms))
                .collect::<Vec<_>>()
        })
        .await;

    Json(modulators)
}

/// GET /modulators/:id - Get modulator by ID
pub async fn get_modulator(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ModulatorResponse>, (StatusCode, Json<ErrorResponse>)> {
    let modulator_id = parse_modulator_id(&id)?;

    let modulator = state
        .with_state(|s| {
            s.modulators
                .get(&modulator_id)
                .map(|ms| modulator_to_api(&modulator_id, ms))
        })
        .await;

    match modulator {
        Some(m) => Ok(Json(m)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Modulator '{}' not found",
                id
            ))),
        )),
    }
}

/// PATCH /modulators/:id - Update multiple modulator params
pub async fn update_modulator(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<ModulatorParamUpdate>,
) -> Result<Json<ModulatorResponse>, (StatusCode, Json<ErrorResponse>)> {
    let modulator_id = parse_modulator_id(&id)?;

    let exists = state
        .with_state(|s| s.modulators.contains_key(&modulator_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Modulator '{}' not found",
                id
            ))),
        ));
    }

    for (param_name, value) in update.params {
        if let Err(e) = state
            .send(
                ModulatorMessage::SetParam {
                    id: modulator_id,
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

    get_modulator(State(state), Path(id)).await
}

/// PUT /modulators/:id/params/:param - Set a single modulator parameter
pub async fn set_modulator_param(
    State(state): State<Arc<AppState>>,
    Path((id, param)): Path<(String, String)>,
    Json(req): Json<ParamSet>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let modulator_id = parse_modulator_id(&id)?;

    let exists = state
        .with_state(|s| s.modulators.contains_key(&modulator_id))
        .await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Modulator '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(
            ModulatorMessage::SetParam {
                id: modulator_id,
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
                "Failed to set param: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}
