use thiserror::Error;
use std::path::PathBuf;
use std::io;

/// Errors that can occur during SFZ parsing
///
/// This enum represents the various errors that can occur when parsing, validating,
/// or processing SFZ files. SFZ errors generally fall into a few categories:
///
/// - File access errors: Problems reading or finding SFZ or sample files
/// - Syntax errors: Malformed SFZ content that doesn't follow the format
/// - Semantic errors: Valid syntax but invalid values or missing required elements
///
/// # Common SFZ Errors
///
/// When working with SFZ files, common issues include:
///
/// - Missing sample files: Ensure paths are correct and samples exist
/// - Invalid syntax: Check for proper section headers `<section>` and opcode=value format
/// - Missing required elements: Each `<region>` needs at least a `sample` opcode
#[derive(Error, Debug)]
pub enum Error {
    /// Input/Output error when reading files
    ///
    /// This occurs when there are problems reading an SFZ file or its samples.
    /// Common causes include:
    /// - File permissions issues
    /// - Attempting to read a directory as a file
    /// - File system errors
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    
    /// Parse error for general syntax problems
    ///
    /// This indicates that the SFZ file contains syntax that cannot be parsed.
    /// Common issues include:
    /// - Missing angle brackets in section headers
    /// - Malformed opcode=value pairs
    /// - Unexpected characters or file encoding issues
    #[error("Parse error: {0}")]
    Parse(String),
    
    /// Invalid opcode value for a particular type
    ///
    /// This occurs when an opcode value cannot be converted to the expected type.
    /// For example:
    /// - Using "foo" for an integer opcode like `key=foo` (should be a number)
    /// - Using an out-of-range value like `volume=1000` (should be -144 to 6)
    #[error("Invalid value '{0}' for type {1}")]
    InvalidOpcodeValue(String, String),
    
    /// Missing required opcode
    ///
    /// This indicates that a required opcode was not found when needed.
    /// Common examples:
    /// - Missing `sample` opcode in a region
    /// - Referencing a controller with `oncc` but the controller isn't defined
    #[error("Opcode '{0}' not found")]
    MissingOpcode(String),
    
    /// Invalid SFZ section
    ///
    /// This indicates that a section in the SFZ file is invalid.
    /// This could occur if:
    /// - A section header is malformed
    /// - A section contains invalid content
    /// - Sections are improperly nested
    #[error("Invalid SFZ section: {0}")]
    InvalidSection(String),
    
    /// Missing required section
    ///
    /// This occurs when a required section is missing.
    /// In SFZ, a valid instrument typically needs:
    /// - At least one `<region>` section
    #[error("Missing required section: {0}")]
    MissingSection(String),
    
    /// File not found
    ///
    /// This occurs when an SFZ file or sample file cannot be found.
    /// Common issues:
    /// - Incorrect paths in SFZ files
    /// - Missing sample files
    /// - Case sensitivity issues on Unix-like systems
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    
    /// Detailed parse error with line and column information
    ///
    /// This provides more context about where in the SFZ file a parse error occurred.
    #[error("Failed to parse SFZ at line {line}, column {column}: {message}")]
    ParseAt {
        /// Line number where the error occurred (1-based)
        line: usize,
        /// Column position where the error occurred (1-based)
        column: usize,
        /// Error message describing the problem
        message: String,
    },
    
    /// Invalid opcode name
    ///
    /// This occurs when an opcode name is not recognized or is invalid.
    /// In SFZ, this typically happens with:
    /// - Typos in opcode names
    /// - Using opcodes from newer SFZ versions in an older parser
    /// - Using undefined custom opcodes
    #[error("Invalid opcode: {0}")]
    InvalidOpcode(String),
    
    /// Invalid value for a specific opcode
    ///
    /// This occurs when an opcode value is invalid for that specific opcode,
    /// even if it might be valid for other opcodes.
    /// For example:
    /// - Using a value outside the allowed range (e.g., `pan=200` when valid range is -100 to 100)
    /// - Using an invalid option name (e.g., `loop_mode=invalid` when only certain modes are allowed)
    #[error("Invalid value for opcode {opcode}: {value}")]
    InvalidValue {
        /// The opcode name
        opcode: String,
        /// The invalid value
        value: String,
    },
    
    /// Missing required header
    ///
    /// This occurs when a required section header is missing.
    /// For example, an SFZ file might require at least one region section.
    #[error("Missing required header {0}")]
    MissingHeader(String),
    
    /// Missing required region definition
    ///
    /// This occurs when an SFZ file has no `<region>` sections.
    /// A valid SFZ instrument typically needs at least one region to produce sound.
    #[error("Missing required region definition")]
    MissingRegion,
} 