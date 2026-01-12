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
    format_ugen_hover, get_all_ugen_functions, get_ugen_cache, get_ugen_completions, get_ugen_doc,
    get_ugen_signature_help, init_ugen_cache, to_snake_case, UGenDefinition, UGenInput,
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
            // ================================================================
            // Filesystem Extension (ext-fs)
            // ================================================================
            ApiFunctionDoc {
                name: "read_file",
                signature: "(path: string) -> string",
                description: "[ext-fs] Read entire file contents as a string.",
                example: "let content = read_file(\"notes.txt\");",
                parameters: &[("path", "string", "Path to the file to read")],
            },
            ApiFunctionDoc {
                name: "read_lines",
                signature: "(path: string) -> Array",
                description: "[ext-fs] Read file contents as an array of lines.",
                example: "let lines = read_lines(\"playlist.txt\");\nfor line in lines { print(line); }",
                parameters: &[("path", "string", "Path to the file to read")],
            },
            ApiFunctionDoc {
                name: "write_file",
                signature: "(path: string, content: string)",
                description: "[ext-fs] Write content to a file (creates or overwrites).",
                example: "write_file(\"output.txt\", \"Hello!\");",
                parameters: &[
                    ("path", "string", "Path to the file"),
                    ("content", "string", "Content to write"),
                ],
            },
            ApiFunctionDoc {
                name: "append_file",
                signature: "(path: string, content: string)",
                description: "[ext-fs] Append content to a file.",
                example: "append_file(\"log.txt\", \"Session started\\n\");",
                parameters: &[
                    ("path", "string", "Path to the file"),
                    ("content", "string", "Content to append"),
                ],
            },
            ApiFunctionDoc {
                name: "file_exists",
                signature: "(path: string) -> bool",
                description: "[ext-fs] Check if a file or directory exists.",
                example: "if file_exists(\"config.json\") { print(\"found\"); }",
                parameters: &[("path", "string", "Path to check")],
            },
            ApiFunctionDoc {
                name: "is_dir",
                signature: "(path: string) -> bool",
                description: "[ext-fs] Check if path is a directory.",
                example: "if is_dir(\"samples\") { let files = list_dir(\"samples\"); }",
                parameters: &[("path", "string", "Path to check")],
            },
            ApiFunctionDoc {
                name: "is_file",
                signature: "(path: string) -> bool",
                description: "[ext-fs] Check if path is a regular file.",
                example: "if is_file(\"song.vibe\") { print(\"found script\"); }",
                parameters: &[("path", "string", "Path to check")],
            },
            ApiFunctionDoc {
                name: "file_size",
                signature: "(path: string) -> int",
                description: "[ext-fs] Get file size in bytes.",
                example: "let size = file_size(\"audio.wav\");",
                parameters: &[("path", "string", "Path to the file")],
            },
            ApiFunctionDoc {
                name: "list_dir",
                signature: "(path: string) -> Array",
                description: "[ext-fs] List contents of a directory.",
                example: "let files = list_dir(\"samples/kicks\");\nfor f in files { print(f); }",
                parameters: &[("path", "string", "Directory path")],
            },
            ApiFunctionDoc {
                name: "create_dir",
                signature: "(path: string)",
                description: "[ext-fs] Create a directory.",
                example: "create_dir(\"output\");",
                parameters: &[("path", "string", "Directory path to create")],
            },
            ApiFunctionDoc {
                name: "create_dir_all",
                signature: "(path: string)",
                description: "[ext-fs] Create a directory and all parent directories.",
                example: "create_dir_all(\"output/renders/2024\");",
                parameters: &[("path", "string", "Directory path to create")],
            },
            ApiFunctionDoc {
                name: "remove_dir",
                signature: "(path: string)",
                description: "[ext-fs] Remove an empty directory.",
                example: "remove_dir(\"temp_folder\");",
                parameters: &[("path", "string", "Directory path to remove")],
            },
            ApiFunctionDoc {
                name: "remove_file",
                signature: "(path: string)",
                description: "[ext-fs] Remove a file.",
                example: "remove_file(\"temp.txt\");",
                parameters: &[("path", "string", "File path to remove")],
            },
            ApiFunctionDoc {
                name: "copy_file",
                signature: "(src: string, dst: string) -> int",
                description: "[ext-fs] Copy a file. Returns bytes copied.",
                example: "let bytes = copy_file(\"original.wav\", \"backup.wav\");",
                parameters: &[
                    ("src", "string", "Source file path"),
                    ("dst", "string", "Destination file path"),
                ],
            },
            ApiFunctionDoc {
                name: "rename_file",
                signature: "(src: string, dst: string)",
                description: "[ext-fs] Rename or move a file.",
                example: "rename_file(\"temp.wav\", \"final_mix.wav\");",
                parameters: &[
                    ("src", "string", "Current path"),
                    ("dst", "string", "New path"),
                ],
            },
            ApiFunctionDoc {
                name: "glob",
                signature: "(pattern: string) -> Array",
                description: "[ext-fs] Find files matching a glob pattern. Supports * (any chars), ** (recursive), ? (single char).",
                example: "let wavs = glob(\"**/*.wav\");",
                parameters: &[("pattern", "string", "Glob pattern (e.g., \"*.wav\", \"**/*.vibe\")")],
            },
            ApiFunctionDoc {
                name: "path_join",
                signature: "(base: string, path: string) -> string",
                description: "[ext-fs] Join path components.",
                example: "let full = path_join(\"samples\", \"kicks/kick_01.wav\");",
                parameters: &[
                    ("base", "string", "Base path"),
                    ("path", "string", "Path to append"),
                ],
            },
            ApiFunctionDoc {
                name: "path_parent",
                signature: "(path: string) -> string",
                description: "[ext-fs] Get parent directory of a path.",
                example: "let dir = path_parent(\"/home/user/song.vibe\");  // /home/user",
                parameters: &[("path", "string", "Path to parse")],
            },
            ApiFunctionDoc {
                name: "path_filename",
                signature: "(path: string) -> string",
                description: "[ext-fs] Get filename from a path.",
                example: "let name = path_filename(\"/path/to/song.wav\");  // song.wav",
                parameters: &[("path", "string", "Path to parse")],
            },
            ApiFunctionDoc {
                name: "path_extension",
                signature: "(path: string) -> string",
                description: "[ext-fs] Get file extension from a path.",
                example: "let ext = path_extension(\"song.vibe\");  // vibe",
                parameters: &[("path", "string", "Path to parse")],
            },
            ApiFunctionDoc {
                name: "path_stem",
                signature: "(path: string) -> string",
                description: "[ext-fs] Get filename without extension.",
                example: "let stem = path_stem(\"song.vibe\");  // song",
                parameters: &[("path", "string", "Path to parse")],
            },
            // ================================================================
            // Shell Execution Extension (ext-exec)
            // ================================================================
            ApiFunctionDoc {
                name: "exec",
                signature: "(command: string) -> string",
                description: "[ext-exec] Execute a command and return stdout as string.",
                example: "let output = exec(\"ls -la\");",
                parameters: &[("command", "string", "Command to execute (space-separated)")],
            },
            ApiFunctionDoc {
                name: "exec_status",
                signature: "(command: string) -> int",
                description: "[ext-exec] Execute a command and return exit status code.",
                example: "let status = exec_status(\"which ffmpeg\");\nif status == 0 { print(\"installed\"); }",
                parameters: &[("command", "string", "Command to execute")],
            },
            ApiFunctionDoc {
                name: "exec_lines",
                signature: "(command: string) -> Array",
                description: "[ext-exec] Execute a command and return stdout as array of lines.",
                example: "let files = exec_lines(\"ls\");\nfor f in files { print(f); }",
                parameters: &[("command", "string", "Command to execute")],
            },
            ApiFunctionDoc {
                name: "exec_with_args",
                signature: "(program: string, args: Array) -> string",
                description: "[ext-exec] Execute program with explicit arguments array (safer for special characters).",
                example: "let result = exec_with_args(\"echo\", [\"Hello\", \"World!\"]);",
                parameters: &[
                    ("program", "string", "Program to run"),
                    ("args", "Array", "Array of arguments"),
                ],
            },
            ApiFunctionDoc {
                name: "exec_full",
                signature: "(command: string) -> Map",
                description: "[ext-exec] Execute command and return full result with stdout, stderr, status, and success.",
                example: "let r = exec_full(\"make\");\nprint(r.stdout); print(r.stderr); print(r.status);",
                parameters: &[("command", "string", "Command to execute")],
            },
            ApiFunctionDoc {
                name: "shell",
                signature: "(script: string) -> string",
                description: "[ext-exec] Execute script through system shell. Supports pipes, redirects, and shell features.",
                example: "let count = shell(\"ls *.wav | wc -l\");",
                parameters: &[("script", "string", "Shell script to execute")],
            },
            ApiFunctionDoc {
                name: "env_var",
                signature: "(name: string) -> string",
                description: "[ext-exec] Get environment variable value.",
                example: "let home = env_var(\"HOME\");",
                parameters: &[("name", "string", "Variable name")],
            },
            ApiFunctionDoc {
                name: "env_var_or",
                signature: "(name: string, default: string) -> string",
                description: "[ext-exec] Get environment variable value with a default.",
                example: "let editor = env_var_or(\"EDITOR\", \"nano\");",
                parameters: &[
                    ("name", "string", "Variable name"),
                    ("default", "string", "Default value if not set"),
                ],
            },
            ApiFunctionDoc {
                name: "set_env_var",
                signature: "(name: string, value: string)",
                description: "[ext-exec] Set an environment variable.",
                example: "set_env_var(\"MY_CONFIG\", \"production\");",
                parameters: &[
                    ("name", "string", "Variable name"),
                    ("value", "string", "Value to set"),
                ],
            },
            ApiFunctionDoc {
                name: "env_vars",
                signature: "() -> Map",
                description: "[ext-exec] Get all environment variables as a map.",
                example: "let vars = env_vars();\nprint(vars[\"PATH\"]);",
                parameters: &[],
            },
            ApiFunctionDoc {
                name: "cwd",
                signature: "() -> string",
                description: "[ext-exec] Get current working directory.",
                example: "let current = cwd();",
                parameters: &[],
            },
            ApiFunctionDoc {
                name: "set_cwd",
                signature: "(path: string)",
                description: "[ext-exec] Change current working directory.",
                example: "set_cwd(\"/tmp\");",
                parameters: &[("path", "string", "Directory to change to")],
            },
            ApiFunctionDoc {
                name: "pid",
                signature: "() -> int",
                description: "[ext-exec] Get current process ID.",
                example: "let process_id = pid();",
                parameters: &[],
            },
            // ================================================================
            // Networking Extension (ext-net)
            // ================================================================
            ApiFunctionDoc {
                name: "http_get",
                signature: "(url: string) -> string",
                description: "[ext-net] Perform HTTP GET request and return body as string.",
                example: "let html = http_get(\"http://example.com\");",
                parameters: &[("url", "string", "URL to fetch (http:// only in basic version)")],
            },
            ApiFunctionDoc {
                name: "http_get_lines",
                signature: "(url: string) -> Array",
                description: "[ext-net] Perform HTTP GET and return body as array of lines.",
                example: "let lines = http_get_lines(\"http://example.com/data.txt\");",
                parameters: &[("url", "string", "URL to fetch")],
            },
            ApiFunctionDoc {
                name: "http_get_json",
                signature: "(url: string) -> Dynamic",
                description: "[ext-net] Perform HTTP GET and parse JSON response.",
                example: "let data = http_get_json(\"http://api.example.com/config\");",
                parameters: &[("url", "string", "URL to fetch")],
            },
            ApiFunctionDoc {
                name: "http_post",
                signature: "(url: string, body: string) -> string",
                description: "[ext-net] Perform HTTP POST with form data.",
                example: "let result = http_post(\"http://api.example.com/submit\", \"name=test\");",
                parameters: &[
                    ("url", "string", "URL to post to"),
                    ("body", "string", "Form data body"),
                ],
            },
            ApiFunctionDoc {
                name: "http_post_json",
                signature: "(url: string, data: Map) -> Dynamic",
                description: "[ext-net] POST JSON data and parse JSON response.",
                example: "let r = http_post_json(\"http://api.example.com/save\", #{ title: \"Song\" });",
                parameters: &[
                    ("url", "string", "URL to post to"),
                    ("data", "Map", "Data to send as JSON"),
                ],
            },
            ApiFunctionDoc {
                name: "url_encode",
                signature: "(s: string) -> string",
                description: "[ext-net] URL encode a string.",
                example: "let encoded = url_encode(\"hello world\");  // hello+world",
                parameters: &[("s", "string", "String to encode")],
            },
            ApiFunctionDoc {
                name: "url_decode",
                signature: "(s: string) -> string",
                description: "[ext-net] URL decode a string.",
                example: "let decoded = url_decode(\"hello+world\");  // hello world",
                parameters: &[("s", "string", "String to decode")],
            },
            ApiFunctionDoc {
                name: "parse_url",
                signature: "(url: string) -> Map",
                description: "[ext-net] Parse URL into components (scheme, host, port, path, query).",
                example: "let parts = parse_url(\"http://example.com:8080/path?q=1\");",
                parameters: &[("url", "string", "URL to parse")],
            },
            ApiFunctionDoc {
                name: "build_query_string",
                signature: "(params: Map) -> string",
                description: "[ext-net] Build URL query string from a map.",
                example: "let query = build_query_string(#{ search: \"test\", limit: 10 });",
                parameters: &[("params", "Map", "Map of parameters")],
            },
            ApiFunctionDoc {
                name: "json_parse",
                signature: "(json: string) -> Dynamic",
                description: "[ext-net] Parse JSON string into Rhai value.",
                example: "let data = json_parse(`{\"name\": \"test\"}`);",
                parameters: &[("json", "string", "JSON string to parse")],
            },
            ApiFunctionDoc {
                name: "json_stringify",
                signature: "(value: Dynamic) -> string",
                description: "[ext-net] Convert Rhai value to JSON string.",
                example: "let json = json_stringify(#{ tempo: 128 });",
                parameters: &[("value", "Dynamic", "Value to convert")],
            },
        ];

        docs.into_iter().map(|d| (d.name, d)).collect()
    })
}

/// Get documentation for a specific API function.
pub fn get_api_function_doc(name: &str) -> Option<&'static ApiFunctionDoc> {
    get_api_docs().get(name)
}
