//! Sequences endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::{
    traits::SequenceConfig, types::Beat, Clip, MelodyId, PatternId, SequenceId, SequenceMessage,
};

use crate::{
    models::{
        ErrorResponse, Sequence, SequenceClip, SequenceCreate, SequenceStartRequest, SequenceUpdate,
    },
    AppState,
};

/// Resolve a sequence identifier (either numeric ID or string name) to a SequenceId.
async fn resolve_sequence_id(
    state: &Arc<AppState>,
    identifier: &str,
) -> Result<SequenceId, (StatusCode, Json<ErrorResponse>)> {
    // First, try to parse as a numeric ID
    if let Ok(num_id) = identifier.parse::<u32>() {
        let sequence_id = SequenceId::new(num_id);
        let exists = state
            .with_state(|s| s.sequences.contains_key(&sequence_id))
            .await;
        if exists {
            return Ok(sequence_id);
        }
        // Fall through to try as name if numeric ID not found
    }

    // Try to find by name
    let found = state
        .with_state(|s| {
            s.sequences
                .iter()
                .find(|(_, ss)| ss.config.name == identifier)
                .map(|(id, _)| *id)
        })
        .await;

    match found {
        Some(id) => Ok(id),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Sequence '{}' not found",
                identifier
            ))),
        )),
    }
}

/// Convert a Clip enum to API model
fn clip_to_api(clip: &Clip) -> SequenceClip {
    match clip {
        Clip::Pattern { id, start, end } => SequenceClip {
            clip_type: "pattern".to_string(),
            name: id.raw().to_string(),
            start_beat: start.to_f64(),
            end_beat: Some(end.to_f64()),
            duration_beats: Some((end.to_f64() - start.to_f64()).max(0.0)),
            once: None,
        },
        Clip::Melody { id, start, end } => SequenceClip {
            clip_type: "melody".to_string(),
            name: id.raw().to_string(),
            start_beat: start.to_f64(),
            end_beat: Some(end.to_f64()),
            duration_beats: Some((end.to_f64() - start.to_f64()).max(0.0)),
            once: None,
        },
        Clip::Fade { start, .. } => SequenceClip {
            clip_type: "fade".to_string(),
            name: String::new(),
            start_beat: start.to_f64(),
            end_beat: None,
            duration_beats: None,
            once: Some(true),
        },
        Clip::Sequence { id, start } => SequenceClip {
            clip_type: "sequence".to_string(),
            name: id.raw().to_string(),
            start_beat: start.to_f64(),
            end_beat: None,
            duration_beats: None,
            once: None,
        },
    }
}

/// Convert internal SequenceState to API Sequence model
fn sequence_to_api(_id: &SequenceId, state: &vibelang_core::SequenceState) -> Sequence {
    Sequence {
        // Use the actual name from config
        name: state.config.name.clone(),
        loop_beats: state.config.length.to_f64(),
        clips: state.config.clips.iter().map(clip_to_api).collect(),
        play_once: Some(!state.looping),
        active: Some(state.playing),
        source_location: None,
    }
}

/// GET /sequences - List all sequences
pub async fn list_sequences(State(state): State<Arc<AppState>>) -> Json<Vec<Sequence>> {
    let sequences = state
        .with_state(|s| {
            s.sequences
                .iter()
                .map(|(id, ss)| sequence_to_api(id, ss))
                .collect::<Vec<_>>()
        })
        .await;

    Json(sequences)
}

/// GET /sequences/:id - Get sequence by ID or name
pub async fn get_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Sequence>, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = resolve_sequence_id(&state, &id).await?;

    let sequence = state
        .with_state(|s| {
            s.sequences
                .get(&sequence_id)
                .map(|ss| sequence_to_api(&sequence_id, ss))
        })
        .await;

    match sequence {
        Some(seq) => Ok(Json(seq)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Sequence '{}' not found",
                id
            ))),
        )),
    }
}

/// PATCH /sequences/:id - Update sequence by ID or name
///
/// Note: Sequence updates (`clips` and `loop_beats`) are not supported via API
/// and require script reload. This endpoint currently returns the current state.
pub async fn update_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<SequenceUpdate>,
) -> Result<Json<Sequence>, (StatusCode, Json<ErrorResponse>)> {
    let _sequence_id = resolve_sequence_id(&state, &id).await?;

    // Note: clips and loop_beats updates require script reload
    if update.clips.is_some() || update.loop_beats.is_some() {
        tracing::warn!(
            "Sequence {} update requested clips or loop_beats change, which requires script reload",
            id
        );
    }

    get_sequence(State(state), Path(id)).await
}

/// POST /sequences/:id/start - Start a sequence by ID or name
pub async fn start_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<Option<SequenceStartRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = resolve_sequence_id(&state, &id).await?;

    let looping = !req.map(|r| r.play_once).unwrap_or(false);

    if let Err(e) = state
        .send(
            SequenceMessage::Start {
                id: sequence_id,
                looping,
            }
            .into(),
        )
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to start sequence: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /sequences/:id/stop - Stop a sequence by ID or name
pub async fn stop_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = resolve_sequence_id(&state, &id).await?;

    if let Err(e) = state
        .send(SequenceMessage::Stop { id: sequence_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to stop sequence: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /sequences/:id/pause - Pause a sequence by ID or name
pub async fn pause_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = resolve_sequence_id(&state, &id).await?;

    if let Err(e) = state
        .send(SequenceMessage::Pause { id: sequence_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to pause sequence: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// Convert API SequenceClip to internal Clip
fn api_clip_to_clip(clip: &SequenceClip) -> Option<Clip> {
    let start = Beat::from_f64(clip.start_beat);
    let end = clip
        .end_beat
        .or(clip.duration_beats.map(|d| clip.start_beat + d))
        .map(Beat::from_f64)
        .unwrap_or_else(|| Beat::from_f64(clip.start_beat + 4.0));

    match clip.clip_type.to_lowercase().as_str() {
        "pattern" => {
            let id = clip.name.parse::<u32>().ok().map(PatternId::new)?;
            Some(Clip::Pattern { id, start, end })
        }
        "melody" => {
            let id = clip.name.parse::<u32>().ok().map(MelodyId::new)?;
            Some(Clip::Melody { id, start, end })
        }
        "sequence" => {
            let id = clip.name.parse::<u32>().ok().map(SequenceId::new)?;
            Some(Clip::Sequence { id, start })
        }
        _ => None,
    }
}

/// POST /sequences - Create a new sequence
pub async fn create_sequence(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SequenceCreate>,
) -> Result<(StatusCode, Json<Sequence>), (StatusCode, Json<ErrorResponse>)> {
    // Generate sequence ID from name hash
    let sequence_id = state
        .with_state(|s| {
            let id = req
                .name
                .bytes()
                .fold(1u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
            let mut id = id % 10000 + 1;
            while s.sequences.contains_key(&SequenceId::new(id)) {
                id += 1;
            }
            SequenceId::new(id)
        })
        .await;

    // Convert API clips to internal Clips
    let clips: Vec<Clip> = req.clips.iter().filter_map(api_clip_to_clip).collect();

    let config = SequenceConfig {
        name: req.name.clone(),
        length: Beat::from_f64(req.loop_beats),
        clips,
    };

    if let Err(e) = state
        .send(
            SequenceMessage::Create {
                id: sequence_id,
                config,
            }
            .into(),
        )
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to create sequence: {}",
                e
            ))),
        ));
    }

    // Return the created sequence
    let sequence = state
        .with_state(|s| {
            s.sequences
                .get(&sequence_id)
                .map(|ss| sequence_to_api(&sequence_id, ss))
        })
        .await;

    match sequence {
        Some(seq) => Ok((StatusCode::CREATED, Json(seq))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(
                "Sequence created but not found in state",
            )),
        )),
    }
}

/// DELETE /sequences/:id - Delete a sequence by ID or name
pub async fn delete_sequence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let sequence_id = resolve_sequence_id(&state, &id).await?;

    if let Err(e) = state
        .send(SequenceMessage::Delete { id: sequence_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to delete sequence: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
