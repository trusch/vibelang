//! Melody API for Rhai scripts.
//!
//! Melodies are pitched sequences that trigger voices with note information.
//!
//! # Scale Degree Notation
//!
//! When using `.root()` and `.scale()`, you can use scale degree notation:
//!
//! - `1` through `7` - Scale degrees (1 = root, 2 = 2nd, etc.)
//! - `1'` - One octave up (can stack: `1''` = two octaves up)
//! - `1,` - One octave down (can stack: `1,,` = two octaves down)
//! - `-` - Tie/hold previous note
//! - `.` or `_` - Rest
//!
//! Example: `.notes("1 2 3 4 | 5 6 7 1'")`  // Scale up, then root one octave up

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::cell::RefCell;
use std::collections::HashMap;
use vibelang_core::traits::{MelodyConfig, NoteEvent};
use vibelang_core::types::Beat;

use super::voice::Voice;
use crate::context;
use vibelang_dsp::notes::parse_note_name;

// Global registry for melodies - allows looking up melodies by name
thread_local! {
    static MELODY_REGISTRY: RefCell<HashMap<String, Melody>> = RefCell::new(HashMap::new());
}

/// Clear the melody registry (called when context is cleared).
pub fn clear_registry() {
    MELODY_REGISTRY.with(|r| r.borrow_mut().clear());
}

/// Get a melody from the registry by name.
fn get_melody(name: &str) -> Option<Melody> {
    MELODY_REGISTRY.with(|r| r.borrow().get(name).cloned())
}

/// Store a melody in the registry.
fn store_melody(melody: &Melody) {
    MELODY_REGISTRY.with(|r| {
        r.borrow_mut().insert(melody.name.clone(), melody.clone());
    });
}

/// A Melody builder for creating melodic patterns.
#[derive(Debug, Clone, CustomType)]
pub struct Melody {
    /// Melody name.
    pub name: String,
    /// Voice name to trigger.
    voice_name: Option<String>,
    /// Notes in the melody.
    notes: Vec<MelodyNote>,
    /// Loop length in beats.
    length: f64,
    /// Default gate (note duration as fraction of step).
    gate: f64,
    /// Transpose in semitones.
    transpose: i64,
    /// Swing amount.
    swing: f32,
    /// Group path.
    group_path: String,
    /// Root note for scale degree mode (e.g., "C4", "D2").
    root: Option<String>,
    /// Scale type for scale degree mode (e.g., "major", "minor").
    scale: Option<String>,
}

/// A note in a melody.
#[derive(Debug, Clone)]
struct MelodyNote {
    beat: f64,
    notes: Vec<u8>, // MIDI note numbers
    velocity: f64,
    gate: f64,
    /// Per-note voice parameters (e.g., cutoff, pan, resonance).
    params: std::collections::HashMap<String, f32>,
}

impl Melody {
    /// Create a new melody with the given name.
    pub fn new(_ctx: NativeCallContext, name: String) -> Self {
        Self {
            name,
            voice_name: None,
            notes: Vec::new(),
            length: 4.0,
            gate: 0.5,
            transpose: 0,
            swing: 0.0,
            group_path: context::current_group_path(),
            root: None,
            scale: None,
        }
    }

    /// Create an anonymous melody (name resolved at finalization).
    pub fn new_anon(_ctx: NativeCallContext) -> Self {
        Self {
            name: String::new(),
            voice_name: None,
            notes: Vec::new(),
            length: 4.0,
            gate: 0.5,
            transpose: 0,
            swing: 0.0,
            group_path: context::current_group_path(),
            root: None,
            scale: None,
        }
    }

    /// Create a melody for testing purposes (no call context needed).
    #[cfg(test)]
    fn new_for_test(name: &str) -> Self {
        Self {
            name: name.to_string(),
            voice_name: None,
            notes: Vec::new(),
            length: 4.0,
            gate: 0.5,
            transpose: 0,
            swing: 0.0,
            group_path: String::new(),
            root: None,
            scale: None,
        }
    }

    /// Resolve the name for an anonymous melody from its voice target.
    ///
    /// Uses the voice name to generate `_{voice_name}_mel`.
    /// No-op if the melody already has a name.
    fn resolve_name(&mut self) {
        if !self.name.is_empty() {
            return;
        }
        let voice = self.voice_name.as_deref().unwrap_or("unknown");
        let base = format!("_{}_mel", voice);
        self.name = context::resolve_auto_name(&base);
    }

    // === Builder methods ===

    /// Set the voice to trigger (by name).
    pub fn on(mut self, voice_name: String) -> Self {
        self.voice_name = Some(voice_name);
        self
    }

    /// Set the voice to trigger (by Voice object).
    ///
    /// This also resolves anonymous voice names and syncs the voice's current state.
    pub fn on_voice(mut self, mut voice: Voice) -> Self {
        voice.resolve_name();
        voice.sync_to_state();
        self.voice_name = Some(voice.name);
        self
    }

    /// Set the root note for scale degree mode.
    ///
    /// When set along with `.scale()`, numeric notes (1-7) are interpreted as scale degrees.
    /// The root can include an octave (e.g., "D2", "A3") or default to octave 4.
    pub fn root(mut self, root: String) -> Self {
        self.root = Some(root);
        self
    }

    /// Set the scale type for scale degree mode.
    ///
    /// Scale types: "major", "minor", "dorian", "phrygian", "lydian",
    /// "mixolydian", "locrian", "harmonic_minor", "melodic_minor",
    /// "pentatonic", "minor_pentatonic", "blues", "chromatic", "whole_tone"
    ///
    /// Unknown scale names are an error (no silent fallback to major).
    pub fn scale(mut self, scale: String) -> Result<Self, Box<EvalAltResult>> {
        if super::helpers::get_scale_intervals(&scale).is_none() {
            return Err(super::helpers::unknown_scale_error(
                "melody.scale()",
                &scale,
            ));
        }
        self.scale = Some(scale);
        Ok(self)
    }

    /// Set notes from a string pattern.
    ///
    /// Supports per-note parameter overrides via `[key=value]` syntax:
    /// - `C4[velocity=100]` — set velocity for this note
    /// - `C4[vel=80,cutoff=2000]` — set velocity and cutoff
    /// - `1[pan=-0.5]` — set pan for scale degree
    /// - `[C4 E4 G4][vel=100]` — set velocity for chord
    pub fn notes(mut self, notes_str: String) -> Self {
        let bars = split_into_bars(&notes_str);
        let num_bars = bars.len().max(1);
        let beats_per_bar = 4.0;

        let loop_length = num_bars as f64 * beats_per_bar;
        if loop_length > self.length {
            self.length = loop_length;
        }

        let mut current_beat = 0.0;
        let mut current_notes: Option<Vec<u8>> = None;
        let mut note_start_beat: f64 = 0.0;
        let mut note_duration: f64 = 0.0;
        let mut current_velocity: f64 = 1.0;
        let mut current_params: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();

        for bar in bars {
            let tokens = tokenize_bar(&bar);
            if tokens.is_empty() {
                current_beat += beats_per_bar;
                continue;
            }

            let beat_per_token = beats_per_bar / tokens.len() as f64;

            for (i, token) in tokens.iter().enumerate() {
                let beat = current_beat + i as f64 * beat_per_token;

                match token {
                    NoteToken::Tie => {
                        if current_notes.is_some() {
                            note_duration += beat_per_token;
                        }
                    }
                    NoteToken::Rest => {
                        if let Some(midi_notes) = current_notes.take() {
                            self.notes.push(MelodyNote {
                                beat: note_start_beat,
                                notes: midi_notes,
                                velocity: current_velocity,
                                gate: note_duration,
                                params: std::mem::take(&mut current_params),
                            });
                        }
                    }
                    NoteToken::Notes(midi_notes, note_params) => {
                        if let Some(prev_notes) = current_notes.take() {
                            self.notes.push(MelodyNote {
                                beat: note_start_beat,
                                notes: prev_notes,
                                velocity: current_velocity,
                                gate: note_duration,
                                params: std::mem::take(&mut current_params),
                            });
                        }
                        current_notes = Some(midi_notes.clone());
                        note_start_beat = beat;
                        note_duration = beat_per_token;
                        current_velocity = note_params.velocity.unwrap_or(1.0);
                        current_params = note_params.params.clone();
                        if let Some(gate_override) = note_params.gate {
                            // Gate override will be applied when committing
                            // Store it as a multiplier on note_duration later
                            // For now, we track it via the params and apply at commit
                            current_params
                                .insert("_gate_override".to_string(), gate_override as f32);
                        }
                    }
                    NoteToken::ScaleDegree {
                        degree,
                        octave_offset,
                        params: note_params,
                    } => {
                        // Resolve scale degree to MIDI note
                        if let Some(midi) =
                            resolve_scale_degree(*degree, *octave_offset, &self.root, &self.scale)
                        {
                            if let Some(prev_notes) = current_notes.take() {
                                self.notes.push(MelodyNote {
                                    beat: note_start_beat,
                                    notes: prev_notes,
                                    velocity: current_velocity,
                                    gate: note_duration,
                                    params: std::mem::take(&mut current_params),
                                });
                            }
                            current_notes = Some(vec![midi]);
                            note_start_beat = beat;
                            note_duration = beat_per_token;
                            current_velocity = note_params.velocity.unwrap_or(1.0);
                            current_params = note_params.params.clone();
                            if let Some(gate_override) = note_params.gate {
                                current_params
                                    .insert("_gate_override".to_string(), gate_override as f32);
                            }
                        }
                    }
                }
            }

            current_beat += beats_per_bar;
        }

        // Commit final note
        if let Some(midi_notes) = current_notes {
            self.notes.push(MelodyNote {
                beat: note_start_beat,
                notes: midi_notes,
                velocity: current_velocity,
                gate: note_duration,
                params: current_params,
            });
        }

        self
    }

    /// Add notes from an array of MIDI note numbers, evenly spaced across the loop.
    ///
    /// Each note in the array is placed at equal intervals within the current loop length.
    pub fn notes_array(mut self, notes_arr: rhai::Array) -> Self {
        if notes_arr.is_empty() {
            return self;
        }

        let beat_per_note = self.length / notes_arr.len() as f64;

        for (i, note) in notes_arr.into_iter().enumerate() {
            let beat = i as f64 * beat_per_note;
            if let Some(midi) = note.as_int().ok().map(|n| n as u8) {
                self.notes.push(MelodyNote {
                    beat,
                    notes: vec![midi],
                    velocity: 1.0,
                    gate: beat_per_note * self.gate,
                    params: std::collections::HashMap::new(),
                });
            } else if let Some(chord_notes) = note.clone().try_cast::<rhai::Array>() {
                // Handle chord (array of notes)
                let midi_notes: Vec<u8> = chord_notes
                    .into_iter()
                    .filter_map(|n| n.as_int().ok().map(|n| n as u8))
                    .collect();
                if !midi_notes.is_empty() {
                    self.notes.push(MelodyNote {
                        beat,
                        notes: midi_notes,
                        velocity: 1.0,
                        gate: beat_per_note * self.gate,
                        params: std::collections::HashMap::new(),
                    });
                }
            }
        }

        self
    }

    /// Add a single note at a specific beat.
    ///
    /// Arguments:
    /// - beat: Position in beats
    /// - note: MIDI note number (0-127)
    /// - velocity: Velocity (0.0-1.0), optional, defaults to 1.0
    /// - duration: Duration in beats, optional, defaults to 0.5
    pub fn add_note(mut self, beat: f64, note: i64, velocity: f64, duration: f64) -> Self {
        self.notes.push(MelodyNote {
            beat,
            notes: vec![note.clamp(0, 127) as u8],
            velocity: velocity.clamp(0.0, 1.0),
            gate: duration,
            params: std::collections::HashMap::new(),
        });
        self
    }

    /// Add a chord (multiple notes) at a specific beat.
    ///
    /// Arguments:
    /// - beat: Position in beats
    /// - chord_notes: Array of MIDI note numbers
    /// - velocity: Velocity (0.0-1.0)
    /// - duration: Duration in beats
    pub fn add_chord(
        mut self,
        beat: f64,
        chord_notes: rhai::Array,
        velocity: f64,
        duration: f64,
    ) -> Self {
        let midi_notes: Vec<u8> = chord_notes
            .into_iter()
            .filter_map(|n| n.as_int().ok().map(|n| n.clamp(0, 127) as u8))
            .collect();

        if !midi_notes.is_empty() {
            self.notes.push(MelodyNote {
                beat,
                notes: midi_notes,
                velocity: velocity.clamp(0.0, 1.0),
                gate: duration,
                params: std::collections::HashMap::new(),
            });
        }
        self
    }

    /// Set the loop length in beats.
    pub fn len(mut self, beats: f64) -> Self {
        self.length = beats;
        self
    }

    /// Set the default gate (note duration).
    pub fn gate(mut self, gate: f64) -> Self {
        self.gate = gate.clamp(0.0, 1.0);
        self
    }

    /// Set the transpose amount in semitones.
    pub fn transpose(mut self, semitones: i64) -> Self {
        self.transpose = semitones;
        self
    }

    /// Set the swing amount.
    pub fn swing(mut self, amount: f64) -> Self {
        self.swing = amount.clamp(0.0, 1.0) as f32;
        self
    }

    /// Sync melody to script state.
    ///
    /// Skips registration for anonymous melodies (empty name) not yet resolved.
    pub fn sync_to_state(&self) {
        if self.name.is_empty() {
            return; // Anonymous melody not yet resolved — defer registration
        }
        tracing::debug!("Melody::sync_to_state called for '{}'", self.name);
        let melody_id = context::get_or_create_melody_id(&self.name);
        let voice_id = self
            .voice_name
            .as_ref()
            .map(|n| context::get_or_create_voice_id(n));

        // Warn if the voice doesn't exist in the script state yet
        if let Some(ref voice_name) = self.voice_name {
            if let Some(vid) = voice_id {
                context::with_state(|state| {
                    if !state.voices.contains_key(&vid) {
                        tracing::warn!(
                            "Melody '{}': voice '{}' not found — make sure to create it with voice(\"{}\")",
                            self.name, voice_name, voice_name
                        );
                    }
                });
            }
        }

        tracing::debug!(
            "Melody '{}': melody_id={:?}, voice_name={:?}, voice_id={:?}, internal_notes_count={}",
            self.name,
            melody_id,
            self.voice_name,
            voice_id,
            self.notes.len()
        );

        // Convert notes to NoteEvents
        let notes: Vec<NoteEvent> = self
            .notes
            .iter()
            .flat_map(|n| {
                let transpose = self.transpose;
                let params = n.params.clone();
                // Handle gate override from per-note params
                let gate = if let Some(gate_override) = params.get("_gate_override") {
                    *gate_override as f64
                } else {
                    n.gate
                };
                // Filter out internal params (prefixed with _)
                let clean_params: std::collections::HashMap<String, f32> = params
                    .into_iter()
                    .filter(|(k, _)| !k.starts_with('_'))
                    .collect();
                n.notes.iter().map(move |&note| {
                    let transposed = (note as i64 + transpose).clamp(0, 127) as u8;
                    NoteEvent {
                        beat: Beat::from_f64(n.beat),
                        note: transposed,
                        velocity: n.velocity as f32,
                        duration: Beat::from_f64(gate),
                        params: clean_params.clone(),
                    }
                })
            })
            .collect();

        tracing::debug!(
            "Melody '{}': converted to {} NoteEvents, length={} beats",
            self.name,
            notes.len(),
            self.length
        );

        if !notes.is_empty() {
            tracing::debug!(
                "Melody '{}': first note at beat {}, note={}, vel={}",
                self.name,
                notes[0].beat.to_f64(),
                notes[0].note,
                notes[0].velocity
            );
        }

        let config = MelodyConfig {
            name: self.name.clone(),
            voice: voice_id,
            notes,
            length: Beat::from_f64(self.length),
            swing: self.swing,
        };

        context::with_state(|state| {
            state.melodies.insert(melody_id, config);
            tracing::debug!(
                "Melody '{}': inserted into script state, total melodies={}",
                self.name,
                state.melodies.len()
            );
        });
    }

    /// Register and apply the melody (chainable).
    ///
    /// For anonymous melodies, resolves the name from the voice target before registering.
    pub fn apply(mut self) -> Self {
        self.resolve_name();
        self.sync_to_state();
        // Store in registry for later lookup
        store_melody(&self);
        self
    }

    /// Start the melody playing (chainable).
    ///
    /// For anonymous melodies, resolves the name from the voice target before registering.
    pub fn start(mut self) -> Self {
        self.resolve_name();
        self.sync_to_state();

        let melody_id = context::get_or_create_melody_id(&self.name);
        context::with_state(|state| {
            state.playing_melodies.insert(melody_id);
            tracing::debug!(
                "Melody '{}': added to playing_melodies, melody_id={:?}, total playing={}",
                self.name,
                melody_id,
                state.playing_melodies.len()
            );
        });

        self
    }

    /// Launch the melody (chainable).
    ///
    /// **Currently an alias of [`Self::start`]** — quantized launch (starting
    /// at the next quantization boundary) is not yet implemented. The melody
    /// starts exactly as if `.start()` had been called.
    pub fn launch(self) -> Self {
        self.start()
    }

    /// Stop the melody.
    pub fn stop(&mut self) {
        let melody_id = context::get_or_create_melody_id(&self.name);
        context::with_state(|state| {
            state.playing_melodies.remove(&melody_id);
        });
    }

    /// Check if the melody is playing.
    pub fn is_playing(&mut self) -> bool {
        let melody_id = context::get_or_create_melody_id(&self.name);
        context::with_state(|state| state.playing_melodies.contains(&melody_id))
    }
}

/// Per-note parameter overrides parsed from `[key=value, ...]` syntax.
///
/// Special parameters:
/// - `velocity` / `vel`: Overrides note velocity (values > 1.0 treated as MIDI 0-127)
/// - `gate`: Overrides note gate/duration ratio
/// - All others: Passed as arbitrary voice/synth params at note-on time
#[derive(Debug, Clone, Default)]
struct NoteParams {
    /// Velocity override (normalized 0.0-1.0).
    velocity: Option<f64>,
    /// Gate override (duration ratio).
    gate: Option<f64>,
    /// Arbitrary voice parameters (cutoff, pan, resonance, etc.).
    params: std::collections::HashMap<String, f32>,
}

/// Token type for bar parsing.
#[derive(Debug, Clone)]
enum NoteToken {
    /// Absolute MIDI note(s) with optional per-note parameters.
    Notes(Vec<u8>, NoteParams),
    /// Scale degree (1-7) with octave offset and optional per-note parameters.
    /// degree: 1-7 (1 = root)
    /// octave_offset: positive = up, negative = down
    ScaleDegree {
        degree: u8,
        octave_offset: i8,
        params: NoteParams,
    },
    /// Tie/hold previous note.
    Tie,
    /// Rest (silence).
    Rest,
}

/// Split pattern string into bars.
fn split_into_bars(pattern: &str) -> Vec<String> {
    pattern
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse per-note parameters from `[key=value, ...]` syntax.
///
/// Reads from the current position in the char iterator, expecting `[` as the
/// next character. Parses key=value pairs separated by commas or spaces.
///
/// Special parameter handling:
/// - `velocity` / `vel`: Normalized to 0.0-1.0 (values > 1.0 treated as MIDI 0-127)
/// - `gate`: Stored as-is (duration ratio)
/// - All others: Stored as f32 voice/synth parameters
///
/// Example inputs: `[velocity=100]`, `[vel=0.8,cutoff=2000,pan=-0.5]`
fn parse_note_params(chars: &mut std::iter::Peekable<std::str::Chars>) -> NoteParams {
    let mut params = NoteParams::default();

    // Check if next char is '['
    if chars.peek() != Some(&'[') {
        return params;
    }
    chars.next(); // consume '['

    // Parse key=value pairs
    loop {
        // Skip whitespace and commas
        while let Some(&c) = chars.peek() {
            if c == ' ' || c == '\t' || c == ',' {
                chars.next();
            } else {
                break;
            }
        }

        // Check for end
        if chars.peek() == Some(&']') {
            chars.next(); // consume ']'
            break;
        }

        // No more chars = unterminated bracket, stop
        if chars.peek().is_none() {
            break;
        }

        // Parse key
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                key.push(chars.next().unwrap());
            } else {
                break;
            }
        }

        // Expect '='
        if chars.peek() != Some(&'=') {
            // Malformed, skip to ']' or end
            while let Some(&c) = chars.peek() {
                if c == ']' {
                    chars.next();
                    return params;
                }
                chars.next();
            }
            return params;
        }
        chars.next(); // consume '='

        // Parse value (can be negative, float)
        let mut value_str = String::new();
        while let Some(&c) = chars.peek() {
            if c == '-' || c == '.' || c.is_ascii_digit() {
                value_str.push(chars.next().unwrap());
            } else {
                break;
            }
        }

        if let Ok(value) = value_str.parse::<f64>() {
            let key_lower = key.to_lowercase();
            match key_lower.as_str() {
                "velocity" | "vel" => {
                    // Normalize: values > 1.0 are treated as MIDI 0-127
                    let normalized = if value > 1.0 {
                        (value / 127.0).clamp(0.0, 1.0)
                    } else {
                        value.clamp(0.0, 1.0)
                    };
                    params.velocity = Some(normalized);
                }
                "gate" => {
                    params.gate = Some(value.max(0.0));
                }
                _ => {
                    params.params.insert(key_lower, value as f32);
                }
            }
        }
    }

    params
}

/// Tokenize a bar string into note tokens.
///
/// Supports:
/// - Absolute notes: `C4`, `D#5`, `Eb3`
/// - Chord brackets: `[C4 E4 G4]` (multiple notes played together)
/// - Scale degrees: `1` through `7` (1 = root)
/// - Octave up: `'` (can stack: `1''` = two octaves up)
/// - Octave down: `,` (can stack: `1,,` = two octaves down)
/// - Tie/hold: `-`
/// - Rest: `.` or `_`
/// - Per-note params: `C4[velocity=100,cutoff=2000]`, `1[vel=80]`
fn tokenize_bar(bar: &str) -> Vec<NoteToken> {
    let mut tokens = Vec::new();
    let mut chars = bar.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // Whitespace is ignored
            ' ' | '\t' | '\n' | '\r' => {}

            // Chord bracket notation: [C4 E4 G4] with optional params [C4 E4 G4][vel=80]
            '[' => {
                let mut chord_notes = Vec::new();
                let mut current_note = String::new();

                while let Some(&next) = chars.peek() {
                    if next == ']' {
                        chars.next(); // consume ']'
                                      // Parse the last note if any
                        if !current_note.is_empty() {
                            if let Some(midi) = parse_note_name(&current_note) {
                                chord_notes.push(midi);
                            }
                        }
                        break;
                    } else if next == ' ' || next == '\t' {
                        chars.next(); // consume whitespace
                                      // Parse accumulated note
                        if !current_note.is_empty() {
                            if let Some(midi) = parse_note_name(&current_note) {
                                chord_notes.push(midi);
                            }
                            current_note.clear();
                        }
                    } else {
                        current_note.push(chars.next().unwrap());
                    }
                }

                if !chord_notes.is_empty() {
                    // Check for per-note params after chord brackets
                    let note_params = parse_note_params(&mut chars);
                    tokens.push(NoteToken::Notes(chord_notes, note_params));
                }
            }

            // Tie/hold marker
            '-' => tokens.push(NoteToken::Tie),

            // Rest markers
            '.' | '_' => tokens.push(NoteToken::Rest),

            // Scale degrees 1-7
            '1'..='7' => {
                let degree = c as u8 - b'0';

                // Parse octave modifiers: ' for up, , for down
                let mut octave_offset: i8 = 0;
                while let Some(&next) = chars.peek() {
                    match next {
                        '\'' => {
                            octave_offset += 1;
                            chars.next();
                        }
                        ',' => {
                            octave_offset -= 1;
                            chars.next();
                        }
                        _ => break,
                    }
                }

                // Check for per-note params after scale degree
                let note_params = parse_note_params(&mut chars);

                tokens.push(NoteToken::ScaleDegree {
                    degree,
                    octave_offset,
                    params: note_params,
                });
            }

            // Absolute note names (A-G), with optional chord suffix
            'A'..='G' | 'a'..='g' => {
                let mut note_str = String::new();
                note_str.push(c.to_ascii_uppercase());

                // Collect accidentals and octave digits
                while let Some(&next) = chars.peek() {
                    match next {
                        '#' | 'b' | '♯' | '♭' | '0'..='9' => {
                            note_str.push(chars.next().unwrap());
                        }
                        _ => break,
                    }
                }

                // Check for chord suffix (e.g., :m7, :maj7, :dim)
                let chord_suffix = if chars.peek() == Some(&':') {
                    chars.next(); // consume ':'
                    let mut suffix = String::new();
                    while let Some(&next) = chars.peek() {
                        // Chord suffixes can contain letters and numbers (m7, maj7, dim7, etc.)
                        if next.is_alphanumeric() {
                            suffix.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    Some(suffix)
                } else {
                    None
                };

                if let Some(root_midi) = parse_note_name(&note_str) {
                    let midi_notes = if let Some(suffix) = chord_suffix {
                        expand_chord(root_midi, &suffix)
                    } else {
                        vec![root_midi]
                    };
                    // Check for per-note params after note/chord
                    let note_params = parse_note_params(&mut chars);
                    tokens.push(NoteToken::Notes(midi_notes, note_params));
                }
            }

            // Unknown characters are ignored
            _ => {}
        }
    }

    tokens
}

/// Expand a chord suffix to a list of MIDI notes.
///
/// Supports common chord types:
/// - Major: (no suffix), maj, M
/// - Minor: m, min
/// - Dominant 7: 7, dom7
/// - Major 7: maj7, M7
/// - Minor 7: m7, min7
/// - Diminished: dim, o
/// - Diminished 7: dim7, o7
/// - Augmented: aug, +
/// - Suspended 2: sus2
/// - Suspended 4: sus4
/// - Add 9: add9
/// - Minor 9: m9, min9
/// - Major 9: maj9, M9
/// - 9: 9 (dominant 9)
fn expand_chord(root: u8, suffix: &str) -> Vec<u8> {
    let intervals: Vec<i8> = match suffix.to_lowercase().as_str() {
        // Major triads
        "" | "maj" | "major" => vec![0, 4, 7],

        // Minor triads
        "m" | "min" | "minor" => vec![0, 3, 7],

        // Dominant 7th
        "7" | "dom7" => vec![0, 4, 7, 10],

        // Major 7th
        "maj7" | "m7" if suffix.starts_with("M") => vec![0, 4, 7, 11], // M7
        "maj7" => vec![0, 4, 7, 11],

        // Minor 7th
        "m7" | "min7" => vec![0, 3, 7, 10],

        // Diminished
        "dim" | "o" => vec![0, 3, 6],

        // Diminished 7th
        "dim7" | "o7" => vec![0, 3, 6, 9],

        // Half-diminished (minor 7 flat 5)
        "m7b5" | "min7b5" | "ø" | "ø7" => vec![0, 3, 6, 10],

        // Augmented
        "aug" | "+" => vec![0, 4, 8],

        // Augmented 7th
        "aug7" | "+7" => vec![0, 4, 8, 10],

        // Suspended
        "sus2" => vec![0, 2, 7],
        "sus4" | "sus" => vec![0, 5, 7],

        // Add chords
        "add9" => vec![0, 4, 7, 14],
        "madd9" | "minadd9" => vec![0, 3, 7, 14],

        // 9th chords
        "9" => vec![0, 4, 7, 10, 14],
        "m9" | "min9" => vec![0, 3, 7, 10, 14],
        "maj9" => vec![0, 4, 7, 11, 14],

        // 6th chords
        "6" => vec![0, 4, 7, 9],
        "m6" | "min6" => vec![0, 3, 7, 9],

        // Power chord
        "5" => vec![0, 7],

        // Default to major triad for unknown suffixes
        _ => vec![0, 4, 7],
    };

    intervals
        .iter()
        .filter_map(|&interval| {
            let note = root as i16 + interval as i16;
            if (0..=127).contains(&note) {
                Some(note as u8)
            } else {
                None
            }
        })
        .collect()
}

/// Get scale intervals for a given scale type.
/// Returns semitone offsets from root for each scale degree (1-7).
///
/// Delegates to the shared table in [`super::helpers`]; unknown names fall
/// back to major here only because [`Melody::scale`] already rejects them
/// with an error before they can reach this point.
fn get_scale_intervals(scale_name: &str) -> Vec<u8> {
    super::helpers::get_scale_intervals(scale_name).unwrap_or_else(|| vec![0, 2, 4, 5, 7, 9, 11])
}

/// Resolve a scale degree to a MIDI note number.
///
/// - degree: 1-7 (1-indexed, 1 = root)
/// - octave_offset: additional octaves up (positive) or down (negative)
/// - root: root note string (e.g., "C4", "D2")
/// - scale: scale name (e.g., "major", "minor")
fn resolve_scale_degree(
    degree: u8,
    octave_offset: i8,
    root: &Option<String>,
    scale: &Option<String>,
) -> Option<u8> {
    // Get root MIDI note (default to C4 = 60)
    let root_midi = root
        .as_ref()
        .and_then(|r| {
            // If root doesn't have an octave, default to 4
            if r.chars().any(|c| c.is_ascii_digit()) {
                parse_note_name(r)
            } else {
                parse_note_name(&format!("{}4", r))
            }
        })
        .unwrap_or(60) as i16;

    // Get scale intervals (default to major)
    let intervals = scale
        .as_ref()
        .map(|s| get_scale_intervals(s))
        .unwrap_or_else(|| vec![0, 2, 4, 5, 7, 9, 11]);

    let scale_len = intervals.len();
    if scale_len == 0 {
        return None;
    }

    // Convert 1-indexed degree to 0-indexed
    // degree 1 = index 0 (root)
    // degree 8 = index 0 + 1 octave
    let degree_0 = (degree as i16) - 1;

    // Handle degrees outside 1-7 range via octave wrapping
    let octave_from_degree = degree_0.div_euclid(scale_len as i16);
    let degree_idx = degree_0.rem_euclid(scale_len as i16) as usize;

    let interval = intervals[degree_idx] as i16;

    // Calculate final MIDI note
    let total_octave_offset = octave_offset as i16 + octave_from_degree;
    let midi = root_midi + interval + (total_octave_offset * 12);

    if (0..=127).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
}

/// Create a new melody builder or return an existing one.
///
/// If a melody with this name already exists in the registry,
/// returns a clone of it. Otherwise creates a new empty melody.
pub fn melody(ctx: NativeCallContext, name: String) -> Melody {
    // Check if melody already exists in registry
    if let Some(existing) = get_melody(&name) {
        return existing;
    }
    // Create new melody
    Melody::new(ctx, name)
}

/// Create an anonymous melody builder (name resolved from voice target).
pub fn melody_anon(ctx: NativeCallContext) -> Melody {
    Melody::new_anon(ctx)
}

/// Register melody API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    engine.build_type::<Melody>();

    // Constructor (named and anonymous overloads)
    engine.register_fn("melody", melody);
    engine.register_fn("melody", melody_anon);

    engine.register_fn("on", Melody::on);
    engine.register_fn("on", Melody::on_voice);
    engine.register_fn("root", Melody::root);
    engine.register_fn("scale", Melody::scale);
    engine.register_fn("notes", Melody::notes);
    engine.register_fn("notes", Melody::notes_array);
    engine.register_fn("add_note", Melody::add_note);
    engine.register_fn("add_chord", Melody::add_chord);
    engine.register_fn("len", Melody::len);
    engine.register_fn("gate", Melody::gate);
    engine.register_fn("transpose", Melody::transpose);
    engine.register_fn("swing", Melody::swing);

    engine.register_fn("apply", Melody::apply);
    engine.register_fn("start", Melody::start);
    engine.register_fn("launch", Melody::launch);
    engine.register_fn("stop", Melody::stop);
    engine.register_fn("is_playing", Melody::is_playing);
    engine.register_get("playing", Melody::is_playing);
    engine.register_get("name", |m: &mut Melody| m.name.clone());
}

use vibelang_core::candidate::{
    AuthoringDeclaration, Cancellation, CandidateError, CandidateFragment, CanonicalF64,
    Composition, DeclarationOwner, DeclarationPayload, DesiredLifecycle, GroupScope,
    LifecycleAction, LifecycleMetadata, MelodyAuthoring, MelodyEventAuthoring, MelodyKind,
    StartMode, TerminalEffect, VoiceKind,
};

use super::voice::VoiceRef;
use crate::foundation::{self, BuilderBase, FoundationError, Observation, RefBase};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MelodyRef {
    base: RefBase,
}

impl MelodyRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<MelodyKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    fn action(self, action: LifecycleAction, role: &str) -> Result<Self, FoundationError> {
        let (effect, cancellation) = match &action {
            LifecycleAction::Start(_) => (TerminalEffect::Start, Cancellation::BeforePlanning),
            LifecycleAction::Stop => (TerminalEffect::Stop, Cancellation::NotCancellable),
            LifecycleAction::Remove => (TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::Cancel => (TerminalEffect::Cancel, Cancellation::BeforePlanning),
            _ => {
                return Err(CandidateError::InvalidLifecycle(
                    "unsupported MelodyRef lifecycle action".into(),
                )
                .into())
            }
        };
        let source = foundation::operation_source(&self.base, role)?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(effect, cancellation),
            action,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn start(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Start(StartMode::Normal), "start")
    }

    pub fn start_now(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Start(StartMode::Immediate), "start-now")
    }

    pub fn stop(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Stop, "stop")
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Remove, "remove")
    }

    pub fn cancel(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Cancel, "cancel")
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MelodyContent {
    Text(String),
    Events(Vec<MelodyEventAuthoring>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MelodyBuilder {
    base: BuilderBase,
    voice: Option<VoiceRef>,
    content: Option<MelodyContent>,
    length: Option<f64>,
    velocity: f64,
}

impl MelodyBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        Self {
            base,
            voice: None,
            content: None,
            length: None,
            velocity: 1.0,
        }
    }

    #[must_use]
    pub fn on(mut self, voice: VoiceRef) -> Self {
        self.voice = Some(voice);
        self
    }

    pub fn notes(mut self, notes: String) -> Result<Self, FoundationError> {
        if matches!(&self.content, Some(MelodyContent::Events(_))) {
            return Err(CandidateError::InvalidAuthoring(
                "Melody text and event content cannot be combined".into(),
            )
            .into());
        }
        self.content = Some(MelodyContent::Text(notes));
        Ok(self)
    }

    pub fn events(mut self, events: Vec<MelodyEventAuthoring>) -> Result<Self, FoundationError> {
        if matches!(&self.content, Some(MelodyContent::Text(_))) {
            return Err(CandidateError::InvalidAuthoring(
                "Melody text and event content cannot be combined".into(),
            )
            .into());
        }
        self.content = Some(MelodyContent::Events(events));
        Ok(self)
    }

    pub fn len(mut self, beats: f64) -> Result<Self, FoundationError> {
        if !beats.is_finite() || beats <= 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Melody length must be finite and positive".into(),
            )
            .into());
        }
        self.length = Some(beats);
        Ok(self)
    }

    pub fn velocity(mut self, velocity: f64) -> Result<Self, FoundationError> {
        if !velocity.is_finite() || !(0.0..=1.0).contains(&velocity) {
            return Err(CandidateError::InvalidAuthoring(
                "Melody velocity must be in 0.0..=1.0".into(),
            )
            .into());
        }
        self.velocity = velocity;
        Ok(self)
    }

    pub(crate) fn build_fragment(
        self,
        lifecycle: DesiredLifecycle,
    ) -> Result<(CandidateFragment, MelodyRef), FoundationError> {
        let voice = self.voice.ok_or_else(|| {
            CandidateError::InvalidAuthoring("MelodyBuilder needs a typed VoiceRef".into())
        })?;
        let content = self.content.ok_or_else(|| {
            CandidateError::InvalidAuthoring("MelodyBuilder needs tagged content".into())
        })?;
        let (events, inferred_length) = match content {
            MelodyContent::Text(text) => parse_melody_text_v2(&text, self.velocity)?,
            MelodyContent::Events(events) => {
                let inferred = validate_melody_events_v2(&events)?;
                (events, inferred)
            }
        };
        let length = self.length.unwrap_or(inferred_length);
        if events.is_empty() {
            return Err(CandidateError::InvalidAuthoring(
                "Melody content must contain at least one event".into(),
            )
            .into());
        }
        if events
            .iter()
            .any(|event| melody_event_end(event) > Beat::from_f64(length).raw())
        {
            return Err(CandidateError::InvalidAuthoring(
                "Melody event extends beyond the declared length".into(),
            )
            .into());
        }
        let declaration = MelodyAuthoring {
            voice: voice.base().typed::<VoiceKind>()?,
            events,
            length_ticks: Beat::from_f64(length).raw(),
            lifecycle,
        };
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::Melody(declaration))?;
        let owner = DeclarationOwner::Structural(self.base.source().syntax_key().clone());
        let metadata = match lifecycle {
            DesiredLifecycle::Dormant => LifecycleMetadata::register(Composition::Standalone),
            DesiredLifecycle::Start(_) => LifecycleMetadata::start(Composition::Standalone),
        };
        let dependency_source = self.base.source().clone();
        let (fragment, reference) = self.base.fragment(
            owner,
            metadata,
            payload,
            [(voice.base().clone(), dependency_source)],
        )?;
        Ok((fragment, MelodyRef::new(reference)?))
    }

    fn terminal(self, lifecycle: DesiredLifecycle) -> Result<MelodyRef, FoundationError> {
        let (fragment, reference) = self.build_fragment(lifecycle)?;
        foundation::commit_fragment(fragment)?;
        Ok(reference)
    }

    pub fn apply(self) -> Result<MelodyRef, FoundationError> {
        self.terminal(DesiredLifecycle::Dormant)
    }

    pub fn start(self) -> Result<MelodyRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Normal))
    }

    pub fn start_now(self) -> Result<MelodyRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Immediate))
    }

    pub fn launch(self) -> Result<MelodyRef, FoundationError> {
        self.start()
    }
}

fn melody_event_end(event: &MelodyEventAuthoring) -> i64 {
    match event {
        MelodyEventAuthoring::Note {
            beat_ticks,
            duration_ticks,
            ..
        }
        | MelodyEventAuthoring::Rest {
            beat_ticks,
            duration_ticks,
        } => beat_ticks.saturating_add(*duration_ticks),
    }
}

fn validate_melody_events_v2(events: &[MelodyEventAuthoring]) -> Result<f64, FoundationError> {
    let mut end = 0_i64;
    for event in events {
        let (beat, duration) = match event {
            MelodyEventAuthoring::Note {
                beat_ticks,
                duration_ticks,
                ..
            }
            | MelodyEventAuthoring::Rest {
                beat_ticks,
                duration_ticks,
            } => (*beat_ticks, *duration_ticks),
        };
        if beat < 0 || duration <= 0 {
            return Err(CandidateError::InvalidAuthoring(
                "Melody events need non-negative beats and positive durations".into(),
            )
            .into());
        }
        end = end.max(beat.saturating_add(duration));
    }
    Ok(Beat::from_raw(end).to_f64())
}

fn parse_melody_text_v2(
    text: &str,
    velocity: f64,
) -> Result<(Vec<MelodyEventAuthoring>, f64), FoundationError> {
    if text.trim().is_empty() {
        return Err(CandidateError::InvalidAuthoring("Melody text cannot be empty".into()).into());
    }
    let bars = text.split('|').collect::<Vec<_>>();
    let mut events = Vec::new();
    for (bar_index, bar) in bars.iter().enumerate() {
        let tokens = bar.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            return Err(CandidateError::InvalidAuthoring(format!(
                "Melody bar {} cannot be empty",
                bar_index + 1
            ))
            .into());
        }
        let step = 4.0 / tokens.len() as f64;
        for (token_index, token) in tokens.iter().enumerate() {
            let beat = bar_index as f64 * 4.0 + token_index as f64 * step;
            let beat_ticks = Beat::from_f64(beat).raw();
            let duration_ticks = Beat::from_f64(step).raw();
            match *token {
                "." | "_" => events.push(MelodyEventAuthoring::Rest {
                    beat_ticks,
                    duration_ticks,
                }),
                "-" => {
                    let previous = events.last_mut().ok_or_else(|| {
                        CandidateError::InvalidAuthoring(
                            "Melody tie needs a preceding note in the same content".into(),
                        )
                    })?;
                    match previous {
                        MelodyEventAuthoring::Note { duration_ticks, .. } => {
                            *duration_ticks = duration_ticks.saturating_add(Beat::from_f64(step).raw());
                        }
                        MelodyEventAuthoring::Rest { .. } => {
                            return Err(CandidateError::InvalidAuthoring(
                                "Melody tie cannot extend a rest".into(),
                            )
                            .into())
                        }
                    }
                }
                note if is_strict_note_name_v2(note) => {
                    let midi_note = parse_note_name(note).ok_or_else(|| {
                        CandidateError::InvalidAuthoring(format!(
                            "Melody note `{note}` is outside the MIDI range"
                        ))
                    })?;
                    events.push(MelodyEventAuthoring::Note {
                        beat_ticks,
                        duration_ticks,
                        midi_note,
                        velocity: CanonicalF64::new(velocity)?,
                    });
                }
                unsupported => {
                    return Err(CandidateError::InvalidAuthoring(format!(
                        "unsupported Melody token `{unsupported}`; v2 text accepts notes, rests, and ties"
                    ))
                    .into())
                }
            }
        }
    }
    Ok((events, bars.len() as f64 * 4.0))
}

fn is_strict_note_name_v2(note: &str) -> bool {
    let mut chars = note.chars().peekable();
    if !matches!(chars.next(), Some('A'..='G' | 'a'..='g')) {
        return false;
    }
    if matches!(chars.peek(), Some('#' | 'b' | '♯' | '♭')) {
        chars.next();
    }
    if chars.peek() == Some(&'-') {
        chars.next();
    }
    let mut digits = 0;
    while matches!(chars.peek(), Some('0'..='9')) {
        chars.next();
        digits += 1;
    }
    chars.next().is_none() && (digits > 0 || !note.ends_with('-'))
}

pub(crate) fn melody_builder_v2(name: String) -> Result<MelodyBuilder, Box<EvalAltResult>> {
    Ok(MelodyBuilder::new(
        foundation::authoring_builder::<MelodyKind>(&name, GroupScope::root())
            .map_err(|error| melody_v2_error(error, Position::NONE))?,
    ))
}

pub(crate) fn melody_ref_v2(name: String) -> Result<MelodyRef, Box<EvalAltResult>> {
    MelodyRef::new(
        foundation::authoring_ref::<MelodyKind>(&name, GroupScope::root())
            .map_err(|error| melody_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| melody_v2_error(error, Position::NONE))
}

fn melody_v2_error(error: FoundationError, position: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        error.to_string().into(),
        position,
    ))
}

#[cfg(test)]
fn install_v2_for_tests(engine: &mut Engine) {
    engine
        .register_type_with_name::<MelodyContent>("MelodyContent")
        .register_type_with_name::<MelodyBuilder>("MelodyBuilder")
        .register_type_with_name::<MelodyRef>("MelodyRef")
        .register_fn("melody", melody_builder_v2)
        .register_fn("melody_ref", melody_ref_v2)
        .register_fn("on", MelodyBuilder::on)
        .register_fn("notes", |builder: MelodyBuilder, notes: String| {
            builder
                .notes(notes)
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("len", |builder: MelodyBuilder, beats: f64| {
            builder
                .len(beats)
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("velocity", |builder: MelodyBuilder, velocity: f64| {
            builder
                .velocity(velocity)
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("apply", |builder: MelodyBuilder| {
            builder
                .apply()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("start", |builder: MelodyBuilder| {
            builder
                .start()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("start_now", |builder: MelodyBuilder| {
            builder
                .start_now()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("launch", |builder: MelodyBuilder| {
            builder
                .launch()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("start", |reference: MelodyRef| {
            reference
                .start()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("start_now", |reference: MelodyRef| {
            reference
                .start_now()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("stop", |reference: MelodyRef| {
            reference
                .stop()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("remove", |reference: MelodyRef| {
            reference
                .remove()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("cancel", |reference: MelodyRef| {
            reference
                .cancel()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        })
        .register_fn("status", |reference: MelodyRef| {
            reference
                .status()
                .map_err(|error| melody_v2_error(error, Position::NONE))
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v2_identity() -> vibelang_core::candidate::EvaluationIdentity {
        vibelang_core::candidate::EvaluationIdentity::new(
            vibelang_core::candidate::LanguageContract::v2(
                vibelang_core::candidate::ContractDigest::from_bytes(b"melody-v2-test"),
            ),
            vibelang_core::candidate::EngineInstanceId::new(),
            vibelang_core::mutation::RuntimeEpoch::new(),
        )
    }

    fn v2_voice_ref(name: &str) -> VoiceRef {
        VoiceRef::new(foundation::authoring_ref::<VoiceKind>(name, GroupScope::root()).unwrap())
            .unwrap()
    }

    #[test]
    fn v2_melody_configuration_is_pure_strict_and_tagged() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let voice = v2_voice_ref("lead");
        let text = MelodyBuilder::new(
            foundation::authoring_builder::<MelodyKind>("text", GroupScope::root()).unwrap(),
        )
        .on(voice.clone())
        .notes("C4 . D4 -".into())
        .unwrap();
        let events = MelodyBuilder::new(
            foundation::authoring_builder::<MelodyKind>("events", GroupScope::root()).unwrap(),
        )
        .on(voice)
        .events(vec![MelodyEventAuthoring::Rest {
            beat_ticks: 0,
            duration_ticks: Beat::ONE.raw(),
        }])
        .unwrap();

        assert!(matches!(text.content, Some(MelodyContent::Text(_))));
        assert!(matches!(events.content, Some(MelodyContent::Events(_))));
        assert!(matches!(
            text.clone().events(vec![MelodyEventAuthoring::Rest {
                beat_ticks: 0,
                duration_ticks: Beat::ONE.raw(),
            }]),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            events.clone().notes("C4".into()),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            parse_melody_text_v2("C4 [velocity=0.5]", 1.0),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        let candidate = foundation::finish_evaluation().unwrap();
        assert!(candidate.declarations().is_empty());
    }

    #[test]
    fn v2_melody_launch_alias_matches_start_and_immediate_remains_distinct() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let voice = crate::api::voice::VoiceBuilder::new(
            foundation::authoring_builder::<VoiceKind>("lead", GroupScope::root()).unwrap(),
        )
        .synth("sine".into())
        .unwrap()
        .apply()
        .unwrap();
        let build = |name: &str| {
            MelodyBuilder::new(
                foundation::authoring_builder::<MelodyKind>(name, GroupScope::root()).unwrap(),
            )
            .on(voice.clone())
            .notes("C4 D4".into())
            .unwrap()
        };
        let started = build("started").start().unwrap();
        build("launched").launch().unwrap();
        build("immediate").start_now().unwrap();

        assert!(matches!(
            started.status(),
            Err(FoundationError::ObservationUnavailable)
        ));
        let candidate = foundation::finish_evaluation().unwrap();
        let lifecycles = candidate
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration.payload() {
                DeclarationPayload::Authoring {
                    declaration: AuthoringDeclaration::Melody(melody),
                    ..
                } => Some((
                    declaration.address().key().as_str(),
                    (melody.lifecycle, declaration.lifecycle().terminal_effect),
                )),
                _ => None,
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            lifecycles["started"],
            (
                DesiredLifecycle::Start(StartMode::Normal),
                TerminalEffect::Start
            )
        );
        assert_eq!(lifecycles["launched"], lifecycles["started"]);
        assert_eq!(
            lifecycles["immediate"].0,
            DesiredLifecycle::Start(StartMode::Immediate)
        );
    }

    // ==================== Tokenizer Tests ====================

    #[test]
    fn test_tokenize_scale_degrees() {
        let tokens = tokenize_bar("1 2 3 4 5 6 7");
        assert_eq!(tokens.len(), 7);

        // Check each is a scale degree
        for (i, token) in tokens.iter().enumerate() {
            match token {
                NoteToken::ScaleDegree {
                    degree,
                    octave_offset,
                    ..
                } => {
                    assert_eq!(*degree, (i + 1) as u8);
                    assert_eq!(*octave_offset, 0);
                }
                _ => panic!("Expected ScaleDegree, got {:?}", token),
            }
        }
    }

    #[test]
    fn test_tokenize_octave_up() {
        let tokens = tokenize_bar("1' 2'' 3'''");
        assert_eq!(tokens.len(), 3);

        match &tokens[0] {
            NoteToken::ScaleDegree {
                degree,
                octave_offset,
                ..
            } => {
                assert_eq!(*degree, 1);
                assert_eq!(*octave_offset, 1);
            }
            _ => panic!("Expected ScaleDegree"),
        }

        match &tokens[1] {
            NoteToken::ScaleDegree {
                degree,
                octave_offset,
                ..
            } => {
                assert_eq!(*degree, 2);
                assert_eq!(*octave_offset, 2);
            }
            _ => panic!("Expected ScaleDegree"),
        }

        match &tokens[2] {
            NoteToken::ScaleDegree {
                degree,
                octave_offset,
                ..
            } => {
                assert_eq!(*degree, 3);
                assert_eq!(*octave_offset, 3);
            }
            _ => panic!("Expected ScaleDegree"),
        }
    }

    #[test]
    fn test_tokenize_octave_down() {
        let tokens = tokenize_bar("1, 2,, 3,,,");
        assert_eq!(tokens.len(), 3);

        match &tokens[0] {
            NoteToken::ScaleDegree {
                degree,
                octave_offset,
                ..
            } => {
                assert_eq!(*degree, 1);
                assert_eq!(*octave_offset, -1);
            }
            _ => panic!("Expected ScaleDegree"),
        }

        match &tokens[1] {
            NoteToken::ScaleDegree {
                degree,
                octave_offset,
                ..
            } => {
                assert_eq!(*degree, 2);
                assert_eq!(*octave_offset, -2);
            }
            _ => panic!("Expected ScaleDegree"),
        }

        match &tokens[2] {
            NoteToken::ScaleDegree {
                degree,
                octave_offset,
                ..
            } => {
                assert_eq!(*degree, 3);
                assert_eq!(*octave_offset, -3);
            }
            _ => panic!("Expected ScaleDegree"),
        }
    }

    #[test]
    fn test_tokenize_mixed_notation() {
        let tokens = tokenize_bar("1 - . C4 2'");
        assert_eq!(tokens.len(), 5);

        assert!(matches!(
            &tokens[0],
            NoteToken::ScaleDegree {
                degree: 1,
                octave_offset: 0,
                ..
            }
        ));
        assert!(matches!(&tokens[1], NoteToken::Tie));
        assert!(matches!(&tokens[2], NoteToken::Rest));
        assert!(matches!(&tokens[3], NoteToken::Notes(_, _)));
        assert!(matches!(
            &tokens[4],
            NoteToken::ScaleDegree {
                degree: 2,
                octave_offset: 1,
                ..
            }
        ));
    }

    #[test]
    fn test_tokenize_ties_and_rests() {
        let tokens = tokenize_bar("1--- . _");
        assert_eq!(tokens.len(), 6);

        assert!(matches!(&tokens[0], NoteToken::ScaleDegree { .. }));
        assert!(matches!(&tokens[1], NoteToken::Tie));
        assert!(matches!(&tokens[2], NoteToken::Tie));
        assert!(matches!(&tokens[3], NoteToken::Tie));
        assert!(matches!(&tokens[4], NoteToken::Rest));
        assert!(matches!(&tokens[5], NoteToken::Rest));
    }

    #[test]
    fn test_tokenize_bracket_chords() {
        // Simple chord: C major triad
        let tokens = tokenize_bar("[C4 E4 G4]");
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            NoteToken::Notes(notes, _) => {
                assert_eq!(notes.len(), 3);
                assert_eq!(notes[0], 60); // C4
                assert_eq!(notes[1], 64); // E4
                assert_eq!(notes[2], 67); // G4
            }
            _ => panic!("Expected Notes"),
        }

        // Chord followed by ties
        let tokens = tokenize_bar("[A3 C4 E4] - - -");
        assert_eq!(tokens.len(), 4);
        assert!(matches!(&tokens[0], NoteToken::Notes(_, _)));
        assert!(matches!(&tokens[1], NoteToken::Tie));
        assert!(matches!(&tokens[2], NoteToken::Tie));
        assert!(matches!(&tokens[3], NoteToken::Tie));

        // Multiple chords
        let tokens = tokenize_bar("[C4 E4] [G4 B4]");
        assert_eq!(tokens.len(), 2);
        match &tokens[0] {
            NoteToken::Notes(notes, _) => {
                assert_eq!(notes.len(), 2);
                assert_eq!(notes[0], 60); // C4
                assert_eq!(notes[1], 64); // E4
            }
            _ => panic!("Expected Notes"),
        }
        match &tokens[1] {
            NoteToken::Notes(notes, _) => {
                assert_eq!(notes.len(), 2);
                assert_eq!(notes[0], 67); // G4
                assert_eq!(notes[1], 71); // B4
            }
            _ => panic!("Expected Notes"),
        }
    }

    // ==================== Scale Degree Resolution Tests ====================

    #[test]
    fn test_resolve_c_major_scale() {
        // C major scale starting at C4 (60)
        let root = Some("C4".to_string());
        let scale = Some("major".to_string());

        // Degree 1 = C4 = 60
        assert_eq!(resolve_scale_degree(1, 0, &root, &scale), Some(60));
        // Degree 2 = D4 = 62
        assert_eq!(resolve_scale_degree(2, 0, &root, &scale), Some(62));
        // Degree 3 = E4 = 64
        assert_eq!(resolve_scale_degree(3, 0, &root, &scale), Some(64));
        // Degree 4 = F4 = 65
        assert_eq!(resolve_scale_degree(4, 0, &root, &scale), Some(65));
        // Degree 5 = G4 = 67
        assert_eq!(resolve_scale_degree(5, 0, &root, &scale), Some(67));
        // Degree 6 = A4 = 69
        assert_eq!(resolve_scale_degree(6, 0, &root, &scale), Some(69));
        // Degree 7 = B4 = 71
        assert_eq!(resolve_scale_degree(7, 0, &root, &scale), Some(71));
    }

    #[test]
    fn test_resolve_a_minor_scale() {
        // A minor scale starting at A3 (57)
        let root = Some("A3".to_string());
        let scale = Some("minor".to_string());

        // Degree 1 = A3 = 57
        assert_eq!(resolve_scale_degree(1, 0, &root, &scale), Some(57));
        // Degree 2 = B3 = 59
        assert_eq!(resolve_scale_degree(2, 0, &root, &scale), Some(59));
        // Degree 3 = C4 = 60 (minor 3rd)
        assert_eq!(resolve_scale_degree(3, 0, &root, &scale), Some(60));
        // Degree 5 = E4 = 64
        assert_eq!(resolve_scale_degree(5, 0, &root, &scale), Some(64));
    }

    #[test]
    fn test_resolve_octave_up() {
        let root = Some("C4".to_string());
        let scale = Some("major".to_string());

        // Degree 1 with octave up = C5 = 72
        assert_eq!(resolve_scale_degree(1, 1, &root, &scale), Some(72));
        // Degree 1 with two octaves up = C6 = 84
        assert_eq!(resolve_scale_degree(1, 2, &root, &scale), Some(84));
    }

    #[test]
    fn test_resolve_octave_down() {
        let root = Some("C4".to_string());
        let scale = Some("major".to_string());

        // Degree 1 with octave down = C3 = 48
        assert_eq!(resolve_scale_degree(1, -1, &root, &scale), Some(48));
        // Degree 1 with two octaves down = C2 = 36
        assert_eq!(resolve_scale_degree(1, -2, &root, &scale), Some(36));
    }

    #[test]
    fn test_resolve_default_root_and_scale() {
        // No root/scale = C4 major
        let root = None;
        let scale = None;

        // Degree 1 should be C4 = 60
        assert_eq!(resolve_scale_degree(1, 0, &root, &scale), Some(60));
        // Degree 5 should be G4 = 67
        assert_eq!(resolve_scale_degree(5, 0, &root, &scale), Some(67));
    }

    #[test]
    fn test_resolve_root_without_octave() {
        // Root without octave defaults to octave 4
        let root = Some("D".to_string());
        let scale = Some("minor".to_string());

        // Degree 1 = D4 = 62
        assert_eq!(resolve_scale_degree(1, 0, &root, &scale), Some(62));
    }

    #[test]
    fn test_resolve_pentatonic_scale() {
        let root = Some("C4".to_string());
        let scale = Some("pentatonic".to_string());

        // C major pentatonic: C D E G A
        assert_eq!(resolve_scale_degree(1, 0, &root, &scale), Some(60)); // C
        assert_eq!(resolve_scale_degree(2, 0, &root, &scale), Some(62)); // D
        assert_eq!(resolve_scale_degree(3, 0, &root, &scale), Some(64)); // E
        assert_eq!(resolve_scale_degree(4, 0, &root, &scale), Some(67)); // G
        assert_eq!(resolve_scale_degree(5, 0, &root, &scale), Some(69)); // A
    }

    #[test]
    fn test_resolve_out_of_range() {
        let root = Some("C1".to_string());
        let scale = Some("major".to_string());

        // Should return None for notes below MIDI 0
        assert_eq!(resolve_scale_degree(1, -3, &root, &scale), None);
    }

    // ==================== Scale Intervals Tests ====================

    #[test]
    fn test_scale_intervals() {
        assert_eq!(get_scale_intervals("major"), vec![0, 2, 4, 5, 7, 9, 11]);
        assert_eq!(get_scale_intervals("minor"), vec![0, 2, 3, 5, 7, 8, 10]);
        assert_eq!(get_scale_intervals("dorian"), vec![0, 2, 3, 5, 7, 9, 10]);
        assert_eq!(get_scale_intervals("pentatonic"), vec![0, 2, 4, 7, 9]);
        assert_eq!(get_scale_intervals("blues"), vec![0, 3, 5, 6, 7, 10]);
    }

    #[test]
    fn test_scale_intervals_case_insensitive() {
        assert_eq!(get_scale_intervals("MAJOR"), vec![0, 2, 4, 5, 7, 9, 11]);
        assert_eq!(get_scale_intervals("Minor"), vec![0, 2, 3, 5, 7, 8, 10]);
    }

    #[test]
    fn test_melody_scale_unknown_name_errors() {
        // Builder-level validation: unknown scale names are rejected with an
        // error listing the valid names (no silent fallback to major).
        let err = Melody::new_for_test("test")
            .scale("unknown".to_string())
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown scale 'unknown'"), "msg = {}", msg);
        assert!(msg.contains("'major'"), "msg = {}", msg);
        assert!(msg.contains("'minor_pentatonic'"), "msg = {}", msg);
    }

    // ==================== Per-Note Parameter Tests ====================

    #[test]
    fn test_parse_note_params_empty() {
        let mut chars = "".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_none());
        assert!(params.gate.is_none());
        assert!(params.params.is_empty());
    }

    #[test]
    fn test_parse_note_params_velocity_midi() {
        let mut chars = "[velocity=100]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_some());
        // 100/127 ≈ 0.787
        let vel = params.velocity.unwrap();
        assert!((vel - 100.0 / 127.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_velocity_normalized() {
        let mut chars = "[vel=0.5]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_some());
        assert!((params.velocity.unwrap() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_multiple() {
        let mut chars = "[vel=100,cutoff=2000,pan=-0.5]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_some());
        assert!((params.params["cutoff"] - 2000.0).abs() < 0.001);
        assert!((params.params["pan"] - (-0.5)).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_gate() {
        let mut chars = "[gate=0.8]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.gate.is_some());
        assert!((params.gate.unwrap() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_with_spaces() {
        let mut chars = "[vel=80, cutoff=1500]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_some());
        assert!((params.params["cutoff"] - 1500.0).abs() < 0.001);
    }

    #[test]
    fn test_tokenize_note_with_velocity() {
        let tokens = tokenize_bar("C4[velocity=100]");
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            NoteToken::Notes(notes, params) => {
                assert_eq!(notes, &[60]); // C4
                assert!(params.velocity.is_some());
                let vel = params.velocity.unwrap();
                assert!((vel - 100.0 / 127.0).abs() < 0.001);
            }
            _ => panic!("Expected Notes"),
        }
    }

    #[test]
    fn test_tokenize_note_with_multiple_params() {
        let tokens = tokenize_bar("D#3[vel=0.8,cutoff=2000]");
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            NoteToken::Notes(notes, params) => {
                assert_eq!(notes, &[51]); // D#3
                assert!(params.velocity.is_some());
                assert!((params.velocity.unwrap() - 0.8).abs() < 0.001);
                assert!((params.params["cutoff"] - 2000.0).abs() < 0.001);
            }
            _ => panic!("Expected Notes"),
        }
    }

    #[test]
    fn test_tokenize_scale_degree_with_params() {
        let tokens = tokenize_bar("1[velocity=100]");
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            NoteToken::ScaleDegree {
                degree,
                octave_offset,
                params,
            } => {
                assert_eq!(*degree, 1);
                assert_eq!(*octave_offset, 0);
                assert!(params.velocity.is_some());
            }
            _ => panic!("Expected ScaleDegree"),
        }
    }

    #[test]
    fn test_tokenize_scale_degree_octave_with_params() {
        let tokens = tokenize_bar("3'[vel=50,pan=-1.0]");
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            NoteToken::ScaleDegree {
                degree,
                octave_offset,
                params,
            } => {
                assert_eq!(*degree, 3);
                assert_eq!(*octave_offset, 1);
                assert!(params.velocity.is_some());
                assert!((params.params["pan"] - (-1.0)).abs() < 0.001);
            }
            _ => panic!("Expected ScaleDegree"),
        }
    }

    #[test]
    fn test_tokenize_chord_brackets_with_params() {
        let tokens = tokenize_bar("[C4 E4 G4][velocity=50]");
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            NoteToken::Notes(notes, params) => {
                assert_eq!(notes.len(), 3);
                assert_eq!(notes[0], 60); // C4
                assert_eq!(notes[1], 64); // E4
                assert_eq!(notes[2], 67); // G4
                assert!(params.velocity.is_some());
            }
            _ => panic!("Expected Notes"),
        }
    }

    #[test]
    fn test_tokenize_chord_suffix_with_params() {
        let tokens = tokenize_bar("C4:m7[vel=0.3]");
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            NoteToken::Notes(notes, params) => {
                // C minor 7th: C4(60) Eb4(63) G4(67) Bb4(70)
                assert_eq!(notes.len(), 4);
                assert_eq!(notes[0], 60);
                assert_eq!(notes[1], 63);
                assert_eq!(notes[2], 67);
                assert_eq!(notes[3], 70);
                assert!(params.velocity.is_some());
                assert!((params.velocity.unwrap() - 0.3).abs() < 0.001);
            }
            _ => panic!("Expected Notes"),
        }
    }

    #[test]
    fn test_tokenize_mixed_params_and_no_params() {
        let tokens = tokenize_bar("C4[velocity=100] C3[velocity=50] C2 C1[velocity=100]");
        assert_eq!(tokens.len(), 4);

        // C4 with velocity
        match &tokens[0] {
            NoteToken::Notes(notes, params) => {
                assert_eq!(notes, &[60]);
                assert!(params.velocity.is_some());
            }
            _ => panic!("Expected Notes"),
        }

        // C3 with velocity
        match &tokens[1] {
            NoteToken::Notes(notes, params) => {
                assert_eq!(notes, &[48]);
                assert!(params.velocity.is_some());
            }
            _ => panic!("Expected Notes"),
        }

        // C2 without params
        match &tokens[2] {
            NoteToken::Notes(notes, params) => {
                assert_eq!(notes, &[36]);
                assert!(params.velocity.is_none());
            }
            _ => panic!("Expected Notes"),
        }

        // C1 with velocity
        match &tokens[3] {
            NoteToken::Notes(notes, params) => {
                assert_eq!(notes, &[24]);
                assert!(params.velocity.is_some());
            }
            _ => panic!("Expected Notes"),
        }
    }

    #[test]
    fn test_tokenize_params_with_arbitrary_voice_params() {
        let tokens = tokenize_bar("C4[cutoff=2000,resonance=0.8,attack=0.01]");
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            NoteToken::Notes(_, params) => {
                assert!(params.velocity.is_none()); // No velocity override
                assert!((params.params["cutoff"] - 2000.0).abs() < 0.001);
                assert!((params.params["resonance"] - 0.8).abs() < 0.001);
                assert!((params.params["attack"] - 0.01).abs() < 0.001);
            }
            _ => panic!("Expected Notes"),
        }
    }

    // ==================== Parser Edge Cases ====================

    #[test]
    fn test_parse_note_params_empty_brackets() {
        let mut chars = "[]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_none());
        assert!(params.gate.is_none());
        assert!(params.params.is_empty());
    }

    #[test]
    fn test_parse_note_params_unterminated_bracket() {
        // Unterminated bracket should not crash, just stop parsing
        let mut chars = "[vel=80".chars().peekable();
        let params = parse_note_params(&mut chars);
        // Should still parse what it can
        assert!(params.velocity.is_some());
    }

    #[test]
    fn test_parse_note_params_no_bracket() {
        // No bracket means no params
        let mut chars = " C4".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_none());
        assert!(params.params.is_empty());
        // The iterator should not consume any chars
        assert_eq!(chars.next(), Some(' '));
    }

    #[test]
    fn test_parse_note_params_malformed_no_equals() {
        // Malformed: key without =value
        let mut chars = "[cutoff]".chars().peekable();
        let params = parse_note_params(&mut chars);
        // Should gracefully handle malformed params
        assert!(params.params.is_empty());
    }

    #[test]
    fn test_parse_note_params_velocity_clamped_above() {
        // Velocity above 127 should be clamped to 1.0
        let mut chars = "[vel=200]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_some());
        assert!(
            params.velocity.unwrap() <= 1.0,
            "Velocity should be clamped to 1.0"
        );
    }

    #[test]
    fn test_parse_note_params_velocity_zero() {
        let mut chars = "[vel=0]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_some());
        assert!((params.velocity.unwrap() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_negative_value() {
        let mut chars = "[pan=-1.0]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!((params.params["pan"] - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_case_insensitive_keys() {
        let mut chars = "[Velocity=100]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(
            params.velocity.is_some(),
            "Velocity key should be case-insensitive"
        );
    }

    #[test]
    fn test_parse_note_params_vel_alias() {
        let mut chars = "[vel=0.7]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_some());
        assert!((params.velocity.unwrap() - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_gate_zero() {
        // Gate of 0 should be clamped to 0 (not negative)
        let mut chars = "[gate=0]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.gate.is_some());
        assert!((params.gate.unwrap() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_integer_param() {
        let mut chars = "[cutoff=800]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!((params.params["cutoff"] - 800.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_many_params() {
        let mut chars = "[vel=80,cutoff=2000,resonance=0.9,pan=-0.3,attack=0.01,release=0.5]"
            .chars()
            .peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_some());
        assert_eq!(params.params.len(), 5); // cutoff, resonance, pan, attack, release
        assert!((params.params["cutoff"] - 2000.0).abs() < 0.001);
        assert!((params.params["resonance"] - 0.9).abs() < 0.001);
        assert!((params.params["pan"] - (-0.3)).abs() < 0.001);
        assert!((params.params["attack"] - 0.01).abs() < 0.001);
        assert!((params.params["release"] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_parse_note_params_spaces_around_values() {
        let mut chars = "[vel=80, cutoff=2000, pan=-0.5]".chars().peekable();
        let params = parse_note_params(&mut chars);
        assert!(params.velocity.is_some());
        assert_eq!(params.params.len(), 2);
    }

    #[test]
    fn test_tokenize_note_without_params_has_no_params() {
        let tokens = tokenize_bar("C4");
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            NoteToken::Notes(notes, params) => {
                assert_eq!(notes, &[60]);
                assert!(params.velocity.is_none());
                assert!(params.gate.is_none());
                assert!(params.params.is_empty());
            }
            _ => panic!("Expected Notes"),
        }
    }

    #[test]
    fn test_tokenize_notes_with_ties_and_params() {
        // Params should only be on the note, not on ties
        let tokens = tokenize_bar("C4[vel=80] - -");
        assert_eq!(tokens.len(), 3);
        match &tokens[0] {
            NoteToken::Notes(_, params) => {
                assert!(params.velocity.is_some());
            }
            _ => panic!("Expected Notes"),
        }
        assert!(matches!(&tokens[1], NoteToken::Tie));
        assert!(matches!(&tokens[2], NoteToken::Tie));
    }

    #[test]
    fn test_tokenize_alternating_params() {
        // Different params on each note
        let tokens = tokenize_bar("C4[cutoff=2000] D4[cutoff=1000] E4[cutoff=500]");
        assert_eq!(tokens.len(), 3);
        for (i, expected_cutoff) in [(0, 2000.0), (1, 1000.0), (2, 500.0)] {
            match &tokens[i] {
                NoteToken::Notes(_, params) => {
                    assert!(
                        (params.params["cutoff"] - expected_cutoff).abs() < 0.001,
                        "Token {} should have cutoff={}",
                        i,
                        expected_cutoff
                    );
                }
                _ => panic!("Expected Notes at index {}", i),
            }
        }
    }

    // ==================== End-to-End Melody Construction Tests ====================

    #[test]
    fn test_melody_notes_velocity_override_flows_to_melody_note() {
        let melody =
            Melody::new_for_test("test").notes("C4[velocity=100] D4[vel=0.5] E4".to_string());

        assert_eq!(melody.notes.len(), 3);

        // C4 with velocity=100 (MIDI) → normalized ≈ 0.787
        let vel_c4 = melody.notes[0].velocity;
        assert!(
            (vel_c4 - 100.0 / 127.0).abs() < 0.001,
            "C4 velocity should be 100/127, got {}",
            vel_c4
        );

        // D4 with vel=0.5 → normalized 0.5
        let vel_d4 = melody.notes[1].velocity;
        assert!(
            (vel_d4 - 0.5).abs() < 0.001,
            "D4 velocity should be 0.5, got {}",
            vel_d4
        );

        // E4 with no velocity override → default 1.0
        let vel_e4 = melody.notes[2].velocity;
        assert!(
            (vel_e4 - 1.0).abs() < 0.001,
            "E4 velocity should be default 1.0, got {}",
            vel_e4
        );
    }

    #[test]
    fn test_melody_notes_arbitrary_params_flow_to_melody_note() {
        let melody = Melody::new_for_test("test")
            .notes("C4[cutoff=2000,pan=-0.5] D4 E4[resonance=0.8]".to_string());

        assert_eq!(melody.notes.len(), 3);

        // C4: cutoff + pan
        assert_eq!(melody.notes[0].params.len(), 2);
        assert!((melody.notes[0].params["cutoff"] - 2000.0).abs() < 0.001);
        assert!((melody.notes[0].params["pan"] - (-0.5)).abs() < 0.001);

        // D4: no params
        assert!(
            melody.notes[1].params.is_empty(),
            "D4 should have no params"
        );

        // E4: resonance
        assert_eq!(melody.notes[2].params.len(), 1);
        assert!((melody.notes[2].params["resonance"] - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_melody_notes_gate_override() {
        let melody = Melody::new_for_test("test").notes("C4[gate=0.5] D4".to_string());

        assert_eq!(melody.notes.len(), 2);

        // C4: gate override stored as internal param
        assert!(
            melody.notes[0].params.contains_key("_gate_override"),
            "Gate override should be stored as internal param"
        );
    }

    #[test]
    fn test_melody_notes_params_with_ties() {
        // Params on a note should persist through ties
        let melody = Melody::new_for_test("test").notes("C4[cutoff=2000] - - D4".to_string());

        assert_eq!(melody.notes.len(), 2);

        // C4 with ties: should have cutoff param and duration of 3 beats
        assert_eq!(melody.notes[0].params.len(), 1);
        assert!((melody.notes[0].params["cutoff"] - 2000.0).abs() < 0.001);

        // D4: no params
        assert!(melody.notes[1].params.is_empty());
    }

    #[test]
    fn test_melody_notes_params_across_bars() {
        let melody = Melody::new_for_test("test")
            .notes("C4[cutoff=2000] D4 E4 F4 | G4[pan=0.5] A4 B4 C5".to_string());

        assert_eq!(melody.notes.len(), 8);

        // First bar: C4 has cutoff
        assert!((melody.notes[0].params["cutoff"] - 2000.0).abs() < 0.001);
        assert!(melody.notes[1].params.is_empty()); // D4
        assert!(melody.notes[2].params.is_empty()); // E4
        assert!(melody.notes[3].params.is_empty()); // F4

        // Second bar: G4 has pan
        assert!((melody.notes[4].params["pan"] - 0.5).abs() < 0.001);
        assert!(melody.notes[5].params.is_empty()); // A4
    }

    #[test]
    fn test_melody_notes_scale_degree_with_params() {
        let melody = Melody::new_for_test("test")
            .root("C4".to_string())
            .scale("major".to_string())
            .unwrap()
            .notes("1[cutoff=2000] 3[pan=-0.5] 5".to_string());

        assert_eq!(melody.notes.len(), 3);

        // Degree 1: C4 with cutoff
        assert!((melody.notes[0].params["cutoff"] - 2000.0).abs() < 0.001);

        // Degree 3: E4 with pan
        assert!((melody.notes[1].params["pan"] - (-0.5)).abs() < 0.001);

        // Degree 5: G4 with no params
        assert!(melody.notes[2].params.is_empty());
    }

    #[test]
    fn test_melody_notes_combined_velocity_and_params() {
        let melody =
            Melody::new_for_test("test").notes("C4[vel=80,cutoff=2000] D4[vel=40]".to_string());

        assert_eq!(melody.notes.len(), 2);

        // C4: velocity override + cutoff param
        let vel_c4 = melody.notes[0].velocity;
        assert!(
            (vel_c4 - 80.0 / 127.0).abs() < 0.001,
            "C4 velocity should be 80/127"
        );
        assert!((melody.notes[0].params["cutoff"] - 2000.0).abs() < 0.001);

        // D4: velocity override only, no extra params
        let vel_d4 = melody.notes[1].velocity;
        assert!(
            (vel_d4 - 40.0 / 127.0).abs() < 0.001,
            "D4 velocity should be 40/127"
        );
        assert!(
            melody.notes[1].params.is_empty(),
            "D4 should have no extra params"
        );
    }

    #[test]
    fn test_melody_notes_chord_with_params_shared() {
        // All notes in a chord should share the same params
        let melody = Melody::new_for_test("test").notes("[C4 E4 G4][cutoff=1500]".to_string());

        // Each note in the chord becomes a separate NoteEvent
        assert_eq!(melody.notes.len(), 1); // Still 1 MelodyNote (chord)
        assert_eq!(melody.notes[0].notes.len(), 3); // 3 MIDI notes
        assert!((melody.notes[0].params["cutoff"] - 1500.0).abs() < 0.001);
    }
}
