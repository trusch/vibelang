//! WebSocket handler for real-time updates.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        OriginalUri, State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use vibelang_core::mutation::{
    EventQueryResult, EventSequence, MutationReceipt, ReceiptEvent, ReceiptState, ResetReason,
    RevisionId, RuntimeEpoch, RuntimeMutationStatus, TerminalOutcome,
};
use vibelang_core::{Clip, FadeTarget, RuntimeHandle, SequenceId, State as RuntimeState};

use crate::{models::HTTP_V2_SCHEMA_VERSION, AppState, ReceiptBroadcastCursor};

/// WebSocket event sent to clients.
#[derive(Debug, Clone, Serialize)]
pub struct WebSocketEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_epoch: Option<RuntimeEpoch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_sequence: Option<EventSequence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_confirmed_revision: Option<RevisionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<MutationReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<RuntimeMutationStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl WebSocketEvent {
    pub(crate) fn receipt(event: ReceiptEvent) -> Self {
        Self {
            event_type: "receipt.updated".into(),
            timestamp: ws_timestamp_ms(),
            runtime_epoch: Some(event.runtime_epoch),
            event_sequence: Some(event.event_sequence),
            last_confirmed_revision: receipt_last_confirmed_revision(&event.receipt),
            receipt: Some(event.receipt),
            status: None,
            data: Some(json!({
                "previous_event_sequence": event.previous_event_sequence,
            })),
        }
    }

    fn telemetry(event_type: &str, data: Value, status: &RuntimeMutationStatus) -> Self {
        Self {
            event_type: event_type.into(),
            timestamp: ws_timestamp_ms(),
            runtime_epoch: Some(status.runtime_epoch),
            event_sequence: status.event_sequence,
            last_confirmed_revision: status.last_confirmed_revision,
            receipt: None,
            status: None,
            data: Some(data),
        }
    }

    fn reset_required(skipped: u64, status: RuntimeMutationStatus) -> Self {
        Self {
            event_type: "receipt.reset_required".into(),
            timestamp: ws_timestamp_ms(),
            runtime_epoch: Some(status.runtime_epoch),
            event_sequence: status.event_sequence,
            last_confirmed_revision: status.last_confirmed_revision,
            receipt: None,
            status: Some(status),
            data: Some(json!({
                "reason": "broadcast_lag",
                "skipped": skipped,
            })),
        }
    }

    fn ledger_reset_required(reason: ResetReason, status: RuntimeMutationStatus) -> Self {
        Self {
            event_type: "receipt.reset_required".into(),
            timestamp: ws_timestamp_ms(),
            runtime_epoch: Some(status.runtime_epoch),
            event_sequence: status.event_sequence,
            last_confirmed_revision: status.last_confirmed_revision,
            receipt: None,
            status: Some(status),
            data: Some(json!({
                "reason": match reason {
                    ResetReason::RuntimeEpochChanged => "runtime_epoch_changed",
                    ResetReason::RetentionExpired => "retention_expired",
                    ResetReason::SequenceAhead => "sequence_ahead",
                },
            })),
        }
    }
}

fn receipt_last_confirmed_revision(receipt: &MutationReceipt) -> Option<RevisionId> {
    match &receipt.state {
        ReceiptState::Terminal(TerminalOutcome::Applied(_)) => receipt.revision,
        ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) => rejected.preserved_revision,
        ReceiptState::Terminal(TerminalOutcome::Partial(partial)) => {
            partial.last_confirmed_revision
        }
        ReceiptState::Terminal(TerminalOutcome::Superseded(_))
        | ReceiptState::Evaluating { .. }
        | ReceiptState::Accepted { .. }
        | ReceiptState::Planning
        | ReceiptState::Staging { .. }
        | ReceiptState::Committing { .. } => receipt.previous_confirmed_revision,
    }
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
const WS_V2_PROTOCOL_VERSION: u16 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
enum V2Subscription {
    Receipt,
    Status,
    Telemetry,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum V2ClientCommand {
    Subscribe {
        #[serde(default)]
        events: Vec<V2Subscription>,
    },
    Unsubscribe {
        #[serde(default)]
        events: Vec<V2Subscription>,
    },
    CatchUp {
        runtime_epoch: RuntimeEpoch,
        after_event_sequence: Option<EventSequence>,
    },
    Status {},
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct V2PlaybackSnapshot {
    transport: V2TransportSnapshot,
    groups: Vec<V2GroupSnapshot>,
    voices: Vec<V2VoiceSnapshot>,
    patterns: Vec<V2PatternSnapshot>,
    melodies: Vec<V2MelodySnapshot>,
    sequences: Vec<V2SequenceSnapshot>,
    fades: Vec<V2FadeSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct V2TransportSnapshot {
    playing: bool,
    beat: f64,
    bar: i64,
    beat_in_bar: i64,
    bpm: f64,
    time_sig: [u8; 2],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct V2GroupSnapshot {
    name: String,
    parent: Option<String>,
    muted: bool,
    soloed: bool,
    meter_peak: Option<f64>,
    voices: Vec<String>,
    patterns: Vec<String>,
    melodies: Vec<String>,
    params: HashMap<String, f32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct V2VoiceSnapshot {
    name: String,
    synth: String,
    muted: bool,
    soloed: bool,
    group: Option<String>,
    active_nodes: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct V2PatternSnapshot {
    name: String,
    playing: bool,
    loop_position: f64,
    loop_length: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct V2MelodySnapshot {
    name: String,
    playing: bool,
    loop_position: f64,
    loop_length: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct V2SequenceSnapshot {
    name: String,
    playing: bool,
    paused: bool,
    position: f64,
    length: f64,
    looping: bool,
    active_clips: Vec<V2ActiveClip>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum V2ActiveClip {
    Pattern {
        name: String,
        clip_index: usize,
        progress: f64,
    },
    Melody {
        name: String,
        clip_index: usize,
        progress: f64,
    },
    Fade {
        name: String,
        clip_index: usize,
        progress: f64,
    },
    Sequence {
        name: String,
        clip_index: usize,
        progress: f64,
        nested_clips: Vec<V2ActiveClip>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct V2FadeSnapshot {
    target_type: String,
    target_name: String,
    param: String,
    start_value: f32,
    current_value: f32,
    target_value: f32,
    progress: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
enum V2Telemetry {
    PlaybackTick {
        snapshot: V2PlaybackSnapshot,
    },
    PlaybackBar {
        snapshot: V2PlaybackSnapshot,
    },
    TransportBeat {
        beat: f64,
        bar: i64,
        beat_in_bar: i64,
    },
    TransportStarted {
        beat: f64,
    },
    TransportStopped {
        beat: f64,
    },
    TransportBpm {
        bpm: f64,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
enum V2ResetReason {
    RuntimeEpochChanged,
    RetentionExpired,
    SequenceAhead,
    BroadcastLag { skipped: u64 },
}

impl From<ResetReason> for V2ResetReason {
    fn from(reason: ResetReason) -> Self {
        match reason {
            ResetReason::RuntimeEpochChanged => Self::RuntimeEpochChanged,
            ResetReason::RetentionExpired => Self::RetentionExpired,
            ResetReason::SequenceAhead => Self::SequenceAhead,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum V2ServerPayload {
    Hello {
        protocol_version: u16,
        server: &'static str,
        capabilities: V2WebSocketCapabilities,
    },
    Receipt {
        event: ReceiptEvent,
    },
    Status {
        status: RuntimeMutationStatus,
    },
    Telemetry {
        telemetry: V2Telemetry,
    },
    ResetRequired {
        reason: V2ResetReason,
        status: RuntimeMutationStatus,
    },
    Error {
        code: &'static str,
        message: String,
    },
}

#[derive(Debug, Serialize)]
struct V2WebSocketCapabilities {
    subscriptions: [&'static str; 3],
    commands: [&'static str; 4],
    catch_up: bool,
    initial_status: bool,
    initial_snapshot: bool,
}

#[derive(Debug, Serialize)]
struct V2ServerFrame {
    schema_version: u16,
    stream_sequence: u64,
    timestamp_ms: f64,
    runtime_epoch: RuntimeEpoch,
    event_sequence: Option<EventSequence>,
    last_confirmed_revision: Option<RevisionId>,
    #[serde(flatten)]
    payload: V2ServerPayload,
}

#[derive(Default)]
struct V2FrameSequencer {
    emitted: u64,
}

#[derive(Clone, Copy)]
struct V2ReceiptCursor {
    runtime_epoch: RuntimeEpoch,
    event_sequence: Option<EventSequence>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum V2CatchUpOutcome {
    CaughtUp,
    ResetRequired,
}

impl V2CatchUpOutcome {
    const fn canonical_reset_required(self) -> bool {
        matches!(self, Self::ResetRequired)
    }

    const fn broadcast_lag_reset_required(self) -> bool {
        matches!(self, Self::CaughtUp)
    }
}

impl V2FrameSequencer {
    fn next(
        &mut self,
        status: &RuntimeMutationStatus,
        payload: V2ServerPayload,
    ) -> Option<V2ServerFrame> {
        self.next_with_freshness(
            status.runtime_epoch,
            status.event_sequence,
            status.last_confirmed_revision,
            payload,
        )
    }

    fn next_with_freshness(
        &mut self,
        runtime_epoch: RuntimeEpoch,
        event_sequence: Option<EventSequence>,
        last_confirmed_revision: Option<RevisionId>,
        payload: V2ServerPayload,
    ) -> Option<V2ServerFrame> {
        self.emitted = self.emitted.checked_add(1)?;
        Some(V2ServerFrame {
            schema_version: HTTP_V2_SCHEMA_VERSION,
            stream_sequence: self.emitted,
            timestamp_ms: ws_timestamp_ms(),
            runtime_epoch,
            event_sequence,
            last_confirmed_revision,
            payload,
        })
    }
}

/// WebSocket upgrade handler.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    OriginalUri(uri): OriginalUri,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let v2 = uri.path() == "/v2/ws";
    let max_body_bytes = state.security.max_body_bytes;
    ws.max_message_size(max_body_bytes)
        .max_frame_size(max_body_bytes)
        .on_upgrade(move |socket| async move {
            if v2 {
                handle_v2_socket(socket, state).await;
            } else {
                handle_socket(socket, state).await;
            }
        })
}

async fn handle_v2_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut events = state.ws_tx.subscribe();
    let mut subscriptions = all_v2_subscriptions();
    let mut sequencer = V2FrameSequencer::default();
    let initial_status = state.handle.mutation_status();
    let mut receipt_cursor = V2ReceiptCursor {
        runtime_epoch: initial_status.runtime_epoch,
        event_sequence: initial_status.event_sequence,
    };

    if send_v2(
        &mut sender,
        &mut sequencer,
        &initial_status,
        V2ServerPayload::Hello {
            protocol_version: WS_V2_PROTOCOL_VERSION,
            server: "vibelang-http",
            capabilities: V2WebSocketCapabilities {
                subscriptions: ["receipt", "status", "telemetry"],
                commands: ["subscribe", "unsubscribe", "catch_up", "status"],
                catch_up: true,
                initial_status: true,
                initial_snapshot: true,
            },
        },
    )
    .await
    .is_err()
    {
        return;
    }
    if send_v2(
        &mut sender,
        &mut sequencer,
        &initial_status,
        V2ServerPayload::Status {
            status: initial_status.clone(),
        },
    )
    .await
    .is_err()
        || send_v2_snapshot(&mut sender, &mut sequencer, &state, &initial_status)
            .await
            .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                let Some(incoming) = incoming else {
                    break;
                };
                match incoming {
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<V2ClientCommand>(&text) {
                            Ok(V2ClientCommand::Subscribe { events }) => {
                                let requested = if events.is_empty() {
                                    all_v2_subscriptions()
                                } else {
                                    events.into_iter().collect()
                                };
                                let telemetry_added = requested.contains(&V2Subscription::Telemetry)
                                    && !subscriptions.contains(&V2Subscription::Telemetry);
                                subscriptions.extend(requested);
                                if telemetry_added {
                                    let status = state.handle.mutation_status();
                                    if send_v2_snapshot(&mut sender, &mut sequencer, &state, &status)
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            }
                            Ok(V2ClientCommand::Unsubscribe { events }) => {
                                if events.is_empty() {
                                    subscriptions.clear();
                                } else {
                                    for subscription in events {
                                        subscriptions.remove(&subscription);
                                    }
                                }
                            }
                            Ok(V2ClientCommand::CatchUp {
                                runtime_epoch,
                                after_event_sequence,
                            }) => {
                                let catch_up = deliver_v2_catch_up(
                                    &mut sender,
                                    &mut sequencer,
                                    &state,
                                    runtime_epoch,
                                    after_event_sequence,
                                    &mut receipt_cursor,
                                    true,
                                    false,
                                )
                                .await;
                                match catch_up {
                                    Ok(outcome)
                                        if outcome.canonical_reset_required()
                                            && subscriptions
                                            .contains(&V2Subscription::Telemetry) =>
                                    {
                                        let status = state.handle.mutation_status();
                                        if send_v2_snapshot(
                                            &mut sender,
                                            &mut sequencer,
                                            &state,
                                            &status,
                                        )
                                        .await
                                        .is_err()
                                        {
                                            break;
                                        }
                                    }
                                    Ok(_) => {}
                                    Err(()) => break,
                                }
                            }
                            Ok(V2ClientCommand::Status {}) => {
                                let status = state.handle.mutation_status();
                                if send_v2(
                                    &mut sender,
                                    &mut sequencer,
                                    &status,
                                    V2ServerPayload::Status {
                                        status: status.clone(),
                                    },
                                )
                                .await
                                .is_err()
                                {
                                    break;
                                }
                            }
                            Err(error) => {
                                let status = state.handle.mutation_status();
                                if send_v2(
                                    &mut sender,
                                    &mut sequencer,
                                    &status,
                                    V2ServerPayload::Error {
                                        code: "invalid_command",
                                        message: error.to_string(),
                                    },
                                )
                                .await
                                .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(Message::Binary(_))
                    | Ok(Message::Ping(_))
                    | Ok(Message::Pong(_)) => {}
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if let Some(receipt_event) = receipt_event_from_legacy(&event) {
                            if receipt_cursor.runtime_epoch == receipt_event.runtime_epoch
                                && receipt_cursor.event_sequence.is_some_and(|sequence| {
                                    receipt_event.event_sequence <= sequence
                                })
                            {
                                continue;
                            }
                            if receipt_cursor_has_gap(
                                &receipt_cursor,
                                receipt_event.runtime_epoch,
                                receipt_event.previous_event_sequence,
                            ) {
                                match deliver_v2_catch_up(
                                    &mut sender,
                                    &mut sequencer,
                                    &state,
                                    receipt_cursor.runtime_epoch,
                                    receipt_cursor.event_sequence,
                                    &mut receipt_cursor,
                                    subscriptions.contains(&V2Subscription::Receipt),
                                    true,
                                )
                                .await
                                {
                                    Ok(outcome)
                                        if outcome.canonical_reset_required()
                                            && subscriptions
                                            .contains(&V2Subscription::Telemetry) =>
                                    {
                                        let status = state.handle.mutation_status();
                                        if send_v2_snapshot(
                                            &mut sender,
                                            &mut sequencer,
                                            &state,
                                            &status,
                                        )
                                        .await
                                        .is_err()
                                        {
                                            break;
                                        }
                                    }
                                    Ok(_) => {}
                                    Err(()) => break,
                                }
                                continue;
                            }
                            receipt_cursor = V2ReceiptCursor {
                                runtime_epoch: receipt_event.runtime_epoch,
                                event_sequence: Some(receipt_event.event_sequence),
                            };
                            let status = state.handle.mutation_status();
                            if subscriptions.contains(&V2Subscription::Receipt)
                                && send_v2_receipt(
                                    &mut sender,
                                    &mut sequencer,
                                    receipt_event,
                                )
                                .await
                                .is_err()
                            {
                                break;
                            }
                            if subscriptions.contains(&V2Subscription::Status)
                                && send_v2(
                                    &mut sender,
                                    &mut sequencer,
                                    &status,
                                    V2ServerPayload::Status {
                                        status: status.clone(),
                                    },
                                )
                                .await
                                .is_err()
                            {
                                break;
                            }
                        } else if let Some((reason, status)) =
                            reset_required_from_legacy(&event)
                        {
                            receipt_cursor = V2ReceiptCursor {
                                runtime_epoch: status.runtime_epoch,
                                event_sequence: status.event_sequence,
                            };
                            if send_v2(
                                &mut sender,
                                &mut sequencer,
                                &status,
                                V2ServerPayload::ResetRequired {
                                    reason,
                                    status: status.clone(),
                                },
                            )
                            .await
                            .is_err()
                            {
                                break;
                            }
                            if subscriptions.contains(&V2Subscription::Telemetry)
                                && send_v2_snapshot(
                                    &mut sender,
                                    &mut sequencer,
                                    &state,
                                    &status,
                                )
                                .await
                                .is_err()
                            {
                                break;
                            }
                        } else if subscriptions.contains(&V2Subscription::Telemetry) {
                            match telemetry_from_legacy(&event) {
                                Ok(Some(telemetry)) => {
                                    let Some(runtime_epoch) = event.runtime_epoch else {
                                        continue;
                                    };
                                    let Some(frame) = sequencer.next_with_freshness(
                                        runtime_epoch,
                                        event.event_sequence,
                                        event.last_confirmed_revision,
                                        V2ServerPayload::Telemetry { telemetry },
                                    ) else {
                                        break;
                                    };
                                    if send_v2_frame(&mut sender, frame).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(None) => {}
                                Err(message) => {
                                    let status = state.handle.mutation_status();
                                    if send_v2(
                                        &mut sender,
                                        &mut sequencer,
                                        &status,
                                        V2ServerPayload::Error {
                                            code: "invalid_server_projection",
                                            message,
                                        },
                                    )
                                    .await
                                    .is_err()
                                    {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        let catch_up = deliver_v2_catch_up(
                            &mut sender,
                            &mut sequencer,
                            &state,
                            receipt_cursor.runtime_epoch,
                            receipt_cursor.event_sequence,
                            &mut receipt_cursor,
                            subscriptions.contains(&V2Subscription::Receipt),
                            true,
                        )
                        .await;
                        let Ok(catch_up) = catch_up else {
                            break;
                        };
                        let status = state.handle.mutation_status();
                        if catch_up.broadcast_lag_reset_required()
                            && send_v2(
                                &mut sender,
                                &mut sequencer,
                                &status,
                                V2ServerPayload::ResetRequired {
                                    reason: V2ResetReason::BroadcastLag { skipped },
                                    status: status.clone(),
                                },
                            )
                            .await
                            .is_err()
                        {
                            break;
                        }
                        if subscriptions.contains(&V2Subscription::Telemetry)
                            && send_v2_snapshot(
                                &mut sender,
                                &mut sequencer,
                                &state,
                                &status,
                            )
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

type V2SocketSender = futures::stream::SplitSink<WebSocket, Message>;

async fn send_v2_frame(sender: &mut V2SocketSender, frame: V2ServerFrame) -> Result<(), ()> {
    let message = serde_json::to_string(&frame).map_err(|_| ())?;
    sender
        .send(Message::Text(message.into()))
        .await
        .map_err(|_| ())
}

async fn send_v2(
    sender: &mut V2SocketSender,
    sequencer: &mut V2FrameSequencer,
    status: &RuntimeMutationStatus,
    payload: V2ServerPayload,
) -> Result<(), ()> {
    let frame = sequencer.next(status, payload).ok_or(())?;
    send_v2_frame(sender, frame).await
}

async fn send_v2_receipt(
    sender: &mut V2SocketSender,
    sequencer: &mut V2FrameSequencer,
    event: ReceiptEvent,
) -> Result<(), ()> {
    let frame = sequencer
        .next_with_freshness(
            event.runtime_epoch,
            Some(event.event_sequence),
            receipt_last_confirmed_revision(&event.receipt),
            V2ServerPayload::Receipt { event },
        )
        .ok_or(())?;
    send_v2_frame(sender, frame).await
}

async fn send_v2_snapshot(
    sender: &mut V2SocketSender,
    sequencer: &mut V2FrameSequencer,
    state: &AppState,
    status: &RuntimeMutationStatus,
) -> Result<(), ()> {
    let snapshot = state
        .with_state(|runtime| {
            serde_json::from_value::<V2PlaybackSnapshot>(build_playback_snapshot(runtime))
        })
        .await;
    match snapshot {
        Ok(snapshot) => {
            send_v2(
                sender,
                sequencer,
                status,
                V2ServerPayload::Telemetry {
                    telemetry: V2Telemetry::PlaybackBar { snapshot },
                },
            )
            .await
        }
        Err(error) => {
            send_v2(
                sender,
                sequencer,
                status,
                V2ServerPayload::Error {
                    code: "invalid_server_projection",
                    message: error.to_string(),
                },
            )
            .await
        }
    }
}

async fn deliver_v2_catch_up(
    sender: &mut V2SocketSender,
    sequencer: &mut V2FrameSequencer,
    state: &AppState,
    runtime_epoch: RuntimeEpoch,
    after_event_sequence: Option<EventSequence>,
    receipt_cursor: &mut V2ReceiptCursor,
    send_receipts: bool,
    deduplicate: bool,
) -> Result<V2CatchUpOutcome, ()> {
    match state
        .handle
        .mutation_events_after(runtime_epoch, after_event_sequence)
    {
        EventQueryResult::Events { events } => {
            if receipt_cursor.runtime_epoch != runtime_epoch {
                receipt_cursor.runtime_epoch = runtime_epoch;
                receipt_cursor.event_sequence = None;
            }
            for event in events {
                let event = state.public_receipt_event(event);
                if deduplicate
                    && receipt_cursor.runtime_epoch == event.runtime_epoch
                    && receipt_cursor
                        .event_sequence
                        .is_some_and(|sequence| event.event_sequence <= sequence)
                {
                    continue;
                }
                receipt_cursor.runtime_epoch = event.runtime_epoch;
                receipt_cursor.event_sequence = Some(
                    receipt_cursor
                        .event_sequence
                        .map_or(event.event_sequence, |current| {
                            current.max(event.event_sequence)
                        }),
                );
                if send_receipts {
                    send_v2_receipt(sender, sequencer, event).await?;
                }
            }
            let status = state.handle.mutation_status();
            send_v2(
                sender,
                sequencer,
                &status,
                V2ServerPayload::Status {
                    status: status.clone(),
                },
            )
            .await?;
            Ok(V2CatchUpOutcome::CaughtUp)
        }
        EventQueryResult::ResetRequired { reason, status } => {
            receipt_cursor.runtime_epoch = status.runtime_epoch;
            receipt_cursor.event_sequence = status.event_sequence;
            send_v2(
                sender,
                sequencer,
                &status,
                V2ServerPayload::ResetRequired {
                    reason: reason.into(),
                    status: (*status).clone(),
                },
            )
            .await?;
            Ok(V2CatchUpOutcome::ResetRequired)
        }
    }
}

fn all_v2_subscriptions() -> HashSet<V2Subscription> {
    [
        V2Subscription::Receipt,
        V2Subscription::Status,
        V2Subscription::Telemetry,
    ]
    .into_iter()
    .collect()
}

fn receipt_sequence_has_gap(
    last_received: Option<EventSequence>,
    previous_event_sequence: Option<EventSequence>,
) -> bool {
    last_received != previous_event_sequence
}

fn receipt_cursor_has_gap(
    cursor: &V2ReceiptCursor,
    event_epoch: RuntimeEpoch,
    previous_event_sequence: Option<EventSequence>,
) -> bool {
    cursor.runtime_epoch != event_epoch
        || receipt_sequence_has_gap(cursor.event_sequence, previous_event_sequence)
}

fn receipt_event_from_legacy(event: &WebSocketEvent) -> Option<ReceiptEvent> {
    let receipt = event.receipt.clone()?;
    let runtime_epoch = event.runtime_epoch?;
    let event_sequence = event.event_sequence?;
    let previous_event_sequence = event
        .data
        .as_ref()
        .and_then(|data| data.get("previous_event_sequence"))
        .filter(|value| !value.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .ok()?;
    Some(ReceiptEvent {
        schema_version: receipt.schema_version,
        runtime_epoch,
        event_sequence,
        previous_event_sequence,
        receipt,
    })
}

fn reset_required_from_legacy(
    event: &WebSocketEvent,
) -> Option<(V2ResetReason, RuntimeMutationStatus)> {
    if event.event_type != "receipt.reset_required" {
        return None;
    }
    let status = event.status.clone()?;
    let data = event.data.as_ref()?;
    let reason = match data.get("reason").and_then(Value::as_str)? {
        "runtime_epoch_changed" => V2ResetReason::RuntimeEpochChanged,
        "retention_expired" => V2ResetReason::RetentionExpired,
        "sequence_ahead" => V2ResetReason::SequenceAhead,
        "broadcast_lag" => V2ResetReason::BroadcastLag {
            skipped: data.get("skipped").and_then(Value::as_u64).unwrap_or(0),
        },
        _ => return None,
    };
    Some((reason, status))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2TransportBeatPayload {
    beat: f64,
    bar: i64,
    beat_in_bar: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2TransportEdgePayload {
    beat: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2TransportBpmPayload {
    bpm: f64,
}

fn telemetry_from_legacy(event: &WebSocketEvent) -> Result<Option<V2Telemetry>, String> {
    let Some(data) = event.data.clone() else {
        return Ok(None);
    };
    match event.event_type.as_str() {
        "playback.tick" => serde_json::from_value(data)
            .map(|snapshot| Some(V2Telemetry::PlaybackTick { snapshot }))
            .map_err(|error| error.to_string()),
        "playback.bar" => serde_json::from_value(data)
            .map(|snapshot| Some(V2Telemetry::PlaybackBar { snapshot }))
            .map_err(|error| error.to_string()),
        "transport.beat" => serde_json::from_value::<V2TransportBeatPayload>(data)
            .map(|payload| {
                Some(V2Telemetry::TransportBeat {
                    beat: payload.beat,
                    bar: payload.bar,
                    beat_in_bar: payload.beat_in_bar,
                })
            })
            .map_err(|error| error.to_string()),
        "transport.started" => serde_json::from_value::<V2TransportEdgePayload>(data)
            .map(|payload| Some(V2Telemetry::TransportStarted { beat: payload.beat }))
            .map_err(|error| error.to_string()),
        "transport.stopped" => serde_json::from_value::<V2TransportEdgePayload>(data)
            .map(|payload| Some(V2Telemetry::TransportStopped { beat: payload.beat }))
            .map_err(|error| error.to_string()),
        "transport.bpm" => serde_json::from_value::<V2TransportBpmPayload>(data)
            .map(|payload| Some(V2Telemetry::TransportBpm { bpm: payload.bpm }))
            .map_err(|error| error.to_string()),
        _ => Ok(None),
    }
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
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            let event = WebSocketEvent::reset_required(
                                skipped,
                                send_state.handle.mutation_status(),
                            );
                            let msg = serde_json::to_string(&event).unwrap_or_default();
                            if sender.send(Message::Text(msg.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
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
                                let status = send_state.handle.mutation_status();
                                let event = send_state
                                    .with_state(|runtime| {
                                        make_snapshot_event("playback.bar", runtime, &status)
                                    })
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
        runtime_epoch: None,
        event_sequence: None,
        last_confirmed_revision: None,
        receipt: None,
        status: None,
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
                    "receipt.updated",
                    "receipt.reset_required",
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

fn make_snapshot_event(
    event_type: &str,
    state: &RuntimeState,
    status: &RuntimeMutationStatus,
) -> WebSocketEvent {
    WebSocketEvent::telemetry(event_type, build_playback_snapshot(state), status)
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
    handle: RuntimeHandle,
    tx: broadcast::Sender<WebSocketEvent>,
    receipt_cursor: Arc<Mutex<ReceiptBroadcastCursor>>,
    redact_receipts: bool,
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
        let status = handle.mutation_status();
        let cursor = receipt_cursor.lock().ok().map(|cursor| *cursor);
        if let Some(cursor) = cursor {
            match handle.mutation_events_after(cursor.runtime_epoch, cursor.event_sequence) {
                EventQueryResult::Events { events } => {
                    for event in events {
                        crate::publish_receipt_event(&tx, &receipt_cursor, event, redact_receipts);
                    }
                }
                EventQueryResult::ResetRequired {
                    reason,
                    status: reset_status,
                } => {
                    if let Ok(mut cursor) = receipt_cursor.lock() {
                        cursor.runtime_epoch = reset_status.runtime_epoch;
                        cursor.event_sequence = reset_status.event_sequence;
                    }
                    let _ = tx.send(WebSocketEvent::ledger_reset_required(
                        reason,
                        (*reset_status).clone(),
                    ));
                }
            }
        }
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
            let mut event = WebSocketEvent::telemetry(event_type, snapshot.clone(), &status);
            event.timestamp = now;
            let _ = tx.send(event);
        }

        if running && last_bar != Some(bar) {
            let mut event = WebSocketEvent::telemetry("playback.bar", snapshot.clone(), &status);
            event.timestamp = now;
            let _ = tx.send(event);
        }

        // transport.beat remains useful for newer clients that only care about transport edges.
        if running && last_sixteenth != Some(sixteenth) {
            let mut event = WebSocketEvent::telemetry(
                "transport.beat",
                json!({
                    "beat": current_beat,
                    "bar": bar,
                    "beat_in_bar": (current_beat % beats_per_bar).floor() as i64,
                }),
                &status,
            );
            event.timestamp = now;
            let _ = tx.send(event);
        }

        if running_changed {
            let event_type = if running {
                "transport.started"
            } else {
                "transport.stopped"
            };
            let mut event = WebSocketEvent::telemetry(
                event_type,
                json!({
                    "beat": current_beat,
                }),
                &status,
            );
            event.timestamp = now;
            let _ = tx.send(event);
        }

        if bpm_changed && last_bpm.is_some() {
            let mut event = WebSocketEvent::telemetry(
                "transport.bpm",
                json!({
                    "bpm": bpm,
                }),
                &status,
            );
            event.timestamp = now;
            let _ = tx.send(event);
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
    use vibelang_core::mutation::{
        LiveState, ReceiptWindow, RevisionId, RuntimeEpoch, MUTATION_SCHEMA_VERSION,
    };
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
    fn v2_commands_are_operation_typed_and_reject_unknown_input() {
        assert!(matches!(
            serde_json::from_str::<V2ClientCommand>(r#"{"action":"status"}"#).unwrap(),
            V2ClientCommand::Status {}
        ));
        assert!(matches!(
            serde_json::from_str::<V2ClientCommand>(
                r#"{"action":"subscribe","events":["receipt","telemetry"]}"#
            )
            .unwrap(),
            V2ClientCommand::Subscribe { events } if events.len() == 2
        ));
        assert!(serde_json::from_str::<V2ClientCommand>(
            r#"{"action":"subscribe","events":["arbitrary"]}"#
        )
        .is_err());
        assert!(
            serde_json::from_str::<V2ClientCommand>(r#"{"action":"status","ignored":true}"#)
                .is_err()
        );
    }

    #[test]
    fn v2_receipt_sequence_gaps_are_detected_for_ledger_catch_up() {
        let first = EventSequence::new(1).unwrap();
        let second = EventSequence::new(2).unwrap();
        let epoch = RuntimeEpoch::new();
        let cursor = V2ReceiptCursor {
            runtime_epoch: epoch,
            event_sequence: Some(first),
        };

        assert!(!receipt_sequence_has_gap(None, None));
        assert!(!receipt_sequence_has_gap(Some(first), Some(first)));
        assert!(receipt_sequence_has_gap(Some(first), Some(second)));
        assert!(receipt_sequence_has_gap(Some(first), None));
        assert!(receipt_sequence_has_gap(None, Some(first)));
        assert!(!receipt_cursor_has_gap(&cursor, epoch, Some(first)));
        assert!(receipt_cursor_has_gap(
            &cursor,
            RuntimeEpoch::new(),
            Some(first)
        ));
    }

    #[test]
    fn v2_frames_have_monotonic_stream_sequence_and_typed_payloads() {
        let epoch = RuntimeEpoch::new();
        let receipt_sequence = EventSequence::new(7).unwrap();
        let revision = RevisionId::new(3).unwrap();
        let status = RuntimeMutationStatus {
            schema_version: MUTATION_SCHEMA_VERSION,
            runtime_epoch: epoch,
            event_sequence: Some(receipt_sequence),
            accepted_through: Some(revision),
            last_confirmed_revision: Some(revision),
            last_rejected_revision: None,
            live_state: LiveState::Clean,
            pending: Vec::new(),
            receipt_window: ReceiptWindow {
                first_event_sequence: Some(receipt_sequence),
                last_event_sequence: Some(receipt_sequence),
                first_revision: Some(revision),
                last_revision: Some(revision),
                expires_before: None,
            },
        };
        let mut sequencer = V2FrameSequencer::default();
        let first = sequencer
            .next(
                &status,
                V2ServerPayload::Status {
                    status: status.clone(),
                },
            )
            .unwrap();
        let second = sequencer
            .next(
                &status,
                V2ServerPayload::Telemetry {
                    telemetry: V2Telemetry::TransportBeat {
                        beat: 4.0,
                        bar: 1,
                        beat_in_bar: 0,
                    },
                },
            )
            .unwrap();

        assert_eq!(first.stream_sequence, 1);
        assert_eq!(second.stream_sequence, 2);
        let first = serde_json::to_value(first).unwrap();
        let second = serde_json::to_value(second).unwrap();
        assert_eq!(first["type"], "status");
        assert_eq!(first["schema_version"], HTTP_V2_SCHEMA_VERSION);
        assert_eq!(second["type"], "telemetry");
        assert_eq!(second["telemetry"]["kind"], "transport_beat");
        assert_eq!(second["event_sequence"], receipt_sequence.to_string());
        assert!(second.get("data").is_none());
    }

    #[test]
    fn v2_reset_reason_is_structured() {
        let reason = serde_json::to_value(V2ResetReason::BroadcastLag { skipped: 12 }).unwrap();
        assert_eq!(reason["reason"], "broadcast_lag");
        assert_eq!(reason["skipped"], 12);
    }

    #[test]
    fn v2_catch_up_distinguishes_canonical_and_broadcast_lag_resets() {
        assert!(!V2CatchUpOutcome::CaughtUp.canonical_reset_required());
        assert!(V2CatchUpOutcome::ResetRequired.canonical_reset_required());
        assert!(V2CatchUpOutcome::CaughtUp.broadcast_lag_reset_required());
        assert!(!V2CatchUpOutcome::ResetRequired.broadcast_lag_reset_required());
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
        assert!(event_names.contains(&"receipt.updated"));
        assert!(event_names.contains(&"receipt.reset_required"));

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
    fn legacy_telemetry_carries_receipt_freshness_without_acknowledging() {
        let epoch = RuntimeEpoch::new();
        let sequence = EventSequence::new(9).unwrap();
        let confirmed = RevisionId::new(3).unwrap();
        let status = RuntimeMutationStatus {
            schema_version: MUTATION_SCHEMA_VERSION,
            runtime_epoch: epoch,
            event_sequence: Some(sequence),
            accepted_through: Some(confirmed),
            last_confirmed_revision: Some(confirmed),
            last_rejected_revision: None,
            live_state: LiveState::Clean,
            pending: Vec::new(),
            receipt_window: ReceiptWindow {
                first_event_sequence: Some(sequence),
                last_event_sequence: Some(sequence),
                first_revision: Some(confirmed),
                last_revision: Some(confirmed),
                expires_before: None,
            },
        };

        let event = WebSocketEvent::telemetry("transport.beat", json!({ "beat": 4.0 }), &status);
        assert_eq!(event.runtime_epoch, Some(epoch));
        assert_eq!(event.event_sequence, Some(sequence));
        assert_eq!(event.last_confirmed_revision, Some(confirmed));
        assert!(event.receipt.is_none());
        assert!(event.status.is_none());

        let reset = WebSocketEvent::reset_required(4, status.clone());
        assert_eq!(reset.event_type, "receipt.reset_required");
        assert_eq!(reset.status, Some(status));
        assert!(reset.receipt.is_none());
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
                output_channels: None,
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
                role: vibelang_core::VoiceRole::Audible,
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
                    modulator_only: false,
                    mono_legato: false,
                    midi_output: None,
                    midi_channel: 0,
                    param_cc_map: HashMap::new(),
                },
                active_nodes: Vec::new(),
                note_nodes: HashMap::new(),
                round_robin_position: 0,
                pending_params: HashMap::new(),
                output_buses: Vec::new(),
                input_buses: Vec::new(),
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
        let typed_snapshot =
            serde_json::from_value::<V2PlaybackSnapshot>(snapshot.clone()).unwrap();
        assert_eq!(typed_snapshot.transport.bpm, 128.0);
        assert_eq!(typed_snapshot.groups[0].name, "drums");
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
