//! Live state endpoint handlers.

use axum::{
    extract::State,
    Json,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use vibelang_core::state::LoopStatus as InternalLoopStatus;
use vibelang_core::FadeTargetType;

use crate::{
    models::{
        ActiveFade, ActiveSequence, ActiveSynth, LiveState, LoopStatus, MeterLevel, TimeSignature,
        TransportState,
    },
    AppState,
};

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

/// GET /live - Get complete live state snapshot
pub async fn get_live_state(
    State(state): State<Arc<AppState>>,
) -> Json<LiveState> {
    let live = state.handle.with_state(|s| {
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

        // Get server timestamp for client-side latency compensation
        let server_time_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // Transport
        let transport = TransportState {
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
            server_time_ms,
        };

        // Active synths
        let active_synths: Vec<ActiveSynth> = s.active_synths.iter().map(|(node_id, info)| {
            ActiveSynth {
                node_id: *node_id,
                synthdef_name: String::new(), // Not stored in ActiveSynth
                voice_name: info.voice_names.first().cloned(),
                group_path: info.group_paths.first().cloned(),
                created_at_beat: None, // Not stored in ActiveSynth
            }
        }).collect();

        // Active sequences - check if sequences exist (may be None if using sequence definitions directly)
        let active_sequences: Vec<ActiveSequence> = s.active_sequences.iter().map(|(name, seq_state)| {
            let loop_beats = s.sequences.get(name).map(|sd| sd.loop_beats).unwrap_or(16.0);
            let current_position = (s.current_beat - seq_state.anchor_beat) % loop_beats;
            let iteration = ((s.current_beat - seq_state.anchor_beat) / loop_beats).floor() as u32;

            // Check if this is play_once by looking at the sequence definition
            let play_once = s.sequences.get(name).map(|sd| sd.play_once).unwrap_or(false);

            ActiveSequence {
                name: name.clone(),
                start_beat: seq_state.anchor_beat,
                current_position,
                loop_beats,
                iteration,
                play_once,
            }
        }).collect();

        // Active fades
        let active_fades: Vec<ActiveFade> = s.fades.iter().enumerate().map(|(i, fo)| {
            // Convert duration from seconds to beats using the current tempo
            let duration_beats = fo.duration_seconds * s.tempo / 60.0;
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

            // Generate an ID from the fade properties
            let id = format!("fade_{}_{}_{}_{}", i, fo.target_name, fo.param_name,
                fo.start_time.elapsed().as_millis() % 10000);

            ActiveFade {
                id,
                name: None,
                target_type: target_type.to_string(),
                target_name: fo.target_name.clone(),
                param_name: fo.param_name.clone(),
                start_value: fo.start_value,
                target_value: fo.target_value,
                current_value,
                duration_beats,
                start_beat: 0.0, // We don't have the original start beat
                progress,
            }
        }).collect();

        // Active notes per voice
        let active_notes: HashMap<String, Vec<u8>> = s.voices.iter()
            .filter(|(_, v)| !v.active_notes.is_empty())
            .map(|(name, v)| (name.clone(), v.active_notes.keys().copied().collect()))
            .collect();

        // Pattern status
        let patterns_status: HashMap<String, LoopStatus> = s.patterns.iter()
            .map(|(name, p)| (name.clone(), loop_status_to_api(&p.status)))
            .collect();

        // Melody status
        let melodies_status: HashMap<String, LoopStatus> = s.melodies.iter()
            .map(|(name, m)| (name.clone(), loop_status_to_api(&m.status)))
            .collect();

        LiveState {
            transport,
            active_synths,
            active_sequences,
            active_fades,
            active_notes,
            patterns_status,
            melodies_status,
        }
    });

    Json(live)
}

/// GET /live/synths - List active synths
pub async fn get_active_synths(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ActiveSynth>> {
    let synths = state.handle.with_state(|s| {
        s.active_synths.iter().map(|(node_id, info)| {
            ActiveSynth {
                node_id: *node_id,
                synthdef_name: String::new(), // Not stored in ActiveSynth
                voice_name: info.voice_names.first().cloned(),
                group_path: info.group_paths.first().cloned(),
                created_at_beat: None, // Not stored in ActiveSynth
            }
        }).collect::<Vec<_>>()
    });

    Json(synths)
}

/// GET /live/sequences - List active sequences
pub async fn get_active_sequences(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ActiveSequence>> {
    let sequences = state.handle.with_state(|s| {
        s.active_sequences.iter().map(|(name, seq_state)| {
            let loop_beats = s.sequences.get(name).map(|sd| sd.loop_beats).unwrap_or(16.0);
            let current_position = (s.current_beat - seq_state.anchor_beat) % loop_beats;
            let iteration = ((s.current_beat - seq_state.anchor_beat) / loop_beats).floor() as u32;
            let play_once = s.sequences.get(name).map(|sd| sd.play_once).unwrap_or(false);

            ActiveSequence {
                name: name.clone(),
                start_beat: seq_state.anchor_beat,
                current_position,
                loop_beats,
                iteration,
                play_once,
            }
        }).collect::<Vec<_>>()
    });

    Json(sequences)
}

/// GET /live/notes - Get currently active notes
pub async fn get_active_notes(
    State(state): State<Arc<AppState>>,
) -> Json<HashMap<String, Vec<u8>>> {
    let notes = state.handle.with_state(|s| {
        s.voices.iter()
            .filter(|(_, v)| !v.active_notes.is_empty())
            .map(|(name, v)| (name.clone(), v.active_notes.keys().copied().collect()))
            .collect::<HashMap<_, _>>()
    });

    Json(notes)
}

/// GET /live/meters - Get audio meter levels for all groups
///
/// Returns real stereo peak and RMS levels for each group, measured post-fader
/// by the link synth's metering UGens. Data is received via OSC /tr messages
/// from SuperCollider's SendTrig UGens at ~20Hz per group.
pub async fn get_meters(
    State(state): State<Arc<AppState>>,
) -> Json<HashMap<String, MeterLevel>> {
    let meters = state.handle.with_state(|s| {
        let mut levels: HashMap<String, MeterLevel> = HashMap::new();

        for path in s.groups.keys() {
            // Use real meter levels from SuperCollider if available
            if let Some(stored) = s.meter_levels.get(path) {
                // Check if data is stale (older than 200ms = ~4 missed updates at 20Hz)
                let is_stale = stored.last_update
                    .map(|t| t.elapsed().as_millis() > 200)
                    .unwrap_or(true);

                if is_stale {
                    // Decay stale meters to 0
                    levels.insert(path.clone(), MeterLevel {
                        peak_left: 0.0,
                        peak_right: 0.0,
                        rms_left: 0.0,
                        rms_right: 0.0,
                    });
                } else {
                    levels.insert(path.clone(), MeterLevel {
                        peak_left: stored.peak_left,
                        peak_right: stored.peak_right,
                        rms_left: stored.rms_left,
                        rms_right: stored.rms_right,
                    });
                }
            } else {
                // No meter data yet - return 0
                levels.insert(path.clone(), MeterLevel {
                    peak_left: 0.0,
                    peak_right: 0.0,
                    rms_left: 0.0,
                    rms_right: 0.0,
                });
            }
        }

        levels
    });

    Json(meters)
}
