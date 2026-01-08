//! Script execution context.
//!
//! Provides thread-local storage for the current script state during execution.
//! Builder APIs use this context to collect their configuration.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
#[cfg(feature = "midi")]
use rhai::FnPtr;
use vibelang_core2::reload::ScriptState;
use vibelang_core2::types::{
    EffectId, GroupId, MelodyId, PatternId, RecordingId, SampleId, SequenceId, SfzId, VoiceId,
};

/// Macro to generate get_or_create_*_id and get_*_id functions.
///
/// This reduces boilerplate for the 9 nearly-identical ID getter functions.
macro_rules! define_id_accessors {
    ($get_or_create:ident, $get:ident, $id_type:ty, $counter:ident, $map:ident, $doc_create:literal, $doc_get:literal) => {
        #[doc = $doc_create]
        pub fn $get_or_create(name: &str) -> $id_type {
            CONTEXT.with(|ctx| {
                let mut borrow = ctx.borrow_mut();
                let c = borrow.as_mut().expect("Script context not initialized");
                if let Some(&id) = c.$map.get(name) {
                    id
                } else {
                    let id = <$id_type>::new(c.$counter);
                    c.$counter += 1;
                    c.$map.insert(name.to_string(), id);
                    id
                }
            })
        }

        #[doc = $doc_get]
        pub fn $get(name: &str) -> Option<$id_type> {
            CONTEXT.with(|ctx| {
                ctx.borrow()
                    .as_ref()
                    .and_then(|c| c.$map.get(name).copied())
            })
        }
    };
}

/// Source location in a script file.
#[derive(Clone, Debug, Default)]
pub struct SourceLocation {
    pub file: Option<PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl SourceLocation {
    pub fn new(file: Option<PathBuf>, line: Option<u32>, column: Option<u32>) -> Self {
        Self { file, line, column }
    }
}

/// Thread-local context for script execution.
struct ScriptContext {
    /// Current script state being built.
    state: ScriptState,

    /// Group path stack for nested define_group calls.
    group_stack: Vec<String>,

    /// Current script file path.
    current_file: Option<PathBuf>,

    /// Import paths for module resolution.
    import_paths: Vec<PathBuf>,

    /// ID counters for generating unique IDs.
    next_group_id: u32,
    next_voice_id: u32,
    next_pattern_id: u32,
    next_melody_id: u32,
    next_sequence_id: u32,
    next_effect_id: u32,
    next_sample_id: u32,
    next_sfz_id: u32,
    next_recording_id: u32,
    /// Counter for MIDI callback IDs.
    #[cfg(feature = "midi")]
    next_callback_id: u64,

    /// Name to ID mappings.
    group_ids: HashMap<String, GroupId>,
    voice_ids: HashMap<String, VoiceId>,
    pattern_ids: HashMap<String, PatternId>,
    melody_ids: HashMap<String, MelodyId>,
    sequence_ids: HashMap<String, SequenceId>,
    effect_ids: HashMap<String, EffectId>,
    sample_ids: HashMap<String, SampleId>,
    sfz_ids: HashMap<String, SfzId>,
    recording_ids: HashMap<String, RecordingId>,

    /// MIDI callback FnPtr storage (callback_id -> FnPtr).
    #[cfg(feature = "midi")]
    midi_callbacks: HashMap<u64, FnPtr>,
}

impl Default for ScriptContext {
    fn default() -> Self {
        Self {
            state: ScriptState::default(),
            group_stack: vec!["main".to_string()],
            current_file: None,
            import_paths: Vec::new(),
            next_group_id: 1, // Start at 1, 0 is reserved
            next_voice_id: 1,
            next_pattern_id: 1,
            next_melody_id: 1,
            next_sequence_id: 1,
            next_effect_id: 1,
            next_sample_id: 1,
            next_sfz_id: 1,
            next_recording_id: 1,
            #[cfg(feature = "midi")]
            next_callback_id: 1,
            group_ids: HashMap::new(),
            voice_ids: HashMap::new(),
            pattern_ids: HashMap::new(),
            melody_ids: HashMap::new(),
            sequence_ids: HashMap::new(),
            effect_ids: HashMap::new(),
            sample_ids: HashMap::new(),
            sfz_ids: HashMap::new(),
            recording_ids: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_callbacks: HashMap::new(),
        }
    }
}

thread_local! {
    static CONTEXT: RefCell<Option<ScriptContext>> = const { RefCell::new(None) };
}

/// Initialize the script context for execution.
pub fn init_context() {
    CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(ScriptContext::default());
    });
}

/// Clear the script context.
pub fn clear_context() {
    CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = None;
    });
}

/// Take the built script state.
pub fn take_state() -> ScriptState {
    CONTEXT.with(|ctx| {
        ctx.borrow_mut()
            .as_mut()
            .map(|c| std::mem::take(&mut c.state))
            .unwrap_or_default()
    })
}

/// Get the current group path.
pub fn current_group_path() -> String {
    CONTEXT.with(|ctx| {
        ctx.borrow()
            .as_ref()
            .map(|c| c.group_stack.join("/"))
            .unwrap_or_else(|| "main".to_string())
    })
}

/// Push a group onto the path stack.
pub fn push_group(name: &str) {
    CONTEXT.with(|ctx| {
        if let Some(c) = ctx.borrow_mut().as_mut() {
            c.group_stack.push(name.to_string());
        }
    });
}

/// Pop a group from the path stack.
pub fn pop_group() {
    CONTEXT.with(|ctx| {
        if let Some(c) = ctx.borrow_mut().as_mut() {
            if c.group_stack.len() > 1 {
                c.group_stack.pop();
            }
        }
    });
}

/// Set the current script file.
pub fn set_current_file(path: Option<PathBuf>) {
    CONTEXT.with(|ctx| {
        if let Some(c) = ctx.borrow_mut().as_mut() {
            c.current_file = path;
        }
    });
}

/// Get the current script file.
pub fn get_current_file() -> Option<PathBuf> {
    CONTEXT.with(|ctx| ctx.borrow().as_ref().and_then(|c| c.current_file.clone()))
}

/// Set import paths.
pub fn set_import_paths(paths: Vec<PathBuf>) {
    CONTEXT.with(|ctx| {
        if let Some(c) = ctx.borrow_mut().as_mut() {
            c.import_paths = paths;
        }
    });
}

// Generate ID accessor functions using the macro
define_id_accessors!(
    get_or_create_group_id, get_group_id, GroupId,
    next_group_id, group_ids,
    "Get or create a group ID by name.",
    "Get a group ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_voice_id, get_voice_id, VoiceId,
    next_voice_id, voice_ids,
    "Get or create a voice ID by name.",
    "Get a voice ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_pattern_id, get_pattern_id, PatternId,
    next_pattern_id, pattern_ids,
    "Get or create a pattern ID by name.",
    "Get a pattern ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_melody_id, get_melody_id, MelodyId,
    next_melody_id, melody_ids,
    "Get or create a melody ID by name.",
    "Get a melody ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_sequence_id, get_sequence_id, SequenceId,
    next_sequence_id, sequence_ids,
    "Get or create a sequence ID by name.",
    "Get a sequence ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_effect_id, get_effect_id, EffectId,
    next_effect_id, effect_ids,
    "Get or create an effect ID by name.",
    "Get an effect ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_sample_id, get_sample_id, SampleId,
    next_sample_id, sample_ids,
    "Get or create a sample ID by name.",
    "Get a sample ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_sfz_id, get_sfz_id, SfzId,
    next_sfz_id, sfz_ids,
    "Get or create an SFZ instrument ID by name.",
    "Get an SFZ ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_recording_id, get_recording_id, RecordingId,
    next_recording_id, recording_ids,
    "Get or create a recording ID by name.",
    "Get a recording ID by name (if it exists)."
);

/// Access the script state mutably.
pub fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut ScriptState) -> R,
{
    CONTEXT.with(|ctx| {
        let mut borrow = ctx.borrow_mut();
        let c = borrow.as_mut().expect("Script context not initialized");
        f(&mut c.state)
    })
}

/// Get tempo from state.
pub fn get_tempo() -> f64 {
    CONTEXT.with(|ctx| {
        ctx.borrow()
            .as_ref()
            .map(|c| c.state.tempo)
            .unwrap_or(120.0)
    })
}

/// Get time signature from state.
pub fn get_time_signature() -> (u8, u8) {
    CONTEXT.with(|ctx| {
        ctx.borrow()
            .as_ref()
            .map(|c| (c.state.time_sig.numerator, c.state.time_sig.denominator))
            .unwrap_or((4, 4))
    })
}

/// Get beats per bar from time signature.
pub fn beats_per_bar() -> f64 {
    let (num, denom) = get_time_signature();
    num as f64 * (4.0 / denom as f64)
}

// ============================================================================
// MIDI Callback Support
// ============================================================================

/// Register a MIDI callback and return its unique ID.
#[cfg(feature = "midi")]
pub fn register_midi_callback(callback: FnPtr) -> u64 {
    CONTEXT.with(|ctx| {
        let mut borrow = ctx.borrow_mut();
        let c = borrow.as_mut().expect("Script context not initialized");
        let id = c.next_callback_id;
        c.next_callback_id += 1;
        c.midi_callbacks.insert(id, callback);
        id
    })
}

/// Get a MIDI callback by ID.
#[cfg(feature = "midi")]
pub fn get_midi_callback(id: u64) -> Option<FnPtr> {
    CONTEXT.with(|ctx| {
        ctx.borrow()
            .as_ref()
            .and_then(|c| c.midi_callbacks.get(&id).cloned())
    })
}

/// Take all MIDI callbacks (clears them from context).
#[cfg(feature = "midi")]
pub fn take_midi_callbacks() -> HashMap<u64, FnPtr> {
    CONTEXT.with(|ctx| {
        ctx.borrow_mut()
            .as_mut()
            .map(|c| std::mem::take(&mut c.midi_callbacks))
            .unwrap_or_default()
    })
}
