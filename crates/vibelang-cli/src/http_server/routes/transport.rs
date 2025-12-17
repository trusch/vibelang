//! Transport endpoint handlers.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::state::StateMessage;

use crate::http_server::{
    models::{ErrorResponse, SeekRequest, TimeSignature, TransportState, TransportUpdate},
    AppState,
};

/// GET /transport - Get current transport state
pub async fn get_transport(
    State(state): State<Arc<AppState>>,
) -> Json<TransportState> {
    let transport = state.handle.with_state(|s| {
        // Find the longest active sequence's loop_beats for display purposes
        let loop_beats: Option<f64> = s.active_sequences.iter()
            .filter(|(_, active)| !active.paused && !active.completed)
            .filter_map(|(seq_name, _)| {
                s.sequences.get(seq_name)
                    .filter(|seq| seq.loop_beats > 0.0)
                    .map(|seq| seq.loop_beats)
            })
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Calculate loop position if we have a loop
        let loop_beat = loop_beats.map(|lb| {
            if lb > 0.0 {
                s.current_beat % lb
            } else {
                s.current_beat
            }
        });

        TransportState {
            bpm: s.tempo as f32,
            time_signature: TimeSignature {
                numerator: s.time_signature.numerator as u8,
                denominator: s.time_signature.denominator as u8,
            },
            running: s.transport_running,
            current_beat: s.current_beat,
            quantization_beats: s.quantization_beats,
            loop_beats,
            loop_beat,
        }
    });

    Json(transport)
}

/// PATCH /transport - Update transport settings
pub async fn update_transport(
    State(state): State<Arc<AppState>>,
    Json(update): Json<TransportUpdate>,
) -> Result<Json<TransportState>, (StatusCode, Json<ErrorResponse>)> {
    // Apply BPM change
    if let Some(bpm) = update.bpm {
        if !(20.0..=999.0).contains(&bpm) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request("BPM must be between 20 and 999")),
            ));
        }
        if let Err(e) = state.handle.send(StateMessage::SetBpm { bpm: bpm as f64 }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to set BPM: {}", e))),
            ));
        }
    }

    // Apply time signature change
    if let Some(ts) = update.time_signature {
        if let Err(e) = state.handle.send(StateMessage::SetTimeSignature {
            numerator: ts.numerator as u32,
            denominator: ts.denominator as u32,
        }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to set time signature: {}", e))),
            ));
        }
    }

    // Apply quantization change
    if let Some(q) = update.quantization_beats {
        if let Err(e) = state.handle.send(StateMessage::SetQuantization { beats: q }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to set quantization: {}", e))),
            ));
        }
    }

    // Return updated state
    Ok(get_transport(State(state)).await)
}

/// POST /transport/start - Start the transport
pub async fn start_transport(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TransportState>, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = state.handle.send(StateMessage::StartScheduler) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to start transport: {}", e))),
        ));
    }

    Ok(get_transport(State(state)).await)
}

/// POST /transport/stop - Stop the transport
pub async fn stop_transport(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TransportState>, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = state.handle.send(StateMessage::StopScheduler) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to stop transport: {}", e))),
        ));
    }

    Ok(get_transport(State(state)).await)
}

/// POST /transport/seek - Seek to a beat position
pub async fn seek_transport(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SeekRequest>,
) -> Result<Json<TransportState>, (StatusCode, Json<ErrorResponse>)> {
    if req.beat < 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::bad_request("Beat position cannot be negative")),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::SeekTransport { beat: req.beat }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to seek: {}", e))),
        ));
    }

    Ok(get_transport(State(state)).await)
}
