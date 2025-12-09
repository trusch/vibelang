//! OS-level keyboard input using rdev
//!
//! This module provides reliable key press and release detection by
//! intercepting keyboard events at the OS level, bypassing terminal limitations.

use crossbeam_channel::{unbounded, Receiver, Sender};
use rdev::{listen, Event, EventType, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// Keyboard events from the OS-level listener
#[derive(Debug, Clone)]
pub enum OsKeyEvent {
    /// A key was pressed
    Press(char),
    /// A key was released
    Release(char),
}

/// OS-level keyboard listener that captures key press and release events
pub struct OsKeyboardListener {
    /// Channel receiver for keyboard events
    event_rx: Receiver<OsKeyEvent>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Listener thread handle
    _thread: JoinHandle<()>,
}

impl OsKeyboardListener {
    /// Start the OS keyboard listener
    ///
    /// Returns None if the listener couldn't be started (e.g., on systems without X11)
    pub fn new() -> Option<Self> {
        // Check if we can listen for keyboard events on this system
        if !is_available() {
            return None;
        }

        let (tx, rx) = unbounded();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        // Spawn listener thread
        let thread = thread::spawn(move || {
            run_listener(tx, shutdown_clone);
        });

        // Give the thread a moment to start
        thread::sleep(std::time::Duration::from_millis(100));

        Some(Self {
            event_rx: rx,
            shutdown,
            _thread: thread,
        })
    }

    /// Try to receive a keyboard event (non-blocking)
    pub fn try_recv(&self) -> Option<OsKeyEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Get the event receiver for use in select! or other patterns
    pub fn receiver(&self) -> &Receiver<OsKeyEvent> {
        &self.event_rx
    }
}

impl Drop for OsKeyboardListener {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

/// Map rdev Key to a character
///
/// Note: rdev reports physical key positions, not logical characters.
/// This mapping is designed for German QWERTZ keyboards:
/// - Physical KeyZ position (US Z) = German Y key (bottom row)
/// - Physical KeyY position (US Y) = German Z key (QWERTY row)
pub fn key_to_char(key: Key) -> Option<char> {
    key_to_char_german(key)
}

/// Map rdev Key to character using German QWERTZ layout
pub fn key_to_char_german(key: Key) -> Option<char> {
    match key {
        // Lower octave: Bottom row (Y X C V B N M , . -)
        // KeyZ = physical Z position on US = German Y key on bottom row
        Key::KeyZ => Some('y'),
        Key::KeyX => Some('x'),
        Key::KeyC => Some('c'),
        Key::KeyV => Some('v'),
        Key::KeyB => Some('b'),
        Key::KeyN => Some('n'),
        Key::KeyM => Some('m'),
        Key::Comma => Some(','),
        Key::Dot => Some('.'),
        // The '-' key on German keyboards (right of '.') is at the physical position of '/' on US keyboards
        Key::Minus | Key::Slash => Some('-'),

        // Lower octave black keys (home row: S F G J K L)
        Key::KeyS => Some('s'),
        Key::KeyF => Some('f'),
        Key::KeyG => Some('g'),
        Key::KeyJ => Some('j'),
        Key::KeyK => Some('k'),
        Key::KeyL => Some('l'),

        // Upper octave: QWERTY row (Q W E R T Z U)
        Key::KeyQ => Some('q'),
        Key::KeyW => Some('w'),
        Key::KeyE => Some('e'),
        Key::KeyR => Some('r'),
        Key::KeyT => Some('t'),
        // KeyY = physical Y position on US = German Z key on QWERTY row
        Key::KeyY => Some('z'),
        Key::KeyU => Some('u'),

        // Upper octave black keys (number row: 1 2 4 5 6)
        Key::Num1 => Some('1'),
        Key::Num2 => Some('2'),
        // 3 is skipped (no black key between E and F)
        Key::Num4 => Some('4'),
        Key::Num5 => Some('5'),
        Key::Num6 => Some('6'),
        // 7 is skipped (no black key between B and C)

        // Control keys
        Key::Escape => Some('\x1b'),
        Key::Space => Some(' '),

        // Octave control
        Key::ShiftLeft | Key::ShiftRight => None, // handled separately
        Key::DownArrow => Some('<'), // octave down (also mapped to < key)
        Key::UpArrow => Some('>'),   // octave up (also mapped to > key)

        _ => None,
    }
}

/// Map rdev Key to character using US QWERTY layout
pub fn key_to_char_us(key: Key) -> Option<char> {
    match key {
        // Lower octave: Bottom row (Z X C V B N M , . /)
        Key::KeyZ => Some('z'),
        Key::KeyX => Some('x'),
        Key::KeyC => Some('c'),
        Key::KeyV => Some('v'),
        Key::KeyB => Some('b'),
        Key::KeyN => Some('n'),
        Key::KeyM => Some('m'),
        Key::Comma => Some(','),
        Key::Dot => Some('.'),
        Key::Slash => Some('/'),

        // Lower octave black keys (home row: S D F G H J K L)
        Key::KeyS => Some('s'),
        Key::KeyD => Some('d'),
        Key::KeyF => Some('f'),
        Key::KeyG => Some('g'),
        Key::KeyH => Some('h'),
        Key::KeyJ => Some('j'),
        Key::KeyK => Some('k'),
        Key::KeyL => Some('l'),

        // Upper octave: QWERTY row (Q W E R T Y U)
        Key::KeyQ => Some('q'),
        Key::KeyW => Some('w'),
        Key::KeyE => Some('e'),
        Key::KeyR => Some('r'),
        Key::KeyT => Some('t'),
        Key::KeyY => Some('y'),
        Key::KeyU => Some('u'),

        // Upper octave black keys (number row: 1 2 3 4 5 6 7)
        Key::Num1 => Some('1'),
        Key::Num2 => Some('2'),
        Key::Num3 => Some('3'),
        Key::Num4 => Some('4'),
        Key::Num5 => Some('5'),
        Key::Num6 => Some('6'),
        Key::Num7 => Some('7'),

        // Control keys
        Key::Escape => Some('\x1b'),
        Key::Space => Some(' '),

        _ => None,
    }
}

/// Run the rdev listener (blocking - runs in its own thread)
fn run_listener(tx: Sender<OsKeyEvent>, shutdown: Arc<AtomicBool>) {
    let callback = move |event: Event| {
        if shutdown.load(Ordering::Relaxed) {
            return;
        }

        match event.event_type {
            EventType::KeyPress(key) => {
                if let Some(c) = key_to_char(key) {
                    let _ = tx.send(OsKeyEvent::Press(c));
                }
            }
            EventType::KeyRelease(key) => {
                if let Some(c) = key_to_char(key) {
                    let _ = tx.send(OsKeyEvent::Release(c));
                }
            }
            _ => {}
        }
    };

    // This blocks until an error occurs
    if let Err(e) = listen(callback) {
        log::error!("OS keyboard listener error: {:?}", e);
    }
}

/// Check if the OS keyboard listener is likely to work on this system
pub fn is_available() -> bool {
    // On Linux, rdev requires X11 or Wayland
    #[cfg(target_os = "linux")]
    {
        std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok()
    }

    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_mapping_german() {
        // Test German layout mapping
        assert_eq!(key_to_char_german(Key::KeyZ), Some('y')); // German Y on US Z position
        assert_eq!(key_to_char_german(Key::KeyY), Some('z')); // German Z on US Y position
        assert_eq!(key_to_char_german(Key::KeyC), Some('c'));
        assert_eq!(key_to_char_german(Key::Num1), Some('1'));
    }

    #[test]
    fn test_key_mapping_us() {
        // Test US layout mapping
        assert_eq!(key_to_char_us(Key::KeyZ), Some('z'));
        assert_eq!(key_to_char_us(Key::KeyY), Some('y'));
        assert_eq!(key_to_char_us(Key::KeyC), Some('c'));
        assert_eq!(key_to_char_us(Key::Num1), Some('1'));
    }
}
