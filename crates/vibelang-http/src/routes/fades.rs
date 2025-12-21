//! Fades endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::state::StateMessage;
use vibelang_core::FadeTargetType;

use crate::{
    models::{ActiveFade, ErrorResponse, FadeCreate},
    AppState,
};

/// Convert internal ActiveFadeJob to API ActiveFade model
fn fade_to_api(id: &str, fo: &vibelang_core::state::ActiveFadeJob, tempo: f64) -> ActiveFade {
    // Convert duration from seconds to beats using the current tempo
    let duration_beats = fo.duration_seconds * tempo / 60.0;
    // Calculate progress based on elapsed time
    let elapsed = fo.start_time.elapsed().as_secs_f64();
    let progress = if fo.duration_seconds > 0.0 {
        (elapsed / fo.duration_seconds).clamp(0.0, 1.0) as f32
    } else {
        1.0
    };

    let current_value = fo.start_value + (fo.target_value - fo.start_value) * progress;

    let target_type = match fo.target_type {
        FadeTargetType::Group => "group",
        FadeTargetType::Voice => "voice",
        FadeTargetType::Effect => "effect",
        FadeTargetType::Pattern => "pattern",
        FadeTargetType::Melody => "melody",
    };

    ActiveFade {
        id: id.to_string(),
        name: None,
        target_type: target_type.to_string(),
        target_name: fo.target_name.clone(),
        param_name: fo.param_name.clone(),
        start_value: fo.start_value,
        target_value: fo.target_value,
        current_value,
        duration_beats,
        start_beat: 0.0, // We don't have the start beat, so use 0
        progress,
    }
}

/// Generate a unique ID for a fade based on its properties
fn generate_fade_id(fo: &vibelang_core::state::ActiveFadeJob, index: usize) -> String {
    format!("fade_{}_{}_{}_{}", index, fo.target_name, fo.param_name,
            fo.start_time.elapsed().as_millis() % 10000)
}

/// GET /fades - List all active fades
pub async fn list_fades(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ActiveFade>> {
    let fades = state.handle.with_state(|s| {
        s.fades.iter()
            .enumerate()
            .map(|(i, fo)| {
                let id = generate_fade_id(fo, i);
                fade_to_api(&id, fo, s.tempo)
            })
            .collect::<Vec<_>>()
    });

    Json(fades)
}

/// POST /fades - Create a new fade
pub async fn create_fade(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FadeCreate>,
) -> Result<(StatusCode, Json<ActiveFade>), (StatusCode, Json<ErrorResponse>)> {
    // Parse target type
    let target_type = match req.target_type.to_lowercase().as_str() {
        "group" => FadeTargetType::Group,
        "voice" => FadeTargetType::Voice,
        "effect" => FadeTargetType::Effect,
        "pattern" => FadeTargetType::Pattern,
        "melody" => FadeTargetType::Melody,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request("Invalid target_type. Must be 'group', 'voice', 'effect', 'pattern', or 'melody'")),
            ));
        }
    };

    // Get current value if start_value not specified
    let start_value = req.start_value.unwrap_or_else(|| {
        state.handle.with_state(|s| {
            match target_type {
                FadeTargetType::Group => {
                    s.groups.get(&req.target_name)
                        .and_then(|g| g.params.get(&req.param_name).copied())
                        .unwrap_or(0.0)
                }
                FadeTargetType::Voice => {
                    s.voices.get(&req.target_name)
                        .and_then(|v| v.params.get(&req.param_name).copied())
                        .unwrap_or(0.0)
                }
                FadeTargetType::Effect => {
                    s.effects.get(&req.target_name)
                        .and_then(|e| e.params.get(&req.param_name).copied())
                        .unwrap_or(0.0)
                }
                FadeTargetType::Pattern => {
                    s.patterns.get(&req.target_name)
                        .and_then(|p| p.params.get(&req.param_name).copied())
                        .unwrap_or(0.0)
                }
                FadeTargetType::Melody => {
                    s.melodies.get(&req.target_name)
                        .and_then(|m| m.params.get(&req.param_name).copied())
                        .unwrap_or(0.0)
                }
            }
        })
    });

    // Convert duration to string format (e.g., "4b" for 4 beats)
    let duration_str = format!("{}b", req.duration_beats);

    // Send the appropriate fade message based on target type
    let result = match target_type {
        FadeTargetType::Group => {
            state.handle.send(StateMessage::FadeGroupParam {
                path: req.target_name.clone(),
                param: req.param_name.clone(),
                target: req.target_value,
                duration: duration_str.clone(),
                delay: None,
                quantize: None,
            })
        }
        FadeTargetType::Voice => {
            state.handle.send(StateMessage::FadeVoiceParam {
                name: req.target_name.clone(),
                param: req.param_name.clone(),
                target: req.target_value,
                duration: duration_str.clone(),
                delay: None,
                quantize: None,
            })
        }
        FadeTargetType::Effect => {
            state.handle.send(StateMessage::FadeEffectParam {
                id: req.target_name.clone(),
                param: req.param_name.clone(),
                target: req.target_value,
                duration: duration_str.clone(),
                delay: None,
                quantize: None,
            })
        }
        FadeTargetType::Pattern => {
            state.handle.send(StateMessage::FadePatternParam {
                name: req.target_name.clone(),
                param: req.param_name.clone(),
                target: req.target_value,
                duration: duration_str.clone(),
                delay: None,
                quantize: None,
            })
        }
        FadeTargetType::Melody => {
            state.handle.send(StateMessage::FadeMelodyParam {
                name: req.target_name.clone(),
                param: req.param_name.clone(),
                target: req.target_value,
                duration: duration_str.clone(),
                delay: None,
                quantize: None,
            })
        }
    };

    if let Err(e) = result {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to create fade: {}", e))),
        ));
    }

    // Generate a fake fade for the response (the actual fade will have a system-generated ID)
    let fake_id = format!("fade_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or(""));
    let current_beat = state.handle.with_state(|s| s.current_beat);

    let fade = ActiveFade {
        id: fake_id,
        name: None,
        target_type: req.target_type,
        target_name: req.target_name,
        param_name: req.param_name,
        start_value,
        target_value: req.target_value,
        current_value: start_value,
        duration_beats: req.duration_beats,
        start_beat: current_beat,
        progress: 0.0,
    };

    Ok((StatusCode::CREATED, Json(fade)))
}

/// DELETE /fades/:id - Cancel an active fade
/// The ID format is: fade_{index}_{target_name}_{param_name}_{timestamp}
/// We extract the fade by index and cancel matching fades.
pub async fn cancel_fade(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Parse the ID to extract the index
    // Format: fade_{index}_{target_name}_{param_name}_{timestamp}
    let parts: Vec<&str> = id.split('_').collect();
    if parts.len() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::bad_request("Invalid fade ID format")),
        ));
    }

    // Try to find the fade by index
    let index: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);

    let fade_info = state.handle.with_state(|s| {
        s.fades.get(index).map(|f| (f.target_type.clone(), f.target_name.clone(), f.param_name.clone()))
    });

    if let Some((target_type, target_name, param_name)) = fade_info {
        if let Err(e) = state.handle.send(StateMessage::CancelFade {
            target_type,
            target_name,
            param_name,
        }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to cancel fade: {}", e))),
            ));
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found("Fade not found")),
        ))
    }
}
