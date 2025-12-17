//! Melody API for Rhai scripts.
//!
//! Melodies are pitched sequences that trigger voices with note information.

use crate::events::{BeatEvent, Pattern as PatternData};
use crate::sequences::{ClipMode, ClipSource, SequenceClip, SequenceDefinition};
use crate::state::{LoopStatus, StateMessage};
use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::collections::HashMap;

use super::context::{self, SourceLocation};
use super::require_handle;

/// A Melody builder for creating melodic patterns.
#[derive(Debug, Clone, CustomType)]
pub struct Melody {
    /// Melody name.
    pub name: String,
    /// Voice name to trigger.
    voice_name: Option<String>,
    /// Notes in the melody.
    notes: Vec<MelodyNote>,
    /// Step pattern string (from .step() method).
    steps: Option<String>,
    /// Notes pattern string (from .notes() method).
    notes_string: Option<String>,
    /// Loop length in beats.
    length: f64,
    /// Default gate (note duration as fraction of step).
    gate: f64,
    /// Transpose in semitones.
    transpose: i64,
    /// Swing amount.
    swing: f64,
    /// Scale name.
    scale: Option<String>,
    /// Root note.
    root: Option<String>,
    /// Group path.
    group_path: String,
    /// Parameters.
    params: HashMap<String, f64>,
    /// Source location where this melody was defined.
    source_location: SourceLocation,
}

/// A note or chord in a melody.
#[derive(Debug, Clone)]
struct MelodyNote {
    beat: f64,
    /// MIDI note numbers (single note = 1 element, chord = multiple)
    notes: Vec<u8>,
    velocity: f64,
    gate: f64,
}

impl Melody {
    /// Create a new melody with the given name and source location from NativeCallContext.
    pub fn new(ctx: NativeCallContext, name: String) -> Self {
        let pos = ctx.call_position();
        let source_location = SourceLocation::new(
            context::get_current_script_file(),
            if pos.is_none() { None } else { pos.line().map(|l| l as u32) },
            if pos.is_none() { None } else { pos.position().map(|c| c as u32) },
        );
        Self {
            name,
            voice_name: None,
            notes: Vec::new(),
            steps: None,
            notes_string: None,
            length: 4.0,
            gate: 0.5,
            transpose: 0,
            swing: 0.0,
            scale: None,
            root: None,
            group_path: context::current_group_path(),
            params: HashMap::new(),
            source_location,
        }
    }

    // === Builder methods ===

    /// Set the voice to trigger (by name).
    pub fn on(mut self, voice_name: String) -> Self {
        self.voice_name = Some(voice_name);
        self
    }

    /// Set the voice to trigger (by Voice object).
    pub fn on_voice(mut self, voice: super::voice::Voice) -> Self {
        self.voice_name = Some(voice.name.clone());
        self
    }

    /// Set the scale.
    pub fn scale(mut self, scale_name: String) -> Self {
        self.scale = Some(scale_name);
        self
    }

    /// Set the root note.
    pub fn root(mut self, root_note: String) -> Self {
        self.root = Some(root_note);
        self
    }

    /// Set notes from an array.
    ///
    /// # Example
    /// ```rhai
    /// melody("lead").notes(["C4", "E4", "G4", "C5"])
    /// ```
    pub fn notes_array(mut self, note_array: rhai::Array) -> Self {
        self.notes.clear();
        let step_duration = self.length / note_array.len().max(1) as f64;

        for (i, note_val) in note_array.iter().enumerate() {
            let beat = i as f64 * step_duration;

            if let Ok(note_str) = note_val.clone().into_immutable_string() {
                let note_str = note_str.as_str();
                if note_str == "-" || note_str == "." || note_str == "_" {
                    // Rest
                    continue;
                }
                if let Some(midi_notes) = parse_note(note_str) {
                    self.notes.push(MelodyNote {
                        beat,
                        notes: midi_notes,
                        velocity: 1.0,
                        gate: self.gate,
                    });
                }
            } else if let Ok(midi) = note_val.as_int() {
                if midi > 0 {
                    self.notes.push(MelodyNote {
                        beat,
                        notes: vec![midi as u8],
                        velocity: 1.0,
                        gate: self.gate,
                    });
                }
            }
        }

        self
    }

    /// Set notes from a string pattern.
    ///
    /// # Example
    /// ```rhai
    /// melody("bass").notes("E1 - - - | G1 - - -")
    /// ```
    ///
    /// Format:
    /// - Note names like "C4", "E1", "G#3"
    /// - `-` extends the previous note (tie)
    /// - `.` is a rest
    /// - `|` separates bars (each bar is 4 beats in 4/4 time)
    /// - Whitespace is optional (for readability)
    pub fn notes(mut self, notes_str: String) -> Self {
        self.notes.clear();

        // Split by bar separator
        let bars: Vec<&str> = notes_str.split('|').collect();
        let num_bars = bars.len();
        let beats_per_bar = 4.0; // Standard 4/4 time

        // Calculate loop length from pattern if not explicitly set via .len()
        // Use the number of bars * 4 beats per bar
        let loop_length = num_bars as f64 * beats_per_bar;
        self.length = loop_length;

        let mut current_beat = 0.0;
        let mut current_notes: Option<Vec<u8>> = None;
        let mut note_start_beat: f64 = 0.0;
        let mut note_duration: f64 = 0.0;

        for bar in bars {
            // Tokenize this bar using character-based parsing (robust to missing whitespace)
            let tokens = tokenize_bar(bar);
            if tokens.is_empty() {
                current_beat += beats_per_bar;
                continue;
            }

            let beat_per_token = beats_per_bar / tokens.len() as f64;

            for (i, token) in tokens.iter().enumerate() {
                let beat = current_beat + i as f64 * beat_per_token;

                match token {
                    NoteToken::Tie => {
                        // Extend current note/chord
                        if current_notes.is_some() {
                            note_duration += beat_per_token;
                        }
                    }
                    NoteToken::Rest => {
                        // Rest - commit any pending note/chord
                        if let Some(midi_notes) = current_notes.take() {
                            self.notes.push(MelodyNote {
                                beat: note_start_beat,
                                notes: midi_notes,
                                velocity: 1.0,
                                gate: note_duration,
                            });
                        }
                    }
                    NoteToken::ScaleDegree(degree, chord_quality) => {
                        // Commit any pending note/chord
                        if let Some(prev_notes) = current_notes.take() {
                            self.notes.push(MelodyNote {
                                beat: note_start_beat,
                                notes: prev_notes,
                                velocity: 1.0,
                                gate: note_duration,
                            });
                        }

                        // Resolve scale degree (and optional chord) to MIDI notes
                        let midi_notes =
                            resolve_scale_degree(*degree, chord_quality, &self.scale, &self.root);
                        current_notes = Some(midi_notes);
                        note_start_beat = beat;
                        note_duration = beat_per_token;
                    }
                    NoteToken::Notes(midi_notes) => {
                        // Commit any pending note/chord
                        if let Some(prev_notes) = current_notes.take() {
                            self.notes.push(MelodyNote {
                                beat: note_start_beat,
                                notes: prev_notes,
                                velocity: 1.0,
                                gate: note_duration,
                            });
                        }

                        // Start new note/chord
                        current_notes = Some(midi_notes.clone());
                        note_start_beat = beat;
                        note_duration = beat_per_token;
                    }
                }
            }

            current_beat += beats_per_bar;
        }

        // Commit final note/chord
        if let Some(midi_notes) = current_notes {
            self.notes.push(MelodyNote {
                beat: note_start_beat,
                notes: midi_notes,
                velocity: 1.0,
                gate: note_duration,
            });
        }

        // Store the original notes string for visual editing
        self.notes_string = Some(notes_str);
        self
    }

    /// Set the step pattern (for rhythm).
    ///
    /// # Format
    ///
    /// The step pattern is a string where:
    /// - Note names (like `C2`, `E3`, `G#4`) start a new note
    /// - `.` extends the previous note (tie) or is a rest if no note is active
    /// - The pattern length is determined by character count
    ///
    /// # Example
    /// ```rhai
    /// melody("bass").step("C2...E2...G2...A2...").len(4.0)
    /// ```
    /// This creates a 4-beat melody with 4 notes (C2, E2, G2, A2), each lasting 1 beat.
    pub fn step(mut self, steps: String) -> Self {
        self.notes.clear();

        // Parse the step pattern into tokens
        // Each token is either a note/chord or a continuation marker
        let mut tokens: Vec<Option<Vec<u8>>> = Vec::new();
        let mut chars = steps.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '.' | '-' | '_' => {
                    // Rest/continuation marker
                    tokens.push(None);
                }
                ' ' | '|' => {
                    // Whitespace and bar separators are ignored
                }
                'A'..='G' | 'a'..='g' => {
                    // Start of a note name - collect the full note
                    let mut note_str = String::new();
                    note_str.push(c.to_ascii_uppercase());

                    // Collect accidentals and octave
                    while let Some(&next) = chars.peek() {
                        match next {
                            '#' | 'b' | '♯' | '♭' => {
                                note_str.push(chars.next().unwrap());
                            }
                            '0'..='9' => {
                                note_str.push(chars.next().unwrap());
                            }
                            _ => break,
                        }
                    }

                    // Check for chord quality suffix (e.g., ":maj7")
                    if chars.peek() == Some(&':') {
                        note_str.push(chars.next().unwrap()); // consume ':'
                        while let Some(&next) = chars.peek() {
                            match next {
                                'a'..='z' | 'A'..='Z' | '0'..='9' => {
                                    note_str.push(chars.next().unwrap());
                                }
                                _ => break,
                            }
                        }
                    }

                    // Parse the note/chord to MIDI
                    if let Some(midi_notes) = parse_note(&note_str) {
                        tokens.push(Some(midi_notes));
                    } else {
                        // Invalid note, treat as rest
                        tokens.push(None);
                    }
                }
                _ => {
                    // Unknown character, skip
                }
            }
        }

        if tokens.is_empty() {
            return self;
        }

        // Calculate beat duration per token
        let beat_per_token = self.length / tokens.len() as f64;

        // Convert tokens to notes
        let mut current_notes: Option<Vec<u8>> = None;
        let mut note_start: f64 = 0.0;
        let mut note_duration: f64 = 0.0;

        for (i, token) in tokens.iter().enumerate() {
            let beat = i as f64 * beat_per_token;

            match token {
                Some(midi_notes) => {
                    // Commit previous note/chord if any
                    if let Some(prev_notes) = current_notes.take() {
                        self.notes.push(MelodyNote {
                            beat: note_start,
                            notes: prev_notes,
                            velocity: 1.0,
                            gate: note_duration,
                        });
                    }
                    // Start new note/chord
                    current_notes = Some(midi_notes.clone());
                    note_start = beat;
                    note_duration = beat_per_token;
                }
                None => {
                    // Extend current note/chord or rest
                    if current_notes.is_some() {
                        note_duration += beat_per_token;
                    }
                }
            }
        }

        // Commit final note/chord
        if let Some(midi_notes) = current_notes {
            self.notes.push(MelodyNote {
                beat: note_start,
                notes: midi_notes,
                velocity: 1.0,
                gate: note_duration,
            });
        }

        self.steps = Some(steps);
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
        self.swing = amount.clamp(0.0, 1.0);
        self
    }

    /// Set quantization.
    pub fn quantize(self, _beats: f64) -> Self {
        // TODO: Implement quantization
        self
    }

    /// Set a parameter.
    pub fn set_param(mut self, param: String, value: f64) -> Self {
        self.params.insert(param, value);
        self
    }

    /// Create a lane for multi-parameter melodies.
    pub fn lane(self, param: String) -> MelodyLaneBuilder {
        MelodyLaneBuilder {
            melody: self,
            param,
        }
    }

    // === Actions ===

    /// Register and apply the melody.
    pub fn apply(&mut self) {
        let handle = require_handle();

        // Capture transpose before the closure to avoid borrow issues
        let transpose = self.transpose;

        // Convert notes to events (chords generate multiple events at the same beat)
        let events: Vec<BeatEvent> = self
            .notes
            .iter()
            .flat_map(|n| {
                let beat = n.beat;
                let velocity = n.velocity;
                let gate = n.gate;
                n.notes.iter().map(move |&note| {
                    let transposed_note = (note as i64 + transpose).clamp(0, 127) as u8;
                    // Convert MIDI note to frequency: freq = 440 * 2^((note - 69) / 12)
                    let freq = 440.0 * 2.0_f64.powf((transposed_note as f64 - 69.0) / 12.0);
                    let mut event = BeatEvent::new(beat, "melody_note");
                    event.controls.push(("freq".to_string(), freq as f32));
                    event.controls.push(("amp".to_string(), velocity as f32));
                    event.controls.push(("gate".to_string(), gate as f32));
                    event
                })
            })
            .collect();

        let loop_pattern = PatternData {
            name: self.name.clone(),
            events,
            loop_length_beats: self.length,
            phase_offset: 0.0,
        };

        // Use notes_string from .notes() method, or steps from .step() method
        let notes_pattern = self.notes_string.clone().or(self.steps.clone());

        let _ = handle.send(StateMessage::CreateMelody {
            name: self.name.clone(),
            group_path: self.group_path.clone(),
            voice_name: self.voice_name.clone(),
            pattern: loop_pattern,
            source_location: self.source_location.clone(),
            notes_pattern,
        });
    }

    /// Start the melody playing (chainable).
    ///
    /// This creates an implicit sequence containing the melody as a looping clip,
    /// then starts that sequence.
    pub fn start(mut self) -> Self {
        self.apply();
        let handle = require_handle();

        // Create an implicit sequence for this melody
        let seq_name = format!("_seq_{}", self.name);
        let seq_def = SequenceDefinition::new(seq_name.clone())
            .with_loop_beats(self.length)
            .with_clip(SequenceClip::new(
                0.0,
                self.length,
                ClipSource::Melody(self.name.clone()),
                ClipMode::Loop,
            ));

        // Register and start the sequence
        let _ = handle.send(StateMessage::CreateSequence { sequence: seq_def });
        let _ = handle.send(StateMessage::StartSequence { name: seq_name });

        self
    }

    /// Stop the melody.
    ///
    /// This stops the implicit sequence that was created by `start()`.
    pub fn stop(&mut self) {
        let handle = require_handle();
        // Stop the implicit sequence
        let seq_name = format!("_seq_{}", self.name);
        let _ = handle.send(StateMessage::StopSequence { name: seq_name });
    }

    /// Launch the melody (start if not playing, chainable).
    pub fn launch(self) -> Self {
        self.start()
    }

    /// Check if the melody is playing.
    pub fn is_playing(&mut self) -> bool {
        let handle = require_handle();
        handle.with_state(|state| {
            state
                .melodies
                .get(&self.name)
                .map(|m| matches!(m.status, LoopStatus::Playing { .. }))
                .unwrap_or(false)
        })
    }

    /// Create a fade builder for a parameter.
    pub fn fade_param(&self, _param: String) {
        // TODO: Implement fade
    }
}

/// Lane builder for multi-parameter melodies.
#[derive(Debug, Clone, CustomType)]
pub struct MelodyLaneBuilder {
    melody: Melody,
    param: String,
}

impl MelodyLaneBuilder {
    /// Set values for this lane.
    pub fn values(self, _values: rhai::Array) -> Melody {
        // TODO: Implement lane values
        self.melody
    }
}

/// Create a new melody builder with source location tracking.
pub fn melody(ctx: NativeCallContext, name: String) -> Melody {
    Melody::new(ctx, name)
}

/// Token type for bar parsing.
#[derive(Debug, Clone)]
enum NoteToken {
    /// A note or chord with MIDI number(s)
    Notes(Vec<u8>),
    /// A scale degree (1-7) with optional chord quality to be resolved later with scale/root context
    ScaleDegree(u8, Option<String>),
    /// Tie/continuation marker (-)
    Tie,
    /// Rest marker (. or _)
    Rest,
}

/// Tokenize a bar string into note tokens using character-based parsing.
/// This is robust to missing whitespace (e.g., "-G2" is parsed as [Tie, Notes([43])]).
/// Supports chord syntax like "C4:maj7".
fn tokenize_bar(bar: &str) -> Vec<NoteToken> {
    let mut tokens = Vec::new();
    let mut chars = bar.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // Whitespace is ignored (just for visual separation)
            ' ' | '\t' | '\n' | '\r' => {}

            // Tie/continuation marker
            '-' => {
                tokens.push(NoteToken::Tie);
            }

            // Rest markers
            '.' | '_' => {
                tokens.push(NoteToken::Rest);
            }

            // Scale degree (1-7), optionally with chord quality (e.g., "1:maj")
            '1'..='7' => {
                let degree = c as u8 - b'0';

                // Check for chord quality suffix (e.g., ":maj7")
                let chord_quality = if chars.peek() == Some(&':') {
                    chars.next(); // consume ':'
                    let mut quality = String::new();
                    // Collect chord quality (letters, numbers)
                    while let Some(&next) = chars.peek() {
                        match next {
                            'a'..='z' | 'A'..='Z' | '0'..='9' => {
                                quality.push(chars.next().unwrap());
                            }
                            _ => break,
                        }
                    }
                    if quality.is_empty() {
                        None
                    } else {
                        Some(quality)
                    }
                } else {
                    None
                };

                tokens.push(NoteToken::ScaleDegree(degree, chord_quality));
            }

            // Start of a note name
            'A'..='G' | 'a'..='g' => {
                let mut note_str = String::new();
                note_str.push(c.to_ascii_uppercase());

                // Collect accidentals and octave digits
                while let Some(&next) = chars.peek() {
                    match next {
                        '#' | 'b' | '♯' | '♭' => {
                            note_str.push(chars.next().unwrap());
                        }
                        '0'..='9' => {
                            note_str.push(chars.next().unwrap());
                        }
                        // Stop at anything else (whitespace, -, ., another note, :, etc.)
                        _ => break,
                    }
                }

                // Check for chord quality suffix (e.g., ":maj7")
                if chars.peek() == Some(&':') {
                    note_str.push(chars.next().unwrap()); // consume ':'
                    // Collect chord quality (letters, numbers, and special chars like 'b' for m7b5)
                    while let Some(&next) = chars.peek() {
                        match next {
                            'a'..='z' | 'A'..='Z' | '0'..='9' => {
                                note_str.push(chars.next().unwrap());
                            }
                            _ => break,
                        }
                    }
                }

                // Parse the note/chord to MIDI
                if let Some(midi_notes) = parse_note(&note_str) {
                    tokens.push(NoteToken::Notes(midi_notes));
                }
                // Invalid notes are silently ignored
            }

            // Unknown characters are ignored
            _ => {}
        }
    }

    tokens
}

/// Get scale intervals by name.
/// Returns semitone offsets from root for each scale degree (1-7).
fn get_scale_intervals(scale_name: &str) -> Vec<i8> {
    match scale_name.to_lowercase().as_str() {
        "major" | "ionian" => vec![0, 2, 4, 5, 7, 9, 11],
        "minor" | "natural_minor" | "aeolian" => vec![0, 2, 3, 5, 7, 8, 10],
        "dorian" => vec![0, 2, 3, 5, 7, 9, 10],
        "phrygian" => vec![0, 1, 3, 5, 7, 8, 10],
        "lydian" => vec![0, 2, 4, 6, 7, 9, 11],
        "mixolydian" => vec![0, 2, 4, 5, 7, 9, 10],
        "locrian" => vec![0, 1, 3, 5, 6, 8, 10],
        "harmonic_minor" => vec![0, 2, 3, 5, 7, 8, 11],
        "melodic_minor" => vec![0, 2, 3, 5, 7, 9, 11],
        "pentatonic" | "major_pentatonic" => vec![0, 2, 4, 7, 9, 12, 14], // Extended to 7 degrees
        "minor_pentatonic" => vec![0, 3, 5, 7, 10, 12, 15],
        "blues" => vec![0, 3, 5, 6, 7, 10, 12],
        _ => vec![0, 2, 4, 5, 7, 9, 11], // Default to major
    }
}

/// Parse a root note name to MIDI note number.
/// Supports formats like "D", "D4", "F#", "F#3", etc.
/// Returns the MIDI note number (defaults to octave 4 if not specified).
fn parse_root_note(root: &str) -> u8 {
    let root = root.trim();
    if root.is_empty() {
        return 60; // Default to C4
    }

    let mut chars = root.chars().peekable();

    // Parse note letter
    let base: i16 = match chars.next().unwrap_or('C').to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => 0,
    };

    // Parse accidental
    let mut accidental: i16 = 0;
    while let Some(&c) = chars.peek() {
        match c {
            '#' | '♯' => {
                accidental += 1;
                chars.next();
            }
            'b' | '♭' => {
                accidental -= 1;
                chars.next();
            }
            _ => break,
        }
    }

    // Parse octave (default to 4 if not specified)
    let octave_str: String = {
        let mut result = String::new();
        // Optional leading minus for negative octaves (e.g., "C-1")
        if chars.peek() == Some(&'-') {
            result.push(chars.next().unwrap());
        }
        // Collect digits
        while chars.peek().map_or(false, |c| c.is_ascii_digit()) {
            result.push(chars.next().unwrap());
        }
        result
    };
    let octave: i16 = if octave_str.is_empty() {
        4 // Default octave
    } else {
        octave_str.parse().unwrap_or(4)
    };

    // Calculate MIDI note: (octave + 1) * 12 + base + accidental
    let midi = (octave + 1) * 12 + base + accidental;
    midi.clamp(0, 127) as u8
}

/// Resolve a scale degree to MIDI note(s).
/// If chord_quality is provided, returns multiple notes forming a chord.
fn resolve_scale_degree(
    degree: u8,
    chord_quality: &Option<String>,
    scale: &Option<String>,
    root: &Option<String>,
) -> Vec<u8> {
    let scale_intervals = scale
        .as_ref()
        .map(|s| get_scale_intervals(s))
        .unwrap_or_else(|| vec![0, 2, 4, 5, 7, 9, 11]); // Default to major

    // parse_root_note returns full MIDI note (e.g., "D4" -> 62, "D" -> 62, "D2" -> 38)
    let base_midi = root.as_ref().map(|r| parse_root_note(r)).unwrap_or(60) as i16;

    // degree is 1-indexed, so subtract 1 for array access
    let degree_idx = (degree.saturating_sub(1) as usize) % scale_intervals.len();
    let interval = scale_intervals[degree_idx] as i16;

    let root_note = (base_midi + interval).clamp(0, 127) as u8;

    // If chord quality is specified, build the chord
    if let Some(quality) = chord_quality {
        if let Some(chord_intervals) = get_chord_intervals(quality) {
            return chord_intervals
                .iter()
                .filter_map(|&offset| {
                    let midi = root_note as i16 + offset as i16;
                    if (0..=127).contains(&midi) {
                        Some(midi as u8)
                    } else {
                        None
                    }
                })
                .collect();
        }
    }

    // Single note
    vec![root_note]
}

/// Get chord intervals by quality name.
/// Returns semitone offsets from root note.
fn get_chord_intervals(quality: &str) -> Option<Vec<i8>> {
    match quality.to_lowercase().as_str() {
        // Triads
        "maj" | "major" => Some(vec![0, 4, 7]),
        "min" | "m" | "minor" => Some(vec![0, 3, 7]),
        "dim" | "diminished" => Some(vec![0, 3, 6]),
        "aug" | "augmented" => Some(vec![0, 4, 8]),
        "sus2" => Some(vec![0, 2, 7]),
        "sus4" => Some(vec![0, 5, 7]),

        // Seventh chords
        "maj7" | "major7" => Some(vec![0, 4, 7, 11]),
        "7" | "dom7" => Some(vec![0, 4, 7, 10]),
        "min7" | "m7" => Some(vec![0, 3, 7, 10]),
        "dim7" => Some(vec![0, 3, 6, 9]),
        "m7b5" | "half-dim" => Some(vec![0, 3, 6, 10]),
        "mmaj7" | "minmaj7" => Some(vec![0, 3, 7, 11]),

        // Extended
        "9" => Some(vec![0, 4, 7, 10, 14]),
        "maj9" => Some(vec![0, 4, 7, 11, 14]),
        "m9" | "min9" => Some(vec![0, 3, 7, 10, 14]),
        "add9" => Some(vec![0, 4, 7, 14]),
        "6" => Some(vec![0, 4, 7, 9]),
        "m6" | "min6" => Some(vec![0, 3, 7, 9]),

        // Power chord
        "5" | "power" => Some(vec![0, 7]),

        _ => None,
    }
}

/// Parse a note or chord to MIDI note number(s).
/// Supports single notes ("C4") and chords ("C4:maj7").
fn parse_note(name: &str) -> Option<Vec<u8>> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    // Check for chord syntax: "C4:maj7" -> split on ":"
    if let Some(colon_pos) = name.find(':') {
        let note_part = &name[..colon_pos];
        let quality = &name[colon_pos + 1..];

        // Parse the root note
        let root = parse_single_note(note_part)?;

        // Get chord intervals and build note list
        let intervals = get_chord_intervals(quality)?;
        let notes: Vec<u8> = intervals
            .iter()
            .filter_map(|&interval| {
                let midi = root as i16 + interval as i16;
                if (0..=127).contains(&midi) {
                    Some(midi as u8)
                } else {
                    None
                }
            })
            .collect();

        if notes.is_empty() {
            None
        } else {
            Some(notes)
        }
    } else {
        // No colon - single note (backward compatible)
        parse_single_note(name).map(|n| vec![n])
    }
}

/// Parse a single note name to MIDI note number.
fn parse_single_note(name: &str) -> Option<u8> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    // First try parsing as a numeric MIDI note (0-127)
    if let Ok(midi) = name.parse::<i32>() {
        if (0..=127).contains(&midi) {
            return Some(midi as u8);
        }
        return None;
    }

    let mut chars = name.chars().peekable();

    // Parse note letter
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    // Parse accidental
    let mut accidental = 0i8;
    while let Some(&c) = chars.peek() {
        match c {
            '#' | '♯' => {
                accidental += 1;
                chars.next();
            }
            'b' | '♭' => {
                accidental -= 1;
                chars.next();
            }
            _ => break,
        }
    }

    // Parse octave - only collect digits (and optional leading minus for negative octaves)
    let octave_str: String = {
        let mut result = String::new();
        // Optional leading minus for negative octaves (e.g., "C-1")
        if chars.peek() == Some(&'-') {
            result.push(chars.next().unwrap());
        }
        // Collect digits only
        while chars.peek().map_or(false, |c| c.is_ascii_digit()) {
            result.push(chars.next().unwrap());
        }
        result
    };
    let octave: i8 = octave_str.parse().unwrap_or(4);

    // Calculate MIDI note
    let midi = (octave + 1) as i16 * 12 + base as i16 + accidental as i16;

    if (0..=127).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
}

/// Register melody API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register Melody type
    engine.build_type::<Melody>();
    engine.build_type::<MelodyLaneBuilder>();

    // Constructor
    engine.register_fn("melody", melody);

    // Builder methods
    engine.register_fn("on", Melody::on);
    engine.register_fn("on", Melody::on_voice);
    engine.register_fn("scale", Melody::scale);
    engine.register_fn("root", Melody::root);
    engine.register_fn("notes", Melody::notes);
    engine.register_fn("notes", Melody::notes_array);
    engine.register_fn("step", Melody::step);
    engine.register_fn("len", Melody::len);
    engine.register_fn("gate", Melody::gate);
    engine.register_fn("transpose", Melody::transpose);
    engine.register_fn("swing", Melody::swing);
    engine.register_fn("quantize", Melody::quantize);
    engine.register_fn("set_param", Melody::set_param);
    engine.register_fn("lane", Melody::lane);

    // Actions
    engine.register_fn("apply", Melody::apply);
    engine.register_fn("start", Melody::start);
    engine.register_fn("stop", Melody::stop);
    engine.register_fn("launch", Melody::launch);
    engine.register_fn("is_playing", Melody::is_playing);

    // Lane builder
    engine.register_fn("values", MelodyLaneBuilder::values);
}
