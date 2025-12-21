//! Diagnostic generation for VibeLang.
//!
//! Generates LSP diagnostics for:
//! - Syntax errors (from Rhai compilation)
//! - Unknown synthdefs
//! - Unknown effects
//! - Unresolved imports

use std::collections::HashSet;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

use crate::analysis::AnalysisResult;

/// Known built-in synthdefs that are always available.
fn builtin_synthdefs() -> HashSet<&'static str> {
    [
        // Sample voices
        "sample_voice_mono",
        "sample_voice_stereo",
        "warp_voice_mono",
        "warp_voice_stereo",
        // SFZ voices
        "sfz_voice_mono",
        "sfz_voice_stereo",
        "sfz_voice",
        // System
        "system_link_audio",
    ]
    .into_iter()
    .collect()
}

/// Generate semantic diagnostics from analysis results.
pub fn generate_semantic_diagnostics(
    analysis: &AnalysisResult,
    known_synthdefs: &HashSet<String>,
    known_effects: &HashSet<String>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let builtins = builtin_synthdefs();

    // Check synthdef references
    for synth_ref in &analysis.synthdef_refs {
        let name = &synth_ref.name;

        // Check if it's a known synthdef, effect, builtin, or locally-defined synthdef
        if !known_synthdefs.contains(name)
            && !known_effects.contains(name)
            && !builtins.contains(name.as_str())
            && !analysis.local_synthdefs.contains(name)
        {
            diagnostics.push(Diagnostic {
                range: synth_ref.range,
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String("unknown-synthdef".to_string())),
                code_description: None,
                source: Some("vibelang".to_string()),
                message: format!(
                    "Unknown synthdef '{}'. Did you mean to import it from stdlib?",
                    name
                ),
                related_information: None,
                tags: None,
                data: Some(serde_json::json!({
                    "synthdef": name,
                    "kind": "synthdef"
                })),
            });
        }
    }

    // Check effect references
    // Note: fx().synth() can use regular synthdefs too, not just define_fx() effects
    for effect_ref in &analysis.effect_refs {
        let name = &effect_ref.name;

        if !known_effects.contains(name)
            && !known_synthdefs.contains(name)
            && !builtins.contains(name.as_str())
            && !analysis.local_synthdefs.contains(name)
        {
            diagnostics.push(Diagnostic {
                range: effect_ref.range,
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String("unknown-effect".to_string())),
                code_description: None,
                source: Some("vibelang".to_string()),
                message: format!(
                    "Unknown effect '{}'. Did you mean to import it from stdlib?",
                    name
                ),
                related_information: None,
                tags: None,
                data: Some(serde_json::json!({
                    "effect": name,
                    "kind": "effect"
                })),
            });
        }
    }

    // Check unresolved imports
    for import in &analysis.imports {
        if import.resolved_path.is_none() {
            diagnostics.push(Diagnostic {
                range: import.range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("unresolved-import".to_string())),
                code_description: None,
                source: Some("vibelang".to_string()),
                message: format!("Cannot resolve import '{}'", import.path),
                related_information: None,
                tags: None,
                data: Some(serde_json::json!({
                    "import": import.path,
                    "kind": "import"
                })),
            });
        }
    }

    diagnostics
}

/// Combine all diagnostics from an analysis.
pub fn all_diagnostics(
    analysis: &AnalysisResult,
    known_synthdefs: &HashSet<String>,
    known_effects: &HashSet<String>,
) -> Vec<Diagnostic> {
    let mut all = analysis.syntax_errors.clone();
    all.extend(analysis.semantic_diagnostics.clone());
    all.extend(generate_semantic_diagnostics(
        analysis,
        known_synthdefs,
        known_effects,
    ));
    all.extend(analysis.lint_diagnostics.clone());
    all
}
