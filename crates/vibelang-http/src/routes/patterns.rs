//! Patterns endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::api::context::SourceLocation;
use vibelang_core::state::{LoopStatus as InternalLoopStatus, StateMessage};

use crate::{
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

    // Get synthdef name from voice
    let synthdef_name = state.handle.with_state(|s| {
        s.voices.get(&req.voice_name)
            .and_then(|v| v.synth_name.clone())
            .unwrap_or_default()
    });

    // Build events from either events array or pattern_string
    let beat_events: Vec<vibelang_core::events::BeatEvent> = if let Some(pattern_str) = &req.pattern_string {
        // Parse pattern string (e.g., "x...x...x...x...")
        parse_pattern_string(pattern_str, req.loop_beats, &synthdef_name)
    } else {
        req.events.iter().map(|e| {
            let mut evt = vibelang_core::events::BeatEvent::new(e.beat, &synthdef_name);
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

/// Parse a pattern string like "x...x...|x.x.x.x." with bar separators
/// Supports: x/X = hit, 1-9 = velocity levels, . = rest, | = bar separator
/// Each bar is 4 beats. Steps per bar determined by character count in each bar.
fn parse_pattern_string(pattern: &str, loop_beats: f64, synthdef_name: &str) -> Vec<vibelang_core::events::BeatEvent> {
    log::debug!("[HTTP] parse_pattern_string: pattern='{}', loop_beats={}, synthdef='{}'",
        pattern, loop_beats, synthdef_name);

    // Split by bar separator
    let bars: Vec<&str> = pattern.split('|').collect();
    let num_bars = bars.len();

    log::debug!("[HTTP] Split into {} bars: {:?}", num_bars, bars);

    if num_bars == 0 {
        return vec![];
    }

    // Calculate beats per bar
    let beats_per_bar = loop_beats / num_bars as f64;

    let mut events = Vec::new();
    let mut current_beat = 0.0;

    for bar in &bars {
        // Get characters for this bar, filtering whitespace
        let chars: Vec<char> = bar.chars().filter(|c| !c.is_whitespace()).collect();

        if chars.is_empty() {
            // Empty bar, just advance the beat
            current_beat += beats_per_bar;
            continue;
        }

        let steps_in_bar = chars.len();
        let beat_per_step = beats_per_bar / steps_in_bar as f64;

        for (step_idx, c) in chars.iter().enumerate() {
            let velocity = match c {
                'x' => Some(1.0),
                'X' | 'o' | 'O' => Some(1.0), // Accents treated as full velocity for now
                '1'..='9' => {
                    let digit = c.to_digit(10).unwrap() as f64;
                    Some(0.1 + (digit / 9.0) * 0.9)
                }
                _ => None, // '.', '-', '_', '0' = rest
            };

            if let Some(vel) = velocity {
                let beat = current_beat + (step_idx as f64 * beat_per_step);
                let mut evt = vibelang_core::events::BeatEvent::new(beat, synthdef_name);
                evt.controls.push(("amp".to_string(), vel as f32));
                events.push(evt);
            }
        }

        current_beat += beats_per_bar;
    }

    log::info!("[HTTP] Parsed {} events: {:?}",
        events.len(),
        events.iter().map(|e| format!("beat={:.3}", e.beat)).collect::<Vec<_>>());

    events
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
    log::info!("[HTTP] PATCH /patterns/{}: pattern_string={:?}, loop_beats={:?}",
        name, update.pattern_string, update.loop_beats);

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

    // Remember if pattern was actually playing so we can restart it
    // Note: is_looping is a config flag (should loop when played), NOT whether it's currently playing
    let was_playing = matches!(current.status, InternalLoopStatus::Playing { .. } | InternalLoopStatus::QueuedStop { .. });

    // Get current loop_length_beats from loop_pattern
    let current_loop_beats = current.loop_pattern.as_ref().map(|lp| lp.loop_length_beats).unwrap_or(4.0);
    let loop_beats = update.loop_beats.unwrap_or(current_loop_beats);

    // Get synthdef name from original events (we need to preserve this)
    let synthdef_name = current.loop_pattern.as_ref()
        .and_then(|lp| lp.events.first())
        .map(|e| e.synth_def.clone())
        .unwrap_or_default();

    // Build new events
    let beat_events: Vec<vibelang_core::events::BeatEvent> = if let Some(pattern_str) = &update.pattern_string {
        parse_pattern_string(pattern_str, loop_beats, &synthdef_name)
    } else if let Some(evts) = &update.events {
        evts.iter().map(|e| {
            let mut evt = vibelang_core::events::BeatEvent::new(e.beat, &synthdef_name);
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

    // Preserve source location from original pattern
    let source_location = SourceLocation::new(
        current.source_location.file.clone(),
        current.source_location.line,
        current.source_location.column,
    );

    if let Err(e) = state.handle.send(StateMessage::CreatePattern {
        name: name.clone(),
        group_path: current.group_path,
        voice_name: current.voice_name,
        pattern,
        source_location,
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

    // Check if there's an active sequence for this pattern (created when pattern.start() is called from code)
    // If so, the sequence will continue to play the pattern automatically - don't start it directly.
    // If there's no sequence, we need to restart the pattern directly.
    let seq_name = format!("_seq_{}", name);
    let has_active_sequence = state.handle.with_state(|s| s.active_sequences.contains_key(&seq_name));

    log::info!("[HTTP] Pattern '{}' update complete: was_playing={}, has_active_sequence '{}' = {}",
        name, was_playing, seq_name, has_active_sequence);

    if was_playing && !has_active_sequence {
        // Pattern was playing directly (not via a sequence), restart it
        log::info!("[HTTP] Restarting pattern '{}' directly (no active sequence)", name);
        let _ = state.handle.send(StateMessage::StartPattern { name: name.clone() });
    }
    // If was_playing && has_active_sequence: the sequence will pick up the updated pattern automatically

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
