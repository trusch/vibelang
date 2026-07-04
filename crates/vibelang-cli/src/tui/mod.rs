//! Terminal UI for vibelang (using the vibelang-core runtime)
//!
//! Provides a real-time display of system state using ratatui

pub mod app;
pub mod keyboard;
pub mod layout;
pub mod logger;
pub mod os_keyboard;
pub mod ui;

pub use app::TuiApp;
pub use logger::init_tui_logger;

use std::sync::Mutex;
use tokio::sync::mpsc;

/// Event types that can be sent to the TUI
#[derive(Clone, Debug)]
pub enum TuiEvent {
    /// A log message to display
    Log { level: log::Level, message: String },
    /// An error occurred
    Error(String),
}

/// Shared TUI event sender (using tokio channel)
static TUI_EVENT_SENDER: Mutex<Option<mpsc::UnboundedSender<TuiEvent>>> = Mutex::new(None);

/// Initialize the TUI event channel
pub fn init_tui_channel() -> mpsc::UnboundedReceiver<TuiEvent> {
    let (sender, receiver) = mpsc::unbounded_channel();
    *TUI_EVENT_SENDER.lock().unwrap() = Some(sender);
    receiver
}

/// Send a TUI event
pub fn send_tui_event(event: TuiEvent) {
    if let Some(sender) = TUI_EVENT_SENDER.lock().unwrap().as_ref() {
        let _ = sender.send(event);
    }
}
