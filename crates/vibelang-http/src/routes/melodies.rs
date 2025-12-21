//! Melodies endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use vibelang_core::api::context::SourceLocation;
use vibelang_core::state::{LoopStatus as InternalLoopStatus, StateMessage};

use crate::{
    models::{ErrorResponse, LoopStatus, Melody, MelodyCreate, MelodyEvent, MelodyUpdate, SourceLocation as ApiSourceLocation, StartRequest, StopRequest},
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

/// Convert internal MelodyState to API Melody model
fn melody_to_api(ms: &vibelang_core::state::MelodyState) -> Melody {
    let (events, loop_beats) = if let Some(ref lp) = ms.loop_pattern {
        let evts = lp.events.iter().map(|e| {
            // Extract note/freq from controls
            let freq = e.controls.iter()
                .find(|(k, _)| k == "freq")
                .map(|(_, v)| *v);
            let duration = e.controls.iter()
                .find(|(k, _)| k == "gate")
                .map(|(_, v)| *v as f64);

            MelodyEvent {
                beat: e.beat,
                note: freq.map(freq_to_note_name).unwrap_or_default(),
                frequency: freq,
                duration,
                velocity: None,
                params: e.controls.iter().cloned().collect(),
            }
        }).collect();
        (evts, lp.loop_length_beats)
    } else {
        (vec![], 4.0)
    };

    Melody {
        name: ms.name.clone(),
        voice_name: ms.voice_name.clone().unwrap_or_default(),
        group_path: ms.group_path.clone(),
        loop_beats,
        events,
        params: ms.params.clone(),
        status: loop_status_to_api(&ms.status),
        is_looping: ms.is_looping,
        source_location: source_location_to_api(&ms.source_location),
        notes_patterns: ms.notes_patterns.clone(),
    }
}

/// Convert frequency to note name (approximate)
fn freq_to_note_name(freq: f32) -> String {
    let notes = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let midi_note = (12.0 * (freq / 440.0).log2() + 69.0).round() as i32;
    if !(0..=127).contains(&midi_note) {
        return format!("{:.1}Hz", freq);
    }
    let octave = (midi_note / 12) - 1;
    let note_index = (midi_note % 12) as usize;
    format!("{}{}", notes[note_index], octave)
}

/// GET /melodies - List all melodies
pub async fn list_melodies(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Melody>> {
    let melodies = state.handle.with_state(|s| {
        s.melodies.values().map(melody_to_api).collect::<Vec<_>>()
    });

    Json(melodies)
}

/// POST /melodies - Create a new melody
pub async fn create_melody(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MelodyCreate>,
) -> Result<(StatusCode, Json<Melody>), (StatusCode, Json<ErrorResponse>)> {
    // Check if melody already exists
    let exists = state.handle.with_state(|s| s.melodies.contains_key(&req.name));
    if exists {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::conflict(&format!("Melody '{}' already exists", req.name))),
        ));
    }

    // Check if voice exists and get its synthdef name
    let voice_info = state.handle.with_state(|s| {
        s.voices.get(&req.voice_name).map(|v| (v.group_path.clone(), v.synth_name.clone()))
    });
    let (voice_group_path, voice_synth_name) = match voice_info {
        Some((gp, sn)) => (gp, sn.unwrap_or_default()),
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::not_found(&format!("Voice '{}' not found", req.voice_name))),
            ));
        }
    };

    // Get group path from voice if not specified
    let group_path = req.group_path.unwrap_or(voice_group_path);

    // Determine notes_patterns from lanes or melody_string
    let notes_patterns: Vec<String> = if let Some(lanes) = &req.lanes {
        lanes.clone()
    } else if let Some(melody_str) = &req.melody_string {
        vec![melody_str.clone()]
    } else {
        vec![]
    };

    // Build events from either events array or lanes/melody_string
    let beat_events: Vec<vibelang_core::events::BeatEvent> = if !notes_patterns.is_empty() {
        // Parse all lanes and combine events
        notes_patterns.iter()
            .flat_map(|lane| parse_melody_string(lane, req.loop_beats, &voice_synth_name))
            .collect()
    } else {
        req.events.iter().map(|e| {
            let mut controls: Vec<(String, f32)> = e.params.iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            if let Some(f) = e.frequency {
                controls.push(("freq".to_string(), f));
            }
            if let Some(d) = e.duration {
                controls.push(("gate".to_string(), d as f32));
            }
            let mut evt = vibelang_core::events::BeatEvent::new(e.beat, &voice_synth_name);
            evt.controls = controls;
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

    // Create the melody
    if let Err(e) = state.handle.send(StateMessage::CreateMelody {
        name: req.name.clone(),
        group_path: group_path.clone(),
        voice_name: Some(req.voice_name.clone()),
        pattern,
        source_location: SourceLocation::new(None, None, None),
        notes_patterns,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to create melody: {}", e))),
        ));
    }

    // Set any params
    for (param_name, value) in &req.params {
        let _ = state.handle.send(StateMessage::SetMelodyParam {
            name: req.name.clone(),
            param: param_name.clone(),
            value: *value,
        });
    }

    // Return the created melody
    let melody = state.handle.with_state(|s| s.melodies.get(&req.name).map(melody_to_api));

    match melody {
        Some(m) => Ok((StatusCode::CREATED, Json(m))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal("Melody created but not found in state")),
        )),
    }
}

/// Parse a simple melody string like "C4 D4 E4 F4"
fn parse_melody_string(melody: &str, loop_beats: f64, synth_def: &str) -> Vec<vibelang_core::events::BeatEvent> {
    let notes: Vec<&str> = melody.split_whitespace().collect();
    if notes.is_empty() {
        return vec![];
    }

    let beat_per_note = loop_beats / notes.len() as f64;
    notes.iter().enumerate()
        .filter_map(|(i, note)| {
            if *note == "-" || *note == "." || note.is_empty() {
                return None;
            }
            let freq = note_name_to_freq(note)?;
            let mut evt = vibelang_core::events::BeatEvent::new(i as f64 * beat_per_note, synth_def);
            evt.controls = vec![
                ("freq".to_string(), freq),
                ("gate".to_string(), beat_per_note as f32 * 0.9),
            ];
            Some(evt)
        })
        .collect()
}

/// Convert note name to frequency
fn note_name_to_freq(note: &str) -> Option<f32> {
    let note = note.trim();
    if note.is_empty() {
        return None;
    }

    // Parse note name and octave (e.g., "C4", "D#5", "Bb3")
    let mut chars = note.chars().peekable();
    let base_note = chars.next()?;

    let accidental = if chars.peek() == Some(&'#') {
        chars.next();
        1
    } else if chars.peek() == Some(&'b') {
        chars.next();
        -1
    } else {
        0
    };

    let octave: i32 = chars.collect::<String>().parse().ok()?;

    let base_semitone = match base_note.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    let midi_note = (octave + 1) * 12 + base_semitone + accidental;
    let freq = 440.0 * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0);
    Some(freq)
}

/// GET /melodies/:name - Get melody by name
pub async fn get_melody(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Melody>, (StatusCode, Json<ErrorResponse>)> {
    let melody = state.handle.with_state(|s| s.melodies.get(&name).map(melody_to_api));

    match melody {
        Some(m) => Ok(Json(m)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Melody '{}' not found", name))),
        )),
    }
}

/// PATCH /melodies/:name - Update melody
pub async fn update_melody(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(update): Json<MelodyUpdate>,
) -> Result<Json<Melody>, (StatusCode, Json<ErrorResponse>)> {
    // Check if melody exists and get current data
    let current = state.handle.with_state(|s| s.melodies.get(&name).cloned());
    let current = match current {
        Some(m) => m,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::not_found(&format!("Melody '{}' not found", name))),
            ));
        }
    };

    // Get the voice's synthdef name, falling back to the existing events' synthdef
    let voice_synth_name = if let Some(ref voice_name) = current.voice_name {
        state.handle.with_state(|s| {
            s.voices.get(voice_name)
                .and_then(|v| v.synth_name.clone())
        })
    } else {
        None
    };

    // If voice lookup failed, try to get synthdef from existing events
    let synth_def = voice_synth_name.unwrap_or_else(|| {
        current.loop_pattern.as_ref()
            .and_then(|lp| lp.events.first())
            .map(|e| e.synth_def.clone())
            .unwrap_or_default()
    });

    // Get current loop_length_beats from loop_pattern
    let current_loop_beats = current.loop_pattern.as_ref().map(|lp| lp.loop_length_beats).unwrap_or(4.0);
    let loop_beats = update.loop_beats.unwrap_or(current_loop_beats);

    // Determine notes_patterns from lanes, melody_string, or existing
    let notes_patterns: Vec<String> = if let Some(lanes) = &update.lanes {
        lanes.clone()
    } else if let Some(melody_str) = &update.melody_string {
        vec![melody_str.clone()]
    } else {
        current.notes_patterns.clone()
    };

    // Build new events
    let beat_events: Vec<vibelang_core::events::BeatEvent> = if update.lanes.is_some() || update.melody_string.is_some() {
        // Parse all lanes and combine events
        notes_patterns.iter()
            .flat_map(|lane| parse_melody_string(lane, loop_beats, &synth_def))
            .collect()
    } else if let Some(evts) = &update.events {
        evts.iter().map(|e| {
            let mut controls: Vec<(String, f32)> = e.params.iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            if let Some(f) = e.frequency {
                controls.push(("freq".to_string(), f));
            }
            if let Some(d) = e.duration {
                controls.push(("gate".to_string(), d as f32));
            }
            let mut evt = vibelang_core::events::BeatEvent::new(e.beat, &synth_def);
            evt.controls = controls;
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

    // Delete and recreate the melody with new data
    let _ = state.handle.send(StateMessage::DeleteMelody { name: name.clone() });

    if let Err(e) = state.handle.send(StateMessage::CreateMelody {
        name: name.clone(),
        group_path: current.group_path,
        voice_name: current.voice_name,
        pattern,
        source_location: SourceLocation::new(None, None, None),
        notes_patterns,
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to update melody: {}", e))),
        ));
    }

    // Set params
    let params_to_set = if update.params.is_empty() {
        current.params.clone()
    } else {
        update.params.iter().map(|(k, v)| (k.clone(), *v)).collect()
    };
    for (param_name, value) in params_to_set {
        let _ = state.handle.send(StateMessage::SetMelodyParam {
            name: name.clone(),
            param: param_name,
            value,
        });
    }

    get_melody(State(state), Path(name)).await
}

/// DELETE /melodies/:name - Delete a melody
pub async fn delete_melody(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.melodies.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Melody '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::DeleteMelody { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to delete melody: {}", e))),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /melodies/:name/start - Start a melody
pub async fn start_melody(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(_req): Json<Option<StartRequest>>,
) -> Result<Json<Melody>, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.melodies.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Melody '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::StartMelody { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to start melody: {}", e))),
        ));
    }

    get_melody(State(state), Path(name)).await
}

/// POST /melodies/:name/stop - Stop a melody
pub async fn stop_melody(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(_req): Json<Option<StopRequest>>,
) -> Result<Json<Melody>, (StatusCode, Json<ErrorResponse>)> {
    let exists = state.handle.with_state(|s| s.melodies.contains_key(&name));
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found(&format!("Melody '{}' not found", name))),
        ));
    }

    if let Err(e) = state.handle.send(StateMessage::StopMelody { name: name.clone() }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal(&format!("Failed to stop melody: {}", e))),
        ));
    }

    get_melody(State(state), Path(name)).await
}
