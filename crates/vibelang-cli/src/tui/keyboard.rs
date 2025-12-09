//! Virtual MIDI keyboard for the TUI
//!
//! Provides a piano keyboard visualization that can be played using computer keys.
//! The keyboard registers itself as a virtual MIDI source and integrates with
//! the existing MIDI routing system.

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
    /// MIDI channel (0-15)
    pub channel: u8,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self::german_layout()
    }
}

impl KeyboardConfig {
    /// Create a German keyboard layout configuration
    ///
    /// Physical keyboard layout (QWERTZ) - Two octave range:
    /// ```text
    ///  Upper octave (number + QWERTY rows):
    ///     1   2     4   5   6        (black keys on number row)
    ///     C#4 D#4   F#4 G#4 A#4
    ///    Q   W   E   R   T   Z   U   (white keys on QWERTY row)
    ///    D4  E4  F4  G4  A4  B4  C5
    ///
    ///  Lower octave (home + bottom rows):
    ///     S     F G     J K L        (black keys on home row)
    ///     A#2   C#D#    F#G#A#
    ///    Y   X   C   V   B   N   M   ,   .   -
    ///    A2  B2  C3  D3  E3  F3  G3  A3  B3  C4
    /// ```
    pub fn german_layout() -> Self {
        let mappings = vec![
            // White keys (bottom row) - left to right
            KeyMapping {
                key: KeyCode::Char('y'),
                display_char: 'Y',
                note_offset: -3, // A2
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('x'),
                display_char: 'X',
                note_offset: -1, // B2
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('c'),
                display_char: 'C',
                note_offset: 0, // C3 (base)
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('v'),
                display_char: 'V',
                note_offset: 2, // D3
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('b'),
                display_char: 'B',
                note_offset: 4, // E3
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('n'),
                display_char: 'N',
                note_offset: 5, // F3
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('m'),
                display_char: 'M',
                note_offset: 7, // G3
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char(','),
                display_char: ',',
                note_offset: 9, // A3
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('.'),
                display_char: '.',
                note_offset: 11, // B3
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('-'),
                display_char: '-',
                note_offset: 12, // C4
                is_black_key: false,
            },
            // Black keys - positioned to match physical keyboard layout
            // S is between Y(A) and X(B) → A#2
            KeyMapping {
                key: KeyCode::Char('s'),
                display_char: 'S',
                note_offset: -2, // A#2/Bb2
                is_black_key: true,
            },
            // F is between C(C) and V(D) → C#3
            KeyMapping {
                key: KeyCode::Char('f'),
                display_char: 'F',
                note_offset: 1, // C#3/Db3
                is_black_key: true,
            },
            // G is between V(D) and B(E) → D#3
            KeyMapping {
                key: KeyCode::Char('g'),
                display_char: 'G',
                note_offset: 3, // D#3/Eb3
                is_black_key: true,
            },
            // (no black key between E and F - H is unused for piano)
            // J is between N(F) and M(G) → F#3
            KeyMapping {
                key: KeyCode::Char('j'),
                display_char: 'J',
                note_offset: 6, // F#3/Gb3
                is_black_key: true,
            },
            // K is between M(G) and ,(A) → G#3
            KeyMapping {
                key: KeyCode::Char('k'),
                display_char: 'K',
                note_offset: 8, // G#3/Ab3
                is_black_key: true,
            },
            // L is between ,(A) and .(B) → A#3
            KeyMapping {
                key: KeyCode::Char('l'),
                display_char: 'L',
                note_offset: 10, // A#3/Bb3
                is_black_key: true,
            },
            // === Upper octave (QWERTY row = white keys, number row = black keys) ===
            // White keys on QWERTY row
            KeyMapping {
                key: KeyCode::Char('q'),
                display_char: 'Q',
                note_offset: 14, // D4
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('w'),
                display_char: 'W',
                note_offset: 16, // E4
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('e'),
                display_char: 'E',
                note_offset: 17, // F4
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('r'),
                display_char: 'R',
                note_offset: 19, // G4
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('t'),
                display_char: 'T',
                note_offset: 21, // A4
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('z'),
                display_char: 'Z',
                note_offset: 23, // B4 (German keyboard has Z here)
                is_black_key: false,
            },
            KeyMapping {
                key: KeyCode::Char('u'),
                display_char: 'U',
                note_offset: 24, // C5
                is_black_key: false,
            },
            // Black keys on number row
            // 1 is above Q(D4) → C#4
            KeyMapping {
                key: KeyCode::Char('1'),
                display_char: '1',
                note_offset: 13, // C#4/Db4
                is_black_key: true,
            },
            // 2 is above W(E4) → D#4
            KeyMapping {
                key: KeyCode::Char('2'),
                display_char: '2',
                note_offset: 15, // D#4/Eb4
                is_black_key: true,
            },
            // 3 is skipped (no black key between E and F)
            // 4 is above R(G4) → F#4
            KeyMapping {
                key: KeyCode::Char('4'),
                display_char: '4',
                note_offset: 18, // F#4/Gb4
                is_black_key: true,
            },
            // 5 is above T(A4) → G#4
            KeyMapping {
                key: KeyCode::Char('5'),
                display_char: '5',
                note_offset: 20, // G#4/Ab4
                is_black_key: true,
            },
            // 6 is above Z(B4) → A#4
            KeyMapping {
                key: KeyCode::Char('6'),
                display_char: '6',
                note_offset: 22, // A#4/Bb4
                is_black_key: true,
            },
            // 7 is skipped (no black key between B and C)
        ];

        Self {
            mappings,
            base_note: C3_MIDI,
            velocity: DEFAULT_VELOCITY,
            channel: 0,
        }
    }

    /// Create a US QWERTY keyboard layout configuration
    ///
    /// Layout (with C = C3 as base):
    /// ```text
    ///   S   D       G   H   J       L
    ///   A#2 C#3     F#3 G#3 A#3     C#4
    ///  Z   X   C   V   B   N   M   ,   .   /
    ///  A2  B2  C3  D3  E3  F3  G3  A3  B3  C4
    /// ```
    pub fn us_layout() -> Self {
        let mut config = Self::german_layout();
        // Only difference: Z instead of Y for the leftmost white key
        if let Some(mapping) = config.mappings.iter_mut().find(|m| m.display_char == 'Y') {
            mapping.key = KeyCode::Char('z');
            mapping.display_char = 'Z';
        }
        // And / instead of - for the rightmost
        if let Some(mapping) = config.mappings.iter_mut().find(|m| m.display_char == '-') {
            mapping.key = KeyCode::Char('/');
            mapping.display_char = '/';
        }
        config
    }

    /// Get the MIDI note for a given key press
    pub fn get_note_for_key(&self, key: KeyCode) -> Option<u8> {
        self.mappings.iter().find(|m| m.key == key).map(|m| {
            let note = self.base_note as i16 + m.note_offset as i16;
            note.clamp(0, 127) as u8
        })
    }

    /// Get the mapping for a given key
    pub fn get_mapping(&self, key: KeyCode) -> Option<&KeyMapping> {
        self.mappings.iter().find(|m| m.key == key)
    }

    /// Check if a key is part of the keyboard
    pub fn is_keyboard_key(&self, key: KeyCode) -> bool {
        self.mappings.iter().any(|m| m.key == key)
    }

    /// Transpose the keyboard up or down
    pub fn transpose(&mut self, semitones: i8) {
        let new_base = self.base_note as i16 + semitones as i16;
        self.base_note = new_base.clamp(0, 127) as u8;
    }

    /// Set the base note directly
    pub fn set_base_note(&mut self, note: u8) {
        self.base_note = note.min(127);
    }

    /// Get all white keys sorted by note offset
    pub fn white_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self
            .mappings
            .iter()
            .filter(|m| !m.is_black_key)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get all black keys sorted by note offset
    pub fn black_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self.mappings.iter().filter(|m| m.is_black_key).collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get lower octave white keys (bottom row: Y X C V B N M , . -)
    /// These have note_offset from -3 to 12
    pub fn lower_white_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self
            .mappings
            .iter()
            .filter(|m| !m.is_black_key && m.note_offset <= 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get upper octave white keys (QWERTY row: Q W E R T Z U)
    /// These have note_offset from 14 to 24
    pub fn upper_white_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self
            .mappings
            .iter()
            .filter(|m| !m.is_black_key && m.note_offset > 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get lower octave black keys (home row: S F G J K L)
    /// These have note_offset from -2 to 10
    pub fn lower_black_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self
            .mappings
            .iter()
            .filter(|m| m.is_black_key && m.note_offset <= 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get upper octave black keys (number row: 1 2 4 5 6)
    /// These have note_offset from 13 to 22
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
/// Must be longer than the OS key repeat delay (typically 300-500ms)
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
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        // Release all notes when hiding
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
    /// Returns the list of notes that were released
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

    /// Handle a key press
    /// Returns Some((note, velocity)) if a note should be triggered
    pub fn key_down(&mut self, key: KeyCode) -> Option<(u8, u8)> {
        log::debug!("key_down called: key={:?}, visible={}", key, self.visible);
        if !self.visible {
            log::debug!("key_down: keyboard not visible, returning None");
            return None;
        }

        if let Some(note) = self.get_note_for_key(key) {
            let now = std::time::Instant::now();
            if !self.pressed_notes.contains(&note) {
                self.pressed_notes.insert(note);
                self.note_to_key.insert(note, key);
                self.note_timestamps.insert(note, now);
                log::debug!("key_down: new note {} triggered with velocity {}", note, self.config.velocity);
                return Some((note, self.config.velocity));
            } else {
                // Key repeat - extend the note by updating timestamp
                self.note_timestamps.insert(note, now);
                log::debug!("key_down: note {} already pressed, extending timestamp", note);
            }
        } else {
            log::debug!("key_down: key {:?} not in keyboard mapping", key);
        }
        None
    }

    /// Touch a note to extend its duration (call on key repeat events)
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

    /// Handle a key release
    /// Returns Some(note) if a note should be released
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

    /// Get notes that have expired (not touched recently) and should be released
    /// Returns the list of expired notes and removes them from tracking
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

        // Remove expired notes from all tracking
        for &note in &expired {
            self.pressed_notes.remove(&note);
            self.note_to_key.remove(&note);
            self.note_timestamps.remove(&note);
        }

        expired
    }

    /// Check if a note is currently pressed
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
        // Release all currently pressed notes before shifting
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
        // Release all currently pressed notes before shifting
        let released: Vec<u8> = self.pressed_notes.drain().collect();
        self.note_to_key.clear();
        self.note_timestamps.clear();
        if self.octave_shift > -4 {
            self.octave_shift -= 1;
        }
        released
    }

    /// Get the current octave display name (e.g., "C3", "C4")
    pub fn octave_name(&self) -> String {
        let base = self.effective_base_note();
        let octave = (base / 12) as i8 - 1; // MIDI octave convention
        format!("C{}", octave)
    }

    /// Get velocity
    pub fn velocity(&self) -> u8 {
        self.config.velocity
    }

    /// Set velocity
    pub fn set_velocity(&mut self, velocity: u8) {
        self.config.velocity = velocity.min(127);
    }

    /// Get the MIDI channel
    pub fn channel(&self) -> u8 {
        self.config.channel
    }

    /// Set the MIDI channel
    pub fn set_channel(&mut self, channel: u8) {
        self.config.channel = channel.min(15);
    }
}

/// Convert a MIDI note number to a note name
pub fn note_name(note: u8) -> String {
    let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let octave = (note / 12) as i8 - 1;
    let name = names[(note % 12) as usize];
    format!("{}{}", name, octave)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_german_layout_notes() {
        let config = KeyboardConfig::german_layout();

        // Check C key maps to C3 (MIDI 48)
        assert_eq!(config.get_note_for_key(KeyCode::Char('c')), Some(48));

        // Check Y key maps to A2 (MIDI 45)
        assert_eq!(config.get_note_for_key(KeyCode::Char('y')), Some(45));

        // Check X key maps to B2 (MIDI 47)
        assert_eq!(config.get_note_for_key(KeyCode::Char('x')), Some(47));

        // Check V key maps to D3 (MIDI 50)
        assert_eq!(config.get_note_for_key(KeyCode::Char('v')), Some(50));
    }

    #[test]
    fn test_black_keys() {
        let config = KeyboardConfig::german_layout();

        // Check S key maps to Bb2 (MIDI 46)
        assert_eq!(config.get_note_for_key(KeyCode::Char('s')), Some(46));

        // Check D key maps to C#3 (MIDI 49)
        assert_eq!(config.get_note_for_key(KeyCode::Char('d')), Some(49));
    }

    #[test]
    fn test_octave_shift() {
        let mut keyboard = VirtualKeyboard::default();

        // Default C key = C3 (48)
        assert_eq!(keyboard.get_note_for_key(KeyCode::Char('c')), Some(48));

        // Octave up: C key = C4 (60)
        keyboard.octave_up();
        assert_eq!(keyboard.get_note_for_key(KeyCode::Char('c')), Some(60));

        // Octave down twice: C key = C2 (36)
        keyboard.octave_down();
        keyboard.octave_down();
        assert_eq!(keyboard.get_note_for_key(KeyCode::Char('c')), Some(36));
    }

    #[test]
    fn test_note_name() {
        assert_eq!(note_name(48), "C3");
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(69), "A4");
        assert_eq!(note_name(45), "A2");
    }
}
