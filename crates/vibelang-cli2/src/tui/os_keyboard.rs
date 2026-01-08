//! OS-level keyboard input using rdev
//!
//! This bypasses terminal limitations for reliable key-up detection.

use rdev::{listen, Event, EventType, Key};
use std::sync::mpsc;

/// Key event from OS-level keyboard listener
#[derive(Debug, Clone)]
pub enum OsKeyEvent {
    /// Key was pressed
    KeyDown(char),
    /// Key was released
    KeyUp(char),
}

/// Start the OS keyboard listener in a background thread
/// Returns a receiver for key events
pub fn start_os_keyboard_listener() -> Option<mpsc::Receiver<OsKeyEvent>> {
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let callback = move |event: Event| {
            match event.event_type {
                EventType::KeyPress(key) => {
                    if let Some(c) = key_to_char(key) {
                        let _ = tx.send(OsKeyEvent::KeyDown(c));
                    }
                }
                EventType::KeyRelease(key) => {
                    if let Some(c) = key_to_char(key) {
                        let _ = tx.send(OsKeyEvent::KeyUp(c));
                    }
                }
                _ => {}
            }
        };

        if let Err(e) = listen(callback) {
            log::warn!("OS keyboard listener error: {:?}", e);
        }
    });

    Some(rx)
}

/// Convert an rdev Key to a character (for German keyboard layout)
fn key_to_char(key: Key) -> Option<char> {
    match key {
        // Number row
        Key::Num1 => Some('1'),
        Key::Num2 => Some('2'),
        Key::Num3 => Some('3'),
        Key::Num4 => Some('4'),
        Key::Num5 => Some('5'),
        Key::Num6 => Some('6'),
        Key::Num7 => Some('7'),
        Key::Num8 => Some('8'),
        Key::Num9 => Some('9'),
        Key::Num0 => Some('0'),

        // QWERTY row
        Key::KeyQ => Some('q'),
        Key::KeyW => Some('w'),
        Key::KeyE => Some('e'),
        Key::KeyR => Some('r'),
        Key::KeyT => Some('t'),
        Key::KeyY => Some('z'), // German layout: Y is at Z position
        Key::KeyU => Some('u'),
        Key::KeyI => Some('i'),
        Key::KeyO => Some('o'),
        Key::KeyP => Some('p'),

        // Home row
        Key::KeyA => Some('a'),
        Key::KeyS => Some('s'),
        Key::KeyD => Some('d'),
        Key::KeyF => Some('f'),
        Key::KeyG => Some('g'),
        Key::KeyH => Some('h'),
        Key::KeyJ => Some('j'),
        Key::KeyK => Some('k'),
        Key::KeyL => Some('l'),

        // Bottom row
        Key::KeyZ => Some('y'), // German layout: Z is at Y position
        Key::KeyX => Some('x'),
        Key::KeyC => Some('c'),
        Key::KeyV => Some('v'),
        Key::KeyB => Some('b'),
        Key::KeyN => Some('n'),
        Key::KeyM => Some('m'),

        // Special keys
        Key::Comma => Some(','),
        Key::Dot => Some('.'),
        Key::Minus => Some('-'),
        Key::Space => Some(' '),

        _ => None,
    }
}
