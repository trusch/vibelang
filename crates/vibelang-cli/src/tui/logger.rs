//! Custom logger that routes to TUI or stderr

use crate::tui;
use log::{Level, Log, Metadata, Record, SetLoggerError};

/// TUI-aware logger
struct TuiLogger {
    tui_mode: bool,
}

impl Log for TuiLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Trace
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let message = format!("{}", record.args());

        if self.tui_mode {
            // Route to TUI
            tui::send_tui_event(tui::TuiEvent::Log {
                level: record.level(),
                message,
            });
        } else {
            // Route to stderr
            eprintln!("[{}] {}", record.level(), message);
        }
    }

    fn flush(&self) {}
}

static LOGGER: std::sync::OnceLock<TuiLogger> = std::sync::OnceLock::new();

/// Initialize the logger in TUI mode
pub fn init_tui_logger() -> Result<(), SetLoggerError> {
    let logger = LOGGER.get_or_init(|| TuiLogger { tui_mode: true });
    log::set_logger(logger)?;
    log::set_max_level(log::LevelFilter::Debug);
    Ok(())
}
