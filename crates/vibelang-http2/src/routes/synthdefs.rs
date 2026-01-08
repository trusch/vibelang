//! SynthDefs endpoint handlers.

use axum::{extract::State, Json};
use std::sync::Arc;

use crate::{models::SynthDefInfo, AppState};

/// GET /synthdefs - List all loaded synthdefs
pub async fn list_synthdefs(State(state): State<Arc<AppState>>) -> Json<Vec<SynthDefInfo>> {
    let synthdefs = state
        .with_state(|s| {
            s.synthdefs
                .iter()
                .map(|name| SynthDefInfo {
                    name: name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .await;

    Json(synthdefs)
}
