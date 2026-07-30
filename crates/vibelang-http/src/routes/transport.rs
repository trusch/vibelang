//! Transport endpoint handlers.

use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use vibelang_core::{Beat, TimeSignature as CoreTimeSignature, TransportMessage};

use crate::{
    models::{ErrorResponse, SeekRequest, TimeSignature, TransportState, TransportUpdate},
    AppState,
};

/// Helper to build TransportState from core state
fn build_transport_state(s: &vibelang_core::State) -> TransportState {
    let server_time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Calculate loop info from active sequences
    let loop_beats = s
        .sequences
        .values()
        .filter(|seq| seq.playing)
        .map(|seq| seq.config.length.to_f64())
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let loop_beat = loop_beats.map(|lb| {
        if lb > 0.0 {
            s.current_beat.to_f64() % lb
        } else {
            0.0
        }
    });

    TransportState {
        bpm: s.tempo,
        time_signature: TimeSignature {
            numerator: s.time_sig.numerator,
            denominator: s.time_sig.denominator,
        },
        running: s.playing,
        current_beat: s.current_beat.to_f64(),
        quantization_beats: 1.0, // Default quantization
        loop_beats,
        loop_beat,
        server_time_ms: Some(server_time_ms),
    }
}

/// GET /transport - Get current transport state
pub async fn get_transport(State(state): State<Arc<AppState>>) -> Json<TransportState> {
    let transport = state.with_state(build_transport_state).await;
    Json(transport)
}

/// PATCH /transport - Update transport settings
pub async fn update_transport(
    State(state): State<Arc<AppState>>,
    Json(update): Json<TransportUpdate>,
) -> Result<Json<TransportState>, (StatusCode, Json<ErrorResponse>)> {
    // Apply BPM change
    if let Some(bpm) = update.bpm {
        if !(1.0..=999.0).contains(&bpm) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request("BPM must be between 1 and 999")),
            ));
        }
        if let Err(e) = state.send(TransportMessage::SetTempo { bpm }.into()).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!(
                    "Failed to set BPM: {}",
                    e
                ))),
            ));
        }
    }

    // Apply time signature change
    if let Some(ts) = update.time_signature {
        let time_sig = CoreTimeSignature {
            numerator: ts.numerator,
            denominator: ts.denominator,
        };
        if let Err(e) = state
            .send(TransportMessage::SetTimeSignature { time_sig }.into())
            .await
        {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!(
                    "Failed to set time signature: {}",
                    e
                ))),
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
    if let Err(e) = state.send(TransportMessage::Start.into()).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to start transport: {}",
                e
            ))),
        ));
    }

    Ok(get_transport(State(state)).await)
}

/// POST /transport/stop - Stop the transport
pub async fn stop_transport(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TransportState>, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = state.send(TransportMessage::Stop.into()).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!(
                "Failed to stop transport: {}",
                e
            ))),
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
            Json(ErrorResponse::bad_request(
                "Beat position cannot be negative",
            )),
        ));
    }

    if let Err(e) = state
        .send(
            TransportMessage::Seek {
                beat: Beat::from_f64(req.beat),
            }
            .into(),
        )
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to seek: {}", e))),
        ));
    }

    Ok(get_transport(State(state)).await)
}
