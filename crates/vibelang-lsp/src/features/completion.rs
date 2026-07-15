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
use crate::data::{get_api_docs, get_ugen_completions};

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
    match object_type {
        Some("Voice") => get_voice_methods(),
        Some("Pattern") => get_pattern_methods(),
        Some("Melody") => get_melody_methods(),
        Some("Sequence") => get_sequence_methods(),
        Some("Fx") => get_fx_methods(),
        Some("Group") => get_group_methods(),
        Some("Fade") => get_fade_methods(),
        Some("Sample") => get_sample_methods(),
        Some("Record") => get_record_methods(),
        Some("SynthdefBuilder") => get_synthdef_builder_methods(),
        Some("FxBuilder") => get_fx_builder_methods(),
        Some("Envelope") => get_envelope_methods(),
        _ => {
            // Return common methods
            let mut methods = get_voice_methods();
            methods.extend(get_pattern_methods());
            methods.extend(get_melody_methods());
            methods
        }
    }
}

fn method_item(name: &str, signature: &str, description: &str) -> CompletionItem {
    method_item_with_snippet(name, signature, description, format!("{}($1)$0", name))
}

fn method_item_with_snippet(
    name: &str,
    signature: &str,
    description: &str,
    snippet: String,
) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::METHOD),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(signature.to_string()),
            description: None,
        }),
        detail: Some(description.to_string()),
        insert_text: Some(snippet),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

fn get_voice_methods() -> Vec<CompletionItem> {
    vec![
        method_item("synth", "(name: string)", "Set the synthdef to use"),
        method_item(
            "on",
            "(source)",
            "Set the sound source (synthdef, sample, or SFZ)",
        ),
        method_item("poly", "(count: int)", "Set polyphony count"),
        method_item("gain", "(level: float)", "Set gain level in dB"),
        method_item(
            "param",
            "(name: string, value: float)",
            "Set a synth parameter",
        ),
        method_item("pan", "(value: float)", "Set pan position (-1 to 1)"),
        method_item("group", "(path: string)", "Assign to a group"),
        method_item("channel", "(n: int)", "Set MIDI channel (0-15)"),
        method_item("mute", "()", "Mute the voice"),
        method_item("unmute", "()", "Unmute the voice"),
        method_item("solo", "()", "Solo the voice"),
        method_item("unsolo", "()", "Unsolo the voice"),
        method_item("apply", "()", "Apply the voice configuration"),
    ]
}

fn get_pattern_methods() -> Vec<CompletionItem> {
    vec![
        method_item("on", "(voice)", "Set the voice to trigger"),
        method_item(
            "on_voice",
            "(voice)",
            "Set the voice to trigger (by object)",
        ),
        method_item("step", "(pattern: string)", "Set step pattern (x/./-)"),
        method_item(
            "euclid",
            "(hits: int, steps: int)",
            "Generate Euclidean rhythm",
        ),
        method_item("len", "(beats: float)", "Set pattern length in beats"),
        method_item("swing", "(amount: float)", "Set swing amount (0-1)"),
        method_item(
            "set_param",
            "(key: string, value)",
            "Set per-step parameter",
        ),
        method_item("apply", "()", "Register pattern without starting"),
        method_item("start", "()", "Start pattern playback"),
        method_item("launch", "()", "Start pattern at next quantization"),
        method_item("stop", "()", "Stop pattern playback"),
    ]
}

fn get_melody_methods() -> Vec<CompletionItem> {
    vec![
        method_item("on", "(voice)", "Set the voice to play"),
        method_item("on_voice", "(voice)", "Set the voice to play (by object)"),
        method_item("notes", "(notes: string)", "Set note sequence"),
        method_item("scale", "(name: string)", "Set scale for degree notation"),
        method_item("root", "(note: string)", "Set root note for scale"),
        method_item("gate", "(duration: float)", "Set default gate duration"),
        method_item("transpose", "(semitones: int)", "Transpose notes"),
        method_item("len", "(beats: float)", "Set melody length in beats"),
        method_item("swing", "(amount: float)", "Set swing amount (0-1)"),
        method_item("add_note", "(beat, note, vel, dur)", "Add a single note"),
        method_item("add_chord", "(beat, notes, vel, dur)", "Add a chord"),
        method_item("apply", "()", "Register melody without starting"),
        method_item("start", "()", "Start melody playback"),
        method_item("launch", "()", "Start melody at next quantization"),
        method_item("stop", "()", "Stop melody playback"),
    ]
}

fn get_sequence_methods() -> Vec<CompletionItem> {
    vec![
        method_item("loop_bars", "(bars: int)", "Set loop length in bars"),
        method_item("loop_beats", "(beats: int)", "Set loop length in beats"),
        method_item("clip", "(range, source)", "Add a clip to the sequence"),
        method_item("clip_once", "(range, source)", "Add a one-shot clip"),
        method_item(
            "clip_loops",
            "(range, source, count)",
            "Add clip with loop count",
        ),
        method_item("apply", "()", "Register sequence without starting"),
        method_item("start", "()", "Start sequence playback"),
        method_item("stop", "()", "Stop sequence playback"),
    ]
}

fn get_fx_methods() -> Vec<CompletionItem> {
    vec![
        method_item("synth", "(name: string)", "Set the effect synthdef"),
        method_item(
            "param",
            "(name: string, value: float)",
            "Set an effect parameter",
        ),
        method_item("bypass", "(bypassed: bool)", "Bypass the effect"),
        method_item("apply", "()", "Apply the effect to the group"),
    ]
}

fn get_group_methods() -> Vec<CompletionItem> {
    vec![
        method_item(
            "body",
            "(closure)",
            "Evaluate content in this group; repeated bodies append/merge contributions for the same resolved group",
        ),
        method_item(
            "alias",
            "(name: string)",
            "Register an alternate group name",
        ),
        method_item("gain", "(level: float)", "Set group gain in dB"),
        method_item("pan", "(value: float)", "Set group pan"),
        method_item("mute", "()", "Mute the group"),
        method_item("unmute", "()", "Unmute the group"),
        method_item("solo", "(bool)", "Solo the group"),
        method_item("unsolo", "()", "Unsolo the group"),
        method_item("set_param", "(key: string, value)", "Set group parameter"),
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
        method_item(
            "over",
            "(duration: string)",
            "Duration string (e.g., '4bar')",
        ),
        method_item("over_bars", "(bars: float)", "Duration in bars"),
        method_item("over_beats", "(beats: float)", "Duration in beats"),
        method_item("curve", "(type: string)", "Fade curve type"),
        method_item("apply", "()", "Register fade without starting"),
        method_item("start", "()", "Start the fade"),
    ]
}

fn get_sample_methods() -> Vec<CompletionItem> {
    vec![
        method_item("attack", "(seconds: float)", "Envelope attack"),
        method_item("sustain", "(level: float)", "Envelope sustain level"),
        method_item("release", "(seconds: float)", "Envelope release"),
        method_item("amp", "(gain: float)", "Playback amplitude"),
        method_item("rate", "(multiplier: float)", "Playback rate"),
        method_item("loop_mode", "(enabled: bool)", "Enable looping"),
        method_item("offset", "(seconds: float)", "Start offset"),
        method_item("length", "(seconds: float)", "Playback length"),
        method_item("warp", "(enabled: bool)", "Enable time-stretch"),
        method_item("speed", "(value: float)", "Time-stretch speed"),
        method_item("pitch", "(multiplier: float)", "Pitch shift"),
        method_item("semitones", "(n: int)", "Pitch shift in semitones"),
        method_item("warp_to_bpm", "(bpm: float)", "Auto-stretch to BPM"),
        method_item("slice", "(start, end)", "Play slice only"),
    ]
}

fn get_record_methods() -> Vec<CompletionItem> {
    vec![
        method_item("bars", "(n: int)", "Duration in bars"),
        method_item("beats", "(n: int)", "Duration in beats"),
        method_item("seconds", "(n: float)", "Duration in seconds"),
        method_item("from_group", "(path: string)", "Source group"),
        method_item("count_in", "(bars: int)", "Count-in bars"),
        method_item("metronome", "(enabled: bool)", "Enable metronome"),
        method_item("to_file", "(path: string)", "Output file path"),
        method_item("channels", "(n: int)", "Channel count"),
        method_item("immediate", "()", "Start immediately"),
        method_item("apply", "()", "Start recording"),
    ]
}

fn get_synthdef_builder_methods() -> Vec<CompletionItem> {
    vec![
        method_item("param", "(name: string, default: float)", "Add a parameter"),
        method_item_with_snippet(
            "input",
            "(name: string)",
            "Declare a mono audio-rate named input",
            "input(\"$1\")$0".to_string(),
        ),
        method_item_with_snippet(
            "input",
            "(name: string, channels: int)",
            "Declare a mono/stereo audio-rate named input; channels must be 1 or 2",
            "input(\"$1\", ${2:2})$0".to_string(),
        ),
        method_item("body", "(closure)", "Set the DSP body"),
    ]
}

fn get_fx_builder_methods() -> Vec<CompletionItem> {
    vec![
        method_item("param", "(name: string, default: float)", "Add a parameter"),
        method_item(
            "body",
            "(closure)",
            "Set the effect DSP body (receives 'input')",
        ),
    ]
}

fn get_envelope_methods() -> Vec<CompletionItem> {
    vec![
        method_item("adsr", "(a, d, s, r)", "ADSR envelope"),
        method_item("asr", "(a, s, r)", "ASR envelope"),
        method_item("perc", "(attack, release)", "Percussive envelope"),
        method_item("triangle", "(dur)", "Triangle envelope"),
        method_item("gate", "(signal)", "Set gate signal"),
        method_item("cleanup_on_finish", "()", "Free synth when done"),
        method_item("time_scale", "(scale: float)", "Scale envelope time"),
        method_item("level_scale", "(scale: float)", "Scale envelope levels"),
        method_item("build", "()", "Build the envelope"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthdef_builder_includes_named_input_completions() {
        let methods = get_synthdef_builder_methods();
        let input_methods: Vec<_> = methods
            .iter()
            .filter(|item| item.label == "input")
            .collect();

        assert_eq!(input_methods.len(), 2);
        assert!(input_methods.iter().any(|item| {
            item.label_details
                .as_ref()
                .and_then(|details| details.detail.as_deref())
                == Some("(name: string)")
        }));
        assert!(input_methods.iter().any(|item| {
            item.label_details
                .as_ref()
                .and_then(|details| details.detail.as_deref())
                == Some("(name: string, channels: int)")
        }));
        assert!(input_methods.iter().all(|item| {
            item.detail
                .as_deref()
                .is_some_and(|detail| detail.contains("audio-rate"))
        }));
    }
}
