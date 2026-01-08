//! Data structures and caching for the LSP.
//!
//! This module provides:
//! - Document store for managing open files
//! - UGen manifest cache
//! - Synthdef information cache

mod document_store;
mod ugen_cache;

pub use document_store::DocumentStore;
pub use ugen_cache::{
    format_ugen_hover, get_all_ugen_functions, get_ugen_cache, get_ugen_completions,
    get_ugen_doc, get_ugen_signature_help, init_ugen_cache, to_snake_case, UGenDefinition, UGenInput,
};

use std::collections::HashMap;
use std::sync::OnceLock;

/// Synthdef information for hover and completion.
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

/// API function documentation.
#[derive(Debug, Clone)]
pub struct ApiFunctionDoc {
    pub name: &'static str,
    pub signature: &'static str,
    pub description: &'static str,
    pub example: &'static str,
    pub parameters: &'static [(&'static str, &'static str, &'static str)], // (name, type, description)
}

/// Static cache for API documentation.
static API_DOCS: OnceLock<HashMap<&'static str, ApiFunctionDoc>> = OnceLock::new();

/// Get API function documentation.
pub fn get_api_docs() -> &'static HashMap<&'static str, ApiFunctionDoc> {
    API_DOCS.get_or_init(|| {
        let docs = vec![
            ApiFunctionDoc {
                name: "set_tempo",
                signature: "(bpm: float)",
                description: "Set the global tempo in BPM (beats per minute).",
                example: "set_tempo(128);",
                parameters: &[("bpm", "float", "Tempo in beats per minute (20-999)")],
            },
            ApiFunctionDoc {
                name: "set_quantization",
                signature: "(grid: string)",
                description: "Set the global quantization grid for clip launches.",
                example: "set_quantization(\"bar\");",
                parameters: &[("grid", "string", "Quantization grid: \"bar\", \"beat\", \"1/2\", \"1/4\", \"1/8\", \"1/16\"")],
            },
            ApiFunctionDoc {
                name: "set_time_signature",
                signature: "(numerator: int, denominator: int)",
                description: "Set the time signature.",
                example: "set_time_signature(4, 4);",
                parameters: &[
                    ("numerator", "int", "Beats per bar"),
                    ("denominator", "int", "Note value (4 = quarter, 8 = eighth)"),
                ],
            },
            ApiFunctionDoc {
                name: "voice",
                signature: "(name: string) -> Voice",
                description: "Create a voice builder for a synth or sample voice. Voices are the sound sources that patterns and melodies trigger.",
                example: "let kick = voice(\"kick\").synth(\"kick_909\").gain(db(-6));",
                parameters: &[("name", "string", "Unique name for this voice")],
            },
            ApiFunctionDoc {
                name: "pattern",
                signature: "(name: string) -> Pattern",
                description: "Create a rhythmic pattern builder. Patterns trigger voices at specified beat positions using step notation.",
                example: "pattern(\"kick\").on(kick).step(\"x...x...x...x...\").start();",
                parameters: &[("name", "string", "Unique name for this pattern")],
            },
            ApiFunctionDoc {
                name: "melody",
                signature: "(name: string) -> Melody",
                description: "Create a melodic sequence builder. Melodies play pitched notes on voices.",
                example: "melody(\"bass\").on(bass).notes(\"E1 - - - | G1 - - -\").start();",
                parameters: &[("name", "string", "Unique name for this melody")],
            },
            ApiFunctionDoc {
                name: "sequence",
                signature: "(name: string) -> Sequence",
                description: "Create a sequence builder for arranging patterns, melodies, and fades over time.",
                example: "sequence(\"intro\").loop_bars(16).clip(0..bars(8), kick_pat).start();",
                parameters: &[("name", "string", "Unique name for this sequence")],
            },
            ApiFunctionDoc {
                name: "define_group",
                signature: "(name: string, body: fn) -> GroupHandle",
                description: "Define a mixer group with hierarchical audio routing. All voices and effects inside are routed through the group.",
                example: "define_group(\"Drums\", || {\n    let kick = voice(\"kick\").synth(\"kick_909\");\n});",
                parameters: &[
                    ("name", "string", "Group name"),
                    ("body", "fn", "Closure containing group contents"),
                ],
            },
            ApiFunctionDoc {
                name: "group",
                signature: "(name: string) -> GroupHandle",
                description: "Get a handle to an existing group by name.",
                example: "group(\"Drums\").mute().now();",
                parameters: &[("name", "string", "Name of existing group")],
            },
            ApiFunctionDoc {
                name: "fx",
                signature: "(name: string) -> Fx",
                description: "Create an effect in the current group's FX chain.",
                example: "fx(\"reverb\").synth(\"reverb\").param(\"mix\", 0.3).apply();",
                parameters: &[("name", "string", "Effect name")],
            },
            ApiFunctionDoc {
                name: "fade",
                signature: "(name: string) -> FadeBuilder",
                description: "Create a parameter fade for smooth transitions.",
                example: "fade(\"intro\").on_group(\"Drums\").param(\"amp\").from(0).to(1).over_bars(8).start();",
                parameters: &[("name", "string", "Fade name")],
            },
            ApiFunctionDoc {
                name: "sample",
                signature: "(name: string, path: string) -> SampleHandle",
                description: "Load an audio sample from a file.",
                example: "let kick = sample(\"kick\", \"samples/kick.wav\");",
                parameters: &[
                    ("name", "string", "Sample identifier"),
                    ("path", "string", "Path to audio file"),
                ],
            },
            ApiFunctionDoc {
                name: "load_sfz",
                signature: "(name: string, path: string) -> SfzInstrumentHandle",
                description: "Load an SFZ instrument from a file.",
                example: "let piano = load_sfz(\"piano\", \"instruments/piano.sfz\");",
                parameters: &[
                    ("name", "string", "Instrument identifier"),
                    ("path", "string", "Path to SFZ file"),
                ],
            },
            ApiFunctionDoc {
                name: "define_synthdef",
                signature: "(name: string) -> SynthDefBuilder",
                description: "Define a new synthesizer with parameters and DSP body.",
                example: "define_synthdef(\"sine\").param(\"freq\", 440.0).body(|freq| sin_ar(freq));",
                parameters: &[("name", "string", "Synthdef name")],
            },
            ApiFunctionDoc {
                name: "define_fx",
                signature: "(name: string) -> FxDefBuilder",
                description: "Define a new effect processor with parameters and DSP body.",
                example: "define_fx(\"my_reverb\").param(\"mix\", 0.3).body(|input, mix| free_verb_ar(input, mix, 0.5, 0.5));",
                parameters: &[("name", "string", "Effect name")],
            },
            ApiFunctionDoc {
                name: "db",
                signature: "(value: float) -> float",
                description: "Convert decibels to linear amplitude. Use for all gain/volume parameters.",
                example: "voice(\"kick\").gain(db(-6));  // Half volume",
                parameters: &[("value", "float", "dB value (negative = quieter)")],
            },
            ApiFunctionDoc {
                name: "bars",
                signature: "(count: float) -> int",
                description: "Convert bars to beats (based on time signature).",
                example: "sequence(\"s\").clip(0..bars(8), pattern);",
                parameters: &[("count", "float", "Number of bars")],
            },
            ApiFunctionDoc {
                name: "note",
                signature: "(numerator: int, denominator: int) -> float",
                description: "Calculate note duration in beats as a fraction.",
                example: "note(1, 4)  // Quarter note = 1 beat in 4/4",
                parameters: &[
                    ("numerator", "int", "Fraction numerator"),
                    ("denominator", "int", "Fraction denominator"),
                ],
            },
            ApiFunctionDoc {
                name: "record",
                signature: "(id: string) -> RecordHandle",
                description: "Create a recording builder for capturing audio.",
                example: "let take = record(\"take1\").from_group(\"main\").bars(4).apply();",
                parameters: &[("id", "string", "Recording identifier")],
            },
            ApiFunctionDoc {
                name: "chord",
                signature: "(root: string, quality: string) -> Array",
                description: "Generate chord notes from a root and quality.",
                example: "let notes = chord(\"C4\", \"maj7\");  // [\"C4\", \"E4\", \"G4\", \"B4\"]",
                parameters: &[
                    ("root", "string", "Root note (e.g., \"C4\")"),
                    ("quality", "string", "Chord quality (e.g., \"maj7\", \"m7\", \"dim\")"),
                ],
            },
            ApiFunctionDoc {
                name: "scale",
                signature: "(root: string, scale_type: string, octaves: int) -> Array",
                description: "Generate scale notes from a root, type, and number of octaves.",
                example: "let notes = scale(\"C4\", \"minor\", 2);",
                parameters: &[
                    ("root", "string", "Root note"),
                    ("scale_type", "string", "Scale type (e.g., \"major\", \"minor\", \"dorian\")"),
                    ("octaves", "int", "Number of octaves"),
                ],
            },
            ApiFunctionDoc {
                name: "envelope",
                signature: "() -> EnvelopeBuilder",
                description: "Create an envelope builder for amplitude or filter envelopes.",
                example: "let env = envelope().adsr(\"10ms\", \"100ms\", 0.5, \"200ms\").build();",
                parameters: &[],
            },
        ];

        docs.into_iter().map(|d| (d.name, d)).collect()
    })
}

/// Get documentation for a specific API function.
pub fn get_api_function_doc(name: &str) -> Option<&'static ApiFunctionDoc> {
    get_api_docs().get(name)
}
