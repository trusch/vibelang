//! MIDI HTTP API endpoints.
//!
//! Provides endpoints for:
//! - Device discovery and management
//! - MIDI input/output routing
//! - Recording control
//! - Clock output control

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vibelang_core::midi::MidiReadiness;
use vibelang_core::types::MidiDeviceId;

use crate::AppState;

// ============================================================================
// DTOs
// ============================================================================

/// Information about a MIDI device.
#[derive(Debug, Serialize)]
pub struct MidiDeviceDto {
    pub id: u32,
    pub name: String,
    pub has_input: bool,
    pub has_output: bool,
}

/// Request to open a MIDI device.
#[derive(Debug, Deserialize)]
pub struct OpenDeviceRequest {
    pub device_id: u32,
}

/// Request to send a MIDI note.
#[derive(Debug, Deserialize)]
pub struct SendNoteRequest {
    pub device_id: u32,
    pub channel: u8,
    pub note: u8,
    pub velocity: Option<u8>,
}

/// Request to send a MIDI CC.
#[derive(Debug, Deserialize)]
pub struct SendCcRequest {
    pub device_id: u32,
    pub channel: u8,
    pub cc: u8,
    pub value: u8,
}

/// Request to start recording.
#[derive(Debug, Deserialize)]
pub struct StartRecordingRequest {
    pub device_id: u32,
    pub channel: Option<u8>,
}

/// Request to stop recording.
#[derive(Debug, Deserialize)]
pub struct StopRecordingRequest {
    pub device_id: u32,
    /// Quantization value for recorded notes (not yet implemented in backend).
    #[allow(dead_code)]
    pub quantize: Option<f64>,
}

/// Recorded note DTO.
/// Reserved for future use when stop_recording returns recording data.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct RecordedNoteDto {
    pub beat: f64,
    pub note: u8,
    pub velocity: u8,
    pub duration: Option<f64>,
}

/// Recording result DTO.
/// Reserved for future use when stop_recording returns recording data.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct RecordingResultDto {
    pub device_id: u32,
    pub note_count: usize,
    pub cc_count: usize,
    pub duration_beats: f64,
    pub notes: Vec<RecordedNoteDto>,
}

/// Request to enable/disable clock output.
/// Reserved for future unified clock control endpoint (currently enable/disable are separate endpoints).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ClockOutputRequest {
    pub device_id: u32,
    pub enabled: bool,
}

/// Response for clock output status.
/// Reserved for future GET /midi/clock/status endpoint.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct ClockStatusDto {
    pub device_id: u32,
    pub enabled: bool,
}

/// Error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ============================================================================
// Device Endpoints
// ============================================================================

/// List all available MIDI devices.
///
/// GET /midi/devices
pub async fn list_devices(State(_state): State<Arc<AppState>>) -> Json<Vec<MidiDeviceDto>> {
    // Note: This requires direct access to the MIDI handler.
    // For now, we'll use the midir crate directly here.
    use midir::{MidiInput, MidiOutput};

    let mut devices = Vec::new();
    let mut seen_names = std::collections::HashMap::new();

    // List input devices
    if let Ok(midi_in) = MidiInput::new("vibelang-http2-list") {
        for (idx, port) in midi_in.ports().iter().enumerate() {
            if let Ok(name) = midi_in.port_name(port) {
                devices.push(MidiDeviceDto {
                    id: idx as u32,
                    name: name.clone(),
                    has_input: true,
                    has_output: false,
                });
                seen_names.insert(name, idx);
            }
        }
    }

    // List output devices
    if let Ok(midi_out) = MidiOutput::new("vibelang-http2-list") {
        for port in midi_out.ports().iter() {
            if let Ok(name) = midi_out.port_name(port) {
                // Check if we already have this device from input
                if let Some(&existing_idx) = seen_names.get(&name) {
                    devices[existing_idx].has_output = true;
                } else {
                    let id = devices.len() as u32;
                    devices.push(MidiDeviceDto {
                        id,
                        name,
                        has_input: false,
                        has_output: true,
                    });
                }
            }
        }
    }

    Json(devices)
}

/// Return the current logical MIDI input readiness independently of engine health.
///
/// GET /midi/readiness
pub async fn get_readiness(State(state): State<Arc<AppState>>) -> Json<MidiReadiness> {
    Json(
        state
            .with_state(|runtime| runtime.midi_readiness.clone())
            .await,
    )
}

/// Open a MIDI input device.
///
/// POST /midi/input/open
pub async fn open_input(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenDeviceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::OpenInput { device: device_id }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to open MIDI input: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Open a MIDI output device.
///
/// POST /midi/output/open
pub async fn open_output(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenDeviceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::OpenOutput { device: device_id }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to open MIDI output: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Close a MIDI device.
///
/// POST /midi/close
pub async fn close_device(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenDeviceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::CloseDevice {
            device: device_id,
        }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to close MIDI device: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

// ============================================================================
// Note/CC Output Endpoints
// ============================================================================

/// Send a MIDI note-on message.
///
/// POST /midi/note/on
pub async fn send_note_on(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendNoteRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::SendNoteOn {
            device: device_id,
            channel: req.channel,
            note: req.note,
            velocity: req.velocity.unwrap_or(100),
        }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to send note on: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Send a MIDI note-off message.
///
/// POST /midi/note/off
pub async fn send_note_off(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendNoteRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::SendNoteOff {
            device: device_id,
            channel: req.channel,
            note: req.note,
        }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to send note off: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Send a MIDI CC message.
///
/// POST /midi/cc
pub async fn send_cc(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendCcRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::SendCC {
            device: device_id,
            channel: req.channel,
            cc: req.cc,
            value: req.value,
        }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to send CC: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

// ============================================================================
// Recording Endpoints
// ============================================================================

/// Start MIDI recording.
///
/// POST /midi/record/start
pub async fn start_recording(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StartRecordingRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);

    let msg = if let Some(channel) = req.channel {
        Message::Midi(MidiMessage::StartRecordingChannel {
            device: device_id,
            channel,
        })
    } else {
        Message::Midi(MidiMessage::StartRecording { device: device_id })
    };

    state.send(msg).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to start recording: {}", e),
            }),
        )
    })?;

    Ok(StatusCode::OK)
}

/// Stop MIDI recording.
///
/// POST /midi/record/stop
///
/// Note: This stops recording but doesn't return the recording data.
/// The recording is stored and can be retrieved via other means (e.g., WebSocket events).
pub async fn stop_recording(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StopRecordingRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);

    state
        .send(Message::Midi(MidiMessage::StopRecording {
            device: device_id,
        }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to stop recording: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

// ============================================================================
// Clock Output Endpoints
// ============================================================================

/// Enable MIDI clock output.
///
/// POST /midi/clock/enable
pub async fn enable_clock_output(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenDeviceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::EnableClockOutput {
            device: device_id,
        }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to enable clock output: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Disable MIDI clock output.
///
/// POST /midi/clock/disable
pub async fn disable_clock_output(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenDeviceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::DisableClockOutput {
            device: device_id,
        }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to disable clock output: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Send MIDI start message.
///
/// POST /midi/transport/start
pub async fn send_midi_start(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenDeviceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::SendStart { device: device_id }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to send MIDI start: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Send MIDI stop message.
///
/// POST /midi/transport/stop
pub async fn send_midi_stop(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenDeviceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::SendStop { device: device_id }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to send MIDI stop: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Send MIDI continue message.
///
/// POST /midi/transport/continue
pub async fn send_midi_continue(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenDeviceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    state
        .send(Message::Midi(MidiMessage::SendContinue {
            device: device_id,
        }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to send MIDI continue: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

// ============================================================================
// Route Management Endpoints
// ============================================================================

/// Request to add a keyboard route.
#[derive(Debug, Deserialize)]
pub struct AddKeyboardRouteRequest {
    pub device_id: u32,
    pub voice_id: u32,
    pub channel: Option<u8>,
    pub note_min: Option<u8>,
    pub note_max: Option<u8>,
    pub transpose: Option<i8>,
}

/// Response for keyboard route creation.
#[derive(Debug, Serialize)]
pub struct AddKeyboardRouteResponse {
    pub message: String,
}

/// Route info DTO.
#[derive(Debug, Serialize)]
pub struct RouteInfoDto {
    /// Note: Route listing is informational only. Routes are primarily managed via Rhai scripts.
    pub message: String,
    pub keyboard_route_count: usize,
}

/// List MIDI routes.
///
/// GET /midi/routes
///
/// Note: This returns basic route info. Routes are primarily managed via Rhai scripts.
pub async fn list_routes(State(_state): State<Arc<AppState>>) -> Json<RouteInfoDto> {
    // Routes are managed by the MidiHandler which isn't directly accessible from HTTP.
    // This returns informational status only.
    Json(RouteInfoDto {
        message: "MIDI routes are managed via Rhai scripts. Use POST /midi/route/keyboard to add a simple route.".to_string(),
        keyboard_route_count: 0, // Can't query directly without state access
    })
}

/// Add a keyboard route.
///
/// POST /midi/route/keyboard
pub async fn add_keyboard_route(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddKeyboardRouteRequest>,
) -> Result<Json<AddKeyboardRouteResponse>, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::types::VoiceId;
    use vibelang_core::Message;

    let device_id = MidiDeviceId::new(req.device_id);
    let voice_id = VoiceId::new(req.voice_id);

    state
        .send(Message::Midi(MidiMessage::AddKeyboardRoute {
            device: device_id,
            voice: voice_id,
            channel: req.channel,
            note_min: req.note_min,
            note_max: req.note_max,
            transpose: req.transpose,
        }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to add keyboard route: {}", e),
                }),
            )
        })?;

    Ok(Json(AddKeyboardRouteResponse {
        message: format!(
            "Keyboard route added: device {} -> voice {}",
            req.device_id, req.voice_id
        ),
    }))
}

/// Remove a keyboard route by index.
///
/// DELETE /midi/route/:index
pub async fn remove_keyboard_route(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(index): axum::extract::Path<usize>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    state
        .send(Message::Midi(MidiMessage::RemoveKeyboardRoute { index }))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to remove keyboard route: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Clear all MIDI routes.
///
/// DELETE /midi/routes
pub async fn clear_routes(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    use vibelang_core::message::MidiMessage;
    use vibelang_core::Message;

    state
        .send(Message::Midi(MidiMessage::ClearRoutes))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to clear routes: {}", e),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibelang_core::midi::{MidiEndpointReadiness, MidiReadinessState};

    #[test]
    fn midi_readiness_json_mirrors_order_and_optional_fields() {
        let readiness = MidiReadiness {
            primary_role: "gamma".to_string(),
            endpoints: vec![
                MidiEndpointReadiness {
                    role: "gamma".to_string(),
                    state: MidiReadinessState::Connected,
                    resolved_name: Some("Gamma:Gamma MIDI 1 24:0".to_string()),
                    detail: None,
                },
                MidiEndpointReadiness {
                    role: "f_midi".to_string(),
                    state: MidiReadinessState::Waiting,
                    resolved_name: None,
                    detail: Some("Waiting for exact ALSA MIDI client 'f_midi'".to_string()),
                },
                MidiEndpointReadiness {
                    role: "panel".to_string(),
                    state: MidiReadinessState::Unavailable,
                    resolved_name: None,
                    detail: Some("permission denied".to_string()),
                },
            ],
        };

        let json = serde_json::to_value(readiness).expect("readiness should serialize");
        assert_eq!(json["primary_role"], "gamma");
        assert_eq!(json["endpoints"][0]["state"], "connected");
        assert_eq!(json["endpoints"][1]["state"], "waiting");
        assert_eq!(json["endpoints"][2]["state"], "unavailable");
        assert!(json["endpoints"][0].get("detail").is_none());
        assert_eq!(
            json["endpoints"][0]["resolved_name"],
            "Gamma:Gamma MIDI 1 24:0"
        );
    }
}
