//! MIDI endpoint handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::state::StateMessage;

use crate::http_server::{
    models::{
        CcRoute, ErrorResponse, ExportQuery, KeyboardRoute, MidiCallback, MidiConnectRequest,
        MidiDeviceInfo, MidiDeviceState, MidiDevicesResponse, MidiRecordingState,
        MidiRecordingUpdate, MidiRouting, MonitorRequest, NoteRoute, RecordedMidiNote,
        RecordedNotesQuery,
    },
    AppState,
};

/// GET /midi/devices - List available and connected MIDI devices
pub async fn list_devices(
    State(state): State<Arc<AppState>>,
) -> Json<MidiDevicesResponse> {
    let (available, connected) = state.handle.with_state(|s| {
        // Get connected devices from state
        let connected: Vec<MidiDeviceState> = s.midi_config.devices.values().map(|d| {
            MidiDeviceState {
                id: d.id,
                info: MidiDeviceInfo {
                    name: d.info.name.clone(),
                    port_index: d.info.port_index,
                    backend: format!("{:?}", d.backend).to_lowercase(),
                },
                backend: format!("{:?}", d.backend).to_lowercase(),
            }
        }).collect();

        // For available devices, we'd need to scan - for now return connected as available too
        let available: Vec<MidiDeviceInfo> = connected.iter().map(|d| d.info.clone()).collect();

        (available, connected)
    });

    Json(MidiDevicesResponse { available, connected })
}

/// POST /midi/devices/:id - Connect to a MIDI device
pub async fn connect_device(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u32>,
    Json(req): Json<Option<MidiConnectRequest>>,
) -> Result<Json<MidiDeviceState>, (StatusCode, Json<ErrorResponse>)> {
    let backend_str = req.map(|r| r.backend).unwrap_or_else(|| "alsa".to_string());

    // Parse backend
    let midi_backend = match backend_str.to_lowercase().as_str() {
        "alsa" => vibelang_core::midi::MidiBackend::Alsa,
        "jack" => vibelang_core::midi::MidiBackend::Jack,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request("Invalid backend. Must be 'alsa' or 'jack'")),
            ));
        }
    };

    // Create MidiDeviceInfo for the message
    let info = vibelang_core::midi::MidiDeviceInfo {
        name: format!("Device {}", id),
        port_index: id as usize,
        backend: midi_backend.clone(),
    };

    // Send connect message
    if let Err(e) = state.handle.send(StateMessage::MidiOpenDevice {
        device_id: id,
        info,
        backend: midi_backend,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to connect device: {}", e))),
        ));
    }

    // Wait a bit for connection
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Return device state
    let device = state.handle.with_state(|s| {
        s.midi_config.devices.get(&id).map(|d| MidiDeviceState {
            id: d.id,
            info: MidiDeviceInfo {
                name: d.info.name.clone(),
                port_index: d.info.port_index,
                backend: format!("{:?}", d.backend).to_lowercase(),
            },
            backend: format!("{:?}", d.backend).to_lowercase(),
        })
    });

    match device {
        Some(d) => Ok(Json(d)),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal("Device connect message sent but device not found in state")),
        )),
    }
}

/// DELETE /midi/devices/:id - Disconnect a MIDI device
pub async fn disconnect_device(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<u32>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Note: MidiCloseDevice message would need to be implemented
    // For now, just return OK
    Ok(StatusCode::NO_CONTENT)
}

/// Extract target type and name from CcTarget
fn cc_target_to_parts(target: &vibelang_core::midi::CcTarget) -> (String, String) {
    match target {
        vibelang_core::midi::CcTarget::Voice(name) => ("voice".to_string(), name.clone()),
        vibelang_core::midi::CcTarget::Effect(name) => ("effect".to_string(), name.clone()),
        vibelang_core::midi::CcTarget::Group(name) => ("group".to_string(), name.clone()),
        vibelang_core::midi::CcTarget::Global(name) => ("global".to_string(), name.clone()),
    }
}

/// GET /midi/routing - Get MIDI routing configuration
pub async fn get_routing(
    State(state): State<Arc<AppState>>,
) -> Json<MidiRouting> {
    let routing = state.handle.with_state(|s| {
        let keyboard_routes: Vec<KeyboardRoute> = s.midi_config.routing.keyboard_routes.iter().map(|r| {
            let (low, high) = r.note_range.unwrap_or((0, 127));
            KeyboardRoute {
                channel: r.channel,
                voice_name: r.voice_name.clone(),
                transpose: r.transpose as i32,
                velocity_curve: "linear".to_string(), // Simplified
                note_range_low: low,
                note_range_high: high,
            }
        }).collect();

        let note_routes: Vec<NoteRoute> = s.midi_config.routing.note_routes.iter().map(|((ch, note), r)| {
            NoteRoute {
                channel: *ch,
                note: *note,
                voice_name: r.voice_name.clone(),
                choke_group: r.choke_group.clone(),
            }
        }).collect();

        let cc_routes: Vec<CcRoute> = s.midi_config.routing.cc_routes.iter().flat_map(|((ch, cc), routes)| {
            routes.iter().map(move |r| {
                let (target_type, target_name) = cc_target_to_parts(&r.target);
                CcRoute {
                    channel: *ch,
                    cc_number: *cc,
                    target_type,
                    target_name,
                    param_name: r.param_name.clone(),
                    min_value: r.min_value,
                    max_value: r.max_value,
                }
            })
        }).collect();

        MidiRouting {
            keyboard_routes,
            note_routes,
            cc_routes,
            pitch_bend_routes: vec![],
            aftertouch_routes: vec![],
            choke_groups: s.midi_config.routing.choke_groups.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }
    });

    Json(routing)
}

/// GET /midi/routing/keyboard - List keyboard routes
pub async fn list_keyboard_routes(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<KeyboardRoute>> {
    let routes = state.handle.with_state(|s| {
        s.midi_config.routing.keyboard_routes.iter().map(|r| {
            let (low, high) = r.note_range.unwrap_or((0, 127));
            KeyboardRoute {
                channel: r.channel,
                voice_name: r.voice_name.clone(),
                transpose: r.transpose as i32,
                velocity_curve: "linear".to_string(),
                note_range_low: low,
                note_range_high: high,
            }
        }).collect::<Vec<_>>()
    });

    Json(routes)
}

/// POST /midi/routing/keyboard - Add a keyboard route
pub async fn add_keyboard_route(
    State(state): State<Arc<AppState>>,
    Json(req): Json<KeyboardRoute>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let route = vibelang_core::midi::KeyboardRoute {
        voice_name: req.voice_name,
        channel: req.channel,
        note_range: Some((req.note_range_low, req.note_range_high)),
        transpose: req.transpose as i8,
        velocity_curve: vibelang_core::midi::VelocityCurve::Linear,
    };

    if let Err(e) = state.handle.send(StateMessage::MidiAddKeyboardRoute { route }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to add keyboard route: {}", e))),
        ));
    }

    Ok(StatusCode::CREATED)
}

/// GET /midi/routing/note - List note routes
pub async fn list_note_routes(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<NoteRoute>> {
    let routes = state.handle.with_state(|s| {
        s.midi_config.routing.note_routes.iter().map(|((ch, note), r)| {
            NoteRoute {
                channel: *ch,
                note: *note,
                voice_name: r.voice_name.clone(),
                choke_group: r.choke_group.clone(),
            }
        }).collect::<Vec<_>>()
    });

    Json(routes)
}

/// POST /midi/routing/note - Add a note route
pub async fn add_note_route(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NoteRoute>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let route = vibelang_core::midi::NoteRoute {
        voice_name: req.voice_name,
        channel: Some(req.channel),
        choke_group: req.choke_group,
        velocity_curve: vibelang_core::midi::VelocityCurve::Linear,
        velocity_params: vec![],
    };

    if let Err(e) = state.handle.send(StateMessage::MidiAddNoteRoute {
        channel: Some(req.channel),
        note: req.note,
        route,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to add note route: {}", e))),
        ));
    }

    Ok(StatusCode::CREATED)
}

/// GET /midi/routing/cc - List CC routes
pub async fn list_cc_routes(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<CcRoute>> {
    let routes = state.handle.with_state(|s| {
        s.midi_config.routing.cc_routes.iter().flat_map(|((ch, cc), routes)| {
            routes.iter().map(move |r| {
                let (target_type, target_name) = cc_target_to_parts(&r.target);
                CcRoute {
                    channel: *ch,
                    cc_number: *cc,
                    target_type,
                    target_name,
                    param_name: r.param_name.clone(),
                    min_value: r.min_value,
                    max_value: r.max_value,
                }
            })
        }).collect::<Vec<_>>()
    });

    Json(routes)
}

/// POST /midi/routing/cc - Add a CC route
pub async fn add_cc_route(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CcRoute>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Parse target type
    let target = match req.target_type.to_lowercase().as_str() {
        "group" => vibelang_core::midi::CcTarget::Group(req.target_name.clone()),
        "voice" => vibelang_core::midi::CcTarget::Voice(req.target_name.clone()),
        "effect" => vibelang_core::midi::CcTarget::Effect(req.target_name.clone()),
        "global" => vibelang_core::midi::CcTarget::Global(req.target_name.clone()),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request("Invalid target_type")),
            ));
        }
    };

    let route = vibelang_core::midi::CcRoute {
        target,
        param_name: req.param_name,
        min_value: req.min_value,
        max_value: req.max_value,
        curve: vibelang_core::midi::ParameterCurve::Linear,
        channel: Some(req.channel),
    };

    if let Err(e) = state.handle.send(StateMessage::MidiAddCcRoute {
        channel: Some(req.channel),
        cc_number: req.cc_number,
        route,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to add CC route: {}", e))),
        ));
    }

    Ok(StatusCode::CREATED)
}

/// GET /midi/callbacks - List MIDI callbacks
pub async fn list_callbacks(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<MidiCallback>> {
    let callbacks = state.handle.with_state(|s| {
        s.midi_config.callbacks.values().map(|c| {
            let (callback_type, channel, note, cc_number, threshold, above_threshold) = match &c.callback_type {
                vibelang_core::state::MidiCallbackType::Note { channel, note, on_note_on, .. } => {
                    ("note".to_string(), *channel, Some(*note), None, None, Some(*on_note_on))
                }
                vibelang_core::state::MidiCallbackType::Cc { channel, cc_number, threshold, above_threshold } => {
                    ("cc".to_string(), *channel, None, Some(*cc_number), *threshold, Some(*above_threshold))
                }
            };

            MidiCallback {
                id: c.id,
                callback_type,
                channel,
                note,
                cc_number,
                threshold,
                above_threshold,
            }
        }).collect::<Vec<_>>()
    });

    Json(callbacks)
}

/// GET /midi/recording - Get MIDI recording state
pub async fn get_recording_state(
    State(state): State<Arc<AppState>>,
) -> Json<MidiRecordingState> {
    let recording = state.handle.with_state(|s| MidiRecordingState {
        recording_enabled: s.midi_recording.recording_enabled,
        quantization: s.midi_recording.quantization,
        max_history_bars: s.midi_recording.max_history_bars,
        note_count: s.midi_recording.notes.len(),
        oldest_beat: s.midi_recording.oldest_beat,
        pending_notes: s.midi_recording.pending_notes.len(),
    });

    Json(recording)
}

/// PATCH /midi/recording - Update MIDI recording settings
pub async fn update_recording_settings(
    State(state): State<Arc<AppState>>,
    Json(update): Json<MidiRecordingUpdate>,
) -> Result<Json<MidiRecordingState>, (StatusCode, Json<ErrorResponse>)> {
    if let Some(enabled) = update.recording_enabled {
        if let Err(e) = state.handle.send(StateMessage::MidiSetRecordingEnabled { enabled }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to update recording: {}", e))),
            ));
        }
    }

    if let Some(quantization) = update.quantization {
        if ![4, 8, 16, 32, 64].contains(&quantization) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request("Quantization must be 4, 8, 16, 32, or 64")),
            ));
        }
        if let Err(e) = state.handle.send(StateMessage::MidiSetRecordingQuantization {
            positions_per_bar: quantization,
        }) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&format!("Failed to update quantization: {}", e))),
            ));
        }
    }

    // Note: max_history_bars is not currently supported via StateMessage
    // if let Some(_max_bars) = update.max_history_bars { }

    Ok(get_recording_state(State(state)).await)
}

/// GET /midi/recording/notes - Get recorded MIDI notes
pub async fn get_recorded_notes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RecordedNotesQuery>,
) -> Json<Vec<RecordedMidiNote>> {
    let notes = state.handle.with_state(|s| {
        s.midi_recording.notes.iter()
            .filter(|n| {
                if let Some(start) = query.start_beat {
                    if n.beat < start {
                        return false;
                    }
                }
                if let Some(end) = query.end_beat {
                    if n.beat >= end {
                        return false;
                    }
                }
                if let Some(ref voice) = query.voice {
                    if &n.voice_name != voice {
                        return false;
                    }
                }
                true
            })
            .map(|n| RecordedMidiNote {
                beat: n.beat,
                note: n.note,
                velocity: n.velocity,
                duration: n.duration,
                raw_beat: n.raw_beat,
                channel: n.channel,
                voice_name: n.voice_name.clone(),
            })
            .collect::<Vec<_>>()
    });

    Json(notes)
}

/// GET /midi/recording/export - Export recorded notes as VibeLang syntax
pub async fn export_recording(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExportQuery>,
) -> String {
    let bars = query.bars.unwrap_or(4);
    let beats_per_bar = state.handle.with_state(|s| s.time_signature.beats_per_bar());

    let notes = state.handle.with_state(|s| {
        let current_beat = s.current_beat;
        let start_beat = query.start_beat.unwrap_or(current_beat - (bars as f64 * beats_per_bar));
        let end_beat = start_beat + (bars as f64 * beats_per_bar);

        s.midi_recording.notes.iter()
            .filter(|n| {
                n.beat >= start_beat && n.beat < end_beat &&
                query.voice.as_ref().map(|v| &n.voice_name == v).unwrap_or(true)
            })
            .map(|n| RecordedMidiNote {
                beat: n.beat - start_beat, // Normalize to 0-based
                note: n.note,
                velocity: n.velocity,
                duration: n.duration,
                raw_beat: n.raw_beat,
                channel: n.channel,
                voice_name: n.voice_name.clone(),
            })
            .collect::<Vec<_>>()
    });

    if query.format == "pattern" {
        // Export as pattern syntax
        export_as_pattern(&notes, bars as f64 * beats_per_bar)
    } else {
        // Export as melody syntax
        export_as_melody(&notes, bars as f64 * beats_per_bar)
    }
}

fn export_as_pattern(notes: &[RecordedMidiNote], _loop_beats: f64) -> String {
    let mut output = String::from("// Pattern export\n");
    for note in notes {
        output.push_str(&format!("// beat {:.2}: note={}, vel={}\n", note.beat, note.note, note.velocity));
    }
    output
}

fn export_as_melody(notes: &[RecordedMidiNote], _loop_beats: f64) -> String {
    let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

    let mut output = String::from("// Melody export\n\"");
    for note in notes {
        let octave = (note.note / 12) as i32 - 1;
        let note_idx = (note.note % 12) as usize;
        output.push_str(&format!("{}{} ", note_names[note_idx], octave));
    }
    output.push('"');
    output
}

/// POST /midi/monitor - Enable/disable MIDI monitoring
pub async fn set_monitor(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MonitorRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = state.handle.send(StateMessage::MidiSetMonitoring { enabled: req.enabled }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to set monitor: {}", e))),
        ));
    }

    Ok(StatusCode::OK)
}
