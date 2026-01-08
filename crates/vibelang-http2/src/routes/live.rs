//! Live state endpoint handlers.

use axum::{extract::State, Json};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use vibelang_core2::{Clip, FadeTarget};

use crate::{
    models::{
        ActiveFade, Effect, Group, LiveState, Melody, MelodyNote, Pattern, PatternStep, Sequence,
        SequenceClip, TimeSignature, TransportState, Voice,
    },
    AppState,
};

/// GET /live - Get complete live state
pub async fn get_live_state(State(state): State<Arc<AppState>>) -> Json<LiveState> {
    let live = state
        .with_state(|s| {
            let server_time_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            let transport = TransportState {
                bpm: s.tempo,
                time_signature: TimeSignature {
                    numerator: s.time_sig.numerator,
                    denominator: s.time_sig.denominator,
                },
                running: s.playing,
                current_beat: s.current_beat.to_f64(),
                server_time_ms,
            };

            let groups: Vec<Group> = s
                .groups
                .iter()
                .map(|(id, gs)| Group {
                    id: id.raw().to_string(),
                    parent_id: gs.parent.as_ref().map(|p| p.raw().to_string()),
                    node_id: gs.node_id.raw() as i32,
                    audio_bus: gs.audio_bus.raw() as i32,
                    link_synth_node_id: gs.link_synth_node_id.map(|n| n.raw() as i32),
                    muted: gs.muted,
                    soloed: gs.soloed,
                    params: gs.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                })
                .collect();

            let voices: Vec<Voice> = s
                .voices
                .iter()
                .map(|(id, vs)| Voice {
                    id: id.raw().to_string(),
                    synthdef: Some(vs.config.synthdef.clone()),
                    group_id: vs.config.group.raw().to_string(),
                    polyphony: vs.config.polyphony,
                    gain: vs.config.params.get("amp").copied().unwrap_or(1.0),
                    muted: vs.config.muted,
                    params: vs.config.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                    sfz_instrument: vs.config.sfz_instrument.map(|s| s.raw().to_string()),
                    active_notes: vs.note_nodes.keys().copied().collect(),
                })
                .collect();

            let patterns: Vec<Pattern> = s
                .patterns
                .iter()
                .map(|(id, ps)| Pattern {
                    id: id.raw().to_string(),
                    voice_id: ps.config.voice.map(|v| v.raw().to_string()).unwrap_or_default(),
                    loop_beats: ps.config.length.to_f64(),
                    steps: ps
                        .config
                        .steps
                        .iter()
                        .map(|step| PatternStep {
                            beat: step.beat.to_f64(),
                            params: step.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                        })
                        .collect(),
                    playing: ps.playing,
                    loop_position: ps.loop_position.to_f64(),
                })
                .collect();

            let melodies: Vec<Melody> = s
                .melodies
                .iter()
                .map(|(id, ms)| Melody {
                    id: id.raw().to_string(),
                    voice_id: ms.config.voice.map(|v| v.raw().to_string()).unwrap_or_default(),
                    loop_beats: ms.config.length.to_f64(),
                    notes: ms
                        .config
                        .notes
                        .iter()
                        .map(|n| MelodyNote {
                            beat: n.beat.to_f64(),
                            note: n.note,
                            velocity: n.velocity,
                            gate: n.duration.to_f64(),
                            params: std::collections::HashMap::new(),
                        })
                        .collect(),
                    playing: ms.playing,
                    loop_position: ms.loop_position.to_f64(),
                })
                .collect();

            let sequences: Vec<Sequence> = s
                .sequences
                .iter()
                .map(|(id, ss)| Sequence {
                    id: id.raw().to_string(),
                    loop_beats: ss.config.length.to_f64(),
                    clips: ss
                        .config
                        .clips
                        .iter()
                        .map(|c| match c {
                            Clip::Pattern { id, start, end } => SequenceClip {
                                clip_type: "pattern".to_string(),
                                target_id: id.raw().to_string(),
                                start_beat: start.to_f64(),
                                end_beat: end.to_f64(),
                            },
                            Clip::Melody { id, start, end } => SequenceClip {
                                clip_type: "melody".to_string(),
                                target_id: id.raw().to_string(),
                                start_beat: start.to_f64(),
                                end_beat: end.to_f64(),
                            },
                            Clip::Fade { start, .. } => SequenceClip {
                                clip_type: "fade".to_string(),
                                target_id: String::new(),
                                start_beat: start.to_f64(),
                                end_beat: start.to_f64(),
                            },
                            Clip::Sequence { id, start } => SequenceClip {
                                clip_type: "sequence".to_string(),
                                target_id: id.raw().to_string(),
                                start_beat: start.to_f64(),
                                end_beat: start.to_f64(),
                            },
                        })
                        .collect(),
                    playing: ss.playing,
                    paused: ss.paused,
                    looping: ss.looping,
                    position: ss.position.to_f64(),
                })
                .collect();

            let effects: Vec<Effect> = s
                .effects
                .iter()
                .map(|(id, es)| Effect {
                    id: id.raw().to_string(),
                    group_id: es.group.raw().to_string(),
                    synthdef: es.synthdef.clone(),
                    node_id: es.node_id.raw() as i32,
                    params: es.params.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                })
                .collect();

            let now = Instant::now();
            let active_fades: Vec<ActiveFade> = s
                .active_fades
                .iter()
                .map(|af| {
                    let (target_type, target_id) = match &af.config.target {
                        FadeTarget::Group(g) => ("group".to_string(), g.raw().to_string()),
                        FadeTarget::Voice(v) => ("voice".to_string(), v.raw().to_string()),
                        FadeTarget::Pattern(p) => ("pattern".to_string(), p.raw().to_string()),
                        FadeTarget::Melody(m) => ("melody".to_string(), m.raw().to_string()),
                        FadeTarget::Effect(e) => ("effect".to_string(), e.raw().to_string()),
                    };
                    ActiveFade {
                        target_type,
                        target_id,
                        param: af.config.param.clone(),
                        from: af.start_value,
                        to: af.config.to,
                        duration_beats: af.config.duration.to_beats(),
                        curve: format!("{:?}", af.config.curve),
                        current_value: af.current_value(now, s.tempo),
                    }
                })
                .collect();

            LiveState {
                transport,
                groups,
                voices,
                patterns,
                melodies,
                sequences,
                effects,
                active_fades,
            }
        })
        .await;

    Json(live)
}

/// GET /live/transport - Get transport state only
pub async fn get_transport_state(State(state): State<Arc<AppState>>) -> Json<TransportState> {
    let transport = state
        .with_state(|s| {
            let server_time_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            TransportState {
                bpm: s.tempo,
                time_signature: TimeSignature {
                    numerator: s.time_sig.numerator,
                    denominator: s.time_sig.denominator,
                },
                running: s.playing,
                current_beat: s.current_beat.to_f64(),
                server_time_ms,
            }
        })
        .await;

    Json(transport)
}

/// GET /live/fades - Get active fades
pub async fn get_active_fades(State(state): State<Arc<AppState>>) -> Json<Vec<ActiveFade>> {
    let fades = state
        .with_state(|s| {
            let now = Instant::now();
            s.active_fades
                .iter()
                .map(|af| {
                    let (target_type, target_id) = match &af.config.target {
                        FadeTarget::Group(g) => ("group".to_string(), g.raw().to_string()),
                        FadeTarget::Voice(v) => ("voice".to_string(), v.raw().to_string()),
                        FadeTarget::Pattern(p) => ("pattern".to_string(), p.raw().to_string()),
                        FadeTarget::Melody(m) => ("melody".to_string(), m.raw().to_string()),
                        FadeTarget::Effect(e) => ("effect".to_string(), e.raw().to_string()),
                    };
                    ActiveFade {
                        target_type,
                        target_id,
                        param: af.config.param.clone(),
                        from: af.start_value,
                        to: af.config.to,
                        duration_beats: af.config.duration.to_beats(),
                        curve: format!("{:?}", af.config.curve),
                        current_value: af.current_value(now, s.tempo),
                    }
                })
                .collect::<Vec<_>>()
        })
        .await;

    Json(fades)
}
