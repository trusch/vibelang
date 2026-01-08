//! Groups endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core2::{GroupId, GroupMessage};

use crate::{
    models::{ErrorResponse, Group, GroupUpdate, ParamSet},
    AppState,
};

/// Parse a group ID from a string path parameter.
fn parse_group_id(id: &str) -> Result<GroupId, (StatusCode, Json<ErrorResponse>)> {
    id.parse::<u32>()
        .map(GroupId::new)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request(&format!(
                    "Invalid group ID '{}': must be a number",
                    id
                ))),
            )
        })
}

/// Convert internal GroupState to API Group model
fn group_to_api(id: &GroupId, state: &vibelang_core2::GroupState) -> Group {
    Group {
        id: id.raw().to_string(),
        parent_id: state.parent.as_ref().map(|p| p.raw().to_string()),
        node_id: state.node_id.raw() as i32,
        audio_bus: state.audio_bus.raw() as i32,
        link_synth_node_id: state.link_synth_node_id.map(|n| n.raw() as i32),
        muted: state.muted,
        soloed: state.soloed,
        params: state.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
    }
}

/// GET /groups - List all groups
pub async fn list_groups(State(state): State<Arc<AppState>>) -> Json<Vec<Group>> {
    let groups = state
        .with_state(|s| {
            s.groups
                .iter()
                .map(|(id, gs)| group_to_api(id, gs))
                .collect::<Vec<_>>()
        })
        .await;

    Json(groups)
}

/// GET /groups/:id - Get group by ID
pub async fn get_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    let group_id = parse_group_id(&id)?;

    let group = state
        .with_state(|s| s.groups.get(&group_id).map(|gs| group_to_api(&group_id, gs)))
        .await;

    match group {
        Some(g) => Ok(Json(g)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Group '{}' not found",
                id
            ))),
        )),
    }
}

/// PATCH /groups/:id - Update group
pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<GroupUpdate>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    let group_id = parse_group_id(&id)?;

    // Check if group exists
    let exists = state.with_state(|s| s.groups.contains_key(&group_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Group '{}' not found",
                id
            ))),
        ));
    }

    // Update params
    for (param_name, value) in update.params {
        if let Err(e) = state
            .send(
                GroupMessage::SetParam {
                    id: group_id,
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

    // Return updated group
    get_group(State(state), Path(id)).await
}

/// POST /groups/:id/mute - Mute a group
pub async fn mute_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = parse_group_id(&id)?;

    let exists = state.with_state(|s| s.groups.contains_key(&group_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Group '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(GroupMessage::Mute { id: group_id, muted: true }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to mute group: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /groups/:id/unmute - Unmute a group
pub async fn unmute_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = parse_group_id(&id)?;

    let exists = state.with_state(|s| s.groups.contains_key(&group_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Group '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(GroupMessage::Mute { id: group_id, muted: false }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to unmute group: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /groups/:id/solo - Solo a group
pub async fn solo_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = parse_group_id(&id)?;

    let exists = state.with_state(|s| s.groups.contains_key(&group_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Group '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(GroupMessage::Solo { id: group_id, solo: true }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to solo group: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /groups/:id/unsolo - Unsolo a group
pub async fn unsolo_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = parse_group_id(&id)?;

    let exists = state.with_state(|s| s.groups.contains_key(&group_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Group '{}' not found",
                id
            ))),
        ));
    }

    if let Err(e) = state
        .send(GroupMessage::Solo { id: group_id, solo: false }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to unsolo group: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// PUT /groups/:id/params/:param - Set a group parameter
pub async fn set_group_param(
    State(state): State<Arc<AppState>>,
    Path((id, param)): Path<(String, String)>,
    Json(req): Json<ParamSet>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = parse_group_id(&id)?;

    // Check if group exists
    let exists = state.with_state(|s| s.groups.contains_key(&group_id)).await;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Group '{}' not found",
                id
            ))),
        ));
    }

    // For now, just set the param directly (fades can be added later)
    if let Err(e) = state
        .send(
            GroupMessage::SetParam {
                id: group_id,
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
