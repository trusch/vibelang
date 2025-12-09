//! vibelang-keys - Terminal MIDI Keyboard for VibeLang
//!
//! A terminal-based MIDI keyboard that lets you play music using your computer keyboard.
//! Features include:
//!
//! - Piano-style visualization in the terminal
//! - Multiple keyboard layouts (German QWERTZ, US QWERTY, or custom)
//! - JACK MIDI output
//! - OS-level key detection for reliable key release handling
//! - Configurable via TOML file
//!
//! # Usage as a Library
//!
//! ```no_run
//! use vibelang_keys::{VirtualKeyboard, KeyboardConfig};
//!
//! // Create a keyboard with default config
//! let mut keyboard = VirtualKeyboard::new(KeyboardConfig::default());
//!
//! // Or load from config file
//! let config = vibelang_keys::Config::load_or_default();
//! let mut keyboard = VirtualKeyboard::new(config.to_keyboard_config());
//!
//! // Handle key events (uses char, not KeyCode)
//! if let Some((note, velocity)) = keyboard.key_down('c') {
//!     println!("Play note {} with velocity {}", note, velocity);
//! }
//! ```

pub mod config;
pub mod error;
pub mod keyboard;
pub mod midi;
pub mod os_keyboard;
pub mod ui;

// Re-export main types
pub use config::{Config, KeyboardLayout, Theme};
pub use error::{Error, Result};
pub use keyboard::{KeyMapping, KeyboardConfig, VirtualKeyboard, note_name, C3_MIDI, DEFAULT_VELOCITY};
pub use midi::{MidiBackend, MidiOutput};
pub use os_keyboard::{OsKeyEvent, OsKeyboardListener, is_available as os_keyboard_available};
pub use ui::{render_keyboard, render_keyboard_compact, render_keyboard_standalone, KeyboardWidget};
