//! Validation engine for VibeLang scripts.
//!
//! This module provides script validation without requiring a running SuperCollider server.
//! It executes scripts with a no-op backend and tracks synthdef definitions and references
//! to detect errors like undefined synthdefs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{unbounded, Receiver};

use crate::api::{create_engine_with_paths, init_api};
use crate::runtime::RuntimeHandle;
use crate::score::extract_synthdef_name;
use crate::scsynth::Scsynth;
use crate::state::{StateManager, StateMessage};

/// Result of script validation.
#[derive(Debug, Default)]
pub struct ValidationResult {
    /// Parse errors (Rhai compilation failures).
    pub parse_errors: Vec<ValidationError>,
    /// Runtime errors (Rhai execution failures).
    pub runtime_errors: Vec<ValidationError>,
    /// References to synthdefs that are not defined.
    pub undefined_synthdefs: Vec<SynthdefReference>,
    /// All synthdefs defined in the script.
    pub defined_synthdefs: HashSet<String>,
    /// All synthdefs referenced in the script.
    pub referenced_synthdefs: Vec<SynthdefReference>,
    /// All voice names defined in the script.
    pub defined_voices: HashSet<String>,
}

impl ValidationResult {
    /// Check if the validation passed (no errors).
    pub fn is_ok(&self) -> bool {
        self.parse_errors.is_empty()
            && self.runtime_errors.is_empty()
            && self.undefined_synthdefs.is_empty()
    }

    /// Get all errors combined.
    pub fn all_errors(&self) -> Vec<&ValidationError> {
        self.parse_errors
            .iter()
            .chain(self.runtime_errors.iter())
            .collect()
    }
}

/// A reference to a synthdef in the script.
#[derive(Debug, Clone)]
pub struct SynthdefReference {
    /// The synthdef name.
    pub name: String,
    /// The voice name that references this synthdef.
    pub voice_name: String,
    /// Source file where the reference occurs.
    pub file: Option<String>,
    /// Line number (1-based).
    pub line: u32,
    /// Column number (1-based).
    pub column: u32,
}

/// An error found during validation.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error message.
    pub message: String,
    /// Source file where the error occurs.
    pub file: Option<String>,
    /// Line number (1-based).
    pub line: Option<u32>,
    /// Column number (1-based).
    pub column: Option<u32>,
}

impl ValidationError {
    /// Create a validation error from a Rhai parse error.
    pub fn from_rhai_parse(err: rhai::ParseError) -> Self {
        let pos = err.position();
        Self {
            message: err.to_string(),
            file: None,
            line: if pos.is_none() {
                None
            } else {
                pos.line().map(|l| l as u32)
            },
            column: if pos.is_none() {
                None
            } else {
                pos.position().map(|c| c as u32)
            },
        }
    }

    /// Create a validation error from a Rhai runtime error.
    pub fn from_rhai_runtime(err: Box<rhai::EvalAltResult>) -> Self {
        let pos = err.position();
        Self {
            message: err.to_string(),
            file: None,
            line: if pos.is_none() {
                None
            } else {
                pos.line().map(|l| l as u32)
            },
            column: if pos.is_none() {
                None
            } else {
                pos.position().map(|c| c as u32)
            },
        }
    }
}

/// Built-in synthdefs that are always available.
fn builtin_synthdefs() -> HashSet<String> {
    [
        "sample_voice_mono",
        "sample_voice_stereo",
        "warp_voice_mono",
        "warp_voice_stereo",
        "sfz_voice_mono",
        "sfz_voice_stereo",
        "sfz_voice",
        "system_link_audio",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Validate a VibeLang script without sending to SuperCollider.
///
/// This function:
/// 1. Creates a no-op runtime (no OSC communication)
/// 2. Sets up a validation deploy callback to track defined synthdefs
/// 3. Executes the script with the full VibeLang API
/// 4. Collects all parse/runtime errors from Rhai
/// 5. Checks for undefined synthdefs
///
/// # Arguments
/// * `content` - The script content to validate
/// * `file_path` - Optional path to the script file (for import resolution)
/// * `import_paths` - Additional paths to search for imports
///
/// # Returns
/// A `ValidationResult` containing any errors found.
pub fn validate_script(
    content: &str,
    file_path: Option<&Path>,
    import_paths: &[PathBuf],
) -> ValidationResult {
    let mut result = ValidationResult::default();

    // Track defined synthdefs via the deploy callback
    let defined_synthdefs = Arc::new(Mutex::new(HashSet::new()));

    // Set up validation deploy callback
    let defined = defined_synthdefs.clone();
    vibelang_dsp::set_deploy_callback(move |bytes| {
        if let Some(name) = extract_synthdef_name(&bytes) {
            defined.lock().unwrap().insert(name);
        }
        Ok(())
    });

    // Create validation runtime with no-op scsynth
    let (message_tx, message_rx) = unbounded();
    let state_manager = StateManager::new();
    let scsynth = Scsynth::noop();
    let (midi_tx, _midi_rx) = unbounded();

    let handle = RuntimeHandle::new_validation(message_tx, state_manager, scsynth, midi_tx);

    // Initialize API with validation handle
    init_api(handle);

    // Reset context state
    crate::api::context::reset();

    // Set script file for source location tracking
    if let Some(path) = file_path {
        crate::api::context::set_current_script_file(Some(path.to_string_lossy().to_string()));
        if let Some(parent) = path.parent() {
            crate::api::context::set_script_dir(parent.to_path_buf());
        }
    }

    // Set import paths
    crate::api::context::set_import_paths(import_paths.to_vec());

    // Create engine with import paths
    let base_path = file_path
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let mut engine = create_engine_with_paths(base_path, import_paths.to_vec());

    // Register DSP API for synthdef definitions
    vibelang_dsp::register_dsp_api(&mut engine);

    // Compile and run
    match engine.compile(content) {
        Ok(ast) => {
            // Run the script
            if let Err(e) = engine.run_ast(&ast) {
                result.runtime_errors.push(ValidationError::from_rhai_runtime(e));
            }
        }
        Err(e) => {
            result.parse_errors.push(ValidationError::from_rhai_parse(e));
        }
    }

    // Collect callback errors (from define_group closures, etc.)
    for cb_err in crate::api::context::take_callback_errors() {
        result.runtime_errors.push(ValidationError {
            message: cb_err.message,
            file: file_path.map(|p| p.to_string_lossy().to_string()),
            line: cb_err.line,
            column: cb_err.column,
        });
    }

    // Collect referenced synthdefs and voice names from messages
    let collected = collect_from_messages(&message_rx);

    // Get defined synthdefs
    result.defined_synthdefs = defined_synthdefs.lock().unwrap().clone();
    result.referenced_synthdefs = collected.synthdef_refs.clone();
    result.defined_voices = collected.voice_names;

    // Check for undefined synthdefs
    let builtin = builtin_synthdefs();
    for reference in collected.synthdef_refs {
        if !result.defined_synthdefs.contains(&reference.name) && !builtin.contains(&reference.name)
        {
            result.undefined_synthdefs.push(reference);
        }
    }

    result
}

/// Collected data from state messages.
struct CollectedData {
    synthdef_refs: Vec<SynthdefReference>,
    voice_names: HashSet<String>,
}

/// Collect synthdef references and voice names from state messages.
fn collect_from_messages(rx: &Receiver<StateMessage>) -> CollectedData {
    let mut data = CollectedData {
        synthdef_refs: Vec::new(),
        voice_names: HashSet::new(),
    };

    while let Ok(msg) = rx.try_recv() {
        if let StateMessage::UpsertVoice {
            name,
            synth_name,
            source_location,
            ..
        } = msg
        {
            // Track the voice name
            data.voice_names.insert(name.clone());

            if let Some(synthdef_name) = synth_name {
                data.synthdef_refs.push(SynthdefReference {
                    name: synthdef_name,
                    voice_name: name,
                    file: source_location.file,
                    line: source_location.line.unwrap_or(0),
                    column: source_location.column.unwrap_or(0),
                });
            }
        }
    }

    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_empty_script() {
        let result = validate_script("", None, &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_syntax_error() {
        let result = validate_script("let x = ;", None, &[]);
        assert!(!result.is_ok());
        assert!(!result.parse_errors.is_empty());
    }

    #[test]
    fn test_validate_undefined_variable() {
        let result = validate_script("print(undefined_var);", None, &[]);
        assert!(!result.is_ok());
        assert!(!result.runtime_errors.is_empty());
    }

    #[test]
    fn test_builtin_synthdefs() {
        let builtins = builtin_synthdefs();
        assert!(builtins.contains("sample_voice_mono"));
        assert!(builtins.contains("sfz_voice"));
    }
}
