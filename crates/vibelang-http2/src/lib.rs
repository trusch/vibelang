//! HTTP REST API server for VibeLang (core2 backend).
//!
//! Provides a REST API and WebSocket endpoint for querying and controlling
//! a running VibeLang session using the vibelang-core2 runtime.
//!
//! # Features
//!
//! - Full CRUD operations for voices, patterns, melodies, sequences
//! - Transport control (play, stop, seek, tempo)
//! - Effect and sample management
//! - MIDI routing and callbacks
//! - Real-time WebSocket events
//! - Live state queries (active synths, meters)
//!
//! # Usage
//!
//! ```ignore
//! use vibelang_http2::{start_server, AppState};
//! use vibelang_core2::RuntimeHandle;
//!
//! let handle = runtime.handle();
//! let state = runtime.state();
//! tokio::spawn(async move {
//!     start_server(handle, state, 1606, None).await;
//! });
//! ```

mod models;
mod routes;
mod websocket;

use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};
use vibelang_core2::{Message, RuntimeHandle, State};

pub use models::*;
pub use routes::eval::{EvalJob, EvalResult};
pub use websocket::WebSocketEvent;

/// Sender type for eval requests.
pub type EvalSender = std::sync::mpsc::Sender<EvalJob>;

/// Shared application state for HTTP handlers.
pub struct AppState {
    /// Runtime handle for sending messages.
    pub handle: RuntimeHandle,
    /// Shared runtime state for reading.
    pub state: Arc<RwLock<State>>,
    /// Broadcast channel for WebSocket events.
    pub ws_tx: broadcast::Sender<WebSocketEvent>,
    /// Channel to send eval requests to the main thread (optional).
    pub eval_tx: Option<EvalSender>,
}

impl AppState {
    /// Read state immutably.
    pub async fn with_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&State) -> R,
    {
        let guard = self.state.read().await;
        f(&guard)
    }

    /// Send a message to the runtime.
    pub async fn send(&self, msg: Message) -> Result<(), vibelang_core2::Error> {
        self.handle.send(msg).await
    }
}

/// Start the HTTP server on the specified port.
///
/// # Arguments
///
/// * `handle` - The VibeLang runtime handle for sending messages
/// * `state` - The shared runtime state for reading
/// * `port` - The port to listen on
/// * `eval_tx` - Optional channel to send code evaluation requests to the main thread
///
/// # Example
///
/// ```ignore
/// let handle = runtime.handle();
/// let state = runtime.state();
/// let (eval_tx, eval_rx) = std::sync::mpsc::channel();
/// tokio::spawn(async move {
///     start_server(handle, state, 1606, Some(eval_tx)).await;
/// });
/// ```
pub async fn start_server(
    handle: RuntimeHandle,
    state: Arc<RwLock<State>>,
    port: u16,
    eval_tx: Option<EvalSender>,
) {
    // Create broadcast channel for WebSocket events
    let (ws_tx, _) = broadcast::channel::<WebSocketEvent>(1024);

    let app_state = Arc::new(AppState {
        handle: handle.clone(),
        state: state.clone(),
        ws_tx: ws_tx.clone(),
        eval_tx,
    });

    // Start the event broadcaster in the background
    let broadcast_state = state.clone();
    let broadcast_tx = ws_tx.clone();
    tokio::spawn(async move {
        websocket::run_event_broadcaster(broadcast_state, broadcast_tx).await;
    });

    // Build the router with all routes
    let app = Router::new()
        // Transport
        .route("/transport", get(routes::transport::get_transport))
        .route("/transport", patch(routes::transport::update_transport))
        .route("/transport/start", post(routes::transport::start_transport))
        .route("/transport/stop", post(routes::transport::stop_transport))
        .route("/transport/seek", post(routes::transport::seek_transport))
        // Groups
        .route("/groups", get(routes::groups::list_groups))
        .route("/groups/{id}", get(routes::groups::get_group))
        .route("/groups/{id}", patch(routes::groups::update_group))
        .route("/groups/{id}/mute", post(routes::groups::mute_group))
        .route("/groups/{id}/unmute", post(routes::groups::unmute_group))
        .route("/groups/{id}/solo", post(routes::groups::solo_group))
        .route("/groups/{id}/unsolo", post(routes::groups::unsolo_group))
        .route(
            "/groups/{id}/params/{param}",
            put(routes::groups::set_group_param),
        )
        // Voices
        .route("/voices", get(routes::voices::list_voices))
        .route("/voices/{id}", get(routes::voices::get_voice))
        .route("/voices/{id}", patch(routes::voices::update_voice))
        .route("/voices/{id}/trigger", post(routes::voices::trigger_voice))
        .route("/voices/{id}/stop", post(routes::voices::stop_voice))
        .route("/voices/{id}/note-on", post(routes::voices::note_on))
        .route("/voices/{id}/note-off", post(routes::voices::note_off))
        .route(
            "/voices/{id}/params/{param}",
            put(routes::voices::set_voice_param),
        )
        .route("/voices/{id}/mute", post(routes::voices::mute_voice))
        .route("/voices/{id}/unmute", post(routes::voices::unmute_voice))
        // Patterns
        .route("/patterns", get(routes::patterns::list_patterns))
        .route("/patterns/{id}", get(routes::patterns::get_pattern))
        .route("/patterns/{id}", patch(routes::patterns::update_pattern))
        .route("/patterns/{id}/start", post(routes::patterns::start_pattern))
        .route("/patterns/{id}/stop", post(routes::patterns::stop_pattern))
        // Melodies
        .route("/melodies", get(routes::melodies::list_melodies))
        .route("/melodies/{id}", get(routes::melodies::get_melody))
        .route("/melodies/{id}", patch(routes::melodies::update_melody))
        .route("/melodies/{id}/start", post(routes::melodies::start_melody))
        .route("/melodies/{id}/stop", post(routes::melodies::stop_melody))
        // Sequences
        .route("/sequences", get(routes::sequences::list_sequences))
        .route("/sequences/{id}", get(routes::sequences::get_sequence))
        .route("/sequences/{id}", patch(routes::sequences::update_sequence))
        .route(
            "/sequences/{id}/start",
            post(routes::sequences::start_sequence),
        )
        .route(
            "/sequences/{id}/stop",
            post(routes::sequences::stop_sequence),
        )
        .route(
            "/sequences/{id}/pause",
            post(routes::sequences::pause_sequence),
        )
        // Effects
        .route("/effects", get(routes::effects::list_effects))
        .route("/effects/{id}", get(routes::effects::get_effect))
        .route("/effects/{id}", patch(routes::effects::update_effect))
        .route("/effects/{id}", delete(routes::effects::delete_effect))
        .route(
            "/effects/{id}/params/{param}",
            put(routes::effects::set_effect_param),
        )
        // Samples
        .route("/samples", get(routes::samples::list_samples))
        .route("/samples/{id}", get(routes::samples::get_sample))
        // SynthDefs
        .route("/synthdefs", get(routes::synthdefs::list_synthdefs))
        // Eval
        .route("/eval", post(routes::eval::eval_code))
        // Live state
        .route("/live", get(routes::live::get_live_state))
        .route("/live/transport", get(routes::live::get_transport_state))
        .route("/live/fades", get(routes::live::get_active_fades))
        // WebSocket
        .route("/ws", get(websocket::ws_handler))
        // Add shared state
        .with_state(app_state)
        // Add CORS middleware
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(
        "HTTP API server starting on http://{}:{}",
        addr.ip(),
        addr.port()
    );

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
