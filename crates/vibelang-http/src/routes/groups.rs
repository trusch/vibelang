//! Groups endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::{
    traits::{FadeConfig, FadeTarget},
    types::Duration,
    FadeMessage, GroupId, GroupMessage, Message,
};

use crate::{
    models::{ErrorResponse, Group, GroupUpdate, ParamSet},
    AppState,
};

/// Resolve a group identifier (either numeric ID or string name) to a GroupId.
async fn resolve_group_id(
    state: &Arc<AppState>,
    identifier: &str,
) -> Result<GroupId, (StatusCode, Json<ErrorResponse>)> {
    // First, try to parse as a numeric ID
    if let Ok(num_id) = identifier.parse::<u32>() {
        let group_id = GroupId::new(num_id);
        let exists = state.with_state(|s| s.groups.contains_key(&group_id)).await;
        if exists {
            return Ok(group_id);
        }
        // Fall through to try as name if numeric ID not found
    }

    // Try to find by name
    let found = state
        .with_state(|s| {
            s.groups
                .iter()
                .find(|(_, gs)| gs.name == identifier)
                .map(|(id, _)| *id)
        })
        .await;

    match found {
        Some(id) => Ok(id),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Group '{}' not found",
                identifier
            ))),
        )),
    }
}

/// Convert internal GroupState to API Group model
fn group_to_api(
    _id: &GroupId,
    state: &vibelang_core::GroupState,
    all_groups: &std::collections::HashMap<GroupId, vibelang_core::GroupState>,
) -> Group {
    // Use the actual name from state
    let name = state.name.clone();
    let path = name.clone();

    // Find children by looking for groups whose parent matches this ID
    let children: Vec<String> = all_groups
        .iter()
        .filter(|(_, gs)| gs.parent.as_ref() == Some(&state.id))
        .map(|(_, child_state)| child_state.name.clone())
        .collect();

    // Get parent name from parent state
    let parent_path = state
        .parent
        .as_ref()
        .and_then(|parent_id| all_groups.get(parent_id).map(|gs| gs.name.clone()));

    Group {
        name,
        path,
        parent_path,
        children,
        node_id: state.node_id.raw() as i32,
        audio_bus: state.audio_bus.raw() as i32,
        link_synth_node_id: state.link_synth_node_id.map(|n| n.raw() as i32),
        muted: state.muted,
        soloed: state.soloed,
        params: state.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        synth_node_ids: None,  // Not tracked in core state
        source_location: None, // Not tracked in core state
    }
}

/// GET /groups - List all groups
pub async fn list_groups(State(state): State<Arc<AppState>>) -> Json<Vec<Group>> {
    let groups = state
        .with_state(|s| {
            s.groups
                .iter()
                .map(|(id, gs)| group_to_api(id, gs, &s.groups))
                .collect::<Vec<_>>()
        })
        .await;

    Json(groups)
}

/// GET /groups/:id - Get group by ID or name
pub async fn get_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    let group_id = resolve_group_id(&state, &id).await?;

    let group = state
        .with_state(|s| {
            s.groups
                .get(&group_id)
                .map(|gs| group_to_api(&group_id, gs, &s.groups))
        })
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

/// PATCH /groups/:id - Update group by ID or name
pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<GroupUpdate>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    let group_id = resolve_group_id(&state, &id).await?;

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

/// POST /groups/:id/mute - Mute a group by ID or name
pub async fn mute_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = resolve_group_id(&state, &id).await?;

    if let Err(e) = state
        .send(
            GroupMessage::Mute {
                id: group_id,
                muted: true,
            }
            .into(),
        )
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

/// POST /groups/:id/unmute - Unmute a group by ID or name
pub async fn unmute_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = resolve_group_id(&state, &id).await?;

    if let Err(e) = state
        .send(
            GroupMessage::Mute {
                id: group_id,
                muted: false,
            }
            .into(),
        )
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

/// POST /groups/:id/solo - Solo a group by ID or name
pub async fn solo_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = resolve_group_id(&state, &id).await?;

    if let Err(e) = state
        .send(
            GroupMessage::Solo {
                id: group_id,
                solo: true,
            }
            .into(),
        )
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

/// POST /groups/:id/unsolo - Unsolo a group by ID or name
pub async fn unsolo_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = resolve_group_id(&state, &id).await?;

    if let Err(e) = state
        .send(
            GroupMessage::Solo {
                id: group_id,
                solo: false,
            }
            .into(),
        )
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

/// PUT /groups/:id/params/:param - Set a group parameter by ID or name
pub async fn set_group_param(
    State(state): State<Arc<AppState>>,
    Path((id, param)): Path<(String, String)>,
    Json(req): Json<ParamSet>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let group_id = resolve_group_id(&state, &id).await?;

    let message: Message = if let Some(fade_beats) = req.fade_beats {
        FadeMessage::Start {
            config: FadeConfig::new(
                FadeTarget::Group(group_id),
                &param,
                req.value,
                Duration::from_beats(fade_beats),
            ),
        }
        .into()
    } else {
        GroupMessage::SetParam {
            id: group_id,
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
