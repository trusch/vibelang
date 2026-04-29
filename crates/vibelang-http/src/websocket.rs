//! WebSocket handler for real-time updates.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use vibelang_core::{Clip, FadeTarget, SequenceId, State as RuntimeState};

use crate::AppState;

/// WebSocket event sent to clients.
#[derive(Debug, Clone, Serialize)]
pub struct WebSocketEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Client subscription message.
#[derive(Debug, Deserialize)]
struct SubscriptionMessage {
    action: String,
    #[serde(default)]
    events: Vec<String>,
}

enum SendCommand {
    SetSubscriptions(Vec<String>),
    SendHello,
    SendCurrentSnapshot,
}

const WS_PROTOCOL_VERSION: u32 = 1;

/// WebSocket upgrade handler.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle a WebSocket connection.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel
    let mut rx = state.ws_tx.subscribe();

    // Use a channel to communicate subscription updates and immediate snapshots
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<SendCommand>(16);

    // Default subscription patterns (all events)
    let initial_subscriptions = vec!["*".to_string()];
    let send_state = state.clone();

    // Spawn task to send events to client
    let send_task = tokio::spawn(async move {
        let mut subscriptions = initial_subscriptions;
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if is_subscribed(&event.event_type, &subscriptions) {
                                let msg = serde_json::to_string(&event).unwrap_or_default();
                                if sender.send(Message::Text(msg.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        SendCommand::SetSubscriptions(new_subs) => {
                            subscriptions = new_subs;
                        }
                        SendCommand::SendHello => {
                            let msg = serde_json::to_string(&make_hello_event()).unwrap_or_default();
                            if sender.send(Message::Text(msg.into())).await.is_err() {
                                break;
                            }
                        }
                        SendCommand::SendCurrentSnapshot => {
                            if subscriptions.iter().any(|pattern| {
                                is_subscribed("playback.tick", std::slice::from_ref(pattern))
                                    || is_subscribed("playback.bar", std::slice::from_ref(pattern))
                            }) {
                                let event = send_state
                                    .with_state(|runtime| make_snapshot_event("playback.bar", runtime))
                                    .await;
                                let msg = serde_json::to_string(&event).unwrap_or_default();
                                if sender.send(Message::Text(msg.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // Send handshake metadata first so clients can reason about the contract,
    // then kick out an initial snapshot so sidebar/header state appears immediately
    // even when transport is stopped.
    let _ = cmd_tx.send(SendCommand::SendHello).await;
    let _ = cmd_tx.send(SendCommand::SendCurrentSnapshot).await;

    // Track subscriptions locally for updating
    let mut local_subscriptions = vec!["*".to_string()];

    // Handle incoming messages from client
    while let Some(msg) = receiver.next().await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(sub_msg) = serde_json::from_str::<SubscriptionMessage>(&text) {
                match sub_msg.action.as_str() {
                    "subscribe" => {
                        if sub_msg.events.is_empty() {
                            local_subscriptions = vec!["*".to_string()];
                        } else {
                            for pattern in sub_msg.events {
                                if !local_subscriptions.contains(&pattern) {
                                    local_subscriptions.push(pattern);
                                }
                            }
                        }
                        let _ = cmd_tx
                            .send(SendCommand::SetSubscriptions(local_subscriptions.clone()))
                            .await;
                        let _ = cmd_tx.send(SendCommand::SendCurrentSnapshot).await;
                    }
                    "unsubscribe" => {
                        for pattern in &sub_msg.events {
                            local_subscriptions.retain(|s| s != pattern);
                        }
                        let _ = cmd_tx
                            .send(SendCommand::SetSubscriptions(local_subscriptions.clone()))
                            .await;
                    }
                    _ => {}
                }
            }
        }
    }

    send_task.abort();
}

/// Check if an event type matches any subscription pattern.
fn is_subscribed(event_type: &str, subscriptions: &[String]) -> bool {
    for pattern in subscriptions {
        if pattern == "*" {
            return true;
        }
        if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            if event_type.starts_with(prefix) {
                return true;
            }
        } else if pattern == event_type {
            return true;
        }
    }
    false
}

fn make_hello_event() -> WebSocketEvent {
    WebSocketEvent {
        event_type: "hello".to_string(),
        timestamp: ws_timestamp_ms(),
        data: Some(json!({
            "protocol_version": WS_PROTOCOL_VERSION,
            "server": "vibelang-http",
            "capabilities": {
                "events": [
                    "hello",
                    "playback.tick",
                    "playback.bar",
                    "transport.beat",
                    "transport.started",
                    "transport.stopped",
                    "transport.bpm",
                ],
                "commands": ["subscribe", "unsubscribe"],
                "wildcard_subscriptions": true,
                "initial_snapshot_event": "playback.bar",
            },
        })),
    }
}

fn ws_timestamp_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn make_snapshot_event(event_type: &str, state: &RuntimeState) -> WebSocketEvent {
    WebSocketEvent {
        event_type: event_type.to_string(),
        timestamp: ws_timestamp_ms(),
        data: Some(build_playback_snapshot(state)),
    }
}

fn build_playback_snapshot(state: &RuntimeState) -> Value {
    json!({
        "transport": build_transport_payload(state),
        "groups": build_groups_payload(state),
        "voices": build_voices_payload(state),
        "patterns": build_patterns_payload(state),
        "melodies": build_melodies_payload(state),
        "sequences": build_sequences_payload(state),
        "fades": build_fades_payload(state),
    })
}

fn build_transport_payload(state: &RuntimeState) -> Value {
    let beats_per_bar = state.time_sig.numerator.max(1) as f64;
    let current_beat = state.current_beat.to_f64();
    let bar = (current_beat / beats_per_bar).floor() as i64;
    let beat_in_bar = (current_beat % beats_per_bar).floor() as i64;

    json!({
        "playing": state.playing,
        "beat": current_beat,
        "bar": bar,
        "beat_in_bar": beat_in_bar,
        "bpm": state.tempo,
        "time_sig": [state.time_sig.numerator, state.time_sig.denominator],
    })
}

fn build_groups_payload(state: &RuntimeState) -> Vec<Value> {
    let mut groups: Vec<_> = state.groups.values().collect();
    groups.sort_by(|a, b| a.name.cmp(&b.name));

    groups
        .into_iter()
        .map(|group| {
            let mut voices: Vec<String> = state
                .voices
                .values()
                .filter(|voice| voice.config.group == group.id)
                .map(|voice| voice.config.name.clone())
                .collect();
            voices.sort();

            let mut patterns: Vec<String> = state
                .patterns
                .values()
                .filter_map(|pattern| {
                    pattern
                        .content
                        .voice
                        .and_then(|voice_id| state.voices.get(&voice_id))
                        .filter(|voice| voice.config.group == group.id)
                        .map(|_| pattern.content.name.clone())
                })
                .collect();
            patterns.sort();

            let mut melodies: Vec<String> = state
                .melodies
                .values()
                .filter_map(|melody| {
                    melody
                        .content
                        .voice
                        .and_then(|voice_id| state.voices.get(&voice_id))
                        .filter(|voice| voice.config.group == group.id)
                        .map(|_| melody.content.name.clone())
                })
                .collect();
            melodies.sort();

            let meter_peak = group
                .link_synth_node_id
                .and_then(|node_id| state.meter_levels.get(&node_id))
                .map(|meter| meter.peak_left.max(meter.peak_right) as f64);

            json!({
                "name": group.name,
                "parent": group.parent.and_then(|parent_id| state.groups.get(&parent_id).map(|parent| parent.name.clone())),
                "muted": group.muted,
                "soloed": group.soloed,
                "meter_peak": meter_peak,
                "voices": voices,
                "patterns": patterns,
                "melodies": melodies,
                "params": group.params,
            })
        })
        .collect()
}

fn build_voices_payload(state: &RuntimeState) -> Vec<Value> {
    let mut voices: Vec<_> = state.voices.values().collect();
    voices.sort_by(|a, b| a.config.name.cmp(&b.config.name));

    voices
        .into_iter()
        .map(|voice| {
            json!({
                "name": voice.config.name,
                "synth": voice.config.synthdef,
                "muted": voice.config.muted,
                "soloed": voice.config.soloed,
                "group": state.groups.get(&voice.config.group).map(|group| group.name.clone()),
                "active_nodes": voice.active_nodes.iter().map(|id| id.raw()).collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn build_patterns_payload(state: &RuntimeState) -> Vec<Value> {
    let mut patterns: Vec<_> = state.patterns.values().collect();
    patterns.sort_by(|a, b| a.content.name.cmp(&b.content.name));

    patterns
        .into_iter()
        .map(|pattern| {
            json!({
                "name": pattern.content.name,
                "playing": pattern.playing,
                "loop_position": pattern.loop_position.to_f64(),
                "loop_length": pattern.content.length.to_f64(),
            })
        })
        .collect()
}

fn build_melodies_payload(state: &RuntimeState) -> Vec<Value> {
    let mut melodies: Vec<_> = state.melodies.values().collect();
    melodies.sort_by(|a, b| a.content.name.cmp(&b.content.name));

    melodies
        .into_iter()
        .map(|melody| {
            json!({
                "name": melody.content.name,
                "playing": melody.playing,
                "loop_position": melody.loop_position.to_f64(),
                "loop_length": melody.content.length.to_f64(),
            })
        })
        .collect()
}

fn build_sequences_payload(state: &RuntimeState) -> Vec<Value> {
    let mut sequences: Vec<_> = state.sequences.values().collect();
    sequences.sort_by(|a, b| a.config.name.cmp(&b.config.name));

    sequences
        .into_iter()
        .map(|sequence| {
            let mut visited = HashSet::new();
            visited.insert(sequence.id);
            json!({
                "name": sequence.config.name,
                "playing": sequence.playing,
                "paused": sequence.paused,
                "position": sequence.position.to_f64(),
                "length": sequence.config.length.to_f64(),
                "looping": sequence.looping,
                "active_clips": build_active_clips(state, sequence, &mut visited),
            })
        })
        .collect()
}

fn build_active_clips(
    state: &RuntimeState,
    sequence: &vibelang_core::SequenceState,
    visited: &mut HashSet<SequenceId>,
) -> Vec<Value> {
    let position = sequence.position;

    sequence
        .config
        .clips
        .iter()
        .enumerate()
        .filter_map(|(clip_index, clip)| match clip {
            Clip::Pattern { id, start, end } if position >= *start && position < *end => {
                let duration = (*end - *start).to_f64().max(0.0001);
                let progress = ((position - *start).to_f64() / duration).clamp(0.0, 1.0);
                let name = state
                    .patterns
                    .get(id)
                    .map(|pattern| pattern.content.name.clone())
                    .unwrap_or_else(|| id.raw().to_string());
                Some(json!({
                    "type": "pattern",
                    "name": name,
                    "clip_index": clip_index,
                    "progress": progress,
                }))
            }
            Clip::Melody { id, start, end } if position >= *start && position < *end => {
                let duration = (*end - *start).to_f64().max(0.0001);
                let progress = ((position - *start).to_f64() / duration).clamp(0.0, 1.0);
                let name = state
                    .melodies
                    .get(id)
                    .map(|melody| melody.content.name.clone())
                    .unwrap_or_else(|| id.raw().to_string());
                Some(json!({
                    "type": "melody",
                    "name": name,
                    "clip_index": clip_index,
                    "progress": progress,
                }))
            }
            Clip::Fade { start, .. } if position >= *start => Some(json!({
                "type": "fade",
                "name": "fade",
                "clip_index": clip_index,
                "progress": 1.0,
            })),
            Clip::Sequence { id, start } if position >= *start => {
                let nested = state.sequences.get(id)?;
                let name = nested.config.name.clone();
                let mut nested_clips = Vec::new();
                if !visited.contains(id) {
                    visited.insert(*id);
                    nested_clips = build_active_clips(state, nested, visited);
                    visited.remove(id);
                }

                let progress = if nested.config.length > vibelang_core::Beat::ZERO {
                    (nested.position.to_f64() / nested.config.length.to_f64()).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                Some(json!({
                    "type": "sequence",
                    "name": name,
                    "clip_index": clip_index,
                    "progress": progress,
                    "nested_clips": nested_clips,
                }))
            }
            _ => None,
        })
        .collect()
}

fn build_fades_payload(state: &RuntimeState) -> Vec<Value> {
    let now = Instant::now();

    state
        .active_fades
        .iter()
        .map(|fade| {
            let (target_name, target_type) = match &fade.config.target {
                FadeTarget::Group(group_id) => (
                    state
                        .groups
                        .get(group_id)
                        .map(|group| group.name.clone())
                        .unwrap_or_else(|| group_id.raw().to_string()),
                    "group",
                ),
                FadeTarget::Voice(voice_id) => (
                    state
                        .voices
                        .get(voice_id)
                        .map(|voice| voice.config.name.clone())
                        .unwrap_or_else(|| voice_id.raw().to_string()),
                    "voice",
                ),
                FadeTarget::Pattern(pattern_id) => (
                    state
                        .patterns
                        .get(pattern_id)
                        .map(|pattern| pattern.content.name.clone())
                        .unwrap_or_else(|| pattern_id.raw().to_string()),
                    "pattern",
                ),
                FadeTarget::Melody(melody_id) => (
                    state
                        .melodies
                        .get(melody_id)
                        .map(|melody| melody.content.name.clone())
                        .unwrap_or_else(|| melody_id.raw().to_string()),
                    "melody",
                ),
                FadeTarget::Effect(effect_id) => (effect_id.raw().to_string(), "effect"),
            };

            let current_value = fade.current_value(now, state.tempo);
            let progress = if (fade.config.to - fade.start_value).abs() > f32::EPSILON {
                ((current_value - fade.start_value) / (fade.config.to - fade.start_value))
                    .clamp(0.0, 1.0) as f64
            } else {
                1.0
            };

            json!({
                "target_type": target_type,
                "target_name": target_name,
                "param": fade.config.param,
                "start_value": fade.start_value,
                "current_value": current_value,
                "target_value": fade.config.to,
                "progress": progress,
            })
        })
        .collect()
}

/// Background task that polls state and broadcasts events.
pub async fn run_event_broadcaster(
    state: Arc<RwLock<RuntimeState>>,
    tx: broadcast::Sender<WebSocketEvent>,
) {
    let mut last_sixteenth: Option<i64> = None;
    let mut last_bar: Option<i64> = None;
    let mut last_running: Option<bool> = None;
    let mut last_bpm: Option<f64> = None;
    let mut last_idle_snapshot: Option<String> = None;

    let mut interval = tokio::time::interval(Duration::from_millis(50)); // 20 Hz polling

    loop {
        interval.tick().await;

        let (snapshot, current_beat, beats_per_bar, running, bpm) = {
            let guard = state.read().await;
            (
                build_playback_snapshot(&guard),
                guard.current_beat.to_f64(),
                guard.time_sig.numerator.max(1) as f64,
                guard.playing,
                guard.tempo,
            )
        };

        let now = ws_timestamp_ms();
        let sixteenth = (current_beat * 4.0).floor() as i64;
        let bar = (current_beat / beats_per_bar).floor() as i64;
        let running_changed = last_running != Some(running);
        let bpm_changed = last_bpm != Some(bpm);

        let should_emit_tick = if running {
            last_sixteenth != Some(sixteenth) || running_changed || bpm_changed
        } else {
            let snapshot_text = serde_json::to_string(&snapshot).unwrap_or_default();
            let changed = last_idle_snapshot.as_ref() != Some(&snapshot_text);
            if changed {
                last_idle_snapshot = Some(snapshot_text);
            }
            changed || running_changed || bpm_changed
        };

        if should_emit_tick {
            let event_type = if running {
                "playback.tick"
            } else {
                "playback.bar"
            };
            let _ = tx.send(WebSocketEvent {
                event_type: event_type.to_string(),
                timestamp: now,
                data: Some(snapshot.clone()),
            });
        }

        if running && last_bar != Some(bar) {
            let _ = tx.send(WebSocketEvent {
                event_type: "playback.bar".to_string(),
                timestamp: now,
                data: Some(snapshot.clone()),
            });
        }

        // transport.beat remains useful for newer clients that only care about transport edges.
        if running && last_sixteenth != Some(sixteenth) {
            let _ = tx.send(WebSocketEvent {
                event_type: "transport.beat".to_string(),
                timestamp: now,
                data: Some(json!({
                    "beat": current_beat,
                    "bar": bar,
                    "beat_in_bar": (current_beat % beats_per_bar).floor() as i64,
                })),
            });
        }

        if running_changed {
            let event_type = if running {
                "transport.started"
            } else {
                "transport.stopped"
            };
            let _ = tx.send(WebSocketEvent {
                event_type: event_type.to_string(),
                timestamp: now,
                data: Some(json!({
                    "beat": current_beat,
                })),
            });
        }

        if bpm_changed && last_bpm.is_some() {
            let _ = tx.send(WebSocketEvent {
                event_type: "transport.bpm".to_string(),
                timestamp: now,
                data: Some(json!({
                    "bpm": bpm,
                })),
            });
        }

        last_sixteenth = Some(sixteenth);
        last_bar = Some(bar);
        last_running = Some(running);
        last_bpm = Some(bpm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use vibelang_core::{
        Beat, BusId, GroupState, MelodyConfig, MelodyState, NodeId, PatternConfig, PatternState,
        SequenceConfig, SequenceState, VoiceConfig, VoiceState,
    };
    use vibelang_core::{GroupId, PatternId, SequenceId, VoiceId};

    #[test]
    fn subscription_patterns_match_as_expected() {
        assert!(is_subscribed("playback.tick", &["*".to_string()]));
        assert!(is_subscribed(
            "transport.started",
            &["transport.*".to_string()]
        ));
        assert!(!is_subscribed(
            "playback.tick",
            &["transport.*".to_string()]
        ));
        assert!(is_subscribed(
            "playback.bar",
            &["playback.tick".to_string(), "playback.bar".to_string()]
        ));
    }

    #[test]
    fn hello_event_advertises_websocket_contract() {
        let hello = make_hello_event();
        assert_eq!(hello.event_type, "hello");

        let data = hello.data.expect("hello payload");
        assert_eq!(
            data.get("protocol_version").and_then(Value::as_u64),
            Some(WS_PROTOCOL_VERSION as u64)
        );
        assert_eq!(
            data.get("server").and_then(Value::as_str),
            Some("vibelang-http")
        );

        let capabilities = data.get("capabilities").expect("capabilities");
        let events = capabilities
            .get("events")
            .and_then(Value::as_array)
            .expect("events array");
        let event_names: Vec<_> = events.iter().filter_map(Value::as_str).collect();
        assert!(event_names.contains(&"hello"));
        assert!(event_names.contains(&"playback.tick"));
        assert!(event_names.contains(&"playback.bar"));
        assert!(event_names.contains(&"transport.beat"));

        let commands = capabilities
            .get("commands")
            .and_then(Value::as_array)
            .expect("commands array");
        let command_names: Vec<_> = commands.iter().filter_map(Value::as_str).collect();
        assert_eq!(command_names, vec!["subscribe", "unsubscribe"]);

        assert_eq!(
            capabilities
                .get("wildcard_subscriptions")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            capabilities
                .get("initial_snapshot_event")
                .and_then(Value::as_str),
            Some("playback.bar")
        );
    }

    #[test]
    fn playback_snapshot_contains_emacs_compatible_project_state() {
        let group_id = GroupId::new(1);
        let voice_id = VoiceId::new(1);
        let pattern_id = PatternId::new(1);
        let sequence_id = SequenceId::new(1);

        let mut state = RuntimeState::default();
        state.playing = true;
        state.tempo = 128.0;
        state.current_beat = Beat::from_f64(5.25);

        state.groups.insert(
            group_id,
            GroupState {
                id: group_id,
                name: "drums".to_string(),
                parent: None,
                node_id: NodeId::new(1000),
                audio_bus: BusId::new(16),
                link_synth_node_id: Some(NodeId::new(1001)),
                muted: false,
                soloed: false,
                params: HashMap::new(),
                output_bus: None,
            },
        );

        state.meter_levels.insert(
            NodeId::new(1001),
            vibelang_core::MeterLevel {
                peak_left: 0.7,
                peak_right: 0.6,
                rms_left: 0.4,
                rms_right: 0.4,
                last_update: None,
            },
        );

        state.voices.insert(
            voice_id,
            VoiceState {
                id: voice_id,
                config: VoiceConfig {
                    name: "kick_voice".to_string(),
                    synthdef: "bd".to_string(),
                    group: group_id,
                    polyphony: 1,
                    params: HashMap::new(),
                    muted: false,
                    soloed: false,
                    sfz_instrument: None,
                    sample_id: None,
                    trigger_mode: "gate".to_string(),
                    round_robin_count: 0,
                    choke_group: None,
                    modulations: HashMap::new(),
                    midi_output: None,
                    midi_channel: 0,
                    param_cc_map: HashMap::new(),
                },
                active_nodes: Vec::new(),
                note_nodes: HashMap::new(),
                round_robin_position: 0,
                pending_params: HashMap::new(),
                output_buses: Vec::new(),
            },
        );

        let mut pattern = PatternState::new(
            pattern_id,
            PatternConfig::with_length("kick", voice_id, 4.0),
        );
        pattern.playing = true;
        pattern.loop_position = Beat::from_f64(1.0);
        state.patterns.insert(pattern_id, pattern);

        let melody_id = vibelang_core::MelodyId::new(1);
        let mut melody =
            MelodyState::new(melody_id, MelodyConfig::with_length("bass", voice_id, 4.0));
        melody.playing = true;
        melody.loop_position = Beat::from_f64(2.0);
        state.melodies.insert(melody_id, melody);

        state.sequences.insert(
            sequence_id,
            SequenceState {
                id: sequence_id,
                config: SequenceConfig::with_length("main", 8.0)
                    .with_clip(Clip::pattern(pattern_id, 0.0, 4.0)),
                playing: true,
                paused: false,
                looping: true,
                position: Beat::from_f64(2.0),
                start_beat: Some(Beat::ZERO),
            },
        );

        let snapshot = build_playback_snapshot(&state);
        let transport = snapshot.get("transport").unwrap();
        assert_eq!(
            transport.get("playing").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(transport.get("bar").and_then(Value::as_i64), Some(1));
        assert_eq!(
            transport.get("beat_in_bar").and_then(Value::as_i64),
            Some(1)
        );
        assert_eq!(transport.get("bpm").and_then(Value::as_f64), Some(128.0));

        let groups = snapshot
            .get("groups")
            .and_then(Value::as_array)
            .expect("groups array");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].get("name").and_then(Value::as_str), Some("drums"));
        assert_eq!(
            groups[0]
                .get("voices")
                .and_then(Value::as_array)
                .map(|v| v.len()),
            Some(1)
        );
        assert_eq!(
            groups[0]
                .get("patterns")
                .and_then(Value::as_array)
                .map(|v| v.len()),
            Some(1)
        );

        let sequences = snapshot
            .get("sequences")
            .and_then(Value::as_array)
            .expect("sequences array");
        assert_eq!(sequences.len(), 1);
        assert_eq!(
            sequences[0].get("name").and_then(Value::as_str),
            Some("main")
        );
        assert_eq!(
            sequences[0].get("playing").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            sequences[0]
                .get("active_clips")
                .and_then(Value::as_array)
                .map(|clips| clips.len()),
            Some(1)
        );
    }
}
