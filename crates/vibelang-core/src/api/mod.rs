//! VibeLang API for Rhai scripting.
//!
//! This module provides the Rhai API bindings for VibeLang. All functions
//! internally use a thread-local RuntimeHandle to communicate with the runtime.
//!
//! # Usage
//!
//! 1. Initialize the API with a RuntimeHandle using `init_api()`
//! 2. Register all functions with a Rhai engine using `register_api()`
//! 3. Execute scripts that call the registered functions

pub mod global;
pub mod voice;
pub mod pattern;
pub mod melody;
pub mod sequence;
pub mod group;
pub mod synthdef;
pub mod helpers;
pub mod context;
pub mod sfz;
pub mod sample;
pub mod midi;

// Re-export MIDI callback functions for use by CLI
pub use midi::{clear_callbacks, clear_midi_devices, execute_pending_callbacks, get_callback_fnptr};

use crate::runtime::RuntimeHandle;
use rhai::Engine;
use std::cell::RefCell;

// Thread-local storage for the runtime handle.
// This allows Rhai functions to access the runtime without passing it explicitly.
thread_local! {
    static RUNTIME_HANDLE: RefCell<Option<RuntimeHandle>> = RefCell::new(None);
}

/// Initialize the API with a RuntimeHandle.
///
/// This must be called before executing any scripts that use the API.
/// The handle is stored in thread-local storage and used by all API functions.
pub fn init_api(handle: RuntimeHandle) {
    RUNTIME_HANDLE.with(|h| {
        *h.borrow_mut() = Some(handle);
    });
}

/// Get the current RuntimeHandle.
///
/// Returns None if `init_api()` hasn't been called on this thread.
pub fn get_handle() -> Option<RuntimeHandle> {
    RUNTIME_HANDLE.with(|h| h.borrow().clone())
}

/// Get the current RuntimeHandle, panicking if not initialized.
///
/// Use this in API functions where the handle is required.
pub fn require_handle() -> RuntimeHandle {
    get_handle().expect("VibeLang API not initialized. Call init_api() first.")
}

/// Register all VibeLang API functions with a Rhai engine.
///
/// This registers:
/// - Global functions (set_tempo, set_quantization, etc.)
/// - Voice builder and methods
/// - Pattern builder and methods
/// - Melody builder and methods
/// - Group management
/// - SynthDef definition
/// - Helper functions (db, note, bars)
pub fn register_api(engine: &mut Engine) {
    // Register global functions
    global::register(engine);

    // Register voice API
    voice::register(engine);

    // Register pattern API
    pattern::register(engine);

    // Register melody API
    melody::register(engine);

    // Register sequence API
    sequence::register(engine);

    // Register group API
    group::register(engine);

    // Register synthdef API
    synthdef::register(engine);

    // Register helper functions
    helpers::register(engine);

    // Register SFZ API
    sfz::register(engine);

    // Register sample API
    sample::register(engine);

    // Register MIDI API
    midi::register(engine);
}

/// Create a Rhai engine with all VibeLang API registered.
pub fn create_engine() -> Engine {
    let mut engine = Engine::new();

    // Set appropriate limits for complex scripts
    engine.set_max_expr_depths(4096, 4096);
    engine.set_max_call_levels(4096);

    // Override print() to route through the log system instead of stdout
    // This ensures print output appears in the TUI log widget
    engine.on_print(|text| {
        log::info!("[script] {}", text);
    });

    // Override debug() similarly
    engine.on_debug(|text, source, pos| {
        let loc = match (source, pos) {
            (Some(src), pos) if !pos.is_none() => format!(" ({}:{})", src, pos),
            (Some(src), _) => format!(" ({})", src),
            (None, pos) if !pos.is_none() => format!(" ({})", pos),
            _ => String::new(),
        };
        log::debug!("[script]{} {}", loc, text);
    });

    register_api(&mut engine);

    engine
}

/// Create a Rhai engine with import path support.
pub fn create_engine_with_paths(
    base_path: std::path::PathBuf,
    import_paths: Vec<std::path::PathBuf>,
) -> Engine {
    let mut engine = create_engine();

    // Create a collection of module resolvers
    let mut collection = rhai::module_resolvers::ModuleResolversCollection::new();

    // 1. Add source-relative resolver first (highest priority)
    let mut source_resolver = rhai::module_resolvers::FileModuleResolver::new();
    source_resolver.set_extension("vibe");
    collection.push(source_resolver);

    // 2. Add the base path resolver (main script directory)
    let mut base_resolver = rhai::module_resolvers::FileModuleResolver::new();
    base_resolver.set_base_path(base_path);
    base_resolver.set_extension("vibe");
    collection.push(base_resolver);

    // 3. Add additional import paths in order
    for import_path in import_paths {
        let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
        resolver.set_base_path(import_path);
        resolver.set_extension("vibe");
        collection.push(resolver);
    }

    engine.set_module_resolver(collection);

    engine
}
