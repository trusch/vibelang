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
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use vibelang_core::RuntimeHandle;

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

    // Use a channel to communicate subscription updates
    let (sub_tx, mut sub_rx) = tokio::sync::mpsc::channel::<Vec<String>>(16);

    // Default subscription patterns (all events)
    let initial_subscriptions = vec!["*".to_string()];

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
                Some(new_subs) = sub_rx.recv() => {
                    subscriptions = new_subs;
                }
            }
        }
    });

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
                        let _ = sub_tx.send(local_subscriptions.clone()).await;
                    }
                    "unsubscribe" => {
                        for pattern in &sub_msg.events {
                            local_subscriptions.retain(|s| s != pattern);
                        }
                        let _ = sub_tx.send(local_subscriptions.clone()).await;
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
        if pattern.ends_with("*") {
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

/// Background task that polls state and broadcasts events.
pub async fn run_event_broadcaster(handle: RuntimeHandle, tx: broadcast::Sender<WebSocketEvent>) {
    let mut last_beat: Option<f64> = None;
    let mut last_running: Option<bool> = None;
    let mut last_bpm: Option<f64> = None;

    let mut interval = tokio::time::interval(Duration::from_millis(50)); // 20 Hz update rate

    loop {
        interval.tick().await;

        // Read current state
        let (current_beat, running, bpm) = handle.with_state(|s| {
            (s.current_beat, s.transport_running, s.tempo)
        });

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0);

        // Check for beat changes (emit on each beat)
        if let Some(last) = last_beat {
            if current_beat.floor() != last.floor() {
                let _ = tx.send(WebSocketEvent {
                    event_type: "transport.beat".to_string(),
                    timestamp: now,
                    data: Some(serde_json::json!({
                        "beat": current_beat,
                        "bar": (current_beat / 4.0).floor() as i32,
                        "beat_in_bar": (current_beat % 4.0).floor() as i32,
                    })),
                });
            }
        }
        last_beat = Some(current_beat);

        // Check for transport state changes
        if last_running != Some(running) {
            let event_type = if running { "transport.started" } else { "transport.stopped" };
            let _ = tx.send(WebSocketEvent {
                event_type: event_type.to_string(),
                timestamp: now,
                data: Some(serde_json::json!({
                    "beat": current_beat,
                })),
            });
            last_running = Some(running);
        }

        // Check for BPM changes
        if last_bpm != Some(bpm) {
            if last_bpm.is_some() {
                let _ = tx.send(WebSocketEvent {
                    event_type: "transport.bpm".to_string(),
                    timestamp: now,
                    data: Some(serde_json::json!({
                        "bpm": bpm,
                    })),
                });
            }
            last_bpm = Some(bpm);
        }
    }
}
