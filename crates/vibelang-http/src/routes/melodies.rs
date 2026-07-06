//! Melodies endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::{
    traits::{MelodyConfig, NoteEvent},
    types::Beat,
    MelodyId, MelodyMessage, VoiceId,
};

use crate::{
    models::{
        ErrorResponse, LoopState, LoopStatus, Melody, MelodyCreate, MelodyEvent, MelodyUpdate,
        StartRequest, StopRequest,
    },
    AppState,
};

/// Resolve a melody identifier (either numeric ID or string name) to a MelodyId.
async fn resolve_melody_id(
    state: &Arc<AppState>,
    identifier: &str,
) -> Result<MelodyId, (StatusCode, Json<ErrorResponse>)> {
    // First, try to parse as a numeric ID
    if let Ok(num_id) = identifier.parse::<u32>() {
        let melody_id = MelodyId::new(num_id);
        let exists = state
            .with_state(|s| s.melodies.contains_key(&melody_id))
            .await;
        if exists {
            return Ok(melody_id);
        }
        // Fall through to try as name if numeric ID not found
    }

    // Try to find by name
    let found = state
        .with_state(|s| {
            s.melodies
                .iter()
                .find(|(_, ms)| ms.content.name == identifier)
                .map(|(id, _)| *id)
        })
        .await;

    match found {
        Some(id) => Ok(id),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Melody '{}' not found",
                identifier
            ))),
        )),
    }
}

/// Convert internal MelodyState to API Melody model
fn melody_to_api(
    _id: &MelodyId,
    state: &vibelang_core::MelodyState,
    voices: &std::collections::HashMap<vibelang_core::VoiceId, vibelang_core::VoiceState>,
) -> Melody {
    // Use the actual name from content
    let name = state.content.name.clone();
    // Get voice name from voice state
    let voice_name = state
        .content
        .voice
        .and_then(|vid| voices.get(&vid))
        .map(|vs| vs.config.name.clone())
        .unwrap_or_default();

    // Get group path from the voice if available
    let group_path = state
        .content
        .voice
        .and_then(|vid| voices.get(&vid))
        .map(|vs| vs.config.group.raw().to_string())
        .unwrap_or_else(|| "0".to_string());

    // Convert playing state to LoopStatus
    let status = LoopStatus {
        state: if state.playing {
            LoopState::Playing
        } else {
            LoopState::Stopped
        },
        start_beat: None,
        stop_beat: None,
    };

    Melody {
        name,
        voice_name,
        group_path,
        loop_beats: state.content.length.to_f64(),
        events: state
            .content
            .notes
            .iter()
            .map(|n| MelodyEvent {
                beat: n.beat.to_f64(),
                note: format!("{}", n.note), // MIDI note number as string
                frequency: None,             // Could calculate from note if needed
                duration: Some(n.duration.to_f64()),
                velocity: Some(n.velocity),
                params: None,
            })
            .collect(),
        params: None,
        status,
        is_looping: true, // Melodies are always looping
        source_location: None,
        notes_patterns: None,
    }
}

/// GET /melodies - List all melodies
pub async fn list_melodies(State(state): State<Arc<AppState>>) -> Json<Vec<Melody>> {
    let melodies = state
        .with_state(|s| {
            s.melodies
                .iter()
                .map(|(id, ms)| melody_to_api(id, ms, &s.voices))
                .collect::<Vec<_>>()
        })
        .await;

    Json(melodies)
}

/// GET /melodies/:id - Get melody by ID or name
pub async fn get_melody(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Melody>, (StatusCode, Json<ErrorResponse>)> {
    let melody_id = resolve_melody_id(&state, &id).await?;

    let melody = state
        .with_state(|s| {
            s.melodies
                .get(&melody_id)
                .map(|ms| melody_to_api(&melody_id, ms, &s.voices))
        })
        .await;

    match melody {
        Some(m) => Ok(Json(m)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!(
                "Melody '{}' not found",
                id
            ))),
        )),
    }
}

/// PATCH /melodies/:id - Update melody by ID or name
///
/// Note: Melody updates (`events` and `loop_beats`) are not supported via API
/// and require script reload. This endpoint currently returns the current state.
pub async fn update_melody(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<MelodyUpdate>,
) -> Result<Json<Melody>, (StatusCode, Json<ErrorResponse>)> {
    let _melody_id = resolve_melody_id(&state, &id).await?;

    // Note: events and loop_beats updates require script reload
    if update.events.is_some() || update.loop_beats.is_some() {
        tracing::warn!(
            "Melody {} update requested events or loop_beats change, which requires script reload",
            id
        );
    }

    get_melody(State(state), Path(id)).await
}

/// POST /melodies/:id/start - Start a melody by ID or name
pub async fn start_melody(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(_req): Json<Option<StartRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let melody_id = resolve_melody_id(&state, &id).await?;

    if let Err(e) = state
        .send(MelodyMessage::Start { id: melody_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to start melody: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// POST /melodies/:id/stop - Stop a melody by ID or name
pub async fn stop_melody(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(_req): Json<Option<StopRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let melody_id = resolve_melody_id(&state, &id).await?;

    if let Err(e) = state
        .send(MelodyMessage::Stop { id: melody_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to stop melody: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::OK)
}

/// Parse a note string to MIDI note number.
/// Supports formats like "C4", "60", "C#4", "Db4".
///
/// Delegates to the canonical parser in `vibelang_dsp::notes`, with the
/// HTTP-specific adjustments this endpoint has always had: numeric strings
/// pass through as-is, the input is lowercased so all-caps flats ("DB4")
/// keep working, out-of-range results are clamped to 0..=127, and
/// unparseable input falls back to middle C (60).
fn parse_note(note: &str) -> u8 {
    // Try parsing as number first
    if let Ok(n) = note.parse::<u8>() {
        return n;
    }

    match vibelang_dsp::notes::parse_note_name_raw(&note.to_lowercase()) {
        Some(v) => v.clamp(0, 127) as u8,
        None => 60, // Default to middle C
    }
}

/// POST /melodies - Create a new melody
pub async fn create_melody(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MelodyCreate>,
) -> Result<(StatusCode, Json<Melody>), (StatusCode, Json<ErrorResponse>)> {
    // Find voice by name
    let voice_id = state
        .with_state(|s| {
            s.voices
                .iter()
                .find(|(_, v)| v.config.name == req.voice_name)
                .map(|(id, _)| *id)
        })
        .await;

    let voice_id = match voice_id {
        Some(id) => id,
        None => {
            // Try parsing as numeric ID
            match req.voice_name.parse::<u32>() {
                Ok(n) => VoiceId::new(n),
                Err(_) => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::bad_request(&format!(
                            "Voice '{}' not found",
                            req.voice_name
                        ))),
                    ));
                }
            }
        }
    };

    // Generate melody ID from name hash
    let melody_id = state
        .with_state(|s| {
            let id = req
                .name
                .bytes()
                .fold(1u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
            let mut id = id % 10000 + 1;
            while s.melodies.contains_key(&MelodyId::new(id)) {
                id += 1;
            }
            MelodyId::new(id)
        })
        .await;

    // Convert events to NoteEvents
    let notes: Vec<NoteEvent> = req
        .events
        .iter()
        .map(|e| {
            NoteEvent::new(
                e.beat,
                parse_note(&e.note),
                e.velocity.unwrap_or(0.8),
                e.duration.unwrap_or(1.0),
            )
        })
        .collect();

    let config = MelodyConfig {
        name: req.name.clone(),
        voice: Some(voice_id),
        notes,
        length: Beat::from_f64(req.loop_beats),
        swing: 0.0,
    };

    if let Err(e) = state
        .send(
            MelodyMessage::Create {
                id: melody_id,
                config,
            }
            .into(),
        )
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to create melody: {}",
                e
            ))),
        ));
    }

    // Return the created melody
    let melody = state
        .with_state(|s| {
            s.melodies
                .get(&melody_id)
                .map(|ms| melody_to_api(&melody_id, ms, &s.voices))
        })
        .await;

    match melody {
        Some(m) => Ok((StatusCode::CREATED, Json(m))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(
                "Melody created but not found in state",
            )),
        )),
    }
}

/// DELETE /melodies/:id - Delete a melody by ID or name
pub async fn delete_melody(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let melody_id = resolve_melody_id(&state, &id).await?;

    if let Err(e) = state
        .send(MelodyMessage::Delete { id: melody_id }.into())
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to delete melody: {}",
                e
            ))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
