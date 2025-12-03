use std::str::FromStr;
use std::path::PathBuf;

use crate::parser::error::Error;
use super::Result;
use crate::parser::path_utils::normalize_path;

/// Trait for parsing opcode values
///
/// This trait provides a standard way to parse raw string values from SFZ files
/// into appropriate Rust types. Each opcode in SFZ has an expected type, and this
/// trait enables type-safe conversion.
///
/// # SFZ Value Types
///
/// SFZ opcodes can have various value types:
///
/// - **Strings**: Sample paths, identifiers, etc. (e.g., `sample=piano_C4.wav`)
/// - **Integers**: Key numbers, velocity values, etc. (e.g., `key=60`)
/// - **Floats**: Volume levels, amplitudes, etc. (e.g., `volume=-6.5`)
/// - **Booleans**: Flags for various features (e.g., `loop_mode=one_shot`)
/// - **Enums**: Values from a predefined set (e.g., `trigger=release`)
///
/// # Type Conversion in SFZ
///
/// In SFZ, all values are represented as strings in the file, but they need to be
/// interpreted as the appropriate type. This trait provides that conversion, with
/// proper error handling for invalid values.
pub trait OpcodeValue: Sized {
    /// Parse an opcode value from string
    ///
    /// This method converts a raw string value from an SFZ file into the
    /// appropriate Rust type for an opcode.
    ///
    /// # Arguments
    ///
    /// * `s` - The string value to parse
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - The parsed value or an error
    fn parse_opcode(s: &str) -> Result<Self>;
}

// Implement OpcodeValue for primitive types
impl OpcodeValue for String {
    /// Parse a string opcode value
    ///
    /// String values in SFZ include sample paths, identifiers, labels, etc.
    ///
    /// # Examples in SFZ
    ///
    /// ```text
    /// sample=piano_C4.wav
    /// label=Piano
    /// ```
    fn parse_opcode(s: &str) -> Result<Self> {
        Ok(s.to_string())
    }
}

impl OpcodeValue for i32 {
    /// Parse an integer opcode value
    ///
    /// Integer values in SFZ include MIDI note numbers, key ranges, etc.
    ///
    /// # Examples in SFZ
    ///
    /// ```text
    /// key=60      // Middle C (C4)
    /// lokey=36    // Lowest key (C2)
    /// hikey=84    // Highest key (C6)
    /// ```
    fn parse_opcode(s: &str) -> Result<Self> {
        s.parse::<i32>().map_err(|_| Error::InvalidOpcodeValue(s.to_string(), "integer".to_string()))
    }
}

impl OpcodeValue for f32 {
    /// Parse a floating-point opcode value
    ///
    /// Float values in SFZ include volume levels, tuning, envelope times, etc.
    ///
    /// # Examples in SFZ
    ///
    /// ```text
    /// volume=-6.0       // Volume in decibels
    /// pan=0.5           // Panning (-1.0 to 1.0)
    /// ampeg_attack=0.01 // Attack time in seconds
    /// ```
    fn parse_opcode(s: &str) -> Result<Self> {
        s.parse::<f32>().map_err(|_| Error::InvalidOpcodeValue(s.to_string(), "float".to_string()))
    }
}

impl OpcodeValue for bool {
    /// Parse a boolean opcode value
    ///
    /// Boolean values in SFZ can be represented in various ways:
    /// - yes/no
    /// - true/false
    /// - 1/0
    /// - on/off
    ///
    /// # Examples in SFZ
    ///
    /// ```text
    /// loop_mode=no_loop
    /// trigger=attack
    /// offset=0
    /// ```
    fn parse_opcode(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "yes" | "true" | "1" | "on" => Ok(true),
            "no" | "false" | "0" | "off" => Ok(false),
            _ => Err(Error::InvalidOpcodeValue(s.to_string(), "boolean".to_string())),
        }
    }
}

impl OpcodeValue for PathBuf {
    /// Parse a file path opcode value
    ///
    /// Path values in SFZ point to sample files or other resources.
    /// This implementation normalizes paths to use the correct path
    /// separators for the current operating system.
    ///
    /// # Examples in SFZ
    ///
    /// ```text
    /// sample=piano_C4.wav
    /// sample=samples/piano/C4.wav
    /// ```
    fn parse_opcode(s: &str) -> Result<Self> {
        // Normalize path separators for the current OS
        let normalized_path = normalize_path(s);
        Ok(PathBuf::from(normalized_path))
    }
}

/// Loop modes for sample playback
///
/// This enum represents the different loop modes available in SFZ.
/// Loop modes control how samples repeat during playback.
///
/// # Loop Modes in SFZ
///
/// - **no_loop**: The sample plays once and stops
/// - **one_shot**: The sample plays once and ignores note-off events
/// - **loop_continuous**: The sample loops continuously until note-off
/// - **loop_sustain**: The sample loops until note-off, then continues to the end
///
/// # Examples in SFZ
///
/// ```text
/// loop_mode=no_loop
/// loop_mode=one_shot
/// loop_mode=loop_continuous
/// loop_mode=loop_sustain
/// ```
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum LoopMode {
    /// No looping, sample plays once
    NoLoop,
    /// Sample plays once, ignoring note-off
    OneShot,
    /// Sample loops continuously
    Loop,
    /// Sample loops until note-off
    LoopContinuous,
}

impl FromStr for LoopMode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "no_loop" | "noloop" => Ok(LoopMode::NoLoop),
            "one_shot" | "oneshot" => Ok(LoopMode::OneShot),
            "loop" | "loop_continuous" => Ok(LoopMode::Loop),
            "loop_sustain" => Ok(LoopMode::LoopContinuous),
            _ => Err(Error::InvalidOpcodeValue(s.to_string(), "LoopMode".to_string())),
        }
    }
}

impl OpcodeValue for LoopMode {
    fn parse_opcode(s: &str) -> Result<Self> {
        s.parse()
    }
}

/// Trigger modes for region playback
///
/// This enum represents the different trigger modes available in SFZ.
/// Trigger modes control what event causes a sample to start playing.
///
/// # Trigger Modes in SFZ
///
/// - **attack**: Sample plays when a key is pressed (note-on)
/// - **release**: Sample plays when a key is released (note-off)
/// - **first**: Sample plays only on the first note in a legato sequence
/// - **legato**: Sample plays only on legato transitions, not the first note
///
/// # Examples in SFZ
///
/// ```text
/// trigger=attack   // Default trigger mode
/// trigger=release  // Release trigger (plays on key release)
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerMode {
    /// Triggered on note-on (key press)
    Attack,
    /// Triggered on note-off (key release)
    Release,
    /// Triggered only on the first note in a legato sequence
    First,
    /// Triggered only on legato transitions
    Legato,
}

impl FromStr for TriggerMode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "attack" => Ok(TriggerMode::Attack),
            "release" => Ok(TriggerMode::Release),
            "first" => Ok(TriggerMode::First),
            "legato" => Ok(TriggerMode::Legato),
            _ => Err(Error::InvalidOpcodeValue(s.to_string(), "TriggerMode".to_string())),
        }
    }
}

impl OpcodeValue for TriggerMode {
    fn parse_opcode(s: &str) -> Result<Self> {
        s.parse()
    }
}

/// Off modes for note endings
///
/// This enum represents the different off modes available in SFZ.
/// Off modes control how samples behave when a note is released.
///
/// # Off Modes in SFZ
///
/// - **fast**: Quickly fade out the note (uses release envelope)
/// - **normal**: Play the normal release portion of the sound
///
/// # Examples in SFZ
///
/// ```text
/// off_mode=fast    // Quick fadeout on note-off
/// off_mode=normal  // Normal release behavior
/// ```
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum OffMode {
    /// Quick fadeout on note-off
    Fast,
    /// Normal release behavior
    Normal,
}

impl FromStr for OffMode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "fast" => Ok(OffMode::Fast),
            "normal" => Ok(OffMode::Normal),
            _ => Err(Error::InvalidOpcodeValue(s.to_string(), "OffMode".to_string())),
        }
    }
}

impl OpcodeValue for OffMode {
    fn parse_opcode(s: &str) -> Result<Self> {
        s.parse()
    }
} 