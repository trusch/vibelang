//! SFZ Parser - embedded in vibelang-sfz
//!
//! Type-safe parser for SFZ format files.

use std::fs;
use std::path::Path;

mod parse;
mod types;
mod error;
pub mod opcodes;
pub mod path_utils;

// Export main types
pub use types::{SfzFile, SfzSection, SfzSectionType};
pub use error::Error;

// Re-export available opcode traits
pub use opcodes::categories::{
    SoundSourceOpcodes,
    RegionLogicOpcodes,
    PerformanceOpcodes,
    AmplitudeEnvelopeOpcodes,
    PitchEnvelopeOpcodes,
    FilterOpcodes,
    FilterEnvelopeOpcodes,
    SamplePlaybackOpcodes,
};
pub use opcodes::{SfzOpcodes, LoopMode, TriggerMode, OffMode};

// Export path utilities
pub use path_utils::{normalize_path, combine_sample_path};

pub type Result<T> = std::result::Result<T, Error>;

/// Parse an SFZ file from a string
pub fn parse_sfz_str(content: &str) -> Result<SfzFile> {
    parse::parse_sfz(content)
}

/// Parse an SFZ file from a file path
pub fn parse_sfz_file<P: AsRef<Path>>(path: P) -> Result<SfzFile> {
    let content = fs::read_to_string(path.as_ref())?;
    let mut sfz = parse_sfz_str(&content)?;

    // Use the absolute path for resolving sample paths
    let absolute_path = fs::canonicalize(path.as_ref())
        .unwrap_or_else(|_| path.as_ref().to_path_buf());

    sfz.source_file = Some(absolute_path);
    Ok(sfz)
}

/// Parse an SFZ file or string content (auto-detect)
pub fn parse_sfz_auto<S: AsRef<str>>(source: S) -> Result<SfzFile> {
    let content = source.as_ref();

    if content.trim_start().starts_with('<') || content.contains('\n') {
        parse_sfz_str(content)
    } else {
        let path = Path::new(content);
        parse_sfz_file(path)
    }
}

// Alias for backward compatibility
pub use parse_sfz_file as parse_sfz;
