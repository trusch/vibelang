//! Hover provider for VibeLang.
//!
//! Provides documentation on hover for:
//! - API functions
//! - UGen functions (oscillators, filters, effects, etc.)
//! - Synthdef names (with parameter info)
//! - Effect names
//! - Note names (with frequency info)

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

/// UGen input definition from manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct UGenInput {
    pub name: String,
    #[serde(rename = "type")]
    pub input_type: String,
    pub default: f64,
    pub description: String,
}

/// UGen definition from manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct UGenDefinition {
    pub name: String,
    pub description: String,
    pub rates: Vec<String>,
    pub inputs: Vec<UGenInput>,
    pub outputs: i32,
    pub category: String,
}

/// Static cache for loaded UGen definitions.
static UGEN_CACHE: OnceLock<HashMap<String, UGenDefinition>> = OnceLock::new();

/// Load UGen manifests from the given directory.
pub fn load_ugen_manifests(manifest_dir: &Path) -> HashMap<String, UGenDefinition> {
    let mut ugens = HashMap::new();

    if !manifest_dir.exists() {
        return ugens;
    }

    let entries = match fs::read_dir(manifest_dir) {
        Ok(e) => e,
        Err(_) => return ugens,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(defs) = serde_json::from_str::<Vec<UGenDefinition>>(&content) {
                    for def in defs {
                        // Map both the original name and snake_case + rate variants
                        for rate in &def.rates {
                            let func_name = format!("{}_{}", to_snake_case(&def.name), rate);
                            ugens.insert(func_name, def.clone());
                        }
                        // Also insert by original name (e.g., "SinOsc")
                        ugens.insert(def.name.clone(), def.clone());
                    }
                }
            }
        }
    }

    ugens
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Initialize UGen cache from workspace or extension path.
pub fn init_ugen_cache(workspace_root: Option<&Path>) {
    UGEN_CACHE.get_or_init(|| {
        // Try workspace path first
        if let Some(root) = workspace_root {
            let manifest_path = root.join("crates/vibelang-dsp/ugen_manifests");
            if manifest_path.exists() {
                return load_ugen_manifests(&manifest_path);
            }
        }
        HashMap::new()
    });
}

/// Get the cached UGen definitions.
pub fn get_ugen_cache() -> &'static HashMap<String, UGenDefinition> {
    UGEN_CACHE.get_or_init(HashMap::new)
}

/// Synthdef information for hover.
#[derive(Debug, Clone)]
pub struct SynthdefInfo {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Vec<ParamInfo>,
    pub category: Option<String>,
}

/// Parameter information.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub default: f64,
    pub description: Option<String>,
}

/// Get hover information for a word.
pub fn get_hover(
    word: &str,
    synthdef_info: &HashMap<String, SynthdefInfo>,
    effect_info: &HashMap<String, SynthdefInfo>,
) -> Option<Hover> {
    // Check if it's an API function
    if let Some(content) = get_api_function_hover(word) {
        return Some(Hover {
            contents: HoverContents::Markup(content),
            range: None,
        });
    }

    // Check if it's a UGen function
    if let Some(content) = get_ugen_hover(word) {
        return Some(Hover {
            contents: HoverContents::Markup(content),
            range: None,
        });
    }

    // Check if it's a synthdef
    if let Some(info) = synthdef_info.get(word) {
        return Some(Hover {
            contents: HoverContents::Markup(format_synthdef_hover(info)),
            range: None,
        });
    }

    // Check if it's an effect
    if let Some(info) = effect_info.get(word) {
        return Some(Hover {
            contents: HoverContents::Markup(format_effect_hover(info)),
            range: None,
        });
    }

    // Check if it's a note name
    if let Some(content) = get_note_hover(word) {
        return Some(Hover {
            contents: HoverContents::Markup(content),
            range: None,
        });
    }

    None
}

/// Get hover for UGen functions.
fn get_ugen_hover(name: &str) -> Option<MarkupContent> {
    let ugens = get_ugen_cache();
    let ugen = ugens.get(name)?;

    // Determine the rate suffix if present
    let rate = if name.ends_with("_ar") {
        Some("ar")
    } else if name.ends_with("_kr") {
        Some("kr")
    } else {
        None
    };

    let mut content = String::new();

    // Title with rate info
    if let Some(r) = rate {
        content.push_str(&format!(
            "### UGen: `{}` ({})\n\n",
            ugen.name,
            if r == "ar" {
                "audio rate"
            } else {
                "control rate"
            }
        ));
    } else {
        content.push_str(&format!("### UGen: `{}`\n\n", ugen.name));
    }

    // Description
    content.push_str(&ugen.description);
    content.push_str("\n\n");

    // Category
    content.push_str(&format!("**Category:** {}\n\n", ugen.category));

    // Available rates
    let rates_str = ugen
        .rates
        .iter()
        .map(|r| format!("`{}_{}`", to_snake_case(&ugen.name), r))
        .collect::<Vec<_>>()
        .join(", ");
    content.push_str(&format!("**Available as:** {}\n\n", rates_str));

    // Inputs/parameters
    if !ugen.inputs.is_empty() {
        content.push_str("**Parameters:**\n\n");
        for input in &ugen.inputs {
            content.push_str(&format!(
                "- `{}` ({}) — {} (default: {})\n",
                input.name, input.input_type, input.description, input.default
            ));
        }
        content.push('\n');
    }

    // Outputs
    content.push_str(&format!("**Outputs:** {}\n", ugen.outputs));

    // Example usage
    if !ugen.inputs.is_empty() {
        let func_name = if let Some(r) = rate {
            format!("{}_{}", to_snake_case(&ugen.name), r)
        } else if ugen.rates.contains(&"ar".to_string()) {
            format!("{}_ar", to_snake_case(&ugen.name))
        } else {
            format!("{}_{}", to_snake_case(&ugen.name), ugen.rates[0])
        };

        let args = ugen
            .inputs
            .iter()
            .map(|i| format!("{}", i.default))
            .collect::<Vec<_>>()
            .join(", ");

        content.push_str(&format!(
            "\n**Example:**\n```rhai\nlet sig = {}({});\n```",
            func_name, args
        ));
    }

    Some(MarkupContent {
        kind: MarkupKind::Markdown,
        value: content,
    })
}

/// Format synthdef hover information.
fn format_synthdef_hover(info: &SynthdefInfo) -> MarkupContent {
    let mut content = format!("### Synthdef: `{}`\n\n", info.name);

    if let Some(ref desc) = info.description {
        content.push_str(desc);
        content.push_str("\n\n");
    }

    if let Some(ref category) = info.category {
        content.push_str(&format!("**Category:** {}\n\n", category));
    }

    if !info.parameters.is_empty() {
        content.push_str("**Parameters:**\n\n");
        for param in &info.parameters {
            content.push_str(&format!(
                "- `{}` (default: {})",
                param.name, param.default
            ));
            if let Some(ref desc) = param.description {
                content.push_str(&format!(" - {}", desc));
            }
            content.push('\n');
        }
    }

    MarkupContent {
        kind: MarkupKind::Markdown,
        value: content,
    }
}

/// Format effect hover information.
fn format_effect_hover(info: &SynthdefInfo) -> MarkupContent {
    let mut content = format!("### Effect: `{}`\n\n", info.name);

    if let Some(ref desc) = info.description {
        content.push_str(desc);
        content.push_str("\n\n");
    }

    if let Some(ref category) = info.category {
        content.push_str(&format!("**Category:** {}\n\n", category));
    }

    if !info.parameters.is_empty() {
        content.push_str("**Parameters:**\n\n");
        for param in &info.parameters {
            content.push_str(&format!(
                "- `{}` (default: {})",
                param.name, param.default
            ));
            if let Some(ref desc) = param.description {
                content.push_str(&format!(" - {}", desc));
            }
            content.push('\n');
        }
    }

    MarkupContent {
        kind: MarkupKind::Markdown,
        value: content,
    }
}

/// Get hover for API functions.
fn get_api_function_hover(name: &str) -> Option<MarkupContent> {
    let docs: HashMap<&str, (&str, &str, &str)> = [
        (
            "voice",
            "Create a voice builder for a synth or sample voice.",
            "voice(name: string) -> Voice",
            "```rhai\nlet kick = voice(\"kick\")\n    .synth(\"kick_909\")\n    .gain(db(-6));\n```",
        ),
        (
            "pattern",
            "Create a rhythmic pattern builder with step notation.",
            "pattern(name: string) -> Pattern",
            "```rhai\npattern(\"kick\")\n    .on(kick)\n    .step(\"x...x...x...x...\")\n    .start();\n```",
        ),
        (
            "melody",
            "Create a melodic sequence builder.",
            "melody(name: string) -> Melody",
            "```rhai\nmelody(\"bass\")\n    .on(bass)\n    .notes(\"E1 - - - | G1 - - -\")\n    .start();\n```",
        ),
        (
            "sequence",
            "Create a sequence builder for arranging patterns and melodies over time.",
            "sequence(name: string) -> Sequence",
            "```rhai\nsequence(\"intro\")\n    .loop_bars(16)\n    .clip(0..bars(8), kick_pat)\n    .start();\n```",
        ),
        (
            "define_group",
            "Define a mixer group with hierarchical audio routing.",
            "define_group(name: string, body: fn) -> GroupHandle",
            "```rhai\ndefine_group(\"Drums\", || {\n    let kick = voice(\"kick\").synth(\"kick_909\");\n    pattern(\"kick_pat\").on(kick).step(\"x...\").start();\n});\n```",
        ),
        (
            "group",
            "Get a handle to an existing group by name.",
            "group(name: string) -> GroupHandle",
            "```rhai\ngroup(\"Drums\").mute().now();\ngroup(\"Drums\").gain(db(-3));\n```",
        ),
        (
            "fx",
            "Create an effect in the current group's FX chain.",
            "fx(name: string) -> Fx",
            "```rhai\nfx(\"reverb\")\n    .synth(\"reverb\")\n    .param(\"mix\", 0.3)\n    .apply();\n```",
        ),
        (
            "fade",
            "Create a parameter fade for smooth transitions.",
            "fade(name: string) -> FadeBuilder",
            "```rhai\nfade(\"intro\")\n    .on_group(\"Drums\")\n    .param(\"amp\")\n    .from(0).to(1)\n    .over_bars(8)\n    .start();\n```",
        ),
        (
            "sample",
            "Load an audio sample from a file.",
            "sample(name: string, path: string) -> SampleHandle",
            "```rhai\nlet kick = sample(\"kick\", \"samples/kick.wav\");\nvoice(\"kick_voice\").on(kick);\n```",
        ),
        (
            "load_sfz",
            "Load an SFZ instrument from a file.",
            "load_sfz(name: string, path: string) -> SfzInstrumentHandle",
            "```rhai\nlet piano = load_sfz(\"piano\", \"instruments/piano.sfz\");\nvoice(\"piano_voice\").on(piano).poly(8);\n```",
        ),
        (
            "define_synthdef",
            "Define a new synthesizer with parameters and DSP body.",
            "define_synthdef(name: string) -> SynthDefBuilder",
            "```rhai\ndefine_synthdef(\"sine\")\n    .param(\"freq\", 440.0)\n    .param(\"amp\", 0.5)\n    .body(|freq, amp| {\n        sin_ar(freq) * amp\n    });\n```",
        ),
        (
            "define_fx",
            "Define a new effect processor.",
            "define_fx(name: string) -> FxDefBuilder",
            "```rhai\ndefine_fx(\"my_delay\")\n    .param(\"time\", 0.25)\n    .param(\"feedback\", 0.5)\n    .body(|input, time, feedback| {\n        delay_ar(input, time) * feedback + input\n    });\n```",
        ),
        (
            "set_tempo",
            "Set the global tempo in BPM (beats per minute).",
            "set_tempo(bpm: float)",
            "```rhai\nset_tempo(128);\n```",
        ),
        (
            "set_quantization",
            "Set the global quantization grid for clip launches.",
            "set_quantization(grid: string)",
            "```rhai\nset_quantization(\"bar\");\nset_quantization(\"beat\");\nset_quantization(\"1/4\");\n```",
        ),
        (
            "set_time_signature",
            "Set the time signature.",
            "set_time_signature(numerator: int, denominator: int)",
            "```rhai\nset_time_signature(4, 4);\nset_time_signature(3, 4);\n```",
        ),
        (
            "db",
            "Convert decibels to linear amplitude. Use for all gain/volume parameters.",
            "db(value: float) -> float",
            "```rhai\nvoice(\"kick\").gain(db(-6));  // Half volume\nfx(\"comp\").param(\"threshold\", db(-12));\n```",
        ),
        (
            "bars",
            "Convert bars to beats (assuming 4/4 time). Essential for sequence clip ranges.",
            "bars(count: float) -> int",
            "```rhai\nsequence(\"intro\")\n    .clip(0..bars(8), pattern)\n    .clip(bars(8)..bars(16), melody);\n```",
        ),
        (
            "note",
            "Calculate note duration in beats as a fraction.",
            "note(numerator: int, denominator: int) -> float",
            "```rhai\nnote(1, 4)   // Quarter note = 0.25 beats\nnote(1, 16)  // Sixteenth note\nnote(3, 8)   // Dotted eighth\n```",
        ),
        // Method functions
        (
            "loop_bars",
            "[Sequence] Set the loop length in bars (4 beats per bar in 4/4).",
            ".loop_bars(bars: float) -> Sequence",
            "```rhai\nsequence(\"verse\").loop_bars(16).clip(...).start();\n```",
        ),
        (
            "loop_beats",
            "[Sequence] Set the loop length in beats.",
            ".loop_beats(beats: float) -> Sequence",
            "```rhai\nsequence(\"fill\").loop_beats(4).clip(...).start();\n```",
        ),
        (
            "clip",
            "[Sequence] Add a clip that loops within the time range. Use with bars() for ranges.",
            ".clip(range: Range, source: Pattern|Melody|Fade) -> Sequence",
            "```rhai\nsequence(\"main\").loop_bars(16)\n    .clip(0..bars(8), kick_pattern)\n    .clip(bars(4)..bars(16), bass_melody)\n    .start();\n```",
        ),
        (
            "clip_once",
            "[Sequence] Add a clip that plays once (no loop) within the time range.",
            ".clip_once(range: Range, source) -> Sequence",
            "```rhai\nsequence(\"intro\").loop_bars(8)\n    .clip_once(bars(6)..bars(8), fill_pattern)\n    .start();\n```",
        ),
        (
            "start",
            "[Pattern/Melody/Sequence] Start playback immediately.",
            ".start() -> Self",
            "```rhai\npattern(\"kick\").on(kick).step(\"x...\").start();\nsequence(\"intro\").loop_bars(16).clip(...).start();\n```",
        ),
        (
            "stop",
            "[Pattern/Melody/Sequence] Stop playback.",
            ".stop() -> Self",
            "```rhai\npattern(\"kick\").stop();\nmelody(\"bass\").stop();\n```",
        ),
        (
            "apply",
            "[Pattern/Melody/Fx] Register without starting. Required before use in sequences.",
            ".apply() -> Self",
            "```rhai\nlet kick_pat = pattern(\"kick\").on(kick).step(\"x...\").apply();\nsequence(\"main\").clip(0..bars(4), kick_pat).start();\n```",
        ),
        (
            "on",
            "[Voice/Pattern/Melody] Set the voice/synthdef/sample to use.",
            ".on(source) -> Self",
            "```rhai\npattern(\"kick\").on(kick_voice).step(\"x...\").start();\nmelody(\"bass\").on(bass_voice).notes(\"C2 E2\").start();\n```",
        ),
        (
            "synth",
            "[Voice/Fx] Set the synthdef to use by name.",
            ".synth(name: string) -> Self",
            "```rhai\nvoice(\"kick\").synth(\"kick_909\").gain(db(-6));\nfx(\"reverb\").synth(\"reverb\").param(\"room\", 0.5).apply();\n```",
        ),
        (
            "gain",
            "[Voice/GroupHandle] Set the volume/gain. Use db() for decibels.",
            ".gain(value: float) -> Self",
            "```rhai\nvoice(\"kick\").synth(\"kick\").gain(db(-6));\ngroup(\"Drums\").gain(db(-3));\n```",
        ),
        (
            "poly",
            "[Voice] Set the polyphony (number of simultaneous voices).",
            ".poly(count: int) -> Voice",
            "```rhai\nvoice(\"piano\").on(piano).poly(8);  // 8-voice polyphony\n```",
        ),
        (
            "step",
            "[Pattern] Set the step pattern string with x (hit), . (rest), - (sustain).",
            ".step(pattern: string) -> Pattern",
            "```rhai\npattern(\"kick\").on(kick).step(\"x...x...x...x...\");\npattern(\"hat\").on(hat).step(\"x.x.x.x. | X.x.X.x.\");\n```",
        ),
        (
            "euclid",
            "[Pattern] Generate a Euclidean rhythm with hits distributed over steps.",
            ".euclid(hits: int, steps: int) -> Pattern",
            "```rhai\npattern(\"perc\").on(perc).euclid(5, 8).start();  // 5 hits in 8 steps\n```",
        ),
        (
            "notes",
            "[Melody] Set the notes to play. Accepts string with | separators or array.",
            ".notes(notes: string | Array) -> Melody",
            "```rhai\nmelody(\"bass\").on(bass).notes(\"E1 - - - | G1 - - -\").start();\nmelody(\"arp\").on(synth).notes([\"C4\", \"E4\", \"G4\"]).start();\n```",
        ),
        (
            "scale",
            "[Melody] Set the scale for note quantization (major, minor, pentatonic, etc).",
            ".scale(name: string) -> Melody",
            "```rhai\nmelody(\"solo\").on(synth).scale(\"minor\").notes(...).start();\n```",
        ),
        (
            "root",
            "[Melody] Set the root note for the scale.",
            ".root(note: string) -> Melody",
            "```rhai\nmelody(\"solo\").on(synth).scale(\"minor\").root(\"E\").notes(...).start();\n```",
        ),
        (
            "gate",
            "[Melody] Set the gate duration for all notes (0.0-1.0).",
            ".gate(duration: float) -> Melody",
            "```rhai\nmelody(\"staccato\").on(synth).notes(\"C4 E4\").gate(0.3).start();\n```",
        ),
        (
            "transpose",
            "[Melody] Transpose all notes by semitones.",
            ".transpose(semitones: int) -> Melody",
            "```rhai\nmelody(\"bass\").on(bass).notes(\"C2 E2\").transpose(12).start();  // Up octave\n```",
        ),
        (
            "len",
            "[Pattern/Melody] Set the loop length in beats.",
            ".len(beats: float) -> Self",
            "```rhai\npattern(\"kick\").on(kick).step(\"x...\").len(4.0).start();\n```",
        ),
        (
            "swing",
            "[Pattern/Melody] Add swing timing (0.0-1.0).",
            ".swing(amount: float) -> Self",
            "```rhai\npattern(\"hat\").on(hat).step(\"x.x.x.x.\").swing(0.3).start();\n```",
        ),
        (
            "mute",
            "[Voice/GroupHandle] Mute the voice or group.",
            ".mute() -> MuteBuilder",
            "```rhai\ngroup(\"Drums\").mute().now();\ngroup(\"Bass\").mute().after(\"4bar\");\n```",
        ),
        (
            "unmute",
            "[GroupHandle] Unmute the group.",
            ".unmute() -> UnmuteBuilder",
            "```rhai\ngroup(\"Drums\").unmute().now();\n```",
        ),
        (
            "now",
            "[MuteBuilder/UnmuteBuilder] Execute the action immediately.",
            ".now()",
            "```rhai\ngroup(\"Drums\").mute().now();\ngroup(\"Bass\").unmute().now();\n```",
        ),
        (
            "after",
            "[MuteBuilder/UnmuteBuilder] Execute after a delay.",
            ".after(time: string)",
            "```rhai\ngroup(\"Drums\").mute().after(\"4bar\");\n```",
        ),
        (
            "param",
            "[Fx/FadeBuilder] Set a parameter value.",
            ".param(name: string, value?: float) -> Self",
            "```rhai\nfx(\"reverb\").synth(\"reverb\").param(\"room\", 0.6).apply();\nfade(\"intro\").on_group(\"Drums\").param(\"amp\").from(0).to(1).start();\n```",
        ),
        (
            "from",
            "[FadeBuilder] Set the starting value for the fade.",
            ".from(value: float) -> FadeBuilder",
            "```rhai\nfade(\"intro\").on_group(\"Drums\").param(\"amp\").from(db(-20)).to(db(0)).start();\n```",
        ),
        (
            "to",
            "[FadeBuilder] Set the target value for the fade.",
            ".to(value: float) -> FadeBuilder",
            "```rhai\nfade(\"outro\").on_group(\"All\").param(\"amp\").to(db(-40)).over_bars(8).start();\n```",
        ),
        (
            "over",
            "[FadeBuilder] Set the duration of the fade.",
            ".over(duration: float | string) -> FadeBuilder",
            "```rhai\nfade(\"slow\").on_group(\"Pad\").param(\"amp\").to(1).over(16.0).start();\nfade(\"fast\").over(\"2bar\").start();\n```",
        ),
        (
            "over_bars",
            "[FadeBuilder] Set the duration in bars.",
            ".over_bars(bars: float) -> FadeBuilder",
            "```rhai\nfade(\"intro\").on_group(\"Drums\").param(\"amp\").from(0).to(1).over_bars(8).start();\n```",
        ),
        (
            "on_group",
            "[FadeBuilder] Target a group for the fade.",
            ".on_group(name: string) -> FadeBuilder",
            "```rhai\nfade(\"intro\").on_group(\"Drums\").param(\"amp\").from(0).to(1).over_bars(8).start();\n```",
        ),
        (
            "on_voice",
            "[FadeBuilder] Target a voice for the fade.",
            ".on_voice(name: string) -> FadeBuilder",
            "```rhai\nfade(\"swell\").on_voice(\"pad\").param(\"amp\").from(0).to(1).over_bars(4).start();\n```",
        ),
        (
            "on_effect",
            "[FadeBuilder] Target an effect for the fade.",
            ".on_effect(name: string) -> FadeBuilder",
            "```rhai\nfade(\"sweep\").on_effect(\"moog\").param(\"cutoff\").from(200).to(8000).over_bars(4).start();\n```",
        ),
        (
            "trigger",
            "[Voice] Trigger the voice immediately.",
            ".trigger(params?: Map) -> Voice",
            "```rhai\nkick_voice.trigger();\nsnare_voice.trigger(#{\"amp\": 0.8});\n```",
        ),
        (
            "note_on",
            "[Voice] Send a MIDI-style note-on message.",
            ".note_on(note: int, velocity?: float) -> Voice",
            "```rhai\npiano.note_on(60, 100);  // Middle C\n```",
        ),
        (
            "note_off",
            "[Voice] Send a MIDI-style note-off message.",
            ".note_off(note: int) -> Voice",
            "```rhai\npiano.note_off(60);  // Release middle C\n```",
        ),
        (
            "bypass",
            "[Fx/Effect] Bypass the effect.",
            ".bypass(enabled: bool) -> Self",
            "```rhai\nfx(\"reverb\").synth(\"reverb\").bypass(true).apply();\n```",
        ),
        (
            "lane",
            "[Pattern/Melody] Create a parameter lane for per-step values.",
            ".lane(param: string) -> LaneBuilder",
            "```rhai\npattern(\"hat\").on(hat).step(\"x.x.x.x.\")\n    .lane(\"amp\").values([1.0, 0.5, 0.8, 0.5])\n    .start();\n```",
        ),
        (
            "values",
            "[LaneBuilder] Set the values for the parameter lane.",
            ".values(values: Array) -> Pattern | Melody",
            "```rhai\n.lane(\"amp\").values([1.0, 0.5, 0.8, 0.5])\n```",
        ),
    ]
    .into_iter()
    .map(|(k, desc, sig, ex)| (k, (desc, sig, ex)))
    .collect();

    docs.get(name).map(|(desc, sig, example)| MarkupContent {
        kind: MarkupKind::Markdown,
        value: format!(
            "### `{}`\n\n{}\n\n**Signature:** `{}`\n\n**Example:**\n{}",
            name, desc, sig, example
        ),
    })
}

/// Get hover for note names.
fn get_note_hover(note: &str) -> Option<MarkupContent> {
    // Parse note name like "C4", "F#3", "Bb5"
    let note_upper = note.to_uppercase();
    let chars: Vec<char> = note_upper.chars().collect();

    if chars.is_empty() {
        return None;
    }

    // Must start with A-G
    let base_note = chars[0];
    if !('A'..='G').contains(&base_note) {
        return None;
    }

    let mut idx = 1;
    let mut accidental = 0i32;

    // Check for accidental
    if idx < chars.len() {
        match chars[idx] {
            '#' => {
                accidental = 1;
                idx += 1;
            }
            'B' if idx + 1 < chars.len() || chars.len() == 2 => {
                // Could be Bb (B-flat) or just B with octave
                if idx + 1 < chars.len() && chars[idx + 1].is_ascii_digit() {
                    // B followed by digit, this is just the note B
                } else if chars.len() == 2 && chars[idx].is_ascii_digit() {
                    // B followed by digit
                } else {
                    accidental = -1;
                    idx += 1;
                }
            }
            _ => {}
        }
    }

    // Parse octave
    if idx >= chars.len() {
        return None;
    }

    let octave_str: String = chars[idx..].iter().collect();
    let octave: i32 = octave_str.parse().ok()?;

    // Calculate MIDI note number
    let base_semitone = match base_note {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    let midi_note = (octave + 1) * 12 + base_semitone + accidental;
    let frequency = 440.0 * 2.0_f64.powf((midi_note as f64 - 69.0) / 12.0);

    Some(MarkupContent {
        kind: MarkupKind::Markdown,
        value: format!(
            "### Note: `{}`\n\n- **MIDI Note:** {}\n- **Frequency:** {:.2} Hz\n- **Octave:** {}",
            note, midi_note, frequency, octave
        ),
    })
}
