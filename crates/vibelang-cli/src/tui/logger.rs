//! Custom logger that routes messages to TUI or env_logger

use log::{Level, LevelFilter, Metadata, Record};
use std::sync::atomic::{AtomicBool, Ordering};

static TUI_MODE: AtomicBool = AtomicBool::new(false);

/// Custom logger that routes to TUI when enabled, otherwise to env_logger
pub struct TuiLogger;

impl log::Log for TuiLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // Always respect the log level filter
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        if TUI_MODE.load(Ordering::Relaxed) {
            // Route to TUI
            // Send to TUI event channel
            if record.level() == Level::Error {
                super::send_tui_event(super::TuiEvent::Error(record.args().to_string()));
            } else {
                super::send_tui_event(super::TuiEvent::Log {
                    level: record.level(),
                    message: record.args().to_string(),
                });
            }
        } else {
            // When not in TUI mode, we need to print to stderr directly
            // since env_logger is not being used as the global logger
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Initialize the logger in normal mode (logs to stderr)
pub fn init_logger() {
    TUI_MODE.store(false, Ordering::Relaxed);

    // Set the custom logger as the global logger
    if log::set_logger(&TUI_LOGGER).is_ok() {
        // Set default level to Info, but allow override via RUST_LOG
        let default_level = std::env::var("RUST_LOG")
            .ok()
            .and_then(|s| s.parse::<LevelFilter>().ok())
            .unwrap_or(LevelFilter::Info);

        log::set_max_level(default_level);
    }
}

/// Initialize the logger in TUI mode (logs to TUI widget only)
pub fn init_tui_logger() {
    TUI_MODE.store(true, Ordering::Relaxed);

    // Set the custom logger as the global logger
    if log::set_logger(&TUI_LOGGER).is_ok() {
        // Set default level to Info, but allow override via RUST_LOG
        let default_level = std::env::var("RUST_LOG")
            .ok()
            .and_then(|s| s.parse::<LevelFilter>().ok())
            .unwrap_or(LevelFilter::Info);

        log::set_max_level(default_level);
    }
}

static TUI_LOGGER: TuiLogger = TuiLogger;
