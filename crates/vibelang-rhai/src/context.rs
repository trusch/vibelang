//! Script execution context.
//!
//! Provides thread-local storage for the current script state during execution.
//! Builder APIs use this context to collect their configuration.
//!
//! ## ID Generation
//!
//! Entity IDs (groups, voices, effects, etc.) are generated using a hash of the
//! entity name. This ensures IDs are **stable across script reloads** - the same
//! name always produces the same ID, regardless of script order.
//!
//! This is critical for live reloading: if IDs shifted based on encounter order,
//! adding a new entity early in the script would cause all subsequent entities
//! to be seen as "changed" during diff calculation, leading to unnecessary
//! recreations and audio glitches.

#[cfg(feature = "midi")]
use rhai::FnPtr;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use vibelang_core::reload::{BodyContribution, GroupAliasError, GroupAliasTarget, ScriptState};
use vibelang_core::types::{
    EffectId, FadeId, GroupId, MelodyId, PatternId, RecordingId, SampleId, SequenceId, SfzId,
    VoiceId,
};

// The canonical name→id derivation lives in vibelang-core so that external
// clients (e.g. the HTTP layer) can resolve names to ids identically.
use vibelang_core::hash_name_to_id;

/// Macro to generate get_or_create_*_id and get_*_id functions.
///
/// This reduces boilerplate for the 9 nearly-identical ID getter functions.
/// IDs are generated using a hash of the entity name for stability across reloads.
macro_rules! define_id_accessors {
    ($get_or_create:ident, $get:ident, $id_type:ty, $counter:ident, $map:ident, $doc_create:literal, $doc_get:literal) => {
        #[doc = $doc_create]
        ///
        /// The ID is generated from a hash of the name, ensuring stability across script reloads.
        /// If a hash collision is detected (different name maps to same ID), the ID is
        /// incremented via linear probing until a free slot is found.
        pub fn $get_or_create(name: &str) -> $id_type {
            CONTEXT.with(|ctx| {
                let mut borrow = ctx.borrow_mut();
                let c = borrow.as_mut().expect("Script context not initialized");
                if let Some(&id) = c.$map.get(name) {
                    id
                } else {
                    // Use hash-based ID for stability across reloads
                    let mut raw = hash_name_to_id(name);
                    // Collision detection: check if another name already has this ID
                    loop {
                        let candidate = <$id_type>::new(raw);
                        let collision =
                            c.$map.values().any(|&existing_id| existing_id == candidate);
                        if !collision {
                            break;
                        }
                        tracing::warn!(
                            "Hash collision detected: '{}' collides at ID {} — probing next slot",
                            name,
                            raw
                        );
                        raw = raw.wrapping_add(1);
                        if raw == 0 {
                            raw = 1;
                        } // Never use 0
                    }
                    let id = <$id_type>::new(raw);
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

    /// Monotonic body contribution ordinal for this script execution.
    next_body_contribution_ordinal: u64,

    /// Stack of active body contribution ids.
    active_body_contributions: Vec<u64>,

    /// Import paths for module resolution.
    import_paths: Vec<PathBuf>,

    /// Auto-name collision counters for anonymous entities.
    /// Maps base name → count. Reset on each script execution.
    auto_name_counts: HashMap<String, u32>,

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
    next_fade_id: u32,
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
    fade_ids: HashMap<String, FadeId>,

    /// MIDI callback FnPtr storage (callback_id -> FnPtr).
    #[cfg(feature = "midi")]
    midi_callbacks: HashMap<u64, FnPtr>,
}

impl Default for ScriptContext {
    fn default() -> Self {
        Self {
            state: ScriptState::default(),
            group_stack: Vec::new(),
            current_file: None,
            next_body_contribution_ordinal: 0,
            active_body_contributions: Vec::new(),
            import_paths: Vec::new(),
            auto_name_counts: HashMap::new(),
            next_group_id: 1, // Start at 1, 0 is reserved
            next_voice_id: 1,
            next_pattern_id: 1,
            next_melody_id: 1,
            next_sequence_id: 1,
            next_effect_id: 1,
            next_sample_id: 1,
            next_sfz_id: 1,
            next_recording_id: 1,
            next_fade_id: 1,
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
            fade_ids: HashMap::new(),
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
        let mut state = ctx
            .borrow_mut()
            .as_mut()
            .map(|c| std::mem::take(&mut c.state))
            .unwrap_or_default();

        // Record deploy hashes for every script-deployed synthdef referenced
        // by a voice or effect. The reload differ compares these against the
        // hashes snapshotted on the last applied reload to detect body-only
        // synthdef edits (same name/params, different compiled graph), which
        // must structurally recreate dependent voices/effects. Names without
        // a registry hash (builtins, sample voices) are simply not recorded.
        //
        // The deploy hash, not the raw content hash: a def the server refused
        // to instantiate and that was re-sent has unchanged source bytes but
        // lost its voices with it, so it has to read as changed exactly once.
        let referenced: std::collections::HashSet<String> = state
            .voices
            .values()
            .map(|v| v.synthdef.clone())
            .chain(state.effects.values().map(|e| e.synthdef.clone()))
            .filter(|name| !name.is_empty())
            .collect();
        for name in referenced {
            if let Some(hash) = vibelang_dsp::get_synthdef_deploy_hash(&name) {
                state.synthdef_hashes.insert(name, hash);
            }
        }

        tracing::debug!(
            "take_state: melodies={}, playing_melodies={}, voices={}, groups={}",
            state.melodies.len(),
            state.playing_melodies.len(),
            state.voices.len(),
            state.groups.len()
        );

        // Log melody details
        for (id, config) in &state.melodies {
            tracing::debug!(
                "take_state: melody {:?} '{}': voice={:?}, notes={}, length={}",
                id,
                config.name,
                config.voice,
                config.notes.len(),
                config.length.to_f64()
            );
        }

        state
    })
}

/// Get the current group path.
///
/// Returns an empty string when no `define_group` scope is active.
/// Top-level definitions live at the root and have no implicit prefix.
pub fn current_group_path() -> String {
    CONTEXT.with(|ctx| {
        ctx.borrow()
            .as_ref()
            .map(|c| c.group_stack.join("/"))
            .unwrap_or_default()
    })
}

/// Execute a closure with the current group path temporarily replaced.
pub fn with_group_path<R>(path: &str, f: impl FnOnce() -> R) -> R {
    let previous = CONTEXT.with(|ctx| {
        let mut borrow = ctx.borrow_mut();
        let c = borrow.as_mut().expect("Script context not initialized");
        let previous = c.group_stack.clone();
        c.group_stack = path.split('/').map(str::to_string).collect();
        previous
    });

    let result = f();

    CONTEXT.with(|ctx| {
        if let Some(c) = ctx.borrow_mut().as_mut() {
            c.group_stack = previous;
        }
    });

    result
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

/// Record an ordered body contribution and mark it as active.
pub fn begin_body_contribution(
    target_group: GroupId,
    target_path: String,
    source: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
) -> u64 {
    CONTEXT.with(|ctx| {
        let mut borrow = ctx.borrow_mut();
        let c = borrow.as_mut().expect("Script context not initialized");
        let ordinal = c.next_body_contribution_ordinal;
        c.next_body_contribution_ordinal += 1;

        let source = source.or_else(|| {
            c.current_file
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
        });
        let include_stack = source.iter().cloned().collect();

        c.state.body_contributions.push(BodyContribution {
            id: ordinal,
            target_group,
            target_path,
            ordinal,
            source,
            include_stack,
            line,
            column,
        });
        c.active_body_contributions.push(ordinal);

        ordinal
    })
}

/// Leave the active body contribution scope.
pub fn end_body_contribution(id: u64) {
    CONTEXT.with(|ctx| {
        if let Some(c) = ctx.borrow_mut().as_mut() {
            if c.active_body_contributions.last().copied() == Some(id) {
                c.active_body_contributions.pop();
            } else {
                c.active_body_contributions.retain(|&active| active != id);
            }
        }
    });
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
// Note: Groups have a specialized implementation that also creates GroupConfig

/// Get or create a group ID by name.
///
/// This also ensures the GroupConfig exists in the ScriptState.
/// This is important for implicit groups like "main" that are referenced
/// but not explicitly defined via define_group().
///
/// The ID is generated from a hash of the name, ensuring stability across script reloads.
pub fn get_or_create_group_id(name: &str) -> GroupId {
    use vibelang_core::reload::GroupConfig;

    CONTEXT.with(|ctx| {
        let mut borrow = ctx.borrow_mut();
        let c = borrow.as_mut().expect("Script context not initialized");

        let group_id = if let Some(&id) = c.group_ids.get(name) {
            id
        } else {
            // Use hash-based ID for stability across reloads
            let mut raw = hash_name_to_id(name);
            // Collision detection: check if another name already has this ID
            loop {
                let candidate = GroupId::new(raw);
                let collision = c
                    .group_ids
                    .values()
                    .any(|&existing_id| existing_id == candidate);
                if !collision {
                    break;
                }
                tracing::warn!(
                    "Hash collision detected for group '{}' at ID {} — probing next slot",
                    name,
                    raw
                );
                raw = raw.wrapping_add(1);
                if raw == 0 {
                    raw = 1;
                }
            }
            let id = GroupId::new(raw);
            c.group_ids.insert(name.to_string(), id);
            id
        };

        // Ensure the group config exists (important for implicit groups like "main")
        if !c.state.groups.contains_key(&group_id) {
            // Determine parent from path (e.g., "main/Drums" -> parent is "main")
            let parent_id = if name.contains('/') {
                let parts: Vec<&str> = name.rsplitn(2, '/').collect();
                if parts.len() > 1 {
                    let parent_path = parts[1];
                    Some(get_or_create_group_id_inner(c, parent_path))
                } else {
                    None
                }
            } else {
                None
            };

            let config = GroupConfig {
                name: name.rsplit('/').next().unwrap_or(name).to_string(),
                parent: parent_id,
                params: Default::default(),
                effects: Vec::new(),
                muted: false,
                soloed: false,
                output_bus: None,
                output_channels: None,
            };
            c.state.groups.insert(group_id, config);
        }

        group_id
    })
}

/// Internal helper to get or create group ID without triggering GroupConfig creation.
/// Used to avoid infinite recursion when setting up parent groups.
fn get_or_create_group_id_inner(c: &mut ScriptContext, name: &str) -> GroupId {
    if let Some(&id) = c.group_ids.get(name) {
        id
    } else {
        // Use hash-based ID for stability across reloads
        let mut raw = hash_name_to_id(name);
        loop {
            let candidate = GroupId::new(raw);
            let collision = c
                .group_ids
                .values()
                .any(|&existing_id| existing_id == candidate);
            if !collision {
                break;
            }
            tracing::warn!(
                "Hash collision detected for group '{}' at ID {} — probing next slot",
                name,
                raw
            );
            raw = raw.wrapping_add(1);
            if raw == 0 {
                raw = 1;
            }
        }
        let id = GroupId::new(raw);
        c.group_ids.insert(name.to_string(), id);
        id
    }
}

/// Get a group ID by name (if it exists).
pub fn get_group_id(name: &str) -> Option<GroupId> {
    CONTEXT.with(|ctx| {
        ctx.borrow()
            .as_ref()
            .and_then(|c| c.group_ids.get(name).copied())
    })
}

/// Resolve a script group reference to its canonical full path.
///
/// References containing `/` are absolute and used as-is. Single-segment
/// references are prefixed with the current group path when one is active,
/// or used bare at the top level (no implicit `main` prefix).
pub fn resolve_group_reference(raw: &str) -> Result<String, GroupAliasError> {
    if let Some(target) = CONTEXT.with(|ctx| {
        ctx.borrow()
            .as_ref()
            .and_then(|c| c.state.group_aliases.get(raw).cloned())
    }) {
        return Ok(target.path);
    }

    let full_path = if raw.contains('/') {
        raw.to_string()
    } else {
        let current = current_group_path();
        if current.is_empty() {
            raw.to_string()
        } else {
            format!("{}/{}", current, raw)
        }
    };

    if !raw.contains('/') {
        let group_id = get_or_create_group_id(&full_path);
        let target = GroupAliasTarget::new(group_id, full_path.clone());
        with_state(|state| state.add_contextual_group_claim(raw.to_string(), target))?;
    }

    Ok(full_path)
}

/// Register a global alias for a canonical group path.
pub fn add_group_alias(alias: String, canonical_path: String) -> Result<(), GroupAliasError> {
    let group_id = get_or_create_group_id(&canonical_path);
    let target = GroupAliasTarget::new(group_id, canonical_path);
    if !alias.is_empty() && !alias.contains('/') {
        let root_path = alias.clone();
        let existing = CONTEXT.with(|ctx| {
            ctx.borrow()
                .as_ref()
                .and_then(|c| c.group_ids.get(&root_path).copied())
        });

        if let Some(existing_group_id) = existing {
            let existing_target = GroupAliasTarget::new(existing_group_id, root_path);
            if existing_target != target {
                return Err(GroupAliasError::ConflictingCanonicalGroupName {
                    alias,
                    existing: existing_target,
                    attempted: target,
                });
            }
        }
    }

    with_state(|state| state.add_group_alias(alias, target))
}

define_id_accessors!(
    get_or_create_voice_id,
    get_voice_id,
    VoiceId,
    next_voice_id,
    voice_ids,
    "Get or create a voice ID by name.",
    "Get a voice ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_pattern_id,
    get_pattern_id,
    PatternId,
    next_pattern_id,
    pattern_ids,
    "Get or create a pattern ID by name.",
    "Get a pattern ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_melody_id,
    get_melody_id,
    MelodyId,
    next_melody_id,
    melody_ids,
    "Get or create a melody ID by name.",
    "Get a melody ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_sequence_id,
    get_sequence_id,
    SequenceId,
    next_sequence_id,
    sequence_ids,
    "Get or create a sequence ID by name.",
    "Get a sequence ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_effect_id,
    get_effect_id,
    EffectId,
    next_effect_id,
    effect_ids,
    "Get or create an effect ID by name.",
    "Get an effect ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_sample_id,
    get_sample_id,
    SampleId,
    next_sample_id,
    sample_ids,
    "Get or create a sample ID by name.",
    "Get a sample ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_sfz_id,
    get_sfz_id,
    SfzId,
    next_sfz_id,
    sfz_ids,
    "Get or create an SFZ instrument ID by name.",
    "Get an SFZ ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_recording_id,
    get_recording_id,
    RecordingId,
    next_recording_id,
    recording_ids,
    "Get or create a recording ID by name.",
    "Get a recording ID by name (if it exists)."
);

define_id_accessors!(
    get_or_create_fade_id,
    get_fade_id,
    FadeId,
    next_fade_id,
    fade_ids,
    "Get or create a fade ID by name.",
    "Get a fade ID by name (if it exists)."
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

/// Mark a voice for auto-triggering (running continuously).
///
/// Called by `voice.run()` to indicate this voice should be triggered
/// automatically on startup and after reloads.
pub fn mark_voice_for_running(name: &str) {
    let voice_id = get_or_create_voice_id(name);
    with_state(|state| {
        state.running_voices.insert(voice_id);
    });
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

/// Resolve an auto-generated name, handling collisions deterministically.
///
/// On first use of a base name, returns it as-is. On subsequent uses,
/// appends `_2`, `_3`, etc. The counter is reset when the script context
/// is re-initialized, ensuring deterministic names across reloads.
pub fn resolve_auto_name(base: &str) -> String {
    CONTEXT.with(|ctx| {
        let mut borrow = ctx.borrow_mut();
        let c = borrow.as_mut().expect("Script context not initialized");
        let count = c.auto_name_counts.entry(base.to_string()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base.to_string()
        } else {
            format!("{}_{}", base, count)
        }
    })
}
