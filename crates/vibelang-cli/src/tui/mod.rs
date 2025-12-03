//! Terminal UI for vibelang
//!
//! Provides a real-time display of system state using ratatui

pub mod app;
pub mod layout;
pub mod logger;
pub mod ui;

pub use app::TuiApp;
pub use logger::{init_logger, init_tui_logger};

use crossbeam_channel::{Receiver, Sender};
use log::Level;
use std::sync::Mutex;

/// Event types that can be sent to the TUI
#[derive(Clone, Debug)]
pub enum TuiEvent {
    /// A log message to display
    Log { level: log::Level, message: String },
    /// An error occurred
    Error(String),
    /// Clear the current error
    ClearError,
}

/// Shared TUI event sender
static TUI_EVENT_SENDER: Mutex<Option<Sender<TuiEvent>>> = Mutex::new(None);

/// Initialize the TUI event channel
pub fn init_tui_channel() -> Receiver<TuiEvent> {
    let (sender, receiver) = crossbeam_channel::unbounded();
    *TUI_EVENT_SENDER.lock().unwrap() = Some(sender);
    receiver
}

/// Send a TUI event
pub fn send_tui_event(event: TuiEvent) {
    if let Some(sender) = TUI_EVENT_SENDER.lock().unwrap().as_ref() {
        let _ = sender.send(event);
    }
}

/// Send a log message to the TUI
pub fn log(message: impl Into<String>) {
    log_with_level(Level::Info, message);
}

pub fn log_with_level(level: Level, message: impl Into<String>) {
    send_tui_event(TuiEvent::Log {
        level,
        message: message.into(),
    });
}

/// Send an error message to the TUI
pub fn error(message: impl Into<String>) {
    send_tui_event(TuiEvent::Error(message.into()));
}

/// Clear the current error
pub fn clear_error() {
    send_tui_event(TuiEvent::ClearError);
}
