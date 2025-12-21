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

use crate::analysis::CompletionContext;

/// API function documentation.
#[derive(Debug, Clone)]
pub struct ApiFunctionDoc {
    pub name: &'static str,
    pub signature: &'static str,
    pub description: &'static str,
    pub example: &'static str,
}

/// Get completions for a given context.
pub fn get_completions(
    context: &CompletionContext,
    known_synthdefs: &HashSet<String>,
    known_effects: &HashSet<String>,
    import_paths: &[PathBuf],
    current_file: Option<&PathBuf>,
) -> Vec<CompletionItem> {
    match context {
        CompletionContext::TopLevel => get_top_level_completions(),
        CompletionContext::SynthdefName => get_synthdef_completions(known_synthdefs, known_effects),
        CompletionContext::EffectName => get_effect_completions(known_effects),
        CompletionContext::ImportPath => get_import_completions(import_paths, current_file),
        CompletionContext::ParamName { synthdef } => get_param_completions(synthdef.as_deref()),
        CompletionContext::NotePattern => get_note_pattern_completions(),
        CompletionContext::MethodChain { object_type } => {
            get_method_completions(object_type.as_deref())
        }
        CompletionContext::Unknown => vec![],
    }
}

/// Top-level API function completions.
fn get_top_level_completions() -> Vec<CompletionItem> {
    let functions = get_api_functions();

    functions
        .into_iter()
        .map(|func| CompletionItem {
            label: func.name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(format!(" {}", func.signature)),
                description: None,
            }),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "{}\n\n**Example:**\n```rhai\n{}\n```",
                    func.description, func.example
                ),
            })),
            insert_text: Some(get_snippet_for_function(func.name)),
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
            "define_synthdef(\"$1\")\n\t.param(\"$2\", $3)\n\t.body(|$2| {\n\t\t$0\n\t})".to_string()
        }
        "define_fx" => {
            "define_fx(\"$1\")\n\t.param(\"$2\", $3)\n\t.body(|input, $2| {\n\t\t$0\n\t})"
                .to_string()
        }
        "set_tempo" => "set_tempo($1)$0".to_string(),
        "set_quantization" => "set_quantization(\"$1\")$0".to_string(),
        "set_time_signature" => "set_time_signature($1, $2)$0".to_string(),
        "db" => "db($1)$0".to_string(),
        "bars" => "bars($1)$0".to_string(),
        "note" => "note($1, $2)$0".to_string(),
        _ => format!("{}($1)$0", name),
    }
}

/// Synthdef name completions.
fn get_synthdef_completions(
    known_synthdefs: &HashSet<String>,
    known_effects: &HashSet<String>,
) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = known_synthdefs
        .iter()
        .map(|name| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("synthdef".to_string()),
            ..Default::default()
        })
        .collect();

    // Also include effects as they can be used with .synth()
    items.extend(known_effects.iter().map(|name| CompletionItem {
        label: name.clone(),
        kind: Some(CompletionItemKind::CLASS),
        detail: Some("effect".to_string()),
        ..Default::default()
    }));

    // Sort alphabetically
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

/// Effect name completions.
fn get_effect_completions(known_effects: &HashSet<String>) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = known_effects
        .iter()
        .map(|name| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("effect".to_string()),
            ..Default::default()
        })
        .collect();

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
                } else if path.extension().map(|e| e == "vibe").unwrap_or(false) {
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
        "stdlib/drums/kicks/",
        "stdlib/drums/snares/",
        "stdlib/drums/hihats/",
        "stdlib/bass/",
        "stdlib/leads/",
        "stdlib/pads/",
        "stdlib/effects/",
    ];

    for suggestion in stdlib_suggestions {
        items.push(CompletionItem {
            label: suggestion.to_string(),
            kind: Some(CompletionItemKind::FOLDER),
            detail: Some("stdlib".to_string()),
            ..Default::default()
        });
    }

    items
}

/// Parameter name completions based on synthdef.
fn get_param_completions(synthdef: Option<&str>) -> Vec<CompletionItem> {
    // Common parameters that most synthdefs have
    let common_params = vec![
        ("freq", "Frequency in Hz"),
        ("amp", "Amplitude (0-1)"),
        ("pan", "Pan position (-1 to 1)"),
        ("gate", "Gate signal"),
        ("attack", "Attack time in seconds"),
        ("decay", "Decay time in seconds"),
        ("sustain", "Sustain level (0-1)"),
        ("release", "Release time in seconds"),
        ("cutoff", "Filter cutoff frequency"),
        ("resonance", "Filter resonance (Q)"),
        ("mix", "Dry/wet mix (0-1)"),
        ("room", "Room size (reverb)"),
        ("feedback", "Feedback amount"),
        ("time", "Time parameter (delay, etc.)"),
        ("rate", "Playback rate"),
        ("detune", "Detuning amount"),
    ];

    // TODO: Get actual parameters from synthdef registry
    let _ = synthdef; // Suppress unused warning

    common_params
        .into_iter()
        .map(|(name, desc)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(desc.to_string()),
            insert_text: Some(format!("\"{}\", ", name)),
            ..Default::default()
        })
        .collect()
}

/// Note and pattern syntax completions.
fn get_note_pattern_completions() -> Vec<CompletionItem> {
    let mut items = vec![
        // Note names
        CompletionItem {
            label: "C4".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Middle C".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "E4".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("E above middle C".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "G4".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("G above middle C".to_string()),
            ..Default::default()
        },
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

    // Add octave range
    for octave in 0..=8 {
        for note in ["C", "D", "E", "F", "G", "A", "B"] {
            items.push(CompletionItem {
                label: format!("{}{}", note, octave),
                kind: Some(CompletionItemKind::VALUE),
                ..Default::default()
            });
        }
    }

    items
}

/// Method chain completions based on object type.
fn get_method_completions(object_type: Option<&str>) -> Vec<CompletionItem> {
    match object_type {
        Some("Voice") => get_voice_methods(),
        Some("Pattern") => get_pattern_methods(),
        Some("Melody") => get_melody_methods(),
        Some("Sequence") => get_sequence_methods(),
        Some("Fx") => get_fx_methods(),
        Some("Group") => get_group_methods(),
        Some("Fade") => get_fade_methods(),
        Some("Sample") => get_sample_methods(),
        _ => {
            // Return common methods
            let mut methods = get_voice_methods();
            methods.extend(get_pattern_methods());
            methods.extend(get_melody_methods());
            methods
        }
    }
}

fn get_voice_methods() -> Vec<CompletionItem> {
    vec![
        method_item("synth", "(name: string)", "Set the synthdef to use"),
        method_item("on", "(source)", "Set the sound source"),
        method_item("poly", "(count: int)", "Set polyphony"),
        method_item("gain", "(level: float)", "Set gain level"),
        method_item("param", "(name: string, value: float)", "Set a parameter"),
        method_item("pan", "(value: float)", "Set pan position (-1 to 1)"),
        method_item("send", "(bus: string, level: float)", "Send to aux bus"),
        method_item("apply", "()", "Apply the voice configuration"),
    ]
}

fn get_pattern_methods() -> Vec<CompletionItem> {
    vec![
        method_item("on", "(voice)", "Set the voice to trigger"),
        method_item("step", "(pattern: string)", "Set step pattern"),
        method_item("euclid", "(hits: int, steps: int)", "Generate Euclidean rhythm"),
        method_item("length", "(bars: float)", "Set pattern length in bars"),
        method_item("swing", "(amount: float)", "Set swing amount (0-1)"),
        method_item("velocity", "(v: float)", "Set velocity (0-1)"),
        method_item("probability", "(p: float)", "Set trigger probability (0-1)"),
        method_item("start", "()", "Start the pattern"),
        method_item("stop", "()", "Stop the pattern"),
    ]
}

fn get_melody_methods() -> Vec<CompletionItem> {
    vec![
        method_item("on", "(voice)", "Set the voice to play"),
        method_item("notes", "(notes: string)", "Set note sequence"),
        method_item("scale", "(name: string)", "Set scale"),
        method_item("root", "(note: string)", "Set root note"),
        method_item("gate", "(duration: float)", "Set gate duration"),
        method_item("transpose", "(semitones: int)", "Transpose notes"),
        method_item("length", "(bars: float)", "Set melody length in bars"),
        method_item("start", "()", "Start the melody"),
        method_item("stop", "()", "Stop the melody"),
    ]
}

fn get_sequence_methods() -> Vec<CompletionItem> {
    vec![
        method_item("loop_bars", "(bars: int)", "Set loop length in bars"),
        method_item("clip", "(range, source)", "Add a clip to the sequence"),
        method_item("clip_once", "(range, source)", "Add a one-shot clip"),
        method_item("start", "()", "Start the sequence"),
        method_item("stop", "()", "Stop the sequence"),
        method_item("pause", "()", "Pause the sequence"),
    ]
}

fn get_fx_methods() -> Vec<CompletionItem> {
    vec![
        method_item("synth", "(name: string)", "Set the effect synthdef"),
        method_item("param", "(name: string, value: float)", "Set a parameter"),
        method_item("bypass", "(bypassed: bool)", "Bypass the effect"),
        method_item("apply", "()", "Apply the effect to the group"),
    ]
}

fn get_group_methods() -> Vec<CompletionItem> {
    vec![
        method_item("gain", "(level: float)", "Set group gain"),
        method_item("pan", "(value: float)", "Set group pan"),
        method_item("mute", "()", "Mute the group"),
        method_item("unmute", "()", "Unmute the group"),
        method_item("solo", "()", "Solo the group"),
        method_item("unsolo", "()", "Unsolo the group"),
        method_item("send", "(bus: string, level: float)", "Send to aux bus"),
        method_item("now", "()", "Execute immediately"),
    ]
}

fn get_fade_methods() -> Vec<CompletionItem> {
    vec![
        method_item("on_group", "(name: string)", "Target a group"),
        method_item("on_voice", "(name: string)", "Target a voice"),
        method_item("on_effect", "(name: string)", "Target an effect"),
        method_item("param", "(name: string)", "Parameter to fade"),
        method_item("from", "(value: float)", "Starting value"),
        method_item("to", "(value: float)", "Ending value"),
        method_item("over_bars", "(bars: float)", "Duration in bars"),
        method_item("over", "(duration: string)", "Duration string"),
        method_item("curve", "(type: string)", "Fade curve type"),
        method_item("start", "()", "Start the fade"),
    ]
}

fn get_sample_methods() -> Vec<CompletionItem> {
    vec![
        method_item("warp_to_bpm", "(bpm: float)", "Time-stretch to BPM"),
        method_item("slice", "(count: int)", "Slice the sample"),
    ]
}

fn method_item(name: &str, signature: &str, description: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::METHOD),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(signature.to_string()),
            description: None,
        }),
        detail: Some(description.to_string()),
        insert_text: Some(format!("{}($1)$0", name)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

/// Get API function documentation.
fn get_api_functions() -> Vec<ApiFunctionDoc> {
    vec![
        ApiFunctionDoc {
            name: "set_tempo",
            signature: "(bpm: float)",
            description: "Set the global tempo in BPM (beats per minute).",
            example: "set_tempo(128);",
        },
        ApiFunctionDoc {
            name: "set_quantization",
            signature: "(grid: string)",
            description: "Set the global quantization grid.",
            example: "set_quantization(\"bar\");",
        },
        ApiFunctionDoc {
            name: "set_time_signature",
            signature: "(numerator: int, denominator: int)",
            description: "Set the time signature.",
            example: "set_time_signature(4, 4);",
        },
        ApiFunctionDoc {
            name: "voice",
            signature: "(name: string) -> Voice",
            description: "Create a voice builder for a synth or sample voice.",
            example: "let kick = voice(\"kick\").synth(\"kick_909\").gain(db(-6));",
        },
        ApiFunctionDoc {
            name: "pattern",
            signature: "(name: string) -> Pattern",
            description: "Create a rhythmic pattern builder with step notation.",
            example: "pattern(\"kick\").on(kick).step(\"x...x...x...x...\").start();",
        },
        ApiFunctionDoc {
            name: "melody",
            signature: "(name: string) -> Melody",
            description: "Create a melodic sequence builder.",
            example: "melody(\"bass\").on(bass).notes(\"E1 - - - | G1 - - -\").start();",
        },
        ApiFunctionDoc {
            name: "sequence",
            signature: "(name: string) -> Sequence",
            description: "Create a sequence builder for arranging patterns and melodies.",
            example: "sequence(\"intro\").loop_bars(16).clip(0..bars(8), kick_pat).start();",
        },
        ApiFunctionDoc {
            name: "define_group",
            signature: "(name: string, body: fn) -> GroupHandle",
            description: "Define a mixer group with hierarchical audio routing.",
            example: "define_group(\"Drums\", || {\n    // voices, patterns, fx...\n});",
        },
        ApiFunctionDoc {
            name: "group",
            signature: "(name: string) -> GroupHandle",
            description: "Get a handle to an existing group.",
            example: "group(\"Drums\").mute().now();",
        },
        ApiFunctionDoc {
            name: "fx",
            signature: "(name: string) -> Fx",
            description: "Create an effect in the current group's FX chain.",
            example: "fx(\"reverb\").synth(\"reverb\").param(\"mix\", 0.3).apply();",
        },
        ApiFunctionDoc {
            name: "fade",
            signature: "(name: string) -> FadeBuilder",
            description: "Create a parameter fade for smooth transitions.",
            example: "fade(\"intro\").on_group(\"Drums\").param(\"amp\").from(0).to(1).over_bars(8).start();",
        },
        ApiFunctionDoc {
            name: "sample",
            signature: "(name: string, path: string) -> SampleHandle",
            description: "Load an audio sample from a file.",
            example: "let kick = sample(\"kick\", \"samples/kick.wav\");",
        },
        ApiFunctionDoc {
            name: "load_sfz",
            signature: "(name: string, path: string) -> SfzInstrumentHandle",
            description: "Load an SFZ instrument from a file.",
            example: "let piano = load_sfz(\"piano\", \"instruments/piano.sfz\");",
        },
        ApiFunctionDoc {
            name: "define_synthdef",
            signature: "(name: string) -> SynthDefBuilder",
            description: "Define a new synthesizer with parameters and DSP body.",
            example: "define_synthdef(\"sine\").param(\"freq\", 440.0).body(|freq| sin_ar(freq));",
        },
        ApiFunctionDoc {
            name: "define_fx",
            signature: "(name: string) -> FxDefBuilder",
            description: "Define a new effect processor.",
            example: "define_fx(\"my_reverb\").param(\"mix\", 0.3).body(|input, mix| ...);",
        },
        ApiFunctionDoc {
            name: "db",
            signature: "(value: float) -> float",
            description: "Convert decibels to linear amplitude.",
            example: "voice(\"kick\").gain(db(-6));  // Half volume",
        },
        ApiFunctionDoc {
            name: "bars",
            signature: "(count: float) -> int",
            description: "Convert bars to beats (assuming 4/4 time).",
            example: "sequence(\"s\").clip(0..bars(8), pattern);",
        },
        ApiFunctionDoc {
            name: "note",
            signature: "(numerator: int, denominator: int) -> float",
            description: "Calculate note duration in beats as a fraction.",
            example: "note(1, 4)  // Quarter note = 0.25 beats",
        },
    ]
}
