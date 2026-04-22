//! Virtual MIDI keyboard for the TUI
//!
//! Provides a piano keyboard visualization that can be played using computer keys.

use crossterm::event::KeyCode;
use std::collections::{HashMap, HashSet};

/// MIDI note number for C3 (middle C in some conventions)
pub const C3_MIDI: u8 = 48;

/// Default velocity for key presses
pub const DEFAULT_VELOCITY: u8 = 100;

/// A key mapping entry: computer key -> MIDI note offset from base
#[derive(Debug, Clone)]
pub struct KeyMapping {
    /// The computer key code
    pub key: KeyCode,
    /// The character to display (for rendering)
    pub display_char: char,
    /// MIDI note offset from the base note (can be negative)
    pub note_offset: i8,
    /// Whether this is a black key (sharp/flat)
    pub is_black_key: bool,
}

/// Virtual keyboard configuration
#[derive(Debug, Clone)]
pub struct KeyboardConfig {
    /// Key mappings (computer key -> note info)
    pub mappings: Vec<KeyMapping>,
    /// Base MIDI note (the note that 'C' key plays)
    pub base_note: u8,
    /// Default velocity for key presses
    pub velocity: u8,
    /// MIDI channel (0-15) - reserved for future MIDI output
    #[allow(dead_code)]
    pub channel: u8,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self::german_layout()
    }
}

impl KeyboardConfig {
    /// Create a German keyboard layout configuration (QWERTZ)
    pub fn german_layout() -> Self {
        let mappings = vec![
            // Lower octave - white keys (bottom row)
            KeyMapping {
                key: KeyCode::Char('y'),
                display_char: 'Y',
                note_offset: -3,
                is_black_key: false,
            }, // A2
            KeyMapping {
                key: KeyCode::Char('x'),
                display_char: 'X',
                note_offset: -1,
                is_black_key: false,
            }, // B2
            KeyMapping {
                key: KeyCode::Char('c'),
                display_char: 'C',
                note_offset: 0,
                is_black_key: false,
            }, // C3
            KeyMapping {
                key: KeyCode::Char('v'),
                display_char: 'V',
                note_offset: 2,
                is_black_key: false,
            }, // D3
            KeyMapping {
                key: KeyCode::Char('b'),
                display_char: 'B',
                note_offset: 4,
                is_black_key: false,
            }, // E3
            KeyMapping {
                key: KeyCode::Char('n'),
                display_char: 'N',
                note_offset: 5,
                is_black_key: false,
            }, // F3
            KeyMapping {
                key: KeyCode::Char('m'),
                display_char: 'M',
                note_offset: 7,
                is_black_key: false,
            }, // G3
            KeyMapping {
                key: KeyCode::Char(','),
                display_char: ',',
                note_offset: 9,
                is_black_key: false,
            }, // A3
            KeyMapping {
                key: KeyCode::Char('.'),
                display_char: '.',
                note_offset: 11,
                is_black_key: false,
            }, // B3
            KeyMapping {
                key: KeyCode::Char('-'),
                display_char: '-',
                note_offset: 12,
                is_black_key: false,
            }, // C4
            // Lower octave - black keys (home row)
            KeyMapping {
                key: KeyCode::Char('s'),
                display_char: 'S',
                note_offset: -2,
                is_black_key: true,
            }, // A#2
            KeyMapping {
                key: KeyCode::Char('f'),
                display_char: 'F',
                note_offset: 1,
                is_black_key: true,
            }, // C#3
            KeyMapping {
                key: KeyCode::Char('g'),
                display_char: 'G',
                note_offset: 3,
                is_black_key: true,
            }, // D#3
            KeyMapping {
                key: KeyCode::Char('j'),
                display_char: 'J',
                note_offset: 6,
                is_black_key: true,
            }, // F#3
            KeyMapping {
                key: KeyCode::Char('k'),
                display_char: 'K',
                note_offset: 8,
                is_black_key: true,
            }, // G#3
            KeyMapping {
                key: KeyCode::Char('l'),
                display_char: 'L',
                note_offset: 10,
                is_black_key: true,
            }, // A#3
            // Upper octave - white keys (QWERTY row)
            KeyMapping {
                key: KeyCode::Char('q'),
                display_char: 'Q',
                note_offset: 14,
                is_black_key: false,
            }, // D4
            KeyMapping {
                key: KeyCode::Char('w'),
                display_char: 'W',
                note_offset: 16,
                is_black_key: false,
            }, // E4
            KeyMapping {
                key: KeyCode::Char('e'),
                display_char: 'E',
                note_offset: 17,
                is_black_key: false,
            }, // F4
            KeyMapping {
                key: KeyCode::Char('r'),
                display_char: 'R',
                note_offset: 19,
                is_black_key: false,
            }, // G4
            KeyMapping {
                key: KeyCode::Char('t'),
                display_char: 'T',
                note_offset: 21,
                is_black_key: false,
            }, // A4
            KeyMapping {
                key: KeyCode::Char('z'),
                display_char: 'Z',
                note_offset: 23,
                is_black_key: false,
            }, // B4
            KeyMapping {
                key: KeyCode::Char('u'),
                display_char: 'U',
                note_offset: 24,
                is_black_key: false,
            }, // C5
            // Upper octave - black keys (number row)
            KeyMapping {
                key: KeyCode::Char('1'),
                display_char: '1',
                note_offset: 13,
                is_black_key: true,
            }, // C#4
            KeyMapping {
                key: KeyCode::Char('2'),
                display_char: '2',
                note_offset: 15,
                is_black_key: true,
            }, // D#4
            KeyMapping {
                key: KeyCode::Char('4'),
                display_char: '4',
                note_offset: 18,
                is_black_key: true,
            }, // F#4
            KeyMapping {
                key: KeyCode::Char('5'),
                display_char: '5',
                note_offset: 20,
                is_black_key: true,
            }, // G#4
            KeyMapping {
                key: KeyCode::Char('6'),
                display_char: '6',
                note_offset: 22,
                is_black_key: true,
            }, // A#4
        ];

        Self {
            mappings,
            base_note: C3_MIDI,
            velocity: DEFAULT_VELOCITY,
            channel: 0,
        }
    }

    /// Get the mapping for a given key
    pub fn get_mapping(&self, key: KeyCode) -> Option<&KeyMapping> {
        self.mappings.iter().find(|m| m.key == key)
    }

    /// Get lower octave white keys
    pub fn lower_white_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self
            .mappings
            .iter()
            .filter(|m| !m.is_black_key && m.note_offset <= 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get upper octave white keys
    pub fn upper_white_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self
            .mappings
            .iter()
            .filter(|m| !m.is_black_key && m.note_offset > 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get lower octave black keys
    pub fn lower_black_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self
            .mappings
            .iter()
            .filter(|m| m.is_black_key && m.note_offset <= 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get upper octave black keys
    pub fn upper_black_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self
            .mappings
            .iter()
            .filter(|m| m.is_black_key && m.note_offset > 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }
}

/// Default note release timeout in milliseconds
const DEFAULT_NOTE_RELEASE_MS: u64 = 400;

/// Virtual keyboard state
#[derive(Debug, Clone)]
pub struct VirtualKeyboard {
    /// Configuration
    pub config: KeyboardConfig,
    /// Currently pressed notes (MIDI note numbers)
    pub pressed_notes: HashSet<u8>,
    /// Map from note to the key that pressed it (for display)
    pub note_to_key: HashMap<u8, KeyCode>,
    /// Timestamp of last touch for each note (for auto-release)
    pub note_timestamps: HashMap<u8, std::time::Instant>,
    /// Whether the keyboard is visible
    pub visible: bool,
    /// Octave shift (applied on top of base_note)
    pub octave_shift: i8,
    /// Duration after which untouched notes are released
    pub note_release_duration: std::time::Duration,
}

impl Default for VirtualKeyboard {
    fn default() -> Self {
        Self::new(KeyboardConfig::default())
    }
}

impl VirtualKeyboard {
    /// Create a new virtual keyboard with the given configuration
    pub fn new(config: KeyboardConfig) -> Self {
        Self {
            config,
            pressed_notes: HashSet::new(),
            note_to_key: HashMap::new(),
            note_timestamps: HashMap::new(),
            visible: false,
            octave_shift: 0,
            note_release_duration: std::time::Duration::from_millis(DEFAULT_NOTE_RELEASE_MS),
        }
    }

    /// Toggle keyboard visibility
    #[allow(dead_code)]
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.pressed_notes.clear();
            self.note_to_key.clear();
            self.note_timestamps.clear();
        }
    }

    /// Show the keyboard
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the keyboard and release all notes
    pub fn hide(&mut self) -> Vec<u8> {
        self.visible = false;
        let released: Vec<u8> = self.pressed_notes.drain().collect();
        self.note_to_key.clear();
        self.note_timestamps.clear();
        released
    }

    /// Release all pressed notes without changing visibility
    #[allow(dead_code)]
    pub fn release_all(&mut self) -> Vec<u8> {
        let released: Vec<u8> = self.pressed_notes.drain().collect();
        self.note_to_key.clear();
        self.note_timestamps.clear();
        released
    }

    /// Get the effective base note (with octave shift)
    pub fn effective_base_note(&self) -> u8 {
        let shifted = self.config.base_note as i16 + (self.octave_shift as i16 * 12);
        shifted.clamp(0, 127) as u8
    }

    /// Get the MIDI note for a key press (with octave shift applied)
    pub fn get_note_for_key(&self, key: KeyCode) -> Option<u8> {
        self.config.get_mapping(key).map(|m| {
            let note = self.effective_base_note() as i16 + m.note_offset as i16;
            note.clamp(0, 127) as u8
        })
    }

    /// Handle a key press - returns Some((note, velocity)) if a note should be triggered
    pub fn key_down(&mut self, key: KeyCode) -> Option<(u8, u8)> {
        if !self.visible {
            return None;
        }

        if let Some(note) = self.get_note_for_key(key) {
            let now = std::time::Instant::now();
            if !self.pressed_notes.contains(&note) {
                self.pressed_notes.insert(note);
                self.note_to_key.insert(note, key);
                self.note_timestamps.insert(note, now);
                return Some((note, self.config.velocity));
            } else {
                // Key repeat - extend the note by updating timestamp
                self.note_timestamps.insert(note, now);
            }
        }
        None
    }

    /// Touch a note to extend its duration
    #[allow(dead_code)]
    pub fn touch_note(&mut self, key: KeyCode) {
        if !self.visible {
            return;
        }
        if let Some(note) = self.get_note_for_key(key) {
            if self.pressed_notes.contains(&note) {
                self.note_timestamps.insert(note, std::time::Instant::now());
            }
        }
    }

    /// Handle a key release - returns Some(note) if a note should be released
    pub fn key_up(&mut self, key: KeyCode) -> Option<u8> {
        if !self.visible {
            return None;
        }

        if let Some(note) = self.get_note_for_key(key) {
            if self.pressed_notes.remove(&note) {
                self.note_to_key.remove(&note);
                self.note_timestamps.remove(&note);
                return Some(note);
            }
        }
        None
    }

    /// Get notes that have expired and should be released
    pub fn get_expired_notes(&mut self) -> Vec<u8> {
        if !self.visible {
            return Vec::new();
        }

        let now = std::time::Instant::now();
        let expired: Vec<u8> = self
            .note_timestamps
            .iter()
            .filter(|(_, &timestamp)| now.duration_since(timestamp) > self.note_release_duration)
            .map(|(&note, _)| note)
            .collect();

        for &note in &expired {
            self.pressed_notes.remove(&note);
            self.note_to_key.remove(&note);
            self.note_timestamps.remove(&note);
        }

        expired
    }

    /// Check if a note is currently pressed
    #[allow(dead_code)]
    pub fn is_note_pressed(&self, note: u8) -> bool {
        self.pressed_notes.contains(&note)
    }

    /// Check if a key mapping corresponds to a pressed note
    pub fn is_key_pressed(&self, mapping: &KeyMapping) -> bool {
        let note = self.effective_base_note() as i16 + mapping.note_offset as i16;
        if !(0..=127).contains(&note) {
            return false;
        }
        self.pressed_notes.contains(&(note as u8))
    }

    /// Shift octave up
    pub fn octave_up(&mut self) -> Vec<u8> {
        let released: Vec<u8> = self.pressed_notes.drain().collect();
        self.note_to_key.clear();
        self.note_timestamps.clear();
        if self.octave_shift < 4 {
            self.octave_shift += 1;
        }
        released
    }

    /// Shift octave down
    pub fn octave_down(&mut self) -> Vec<u8> {
        let released: Vec<u8> = self.pressed_notes.drain().collect();
        self.note_to_key.clear();
        self.note_timestamps.clear();
        if self.octave_shift > -4 {
            self.octave_shift -= 1;
        }
        released
    }

    /// Get the current octave display name
    pub fn octave_name(&self) -> String {
        let base = self.effective_base_note();
        let octave = (base / 12) as i8 - 1;
        format!("C{}", octave)
    }

    /// Get velocity
    pub fn velocity(&self) -> u8 {
        self.config.velocity
    }

    /// Set velocity
    #[allow(dead_code)]
    pub fn set_velocity(&mut self, velocity: u8) {
        self.config.velocity = velocity.min(127);
    }

    /// Get the MIDI channel
    #[allow(dead_code)]
    pub fn channel(&self) -> u8 {
        self.config.channel
    }

    /// Set the MIDI channel
    #[allow(dead_code)]
    pub fn set_channel(&mut self, channel: u8) {
        self.config.channel = channel.min(15);
    }
}
