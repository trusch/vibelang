//! HTTP REST API server for VibeLang.
//!
//! Provides a REST API and WebSocket endpoint for querying and controlling
//! a running VibeLang session.
//!
//! # Features
//!
//! - Full CRUD operations for voices, patterns, melodies, sequences
//! - Transport control (play, stop, seek, tempo)
//! - Effect and sample management
//! - MIDI routing and recording
//! - Real-time WebSocket events
//! - Live state queries (active synths, meters)
//!
//! # Usage
//!
//! ```ignore
//! use vibelang_http::{start_server, EvalJob, EvalSender};
//! use vibelang_core::RuntimeHandle;
//!
//! let (eval_tx, eval_rx) = std::sync::mpsc::channel();
//! tokio::spawn(async move {
//!     start_server(handle, 1606, Some(eval_tx)).await;
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
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use vibelang_core::RuntimeHandle;

pub use models::*;
pub use routes::eval::{EvalJob, EvalResult};
pub use websocket::WebSocketEvent;

/// Sender type for eval requests.
pub type EvalSender = std::sync::mpsc::Sender<EvalJob>;

/// Shared application state for HTTP handlers.
pub struct AppState {
    /// Runtime handle for state access and message sending.
    pub handle: RuntimeHandle,
    /// Broadcast channel for WebSocket events.
    pub ws_tx: broadcast::Sender<WebSocketEvent>,
    /// Channel to send eval requests to the main thread (optional).
    pub eval_tx: Option<EvalSender>,
}

/// Start the HTTP server on the specified port.
///
/// # Arguments
///
/// * `handle` - The VibeLang runtime handle for accessing state
/// * `port` - The port to listen on
/// * `eval_tx` - Optional channel to send code evaluation requests to the main thread
///
/// # Example
///
/// ```ignore
/// let handle = runtime.handle();
/// let (eval_tx, eval_rx) = std::sync::mpsc::channel();
/// tokio::spawn(async move {
///     start_server(handle, 1606, Some(eval_tx)).await;
/// });
/// ```
pub async fn start_server(handle: RuntimeHandle, port: u16, eval_tx: Option<EvalSender>) {
    // Create broadcast channel for WebSocket events
    let (ws_tx, _) = broadcast::channel::<WebSocketEvent>(1024);

    let state = Arc::new(AppState {
        handle: handle.clone(),
        ws_tx: ws_tx.clone(),
        eval_tx,
    });

    // Start the event broadcaster in the background
    let broadcast_handle = handle.clone();
    let broadcast_tx = ws_tx.clone();
    tokio::spawn(async move {
        websocket::run_event_broadcaster(broadcast_handle, broadcast_tx).await;
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
        .route("/groups", post(routes::groups::create_group))
        .route("/groups/:path", get(routes::groups::get_group))
        .route("/groups/:path", patch(routes::groups::update_group))
        .route("/groups/:path", delete(routes::groups::delete_group))
        .route("/groups/:path/mute", post(routes::groups::mute_group))
        .route("/groups/:path/unmute", post(routes::groups::unmute_group))
        .route("/groups/:path/solo", post(routes::groups::solo_group))
        .route("/groups/:path/unsolo", post(routes::groups::unsolo_group))
        .route(
            "/groups/:path/params/:param",
            put(routes::groups::set_group_param),
        )
        // Voices
        .route("/voices", get(routes::voices::list_voices))
        .route("/voices", post(routes::voices::create_voice))
        .route("/voices/:name", get(routes::voices::get_voice))
        .route("/voices/:name", patch(routes::voices::update_voice))
        .route("/voices/:name", delete(routes::voices::delete_voice))
        .route("/voices/:name/trigger", post(routes::voices::trigger_voice))
        .route("/voices/:name/stop", post(routes::voices::stop_voice))
        .route("/voices/:name/note-on", post(routes::voices::note_on))
        .route("/voices/:name/note-off", post(routes::voices::note_off))
        .route(
            "/voices/:name/params/:param",
            put(routes::voices::set_voice_param),
        )
        .route("/voices/:name/mute", post(routes::voices::mute_voice))
        .route("/voices/:name/unmute", post(routes::voices::unmute_voice))
        // Patterns
        .route("/patterns", get(routes::patterns::list_patterns))
        .route("/patterns", post(routes::patterns::create_pattern))
        .route("/patterns/:name", get(routes::patterns::get_pattern))
        .route("/patterns/:name", patch(routes::patterns::update_pattern))
        .route("/patterns/:name", delete(routes::patterns::delete_pattern))
        .route("/patterns/:name/start", post(routes::patterns::start_pattern))
        .route("/patterns/:name/stop", post(routes::patterns::stop_pattern))
        // Melodies
        .route("/melodies", get(routes::melodies::list_melodies))
        .route("/melodies", post(routes::melodies::create_melody))
        .route("/melodies/:name", get(routes::melodies::get_melody))
        .route("/melodies/:name", patch(routes::melodies::update_melody))
        .route("/melodies/:name", delete(routes::melodies::delete_melody))
        .route("/melodies/:name/start", post(routes::melodies::start_melody))
        .route("/melodies/:name/stop", post(routes::melodies::stop_melody))
        // Sequences
        .route("/sequences", get(routes::sequences::list_sequences))
        .route("/sequences", post(routes::sequences::create_sequence))
        .route("/sequences/:name", get(routes::sequences::get_sequence))
        .route("/sequences/:name", patch(routes::sequences::update_sequence))
        .route("/sequences/:name", delete(routes::sequences::delete_sequence))
        .route(
            "/sequences/:name/start",
            post(routes::sequences::start_sequence),
        )
        .route(
            "/sequences/:name/stop",
            post(routes::sequences::stop_sequence),
        )
        .route(
            "/sequences/:name/pause",
            post(routes::sequences::pause_sequence),
        )
        // Effects
        .route("/effects", get(routes::effects::list_effects))
        .route("/effects", post(routes::effects::create_effect))
        .route("/effects/:id", get(routes::effects::get_effect))
        .route("/effects/:id", patch(routes::effects::update_effect))
        .route("/effects/:id", delete(routes::effects::delete_effect))
        .route(
            "/effects/:id/params/:param",
            put(routes::effects::set_effect_param),
        )
        // Samples
        .route("/samples", get(routes::samples::list_samples))
        .route("/samples", post(routes::samples::load_sample))
        .route("/samples/:id", get(routes::samples::get_sample))
        .route("/samples/:id", delete(routes::samples::free_sample))
        // SynthDefs
        .route("/synthdefs", get(routes::synthdefs::list_synthdefs))
        .route("/synthdefs/:name", get(routes::synthdefs::get_synthdef))
        // Eval
        .route("/eval", post(routes::eval::eval_code))
        // Fades
        .route("/fades", get(routes::fades::list_fades))
        .route("/fades", post(routes::fades::create_fade))
        .route("/fades/:id", delete(routes::fades::cancel_fade))
        // MIDI
        .route("/midi/devices", get(routes::midi::list_devices))
        .route("/midi/devices/:id", post(routes::midi::connect_device))
        .route("/midi/devices/:id", delete(routes::midi::disconnect_device))
        .route("/midi/routing", get(routes::midi::get_routing))
        .route("/midi/routing/keyboard", get(routes::midi::list_keyboard_routes))
        .route("/midi/routing/keyboard", post(routes::midi::add_keyboard_route))
        .route("/midi/routing/note", get(routes::midi::list_note_routes))
        .route("/midi/routing/note", post(routes::midi::add_note_route))
        .route("/midi/routing/cc", get(routes::midi::list_cc_routes))
        .route("/midi/routing/cc", post(routes::midi::add_cc_route))
        .route("/midi/callbacks", get(routes::midi::list_callbacks))
        .route("/midi/recording", get(routes::midi::get_recording_state))
        .route("/midi/recording", patch(routes::midi::update_recording_settings))
        .route("/midi/recording/notes", get(routes::midi::get_recorded_notes))
        .route("/midi/recording/export", get(routes::midi::export_recording))
        .route("/midi/monitor", post(routes::midi::set_monitor))
        // Live state
        .route("/live", get(routes::live::get_live_state))
        .route("/live/synths", get(routes::live::get_active_synths))
        .route("/live/sequences", get(routes::live::get_active_sequences))
        .route("/live/notes", get(routes::live::get_active_notes))
        .route("/live/meters", get(routes::live::get_meters))
        // WebSocket
        .route("/ws", get(websocket::ws_handler))
        // Add shared state
        .with_state(state)
        // Add CORS middleware
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!(
        "HTTP API server starting on http://{}:{}",
        addr.ip(),
        addr.port()
    );

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
