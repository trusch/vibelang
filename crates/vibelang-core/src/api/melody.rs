//! Melody API for Rhai scripts.
//!
//! Melodies are pitched sequences that trigger voices with note information.

use crate::events::{BeatEvent, Pattern as PatternData};
use crate::sequences::{ClipMode, ClipSource, SequenceClip, SequenceDefinition};
use crate::state::{LoopStatus, StateMessage};
use rhai::{CustomType, Dynamic, Engine, EvalAltResult, Position, TypeBuilder};
use std::collections::HashMap;

use super::{context, require_handle};

/// A Melody builder for creating melodic patterns.
#[derive(Debug, Clone, CustomType)]
pub struct Melody {
    /// Melody name.
    pub name: String,
    /// Voice name to trigger.
    voice_name: Option<String>,
    /// Notes in the melody.
    notes: Vec<MelodyNote>,
    /// Step pattern string.
    steps: Option<String>,
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
}

/// A note in a melody.
#[derive(Debug, Clone)]
struct MelodyNote {
    beat: f64,
    note: u8,
    velocity: f64,
    gate: f64,
}

impl Melody {
    /// Create a new melody with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            voice_name: None,
            notes: Vec::new(),
            steps: None,
            length: 4.0,
            gate: 0.5,
            transpose: 0,
            swing: 0.0,
            scale: None,
            root: None,
            group_path: context::current_group_path(),
            params: HashMap::new(),
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
                if let Some(midi) = parse_note(note_str) {
                    self.notes.push(MelodyNote {
                        beat,
                        note: midi,
                        velocity: 1.0,
                        gate: self.gate,
                    });
                }
            } else if let Ok(midi) = note_val.as_int() {
                if midi > 0 {
                    self.notes.push(MelodyNote {
                        beat,
                        note: midi as u8,
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
        let mut current_note: Option<u8> = None;
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
                        // Extend current note
                        if current_note.is_some() {
                            note_duration += beat_per_token;
                        }
                    }
                    NoteToken::Rest => {
                        // Rest - commit any pending note
                        if let Some(midi) = current_note {
                            self.notes.push(MelodyNote {
                                beat: note_start_beat,
                                note: midi,
                                velocity: 1.0,
                                gate: note_duration,
                            });
                            current_note = None;
                        }
                    }
                    NoteToken::Note(midi) => {
                        // Commit any pending note
                        if let Some(prev_midi) = current_note {
                            self.notes.push(MelodyNote {
                                beat: note_start_beat,
                                note: prev_midi,
                                velocity: 1.0,
                                gate: note_duration,
                            });
                        }

                        // Start new note
                        current_note = Some(*midi);
                        note_start_beat = beat;
                        note_duration = beat_per_token;
                    }
                }
            }

            current_beat += beats_per_bar;
        }

        // Commit final note
        if let Some(midi) = current_note {
            self.notes.push(MelodyNote {
                beat: note_start_beat,
                note: midi,
                velocity: 1.0,
                gate: note_duration,
            });
        }

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
        // Each token is either a note name or a continuation marker
        let mut tokens: Vec<Option<u8>> = Vec::new();
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

                    // Parse the note to MIDI
                    if let Some(midi) = parse_note(&note_str) {
                        tokens.push(Some(midi));
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
        let mut current_note: Option<u8> = None;
        let mut note_start: f64 = 0.0;
        let mut note_duration: f64 = 0.0;

        for (i, token) in tokens.iter().enumerate() {
            let beat = i as f64 * beat_per_token;

            match token {
                Some(midi) => {
                    // Commit previous note if any
                    if let Some(prev_midi) = current_note {
                        self.notes.push(MelodyNote {
                            beat: note_start,
                            note: prev_midi,
                            velocity: 1.0,
                            gate: note_duration,
                        });
                    }
                    // Start new note
                    current_note = Some(*midi);
                    note_start = beat;
                    note_duration = beat_per_token;
                }
                None => {
                    // Extend current note or rest
                    if current_note.is_some() {
                        note_duration += beat_per_token;
                    }
                }
            }
        }

        // Commit final note
        if let Some(midi) = current_note {
            self.notes.push(MelodyNote {
                beat: note_start,
                note: midi,
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

        // Convert notes to events
        let events: Vec<BeatEvent> = self
            .notes
            .iter()
            .map(|n| {
                let transposed_note = (n.note as i64 + self.transpose).clamp(0, 127) as u8;
                // Convert MIDI note to frequency: freq = 440 * 2^((note - 69) / 12)
                let freq = 440.0 * 2.0_f64.powf((transposed_note as f64 - 69.0) / 12.0);
                // n.gate already contains the actual beat duration from parsing
                let duration = n.gate;
                let mut event = BeatEvent::new(n.beat, "melody_note");
                event.controls.push(("freq".to_string(), freq as f32));
                event.controls.push(("amp".to_string(), n.velocity as f32));
                event.controls.push(("gate".to_string(), duration as f32));
                event
            })
            .collect();

        let loop_pattern = PatternData {
            name: self.name.clone(),
            events,
            loop_length_beats: self.length,
            phase_offset: 0.0,
        };

        let _ = handle.send(StateMessage::CreateMelody {
            name: self.name.clone(),
            group_path: self.group_path.clone(),
            voice_name: self.voice_name.clone(),
            pattern: loop_pattern,
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

/// Create a new melody builder.
pub fn melody(name: String) -> Melody {
    Melody::new(name)
}

/// Token type for bar parsing.
#[derive(Debug, Clone)]
enum NoteToken {
    /// A note with its MIDI number
    Note(u8),
    /// Tie/continuation marker (-)
    Tie,
    /// Rest marker (. or _)
    Rest,
}

/// Tokenize a bar string into note tokens using character-based parsing.
/// This is robust to missing whitespace (e.g., "-G2" is parsed as [Tie, Note(43)]).
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
                        // Stop at anything else (whitespace, -, ., another note, etc.)
                        _ => break,
                    }
                }

                // Parse the note to MIDI
                if let Some(midi) = parse_note(&note_str) {
                    tokens.push(NoteToken::Note(midi));
                }
                // Invalid notes are silently ignored
            }

            // Unknown characters are ignored
            _ => {}
        }
    }

    tokens
}

/// Parse a note name to MIDI note number.
fn parse_note(name: &str) -> Option<u8> {
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
