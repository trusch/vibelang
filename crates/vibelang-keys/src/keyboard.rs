//! Virtual MIDI keyboard
//!
//! Provides a piano keyboard that can be played using computer keys.
//! Supports multiple keyboard layouts and octave shifting.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// MIDI note number for C3 (middle C in some conventions)
pub const C3_MIDI: u8 = 48;

/// Default velocity for key presses
pub const DEFAULT_VELOCITY: u8 = 100;

/// Default note release timeout in milliseconds
/// Must be longer than the OS key repeat delay (typically 300-500ms)
pub const DEFAULT_NOTE_RELEASE_MS: u64 = 400;

/// A key mapping entry: computer key character -> MIDI note offset from base
#[derive(Debug, Clone)]
pub struct KeyMapping {
    /// The character representing this key (lowercase)
    pub key_char: char,
    /// The character to display (for rendering, usually uppercase)
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
    /// Note release duration for auto-release
    pub note_release_duration: Duration,
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
            // Lower octave white keys (bottom row)
            KeyMapping { key_char: 'y', display_char: 'Y', note_offset: -3, is_black_key: false }, // A2
            KeyMapping { key_char: 'x', display_char: 'X', note_offset: -1, is_black_key: false }, // B2
            KeyMapping { key_char: 'c', display_char: 'C', note_offset: 0, is_black_key: false },  // C3 (base)
            KeyMapping { key_char: 'v', display_char: 'V', note_offset: 2, is_black_key: false },  // D3
            KeyMapping { key_char: 'b', display_char: 'B', note_offset: 4, is_black_key: false },  // E3
            KeyMapping { key_char: 'n', display_char: 'N', note_offset: 5, is_black_key: false },  // F3
            KeyMapping { key_char: 'm', display_char: 'M', note_offset: 7, is_black_key: false },  // G3
            KeyMapping { key_char: ',', display_char: ',', note_offset: 9, is_black_key: false },  // A3
            KeyMapping { key_char: '.', display_char: '.', note_offset: 11, is_black_key: false }, // B3
            KeyMapping { key_char: '-', display_char: '-', note_offset: 12, is_black_key: false }, // C4
            // Lower octave black keys (home row)
            KeyMapping { key_char: 's', display_char: 'S', note_offset: -2, is_black_key: true },  // A#2/Bb2
            KeyMapping { key_char: 'f', display_char: 'F', note_offset: 1, is_black_key: true },   // C#3/Db3
            KeyMapping { key_char: 'g', display_char: 'G', note_offset: 3, is_black_key: true },   // D#3/Eb3
            KeyMapping { key_char: 'j', display_char: 'J', note_offset: 6, is_black_key: true },   // F#3/Gb3
            KeyMapping { key_char: 'k', display_char: 'K', note_offset: 8, is_black_key: true },   // G#3/Ab3
            KeyMapping { key_char: 'l', display_char: 'L', note_offset: 10, is_black_key: true },  // A#3/Bb3
            // Upper octave white keys (QWERTY row)
            KeyMapping { key_char: 'q', display_char: 'Q', note_offset: 14, is_black_key: false }, // D4
            KeyMapping { key_char: 'w', display_char: 'W', note_offset: 16, is_black_key: false }, // E4
            KeyMapping { key_char: 'e', display_char: 'E', note_offset: 17, is_black_key: false }, // F4
            KeyMapping { key_char: 'r', display_char: 'R', note_offset: 19, is_black_key: false }, // G4
            KeyMapping { key_char: 't', display_char: 'T', note_offset: 21, is_black_key: false }, // A4
            KeyMapping { key_char: 'z', display_char: 'Z', note_offset: 23, is_black_key: false }, // B4
            KeyMapping { key_char: 'u', display_char: 'U', note_offset: 24, is_black_key: false }, // C5
            // Upper octave black keys (number row)
            KeyMapping { key_char: '1', display_char: '1', note_offset: 13, is_black_key: true },  // C#4/Db4
            KeyMapping { key_char: '2', display_char: '2', note_offset: 15, is_black_key: true },  // D#4/Eb4
            KeyMapping { key_char: '4', display_char: '4', note_offset: 18, is_black_key: true },  // F#4/Gb4
            KeyMapping { key_char: '5', display_char: '5', note_offset: 20, is_black_key: true },  // G#4/Ab4
            KeyMapping { key_char: '6', display_char: '6', note_offset: 22, is_black_key: true },  // A#4/Bb4
        ];

        Self {
            mappings,
            base_note: C3_MIDI,
            velocity: DEFAULT_VELOCITY,
            channel: 0,
            note_release_duration: Duration::from_millis(DEFAULT_NOTE_RELEASE_MS),
        }
    }

    /// Create a US QWERTY keyboard layout configuration
    ///
    /// Same as German but with Z instead of Y on bottom row
    pub fn us_layout() -> Self {
        let mut config = Self::german_layout();
        // Swap Y and Z positions for US layout
        if let Some(mapping) = config.mappings.iter_mut().find(|m| m.display_char == 'Y') {
            mapping.key_char = 'z';
            mapping.display_char = 'Z';
        }
        if let Some(mapping) = config.mappings.iter_mut().find(|m| m.display_char == 'Z') {
            mapping.key_char = 'y';
            mapping.display_char = 'Y';
        }
        // Use / instead of - for the rightmost key
        if let Some(mapping) = config.mappings.iter_mut().find(|m| m.display_char == '-') {
            mapping.key_char = '/';
            mapping.display_char = '/';
        }
        config
    }

    /// Get the MIDI note for a given key character
    pub fn get_note_for_char(&self, c: char) -> Option<u8> {
        let c = c.to_ascii_lowercase();
        self.mappings.iter().find(|m| m.key_char == c).map(|m| {
            let note = self.base_note as i16 + m.note_offset as i16;
            note.clamp(0, 127) as u8
        })
    }

    /// Get the mapping for a given key character
    pub fn get_mapping(&self, c: char) -> Option<&KeyMapping> {
        let c = c.to_ascii_lowercase();
        self.mappings.iter().find(|m| m.key_char == c)
    }

    /// Check if a character is part of the keyboard
    pub fn is_keyboard_char(&self, c: char) -> bool {
        let c = c.to_ascii_lowercase();
        self.mappings.iter().any(|m| m.key_char == c)
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
        let mut keys: Vec<_> = self.mappings.iter().filter(|m| !m.is_black_key).collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get all black keys sorted by note offset
    pub fn black_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self.mappings.iter().filter(|m| m.is_black_key).collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get lower octave white keys (bottom row)
    pub fn lower_white_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self.mappings.iter()
            .filter(|m| !m.is_black_key && m.note_offset <= 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get upper octave white keys (QWERTY row)
    pub fn upper_white_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self.mappings.iter()
            .filter(|m| !m.is_black_key && m.note_offset > 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get lower octave black keys (home row)
    pub fn lower_black_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self.mappings.iter()
            .filter(|m| m.is_black_key && m.note_offset <= 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }

    /// Get upper octave black keys (number row)
    pub fn upper_black_keys(&self) -> Vec<&KeyMapping> {
        let mut keys: Vec<_> = self.mappings.iter()
            .filter(|m| m.is_black_key && m.note_offset > 12)
            .collect();
        keys.sort_by_key(|m| m.note_offset);
        keys
    }
}

/// Virtual keyboard state
#[derive(Debug, Clone)]
pub struct VirtualKeyboard {
    /// Configuration
    pub config: KeyboardConfig,
    /// Currently pressed notes (MIDI note numbers)
    pub pressed_notes: HashSet<u8>,
    /// Map from note to the key character that pressed it (for display)
    pub note_to_key: HashMap<u8, char>,
    /// Timestamp of last touch for each note (for auto-release)
    pub note_timestamps: HashMap<u8, Instant>,
    /// Whether the keyboard is visible
    pub visible: bool,
    /// Octave shift (applied on top of base_note)
    pub octave_shift: i8,
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
            visible: true,
            octave_shift: 0,
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
    /// Returns the list of notes that were released
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

    /// Get the MIDI note for a key character (with octave shift applied)
    pub fn get_note_for_char(&self, c: char) -> Option<u8> {
        let c = c.to_ascii_lowercase();
        self.config.get_mapping(c).map(|m| {
            let note = self.effective_base_note() as i16 + m.note_offset as i16;
            note.clamp(0, 127) as u8
        })
    }

    /// Handle a key press
    /// Returns Some((note, velocity)) if a note should be triggered
    pub fn key_down(&mut self, c: char) -> Option<(u8, u8)> {
        if !self.visible {
            return None;
        }

        if let Some(note) = self.get_note_for_char(c) {
            let now = Instant::now();
            if !self.pressed_notes.contains(&note) {
                self.pressed_notes.insert(note);
                self.note_to_key.insert(note, c.to_ascii_lowercase());
                self.note_timestamps.insert(note, now);
                return Some((note, self.config.velocity));
            } else {
                // Key repeat - extend the note by updating timestamp
                self.note_timestamps.insert(note, now);
            }
        }
        None
    }

    /// Touch a note to extend its duration (call on key repeat events)
    pub fn touch_note(&mut self, c: char) {
        if !self.visible {
            return;
        }
        if let Some(note) = self.get_note_for_char(c) {
            if self.pressed_notes.contains(&note) {
                self.note_timestamps.insert(note, Instant::now());
            }
        }
    }

    /// Handle a key release
    /// Returns Some(note) if a note should be released
    pub fn key_up(&mut self, c: char) -> Option<u8> {
        if !self.visible {
            return None;
        }

        if let Some(note) = self.get_note_for_char(c) {
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

        let now = Instant::now();
        let release_duration = self.config.note_release_duration;
        let expired: Vec<u8> = self.note_timestamps.iter()
            .filter(|(_, &timestamp)| now.duration_since(timestamp) > release_duration)
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
        if note < 0 || note > 127 {
            return false;
        }
        self.pressed_notes.contains(&(note as u8))
    }

    /// Shift octave up
    /// Returns the list of notes that were released before shifting
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
    /// Returns the list of notes that were released before shifting
    pub fn octave_down(&mut self) -> Vec<u8> {
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
        assert_eq!(config.get_note_for_char('c'), Some(48));

        // Check Y key maps to A2 (MIDI 45)
        assert_eq!(config.get_note_for_char('y'), Some(45));

        // Check X key maps to B2 (MIDI 47)
        assert_eq!(config.get_note_for_char('x'), Some(47));

        // Check V key maps to D3 (MIDI 50)
        assert_eq!(config.get_note_for_char('v'), Some(50));
    }

    #[test]
    fn test_black_keys() {
        let config = KeyboardConfig::german_layout();

        // Check S key maps to Bb2 (MIDI 46)
        assert_eq!(config.get_note_for_char('s'), Some(46));

        // Check F key maps to C#3 (MIDI 49)
        assert_eq!(config.get_note_for_char('f'), Some(49));
    }

    #[test]
    fn test_case_insensitive() {
        let config = KeyboardConfig::german_layout();

        // Both upper and lowercase should work
        assert_eq!(config.get_note_for_char('c'), config.get_note_for_char('C'));
        assert_eq!(config.get_note_for_char('y'), config.get_note_for_char('Y'));
    }

    #[test]
    fn test_octave_shift() {
        let mut keyboard = VirtualKeyboard::default();

        // Default C key = C3 (48)
        assert_eq!(keyboard.get_note_for_char('c'), Some(48));

        // Octave up: C key = C4 (60)
        keyboard.octave_up();
        assert_eq!(keyboard.get_note_for_char('c'), Some(60));

        // Octave down twice: C key = C2 (36)
        keyboard.octave_down();
        keyboard.octave_down();
        assert_eq!(keyboard.get_note_for_char('c'), Some(36));
    }

    #[test]
    fn test_note_name() {
        assert_eq!(note_name(48), "C3");
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(69), "A4");
        assert_eq!(note_name(45), "A2");
    }

    #[test]
    fn test_key_down_up() {
        let mut keyboard = VirtualKeyboard::default();

        // Press C key
        let result = keyboard.key_down('c');
        assert!(result.is_some());
        let (note, velocity) = result.unwrap();
        assert_eq!(note, 48);
        assert_eq!(velocity, DEFAULT_VELOCITY);
        assert!(keyboard.is_note_pressed(48));

        // Pressing again should not trigger (but extend timestamp)
        let result = keyboard.key_down('c');
        assert!(result.is_none());

        // Release C key
        let result = keyboard.key_up('c');
        assert_eq!(result, Some(48));
        assert!(!keyboard.is_note_pressed(48));
    }
}
