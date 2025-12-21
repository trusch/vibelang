//! Sequences endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::state::StateMessage;

use crate::{
    models::{ErrorResponse, Sequence, SequenceClip, SequenceCreate, SequenceStartRequest, SequenceUpdate, SourceLocation as ApiSourceLocation},
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

/// Convert internal SequenceDefinition to API Sequence model
fn sequence_to_api(sd: &vibelang_core::sequences::SequenceDefinition, active: bool) -> Sequence {
    let clips = sd.clips.iter().map(|c| {
        let (clip_type, name) = match &c.source {
            vibelang_core::sequences::ClipSource::Pattern(n) => ("pattern", n.clone()),
            vibelang_core::sequences::ClipSource::Melody(n) => ("melody", n.clone()),
            vibelang_core::sequences::ClipSource::Fade(n) => ("fade", n.clone()),
            vibelang_core::sequences::ClipSource::Sequence(n) => ("sequence", n.clone()),
        };

        let mode = match &c.mode {
            vibelang_core::sequences::ClipMode::Loop => "loop".to_string(),
            vibelang_core::sequences::ClipMode::Once => "once".to_string(),
            vibelang_core::sequences::ClipMode::LoopCount(n) => format!("loop:{}", n),
        };

        SequenceClip {
            clip_type: clip_type.to_string(),
            name,
            start_beat: c.start,
            end_beat: c.end,
            mode,
        }
    }).collect();

    Sequence {
        name: sd.name.clone(),
        loop_beats: sd.loop_beats,
        clips,
        play_once: sd.play_once,
        active,
        source_location: source_location_to_api(&sd.source_location),
    }
}

/// Parse clip mode string to ClipMode enum
fn parse_clip_mode(mode: &str) -> vibelang_core::sequences::ClipMode {
    if mode == "loop" {
        vibelang_core::sequences::ClipMode::Loop
    } else if mode == "once" {
        vibelang_core::sequences::ClipMode::Once
    } else if let Some(count_str) = mode.strip_prefix("loop:") {
        let count = count_str.parse().unwrap_or(1);
        vibelang_core::sequences::ClipMode::LoopCount(count)
    } else {
        vibelang_core::sequences::ClipMode::Once
    }
}

/// GET /sequences - List all sequences
pub async fn list_sequences(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Sequence>> {
    let sequences = state.handle.with_state(|s| {
        s.sequences.values().map(|sd| {
            let active = s.active_sequences.contains_key(&sd.name);
            sequence_to_api(sd, active)
        }).collect::<Vec<_>>()
    });

    Json(sequences)
}

/// POST /sequences - Create a new sequence
pub async fn create_sequence(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SequenceCreate>,
) -> Result<(StatusCode, Json<Sequence>), (StatusCode, Json<ErrorResponse>)> {
    // Check if sequence already exists
    let exists = state.handle.with_state(|s| s.sequences.contains_key(&req.name));
    if exists {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::conflict(&format!("Sequence '{}' already exists", req.name))),
        ));
    }

    // Build clips
    let clips: Vec<vibelang_core::sequences::SequenceClip> = req.clips.iter().map(|c| {
        let source = match c.clip_type.as_str() {
            "pattern" => vibelang_core::sequences::ClipSource::Pattern(c.name.clone()),
            "melody" => vibelang_core::sequences::ClipSource::Melody(c.name.clone()),
            "fade" => vibelang_core::sequences::ClipSource::Fade(c.name.clone()),
            "sequence" => vibelang_core::sequences::ClipSource::Sequence(c.name.clone()),
            _ => vibelang_core::sequences::ClipSource::Pattern(c.name.clone()),
        };

        let mode = parse_clip_mode(&c.mode);

        vibelang_core::sequences::SequenceClip {
            start: c.start_beat,
            end: c.end_beat,
            source,
            mode,
        }
    }).collect();

    // Create the sequence definition
    let sequence = vibelang_core::sequences::SequenceDefinition {
        name: req.name.clone(),
        loop_beats: req.loop_beats,
        clips,
        generation: 0,
        play_once: false,
        source_location: vibelang_core::api::context::SourceLocation::unknown(),
    };

    // Create the sequence
    if let Err(e) = state.handle.send(StateMessage::CreateSequence { sequence }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to create sequence: {}", e))),
        ));
    }

    // Return the created sequence
    let sequence = state.handle.with_state(|s| {
        s.sequences.get(&req.name).map(|sd| {
            let active = s.active_sequences.contains_key(&sd.name);
            sequence_to_api(sd, active)
        })
    });

    match sequence {
        Some(seq) => Ok((StatusCode::CREATED, Json(seq))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal("Sequence created but not found in state")),
        )),
    }
}

/// GET /sequences/:name - Get sequence by name
pub async fn get_sequence(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Sequence>, (StatusCode, Json<ErrorResponse>)> {
    let sequence = state.handle.with_state(|s| {
        s.sequences.get(&name).map(|sd| {
            let active = s.active_sequences.contains_key(&sd.name);
            sequence_to_api(sd, active)
        })
    });

    match sequence {
        Some(seq) => Ok(Json(seq)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Sequence '{}' not found", name))),
        )),
    }
}

/// PATCH /sequences/:name - Update sequence
pub async fn update_sequence(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(update): Json<SequenceUpdate>,
) -> Result<Json<Sequence>, (StatusCode, Json<ErrorResponse>)> {
    // Check if sequence exists and get current data
    let current = state.handle.with_state(|s| s.sequences.get(&name).cloned());
    let current = match current {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::not_found(&format!("Sequence '{}' not found", name))),
            ));
        }
    };

    let loop_beats = update.loop_beats.unwrap_or(current.loop_beats);
    let clips: Vec<vibelang_core::sequences::SequenceClip> = if let Some(new_clips) = &update.clips {
        new_clips.iter().map(|c| {
            let source = match c.clip_type.as_str() {
                "pattern" => vibelang_core::sequences::ClipSource::Pattern(c.name.clone()),
                "melody" => vibelang_core::sequences::ClipSource::Melody(c.name.clone()),
                "fade" => vibelang_core::sequences::ClipSource::Fade(c.name.clone()),
                "sequence" => vibelang_core::sequences::ClipSource::Sequence(c.name.clone()),
                _ => vibelang_core::sequences::ClipSource::Pattern(c.name.clone()),
            };

            let mode = parse_clip_mode(&c.mode);

            vibelang_core::sequences::SequenceClip {
                start: c.start_beat,
                end: c.end_beat,
                source,
                mode,
            }
        }).collect()
    } else {
        current.clips.clone()
    };

    // Delete and recreate
    let _ = state.handle.send(StateMessage::DeleteSequence { name: name.clone() });

    // Create the sequence definition
    let sequence = vibelang_core::sequences::SequenceDefinition {
        name: name.clone(),
        loop_beats,
        clips,
        generation: 0,
        play_once: current.play_once,
        source_location: current.source_location.clone(),
    };

    if let Err(e) = state.handle.send(StateMessage::CreateSequence { sequence }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to update sequence: {}", e))),
        ));
    }

    get_sequence(State(state), Path(name)).await
}

/// DELETE /sequences/:name - Delete a sequence
pub async fn delete_sequence(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.sequences.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Sequence '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::DeleteSequence { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to delete sequence: {}", e))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /sequences/:name/start - Start a sequence
pub async fn start_sequence(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<Option<SequenceStartRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.sequences.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Sequence '{}' not found", name))),
        ));
    }

    let play_once = req.map(|r| r.play_once).unwrap_or(false);

    let msg = if play_once {
        StateMessage::StartSequenceOnce { name: name.clone() }
    } else {
        StateMessage::StartSequence { name: name.clone() }
    };

    if let Err(e) = state.handle.send(msg) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to start sequence: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /sequences/:name/stop - Stop a sequence
pub async fn stop_sequence(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.sequences.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Sequence '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::StopSequence { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to stop sequence: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /sequences/:name/pause - Pause a sequence
pub async fn pause_sequence(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.sequences.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Sequence '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::PauseSequence { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to pause sequence: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}
