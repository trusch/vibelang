//! SynthDefs endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::{
    models::{ErrorResponse, SynthDefInfo},
    AppState,
};

/// GET /synthdefs - List all loaded synthdefs
pub async fn list_synthdefs(State(state): State<Arc<AppState>>) -> Json<Vec<SynthDefInfo>> {
    let synthdefs = state
        .with_state(|s| {
            s.synthdefs
                .iter()
                .map(|name| SynthDefInfo {
                    name: name.clone(),
                    params: Vec::new(), // Params not tracked in core state
                    source: None,       // Source not tracked in core state
                })
                .collect::<Vec<_>>()
        })
        .await;

    Json(synthdefs)
}

/// GET /synthdefs/:name - Get synthdef by name
pub async fn get_synthdef(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<SynthDefInfo>, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.with_state(|s| s.synthdefs.contains(&name)).await;

    if exists {
        Ok(Json(SynthDefInfo {
            name,
            params: Vec::new(), // Params not tracked in core state
            source: None,       // Source not tracked in core state
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "SynthDef '{}' not found",
                name
            ))),
        ))
    }
}
