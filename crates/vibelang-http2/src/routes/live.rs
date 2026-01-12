//! Live state endpoint handlers.

use axum::{extract::State, Json};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use vibelang_core2::FadeTarget;

use crate::{
    models::{
        ActiveFade, ActiveSequence, ActiveSynth, FadeTargetType, LiveState, LoopState, LoopStatus,
        MeterLevel, MeterLevels, TimeSignature, TransportState,
    },
    AppState,
};

/// Build transport state with loop information
fn build_transport_state(s: &vibelang_core2::State) -> TransportState {
    let server_time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Calculate loop_beats from longest active sequence
    let loop_beats = s
        .sequences
        .values()
        .filter(|seq| seq.playing)
        .map(|seq| seq.config.length.to_f64())
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let loop_beat = loop_beats.map(|lb| s.current_beat.to_f64() % lb);

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

/// GET /live - Get complete live state
pub async fn get_live_state(State(state): State<Arc<AppState>>) -> Json<LiveState> {
    let live = state
        .with_state(|s| {
            let transport = build_transport_state(s);

            // Build active synths from all running voice nodes
            let active_synths: Vec<ActiveSynth> = s
                .voices
                .iter()
                .flat_map(|(voice_id, vs)| {
                    vs.active_nodes.iter().map(move |node_id| ActiveSynth {
                        node_id: node_id.raw() as i32,
                        synthdef_name: vs.config.synthdef.clone(),
                        voice_name: Some(voice_id.raw().to_string()),
                        group_path: Some(vs.config.group.raw().to_string()),
                        created_at_beat: None,
                    })
                })
                .collect();

            // Build active sequences
            let active_sequences: Vec<ActiveSequence> = s
                .sequences
                .iter()
                .filter(|(_, ss)| ss.playing)
                .map(|(id, ss)| ActiveSequence {
                    name: id.raw().to_string(),
                    start_beat: 0.0, // Not tracked in core
                    current_position: ss.position.to_f64(),
                    loop_beats: ss.config.length.to_f64(),
                    iteration: None,
                    play_once: Some(!ss.looping),
                })
                .collect();

            // Build active fades
            let now = Instant::now();
            let active_fades: Vec<ActiveFade> = s
                .active_fades
                .iter()
                .enumerate()
                .map(|(idx, af)| {
                    let (target_type, target_name) = match &af.config.target {
                        FadeTarget::Group(g) => (FadeTargetType::Group, g.raw().to_string()),
                        FadeTarget::Voice(v) => (FadeTargetType::Voice, v.raw().to_string()),
                        FadeTarget::Pattern(p) => (FadeTargetType::Group, p.raw().to_string()), // Map to group type
                        FadeTarget::Melody(m) => (FadeTargetType::Group, m.raw().to_string()),
                        FadeTarget::Effect(e) => (FadeTargetType::Effect, e.raw().to_string()),
                    };

                    let current = af.current_value(now, s.tempo);
                    let progress = if af.config.duration.to_beats() > 0.0 {
                        (current - af.start_value) / (af.config.to - af.start_value)
                    } else {
                        1.0
                    };

                    ActiveFade {
                        id: format!("fade_{}", idx),
                        name: None,
                        target_type,
                        target_name,
                        param_name: af.config.param.clone(),
                        start_value: af.start_value,
                        target_value: af.config.to,
                        current_value: Some(current),
                        duration_beats: af.config.duration.to_beats(),
                        start_beat: None,
                        progress: progress.clamp(0.0, 1.0) as f64,
                    }
                })
                .collect();

            // Build active notes map (voice name -> active note numbers)
            let active_notes: HashMap<String, Vec<u8>> = s
                .voices
                .iter()
                .filter(|(_, vs)| !vs.note_nodes.is_empty())
                .map(|(id, vs)| {
                    (
                        id.raw().to_string(),
                        vs.note_nodes.keys().copied().collect(),
                    )
                })
                .collect();

            // Build patterns status
            let patterns_status: HashMap<String, LoopStatus> = s
                .patterns
                .iter()
                .map(|(id, ps)| {
                    (
                        id.raw().to_string(),
                        LoopStatus {
                            state: if ps.playing {
                                LoopState::Playing
                            } else {
                                LoopState::Stopped
                            },
                            start_beat: None,
                            stop_beat: None,
                        },
                    )
                })
                .collect();

            // Build melodies status
            let melodies_status: HashMap<String, LoopStatus> = s
                .melodies
                .iter()
                .map(|(id, ms)| {
                    (
                        id.raw().to_string(),
                        LoopStatus {
                            state: if ms.playing {
                                LoopState::Playing
                            } else {
                                LoopState::Stopped
                            },
                            start_beat: None,
                            stop_beat: None,
                        },
                    )
                })
                .collect();

            LiveState {
                transport,
                active_synths,
                active_sequences,
                active_fades,
                active_notes: if active_notes.is_empty() {
                    None
                } else {
                    Some(active_notes)
                },
                patterns_status: Some(patterns_status),
                melodies_status: Some(melodies_status),
            }
        })
        .await;

    Json(live)
}

/// GET /live/transport - Get transport state only
pub async fn get_transport_state(State(state): State<Arc<AppState>>) -> Json<TransportState> {
    let transport = state.with_state(build_transport_state).await;
    Json(transport)
}

/// GET /live/fades - Get active fades
pub async fn get_active_fades(State(state): State<Arc<AppState>>) -> Json<Vec<ActiveFade>> {
    let fades = state
        .with_state(|s| {
            let now = Instant::now();
            s.active_fades
                .iter()
                .enumerate()
                .map(|(idx, af)| {
                    let (target_type, target_name) = match &af.config.target {
                        FadeTarget::Group(g) => (FadeTargetType::Group, g.raw().to_string()),
                        FadeTarget::Voice(v) => (FadeTargetType::Voice, v.raw().to_string()),
                        FadeTarget::Pattern(p) => (FadeTargetType::Group, p.raw().to_string()),
                        FadeTarget::Melody(m) => (FadeTargetType::Group, m.raw().to_string()),
                        FadeTarget::Effect(e) => (FadeTargetType::Effect, e.raw().to_string()),
                    };

                    let current = af.current_value(now, s.tempo);
                    let progress = if af.config.duration.to_beats() > 0.0 {
                        (current - af.start_value) / (af.config.to - af.start_value)
                    } else {
                        1.0
                    };

                    ActiveFade {
                        id: format!("fade_{}", idx),
                        name: None,
                        target_type,
                        target_name,
                        param_name: af.config.param.clone(),
                        start_value: af.start_value,
                        target_value: af.config.to,
                        current_value: Some(current),
                        duration_beats: af.config.duration.to_beats(),
                        start_beat: None,
                        progress: progress.clamp(0.0, 1.0) as f64,
                    }
                })
                .collect::<Vec<_>>()
        })
        .await;

    Json(fades)
}

/// GET /live/meters - Get meter levels for all groups
///
/// Returns real-time meter levels from the link synths.
/// The system_link_audio synthdef sends SendTrig messages at ~20Hz with meter data.
/// If meters haven't been updated in 200ms, they decay to 0.
pub async fn get_meters(State(state): State<Arc<AppState>>) -> Json<MeterLevels> {
    let meters = state
        .with_state(|s| {
            s.groups
                .iter()
                .map(|(group_id, group_state)| {
                    // Look up meter levels by the link synth node ID
                    let (peak_l, peak_r, rms_l, rms_r) = group_state
                        .link_synth_node_id
                        .and_then(|node_id| s.meter_levels.get(&node_id))
                        .map(|m| m.decayed())
                        .unwrap_or((0.0, 0.0, 0.0, 0.0));

                    (
                        group_id.raw().to_string(),
                        MeterLevel {
                            peak_left: peak_l,
                            peak_right: peak_r,
                            rms_left: rms_l,
                            rms_right: rms_r,
                        },
                    )
                })
                .collect::<HashMap<_, _>>()
        })
        .await;

    Json(meters)
}
