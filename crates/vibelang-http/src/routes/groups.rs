//! Groups endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::collections::HashMap;
use std::sync::Arc;
use vibelang_core::api::context::SourceLocation;
use vibelang_core::state::StateMessage;

use crate::{
    models::{ErrorResponse, Group, GroupCreate, GroupUpdate, ParamSet, SourceLocation as ApiSourceLocation},
    AppState,
};

/// Convert internal SourceLocation to API model
fn source_location_to_api(sl: &vibelang_core::api::context::SourceLocation) -> Option<ApiSourceLocation> {
    // Only return Some if there's at least a file path
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

/// Convert internal GroupState to API Group model
fn group_to_api(gs: &vibelang_core::state::GroupState, children: Vec<String>) -> Group {
    Group {
        name: gs.name.clone(),
        path: gs.path.clone(),
        parent_path: gs.parent_path.clone(),
        children,
        node_id: gs.node_id,
        audio_bus: gs.audio_bus,
        link_synth_node_id: gs.link_synth_node_id,
        muted: gs.muted,
        soloed: gs.soloed,
        params: gs.params.clone(),
        synth_node_ids: gs.synth_node_ids.clone(),
        source_location: source_location_to_api(&gs.source_location),
    }
}

/// Find children of a group by path
fn find_children(groups: &HashMap<String, vibelang_core::state::GroupState>, parent_path: &str) -> Vec<String> {
    groups
        .values()
        .filter(|g| g.parent_path.as_deref() == Some(parent_path))
        .map(|g| g.path.clone())
        .collect()
}

/// GET /groups - List all groups
pub async fn list_groups(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Group>> {
    let groups = state.handle.with_state(|s| {
        s.groups
            .values()
            .map(|gs| {
                let children = find_children(&s.groups, &gs.path);
                group_to_api(gs, children)
            })
            .collect::<Vec<_>>()
    });

    Json(groups)
}

/// POST /groups - Create a new group
pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GroupCreate>,
) -> Result<(StatusCode, Json<Group>), (StatusCode, Json<ErrorResponse>)> {
    // Construct the full path
    let full_path = if req.parent_path == "main" {
        format!("main/{}", req.name)
    } else {
        format!("{}/{}", req.parent_path, req.name)
    };

    // Check if group already exists
    let exists = state.handle.with_state(|s| s.groups.contains_key(&full_path));
    if exists {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::conflict(&format!("Group '{}' already exists", full_path))),
        ));
    }

    // Check if parent exists
    let parent_exists = state.handle.with_state(|s| s.groups.contains_key(&req.parent_path));
    if !parent_exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Parent group '{}' not found", req.parent_path))),
        ));
    }

    // Register the group
    if let Err(e) = state.handle.send(StateMessage::RegisterGroup {
        name: req.name.clone(),
        path: full_path.clone(),
        parent_path: Some(req.parent_path.clone()),
        node_id: -1, // Will be assigned by the runtime
        source_location: SourceLocation::new(None, None, None),
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to create group: {}", e))),
        ));
    }

    // Set initial params
    for (name, value) in req.params {
        let _ = state.handle.send(StateMessage::SetGroupParam {
            path: full_path.clone(),
            param: name,
            value,
        });
    }

    // Return the created group
    let group = state.handle.with_state(|s| {
        s.groups.get(&full_path).map(|gs| {
            let children = find_children(&s.groups, &gs.path);
            group_to_api(gs, children)
        })
    });

    match group {
        Some(g) => Ok((StatusCode::CREATED, Json(g))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal("Group created but not found in state")),
        )),
    }
}

/// GET /groups/*path - Get group by path
pub async fn get_group(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    let group = state.handle.with_state(|s| {
        s.groups.get(&path).map(|gs| {
            let children = find_children(&s.groups, &gs.path);
            group_to_api(gs, children)
        })
    });

    match group {
        Some(g) => Ok(Json(g)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Group '{}' not found", path))),
        )),
    }
}

/// PATCH /groups/*path - Update group parameters
pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
    Json(update): Json<GroupUpdate>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    // Check if group exists
    let exists = state.handle.with_state(|s| s.groups.contains_key(&path));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Group '{}' not found", path))),
        ));
    }

    // Update params
    for (name, value) in update.params {
        if let Err(e) = state.handle.send(StateMessage::SetGroupParam {
            path: path.clone(),
            param: name,
            value,
        }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to update param: {}", e))),
            ));
        }
    }

    // Return updated group
    get_group(State(state), Path(path)).await
}

/// DELETE /groups/*path - Delete a group
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Cannot delete main group
    if path == "main" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::bad_request("Cannot delete the main group")),
        ));
    }

    // Check if group exists
    let exists = state.handle.with_state(|s| s.groups.contains_key(&path));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Group '{}' not found", path))),
        ));
    }

    // Unregister the group
    if let Err(e) = state.handle.send(StateMessage::UnregisterGroup { path: path.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to delete group: {}", e))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /groups/*path/mute - Mute a group
pub async fn mute_group(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    // Check if group exists
    let exists = state.handle.with_state(|s| s.groups.contains_key(&path));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Group '{}' not found", path))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::MuteGroup { path: path.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to mute group: {}", e))),
        ));
    }

    get_group(State(state), Path(path)).await
}

/// POST /groups/*path/unmute - Unmute a group
pub async fn unmute_group(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.groups.contains_key(&path));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Group '{}' not found", path))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::UnmuteGroup { path: path.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to unmute group: {}", e))),
        ));
    }

    get_group(State(state), Path(path)).await
}

/// POST /groups/*path/solo - Solo a group
pub async fn solo_group(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.groups.contains_key(&path));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Group '{}' not found", path))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::SoloGroup { path: path.clone(), solo: true }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to solo group: {}", e))),
        ));
    }

    get_group(State(state), Path(path)).await
}

/// POST /groups/*path/unsolo - Remove solo from a group
pub async fn unsolo_group(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Result<Json<Group>, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.groups.contains_key(&path));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Group '{}' not found", path))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::SoloGroup { path: path.clone(), solo: false }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to unsolo group: {}", e))),
        ));
    }

    get_group(State(state), Path(path)).await
}

/// PUT /groups/:path/params/:param - Set a group parameter
pub async fn set_group_param(
    State(state): State<Arc<AppState>>,
    Path((path, param)): Path<(String, String)>,
    Json(req): Json<ParamSet>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Check if group exists
    let exists = state.handle.with_state(|s| s.groups.contains_key(&path));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Group '{}' not found", path))),
        ));
    }

    // If fade_beats is specified, use a fade; otherwise set immediately
    if let Some(duration_beats) = req.fade_beats {
        let duration_str = format!("{}b", duration_beats);
        if let Err(e) = state.handle.send(StateMessage::FadeGroupParam {
            path: path.clone(),
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
    } else if let Err(e) = state.handle.send(StateMessage::SetGroupParam {
        path: path.clone(),
        param: param.clone(),
        value: req.value,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to set param: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}
