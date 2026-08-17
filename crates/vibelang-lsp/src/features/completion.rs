//! Completion provider for VibeLang.
//!
//! Provides intelligent code completion for:
//! - API functions (voice, pattern, melody, etc.)
//! - Synthdef names
//! - Effect names
//! - Import paths
//! - Method chains
//! - Parameter names

use std::collections::HashSet;
use std::path::PathBuf;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation,
    InsertTextFormat, MarkupContent, MarkupKind,
};

use crate::analysis::{AnalysisResult, CompletionContext};
use crate::data::{get_api_docs, get_api_method_docs, get_ugen_completions, ApiMethodDoc};

/// Get completions for a given context.
pub fn get_completions(
    context: &CompletionContext,
    known_synthdefs: &HashSet<String>,
    known_effects: &HashSet<String>,
    import_paths: &[PathBuf],
    current_file: Option<&PathBuf>,
    analysis: Option<&AnalysisResult>,
) -> Vec<CompletionItem> {
    match context {
        CompletionContext::TopLevel => get_top_level_completions(),
        CompletionContext::SynthdefName => get_synthdef_completions(known_synthdefs, analysis),
        CompletionContext::EffectName => get_effect_completions(known_effects, analysis),
        CompletionContext::ImportPath => get_import_completions(import_paths, current_file),
        CompletionContext::ParamName { synthdef } => get_param_completions(synthdef.as_deref()),
        CompletionContext::NotePattern => get_note_pattern_completions(),
        CompletionContext::MethodChain { object_type } => {
            get_method_completions(object_type.as_deref())
        }
        CompletionContext::FunctionCall { .. } => vec![], // Handled by signature help
        CompletionContext::DspBody => get_ugen_completions(), // UGen functions inside .body()
        CompletionContext::Unknown => vec![],
    }
}

/// Top-level API function completions.
fn get_top_level_completions() -> Vec<CompletionItem> {
    let docs = get_api_docs();

    docs.values()
        .map(|func| CompletionItem {
            label: func.name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(format!(" {}", func.signature)),
                description: None,
            }),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: if func.example.is_empty() {
                    func.description.clone()
                } else {
                    format!(
                        "{}\n\n**Example:**\n```rhai\n{}\n```",
                        func.description, func.example
                    )
                },
            })),
            insert_text: Some(get_snippet_for_function(&func.name)),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        })
        .collect()
}

/// Get a snippet template for a function.
fn get_snippet_for_function(name: &str) -> String {
    match name {
        "voice" => "voice(\"$1\")$0".to_string(),
        "pattern" => "pattern(\"$1\")$0".to_string(),
        "melody" => "melody(\"$1\")$0".to_string(),
        "sequence" => "sequence(\"$1\")$0".to_string(),
        "define_group" => "define_group(\"$1\", || {\n\t$0\n})".to_string(),
        "group" => "group(\"$1\")$0".to_string(),
        "fx" => "fx(\"$1\")$0".to_string(),
        "fade" => "fade(\"$1\")$0".to_string(),
        "sample" => "sample(\"$1\", \"$2\")$0".to_string(),
        "load_sfz" => "load_sfz(\"$1\", \"$2\")$0".to_string(),
        "define_synthdef" => {
            "define_synthdef(\"$1\")\n\t.param(\"$2\", $3)\n\t.body(|$2| {\n\t\t$0\n\t})"
                .to_string()
        }
        "define_fx" => {
            "define_fx(\"$1\")\n\t.param(\"$2\", $3)\n\t.body(|input, $2| {\n\t\t$0\n\t})"
                .to_string()
        }
        "set_tempo" => "set_tempo($1)$0".to_string(),
        "set_quantization" => "set_quantization(${1:4.0})$0".to_string(),
        "set_time_signature" => "set_time_signature($1, $2)$0".to_string(),
        "db" => "db($1)$0".to_string(),
        "bars" => "bars($1)$0".to_string(),
        "note" => "note(\"${1:C4}\")$0".to_string(),
        "record" => "record(\"$1\")$0".to_string(),
        "chord" => "chord(\"${1:C}\", ${2:4})$0".to_string(),
        "scale" => "scale(\"$1\", \"$2\", $3)$0".to_string(),
        "envelope" => "envelope()$0".to_string(),
        // Filesystem extension (ext-fs)
        "read_file" => "read_file(\"$1\")$0".to_string(),
        "read_lines" => "read_lines(\"$1\")$0".to_string(),
        "write_file" => "write_file(\"$1\", $2)$0".to_string(),
        "append_file" => "append_file(\"$1\", $2)$0".to_string(),
        "file_exists" => "file_exists(\"$1\")$0".to_string(),
        "is_dir" => "is_dir(\"$1\")$0".to_string(),
        "is_file" => "is_file(\"$1\")$0".to_string(),
        "file_size" => "file_size(\"$1\")$0".to_string(),
        "list_dir" => "list_dir(\"$1\")$0".to_string(),
        "create_dir" => "create_dir(\"$1\")$0".to_string(),
        "create_dir_all" => "create_dir_all(\"$1\")$0".to_string(),
        "remove_dir" => "remove_dir(\"$1\")$0".to_string(),
        "remove_file" => "remove_file(\"$1\")$0".to_string(),
        "copy_file" => "copy_file(\"$1\", \"$2\")$0".to_string(),
        "rename_file" => "rename_file(\"$1\", \"$2\")$0".to_string(),
        "glob" => "glob(\"$1\")$0".to_string(),
        "path_join" => "path_join(\"$1\", \"$2\")$0".to_string(),
        "path_parent" => "path_parent(\"$1\")$0".to_string(),
        "path_filename" => "path_filename(\"$1\")$0".to_string(),
        "path_extension" => "path_extension(\"$1\")$0".to_string(),
        "path_stem" => "path_stem(\"$1\")$0".to_string(),
        // Shell execution extension (ext-exec)
        "exec" => "exec(\"$1\")$0".to_string(),
        "exec_status" => "exec_status(\"$1\")$0".to_string(),
        "exec_lines" => "exec_lines(\"$1\")$0".to_string(),
        "exec_with_args" => "exec_with_args(\"$1\", [$2])$0".to_string(),
        "exec_full" => "exec_full(\"$1\")$0".to_string(),
        "shell" => "shell(\"$1\")$0".to_string(),
        "env_var" => "env_var(\"$1\")$0".to_string(),
        "env_var_or" => "env_var_or(\"$1\", \"$2\")$0".to_string(),
        "set_env_var" => "set_env_var(\"$1\", \"$2\")$0".to_string(),
        "env_vars" => "env_vars()$0".to_string(),
        "cwd" => "cwd()$0".to_string(),
        "set_cwd" => "set_cwd(\"$1\")$0".to_string(),
        "pid" => "pid()$0".to_string(),
        // Networking extension (ext-net)
        "http_get" => "http_get(\"$1\")$0".to_string(),
        "http_get_lines" => "http_get_lines(\"$1\")$0".to_string(),
        "http_get_json" => "http_get_json(\"$1\")$0".to_string(),
        "http_post" => "http_post(\"$1\", $2)$0".to_string(),
        "http_post_json" => "http_post_json(\"$1\", #{ $2 })$0".to_string(),
        "url_encode" => "url_encode($1)$0".to_string(),
        "url_decode" => "url_decode($1)$0".to_string(),
        "parse_url" => "parse_url(\"$1\")$0".to_string(),
        "build_query_string" => "build_query_string(#{ $1 })$0".to_string(),
        "json_parse" => "json_parse($1)$0".to_string(),
        "json_stringify" => "json_stringify($1)$0".to_string(),
        _ => format!("{}($1)$0", name),
    }
}

/// Synthdef name completions.
fn get_synthdef_completions(
    known_synthdefs: &HashSet<String>,
    analysis: Option<&AnalysisResult>,
) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = known_synthdefs
        .iter()
        .map(|name| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("synthdef (stdlib)".to_string()),
            ..Default::default()
        })
        .collect();

    // Add local synthdefs from analysis
    if let Some(analysis) = analysis {
        for name in &analysis.local_synthdefs {
            if !known_synthdefs.contains(name) {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::CLASS),
                    detail: Some("synthdef (local)".to_string()),
                    ..Default::default()
                });
            }
        }
    }

    // Sort alphabetically
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

/// Effect name completions.
fn get_effect_completions(
    known_effects: &HashSet<String>,
    analysis: Option<&AnalysisResult>,
) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = known_effects
        .iter()
        .map(|name| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("effect (stdlib)".to_string()),
            ..Default::default()
        })
        .collect();

    // Add local effects from analysis
    if let Some(analysis) = analysis {
        for name in &analysis.local_effects {
            if !known_effects.contains(name) {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::CLASS),
                    detail: Some("effect (local)".to_string()),
                    ..Default::default()
                });
            }
        }
    }

    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

/// Import path completions.
fn get_import_completions(
    import_paths: &[PathBuf],
    _current_file: Option<&PathBuf>,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Scan import paths for .vibe files
    for base_path in import_paths {
        if let Ok(entries) = std::fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Suggest directory as import prefix
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        items.push(CompletionItem {
                            label: format!("{}/", name),
                            kind: Some(CompletionItemKind::FOLDER),
                            ..Default::default()
                        });
                    }
                } else if path.extension().is_some_and(|e| e == "vibe") {
                    // Suggest .vibe file
                    if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                        items.push(CompletionItem {
                            label: format!("{}.vibe", name),
                            kind: Some(CompletionItemKind::FILE),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    // Add common stdlib paths
    let stdlib_suggestions = [
        ("stdlib/drums/kicks/", "Kick drum synthdefs"),
        ("stdlib/drums/snares/", "Snare drum synthdefs"),
        ("stdlib/drums/hihats/", "Hi-hat synthdefs"),
        ("stdlib/bass/", "Bass synthdefs"),
        ("stdlib/leads/", "Lead synthdefs"),
        ("stdlib/pads/", "Pad synthdefs"),
        ("stdlib/effects/", "Effect synthdefs"),
    ];

    for (path, desc) in stdlib_suggestions {
        items.push(CompletionItem {
            label: path.to_string(),
            kind: Some(CompletionItemKind::FOLDER),
            detail: Some(desc.to_string()),
            ..Default::default()
        });
    }

    items
}

/// Parameter name completions.
fn get_param_completions(synthdef: Option<&str>) -> Vec<CompletionItem> {
    // Common parameters that most synthdefs have
    let common_params = vec![
        ("freq", "Frequency in Hz", 440.0),
        ("amp", "Amplitude (0-1)", 0.5),
        ("pan", "Pan position (-1 to 1)", 0.0),
        ("gate", "Gate signal", 1.0),
        ("attack", "Attack time in seconds", 0.01),
        ("decay", "Decay time in seconds", 0.1),
        ("sustain", "Sustain level (0-1)", 0.7),
        ("release", "Release time in seconds", 0.3),
        ("cutoff", "Filter cutoff frequency", 1000.0),
        ("resonance", "Filter resonance (Q)", 0.5),
        ("mix", "Dry/wet mix (0-1)", 0.5),
        ("room", "Room size (reverb)", 0.5),
        ("feedback", "Feedback amount", 0.5),
        ("time", "Time parameter (delay)", 0.25),
        ("rate", "Playback rate", 1.0),
        ("detune", "Detuning amount", 0.0),
    ];

    let _ = synthdef; // TODO: Get actual parameters from synthdef registry

    common_params
        .into_iter()
        .map(|(name, desc, default)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(format!("{} (default: {})", desc, default)),
            insert_text: Some(format!("\"{}\", ", name)),
            ..Default::default()
        })
        .collect()
}

/// Note and pattern syntax completions.
fn get_note_pattern_completions() -> Vec<CompletionItem> {
    let mut items = vec![
        // Pattern tokens
        CompletionItem {
            label: "x".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Trigger (normal velocity)".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "X".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Trigger (high velocity)".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: ".".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Rest".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "-".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Hold/sustain previous note".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "|".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Bar separator".to_string()),
            ..Default::default()
        },
    ];

    // Add common notes
    for octave in 2..=6 {
        for note in ["C", "D", "E", "F", "G", "A", "B"] {
            items.push(CompletionItem {
                label: format!("{}{}", note, octave),
                kind: Some(CompletionItemKind::VALUE),
                detail: Some(format!("{} (octave {})", note, octave)),
                ..Default::default()
            });
        }
    }

    // Add chord types
    let chord_types = [
        "maj", "min", "7", "maj7", "m7", "dim", "aug", "sus2", "sus4",
    ];
    for chord in chord_types {
        items.push(CompletionItem {
            label: format!(":{}", chord),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some(format!("{} chord", chord)),
            ..Default::default()
        });
    }

    items
}

/// Method chain completions based on object type.
fn get_method_completions(object_type: Option<&str>) -> Vec<CompletionItem> {
    get_api_method_docs()
        .iter()
        .filter(|method| object_type.is_none_or(|receiver| method.receiver == receiver))
        .map(method_item)
        .collect()
}

fn method_item(method: &ApiMethodDoc) -> CompletionItem {
    CompletionItem {
        label: method.name.clone(),
        kind: Some(CompletionItemKind::METHOD),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(method.signature.clone()),
            description: Some(method.receiver.clone()),
        }),
        detail: Some(format!(
            "{} Lifecycle: {}/{} ({}).",
            method.description,
            method.lifecycle.phase,
            method.lifecycle.terminal,
            method.lifecycle.classification
        )),
        insert_text: Some(format!("{}($1)$0", method.name)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        data: Some(serde_json::json!({
            "source": "public-api-manifest-v1",
            "receiver": method.receiver,
            "lifecycle": {
                "phase": method.lifecycle.phase,
                "terminal": method.lifecycle.terminal,
                "classification": method.lifecycle.classification,
            },
        })),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn synthdef_builder_includes_named_input_completions() {
        let methods = get_method_completions(Some("SynthDefBuilderHandle"));
        let input_methods: Vec<_> = methods
            .iter()
            .filter(|item| item.label == "input")
            .collect();

        assert_eq!(input_methods.len(), 2);
        assert!(input_methods.iter().any(|item| {
            item.label_details
                .as_ref()
                .and_then(|details| details.detail.as_deref())
                .is_some_and(|signature| {
                    signature
                        .contains("input(_: vibelang_dsp::api::SynthDefBuilderHandle, _: string)")
                })
        }));
        assert!(input_methods.iter().any(|item| {
            item.label_details
                .as_ref()
                .and_then(|details| details.detail.as_deref())
                .is_some_and(|signature| {
                    signature.contains(
                        "input(_: vibelang_dsp::api::SynthDefBuilderHandle, _: string, _: i64)",
                    )
                })
        }));
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct MethodProjection {
        receiver: String,
        name: String,
        signature: String,
        phase: String,
        terminal: String,
        classification: String,
    }

    fn expected_projection(receiver: Option<&str>) -> Vec<MethodProjection> {
        let mut expected = get_api_method_docs()
            .iter()
            .filter(|method| receiver.is_none_or(|receiver| method.receiver == receiver))
            .map(|method| MethodProjection {
                receiver: method.receiver.clone(),
                name: method.name.clone(),
                signature: method.signature.clone(),
                phase: method.lifecycle.phase.clone(),
                terminal: method.lifecycle.terminal.clone(),
                classification: method.lifecycle.classification.clone(),
            })
            .collect::<Vec<_>>();
        expected.sort();
        expected
    }

    fn active_projection(items: &[CompletionItem]) -> Result<Vec<MethodProjection>, String> {
        let mut active = items
            .iter()
            .map(|item| {
                let details = item
                    .label_details
                    .as_ref()
                    .ok_or_else(|| format!("{} has no label details", item.label))?;
                let data = item
                    .data
                    .as_ref()
                    .ok_or_else(|| format!("{} has no manifest data", item.label))?;
                let lifecycle = data
                    .get("lifecycle")
                    .ok_or_else(|| format!("{} has no lifecycle data", item.label))?;
                let string_field = |value: &serde_json::Value, field: &str| {
                    value
                        .get(field)
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .ok_or_else(|| format!("{} has no string `{field}`", item.label))
                };
                Ok(MethodProjection {
                    receiver: string_field(data, "receiver")?,
                    name: item.label.clone(),
                    signature: details
                        .detail
                        .clone()
                        .ok_or_else(|| format!("{} has no signature", item.label))?,
                    phase: string_field(lifecycle, "phase")?,
                    terminal: string_field(lifecycle, "terminal")?,
                    classification: string_field(lifecycle, "classification")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        active.sort();
        Ok(active)
    }

    fn validate_exact_projection(
        receiver: Option<&str>,
        items: &[CompletionItem],
    ) -> Result<(), String> {
        let expected = expected_projection(receiver);
        let active = active_projection(items)?;
        if active == expected {
            Ok(())
        } else {
            Err(format!(
                "active MethodChain projection does not exactly match manifest: expected {} rows, got {}",
                expected.len(),
                active.len()
            ))
        }
    }

    #[test]
    fn method_completions_exactly_match_manifest_receiver_overloads_and_lifecycle() {
        let receivers = get_api_method_docs()
            .iter()
            .map(|method| method.receiver.as_str())
            .collect::<BTreeSet<_>>();

        for receiver in receivers {
            validate_exact_projection(Some(receiver), &get_method_completions(Some(receiver)))
                .unwrap();
        }
        validate_exact_projection(None, &get_method_completions(None)).unwrap();
    }

    #[test]
    fn structural_negative_fixtures_reject_nonexistent_pairs_and_stale_rows() {
        let seed = get_api_method_docs().first().unwrap();
        let nonexistent_receiver = format!("{}__fixture", seed.receiver);
        let nonexistent_name = format!("{}__fixture", seed.name);
        assert!(get_api_method_docs().iter().all(|method| {
            method.receiver != nonexistent_receiver || method.name != nonexistent_name
        }));
        assert!(get_method_completions(Some(&nonexistent_receiver)).is_empty());

        let mut nonexistent_pair = get_method_completions(None);
        let mut invented = nonexistent_pair.first().unwrap().clone();
        invented.label.clone_from(&nonexistent_name);
        invented.data = Some(serde_json::json!({
            "source": "handwritten-fixture",
            "receiver": nonexistent_receiver,
            "lifecycle": {
                "phase": "fixture",
                "terminal": "fixture",
                "classification": "fixture",
            },
        }));
        nonexistent_pair.push(invented);
        assert!(validate_exact_projection(None, &nonexistent_pair).is_err());

        let mut stale_rows = get_method_completions(Some(&seed.receiver));
        let mut stale = stale_rows.first().unwrap().clone();
        stale.label = nonexistent_name;
        stale_rows.push(stale);
        assert!(validate_exact_projection(Some(&seed.receiver), &stale_rows).is_err());
    }
}
