//! SynthDefs endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::{
    models::{ErrorResponse, SynthDef, SynthDefParam},
    AppState,
};

/// Convert internal SynthDefInfo to API SynthDef model
fn synthdef_to_api(name: &str, _bytes: &[u8]) -> SynthDef {
    // Note: We can't easily extract params from compiled synthdef bytes
    // In a full implementation, we'd store param metadata separately
    SynthDef {
        name: name.to_string(),
        params: vec![
            // Common params that most synthdefs have
            SynthDefParam {
                name: "freq".to_string(),
                default_value: 440.0,
                min_value: Some(20.0),
                max_value: Some(20000.0),
            },
            SynthDefParam {
                name: "amp".to_string(),
                default_value: 0.5,
                min_value: Some(0.0),
                max_value: Some(1.0),
            },
            SynthDefParam {
                name: "gate".to_string(),
                default_value: 1.0,
                min_value: Some(0.0),
                max_value: Some(1.0),
            },
            SynthDefParam {
                name: "out".to_string(),
                default_value: 0.0,
                min_value: None,
                max_value: None,
            },
        ],
        source: "user".to_string(),
    }
}

/// GET /synthdefs - List all synthdefs
pub async fn list_synthdefs(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<SynthDef>> {
    let synthdefs = state.handle.with_state(|s| {
        s.synthdefs.iter()
            .map(|(name, bytes)| synthdef_to_api(name, bytes))
            .collect::<Vec<_>>()
    });

    Json(synthdefs)
}

/// GET /synthdefs/:name - Get synthdef by name
pub async fn get_synthdef(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<SynthDef>, (StatusCode, Json<ErrorResponse>)> {
    let synthdef = state.handle.with_state(|s| {
        s.synthdefs.get(&name).map(|bytes| synthdef_to_api(&name, bytes))
    });

    match synthdef {
        Some(sd) => Ok(Json(sd)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("SynthDef '{}' not found", name))),
        )),
    }
}
