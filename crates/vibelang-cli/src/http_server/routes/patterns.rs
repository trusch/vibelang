//! Patterns endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::api::context::SourceLocation;
use vibelang_core::state::{LoopStatus as InternalLoopStatus, StateMessage};

use crate::http_server::{
    models::{ErrorResponse, LoopStatus, Pattern, PatternCreate, PatternEvent, PatternUpdate, SourceLocation as ApiSourceLocation, StartRequest, StopRequest},
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

/// Convert internal LoopStatus to API model
fn loop_status_to_api(status: &InternalLoopStatus) -> LoopStatus {
    match status {
        InternalLoopStatus::Stopped => LoopStatus {
            state: "stopped".to_string(),
            start_beat: None,
            stop_beat: None,
        },
        InternalLoopStatus::Queued { start_beat } => LoopStatus {
            state: "queued".to_string(),
            start_beat: Some(*start_beat),
            stop_beat: None,
        },
        InternalLoopStatus::Playing { start_beat } => LoopStatus {
            state: "playing".to_string(),
            start_beat: Some(*start_beat),
            stop_beat: None,
        },
        InternalLoopStatus::QueuedStop { start_beat, stop_beat } => LoopStatus {
            state: "queued_stop".to_string(),
            start_beat: Some(*start_beat),
            stop_beat: Some(*stop_beat),
        },
    }
}

/// Convert internal PatternState to API Pattern model
fn pattern_to_api(ps: &vibelang_core::state::PatternState) -> Pattern {
    let (events, loop_beats) = if let Some(ref lp) = ps.loop_pattern {
        let evts = lp.events.iter().map(|e| {
            PatternEvent {
                beat: e.beat,
                params: e.controls.iter().cloned().collect(),
            }
        }).collect();
        (evts, lp.loop_length_beats)
    } else {
        (vec![], 4.0)
    };

    Pattern {
        name: ps.name.clone(),
        voice_name: ps.voice_name.clone().unwrap_or_default(),
        group_path: ps.group_path.clone(),
        loop_beats,
        events,
        params: ps.params.clone(),
        status: loop_status_to_api(&ps.status),
        is_looping: ps.is_looping,
        source_location: source_location_to_api(&ps.source_location),
        step_pattern: ps.step_pattern.clone(),
    }
}

/// GET /patterns - List all patterns
pub async fn list_patterns(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Pattern>> {
    let patterns = state.handle.with_state(|s| {
        s.patterns.values().map(pattern_to_api).collect::<Vec<_>>()
    });

    Json(patterns)
}

/// POST /patterns - Create a new pattern
pub async fn create_pattern(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PatternCreate>,
) -> Result<(StatusCode, Json<Pattern>), (StatusCode, Json<ErrorResponse>)> {
    // Check if pattern already exists
    let exists = state.handle.with_state(|s| s.patterns.contains_key(&req.name));
    if exists {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::conflict(&format!("Pattern '{}' already exists", req.name))),
        ));
    }

    // Check if voice exists
    let voice_exists = state.handle.with_state(|s| s.voices.contains_key(&req.voice_name));
    if !voice_exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Voice '{}' not found", req.voice_name))),
        ));
    }

    // Get group path from voice if not specified
    let group_path = req.group_path.unwrap_or_else(|| {
        state.handle.with_state(|s| {
            s.voices.get(&req.voice_name)
                .map(|v| v.group_path.clone())
                .unwrap_or_else(|| "main".to_string())
        })
    });

    // Build events from either events array or pattern_string
    let beat_events: Vec<vibelang_core::events::BeatEvent> = if let Some(pattern_str) = &req.pattern_string {
        // Parse pattern string (e.g., "x...x...x...x...")
        parse_pattern_string(pattern_str, req.loop_beats)
    } else {
        req.events.iter().map(|e| {
            let mut evt = vibelang_core::events::BeatEvent::new(e.beat, "");
            evt.controls = e.params.iter().map(|(k, v)| (k.clone(), *v)).collect();
            evt
        }).collect()
    };

    // Create the Pattern object
    let pattern = vibelang_core::events::Pattern {
        name: req.name.clone(),
        events: beat_events,
        loop_length_beats: req.loop_beats,
        phase_offset: 0.0,
    };

    // Create the pattern
    if let Err(e) = state.handle.send(StateMessage::CreatePattern {
        name: req.name.clone(),
        group_path: group_path.clone(),
        voice_name: Some(req.voice_name.clone()),
        pattern,
        source_location: SourceLocation::new(None, None, None),
        step_pattern: req.pattern_string.clone(),
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to create pattern: {}", e))),
        ));
    }

    // Set any params
    for (param_name, value) in &req.params {
        let _ = state.handle.send(StateMessage::SetPatternParam {
            name: req.name.clone(),
            param: param_name.clone(),
            value: *value,
        });
    }

    // Return the created pattern
    let pattern = state.handle.with_state(|s| s.patterns.get(&req.name).map(pattern_to_api));

    match pattern {
        Some(p) => Ok((StatusCode::CREATED, Json(p))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal("Pattern created but not found in state")),
        )),
    }
}

/// Parse a simple pattern string like "x...x...x...x..."
fn parse_pattern_string(pattern: &str, loop_beats: f64) -> Vec<vibelang_core::events::BeatEvent> {
    let chars: Vec<char> = pattern.chars().collect();
    if chars.is_empty() {
        return vec![];
    }

    let beat_per_step = loop_beats / chars.len() as f64;
    chars.iter().enumerate()
        .filter(|(_, c)| **c == 'x' || **c == 'X')
        .map(|(i, _)| vibelang_core::events::BeatEvent::new(i as f64 * beat_per_step, ""))
        .collect()
}

/// GET /patterns/:name - Get pattern by name
pub async fn get_pattern(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Pattern>, (StatusCode, Json<ErrorResponse>)> {
    let pattern = state.handle.with_state(|s| s.patterns.get(&name).map(pattern_to_api));

    match pattern {
        Some(p) => Ok(Json(p)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Pattern '{}' not found", name))),
        )),
    }
}

/// PATCH /patterns/:name - Update pattern
pub async fn update_pattern(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(update): Json<PatternUpdate>,
) -> Result<Json<Pattern>, (StatusCode, Json<ErrorResponse>)> {
    // Check if pattern exists and get current data
    let current = state.handle.with_state(|s| s.patterns.get(&name).cloned());
    let current = match current {
        Some(p) => p,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::not_found(&format!("Pattern '{}' not found", name))),
            ));
        }
    };

    // Get current loop_length_beats from loop_pattern
    let current_loop_beats = current.loop_pattern.as_ref().map(|lp| lp.loop_length_beats).unwrap_or(4.0);
    let loop_beats = update.loop_beats.unwrap_or(current_loop_beats);

    // Build new events
    let beat_events: Vec<vibelang_core::events::BeatEvent> = if let Some(pattern_str) = &update.pattern_string {
        parse_pattern_string(pattern_str, loop_beats)
    } else if let Some(evts) = &update.events {
        evts.iter().map(|e| {
            let mut evt = vibelang_core::events::BeatEvent::new(e.beat, "");
            evt.controls = e.params.iter().map(|(k, v)| (k.clone(), *v)).collect();
            evt
        }).collect()
    } else if let Some(ref lp) = current.loop_pattern {
        lp.events.clone()
    } else {
        vec![]
    };

    // Create the Pattern object
    let pattern = vibelang_core::events::Pattern {
        name: name.clone(),
        events: beat_events,
        loop_length_beats: loop_beats,
        phase_offset: 0.0,
    };

    // Delete and recreate the pattern with new data
    let _ = state.handle.send(StateMessage::DeletePattern { name: name.clone() });

    if let Err(e) = state.handle.send(StateMessage::CreatePattern {
        name: name.clone(),
        group_path: current.group_path,
        voice_name: current.voice_name,
        pattern,
        source_location: SourceLocation::new(None, None, None),
        step_pattern: update.pattern_string.clone().or(current.step_pattern),
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to update pattern: {}", e))),
        ));
    }

    // Set params
    let params_to_set = if update.params.is_empty() {
        current.params.clone()
    } else {
        update.params.iter().map(|(k, v)| (k.clone(), *v)).collect()
    };
    for (param_name, value) in params_to_set {
        let _ = state.handle.send(StateMessage::SetPatternParam {
            name: name.clone(),
            param: param_name,
            value,
        });
    }

    get_pattern(State(state), Path(name)).await
}

/// DELETE /patterns/:name - Delete a pattern
pub async fn delete_pattern(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.patterns.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Pattern '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::DeletePattern { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to delete pattern: {}", e))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /patterns/:name/start - Start a pattern
pub async fn start_pattern(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(_req): Json<Option<StartRequest>>,
) -> Result<Json<Pattern>, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.patterns.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Pattern '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::StartPattern { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to start pattern: {}", e))),
        ));
    }

    get_pattern(State(state), Path(name)).await
}

/// POST /patterns/:name/stop - Stop a pattern
pub async fn stop_pattern(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(_req): Json<Option<StopRequest>>,
) -> Result<Json<Pattern>, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.patterns.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Pattern '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::StopPattern { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to stop pattern: {}", e))),
        ));
    }

    get_pattern(State(state), Path(name)).await
}
