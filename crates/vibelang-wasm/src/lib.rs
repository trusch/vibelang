//! VibeLang WASM bindings with native runtime.
//!
//! This crate provides WASM bindings for running VibeLang in the browser
//! using the native Rust runtime and scheduler.
//!
//! ## Usage (JavaScript)
//!
//! ```javascript
//! import init, { VibelangRuntime } from 'vibelang-wasm';
//!
//! // Initialize WASM module
//! await init();
//!
//! // Create runtime (connects to SuperSonic)
//! const runtime = new VibelangRuntime();
//! await runtime.init();
//!
//! // Execute a script
//! const result = await runtime.execute(`
//!     set_tempo(128);
//!     define_group("Synth", || {
//!         let lead = voice("lead").synth("kick");
//!         pattern("beat").on(lead).step("x... x... x... x...").start();
//!     });
//! `);
//!
//! // Start the tick loop (call this ~60fps)
//! function gameLoop() {
//!     runtime.tick();
//!     requestAnimationFrame(gameLoop);
//! }
//! gameLoop();
//! ```

// This crate is only functional on wasm32 target, but we allow it to compile
// on native targets for workspace-wide checks (cargo test --workspace, clippy, etc.)
#![cfg_attr(not(target_arch = "wasm32"), allow(unused_imports, dead_code))]

use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use vibelang_core::backends::WebScsynthBackend;
#[cfg(target_arch = "wasm32")]
use vibelang_core::message::TransportMessage;
#[cfg(target_arch = "wasm32")]
use vibelang_core::{Message, Runtime};
use vibelang_dsp::{
    clear_effect_registry, clear_synthdef_registry, get_all_effects_encoded,
    get_all_synthdefs_encoded, set_deploy_callback, system_synthdefs,
};
use vibelang_rhai::ScriptEngine;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// Initialize panic hook for better error messages in browser console
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();

    // Set a no-op deploy callback since we handle synthdefs manually in WASM
    set_deploy_callback(|_| Ok(()));
}

fn parse_note_name(name: &str) -> Option<u8> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let mut chars = name.chars().peekable();
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    let mut accidental = 0i8;
    while let Some(&c) = chars.peek() {
        match c {
            '#' | '\u{266f}' => {
                accidental += 1;
                chars.next();
            }
            'b' | '\u{266d}' => {
                accidental -= 1;
                chars.next();
            }
            _ => break,
        }
    }

    let octave_str: String = chars.collect();
    let octave = if octave_str.is_empty() {
        4
    } else {
        octave_str.parse().ok()?
    };
    let note = (octave + 1) as i16 * 12 + base as i16 + accidental as i16;

    (0..=127).contains(&note).then_some(note as u8)
}

/// Result from executing a VibeLang script.
#[derive(Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Whether execution succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Number of groups created.
    pub groups: usize,
    /// Number of voices created.
    pub voices: usize,
    /// Number of patterns created.
    pub patterns: usize,
    /// Number of melodies created.
    pub melodies: usize,
    /// Tempo in BPM.
    pub tempo: f64,
}

/// Compiled synthdef ready to send to SuperSonic.
#[derive(Serialize, Deserialize)]
pub struct CompiledSynthdef {
    /// Synthdef name.
    pub name: String,
    /// Compiled bytes.
    pub data: Vec<u8>,
}

/// VibeLang runtime for WASM.
///
/// This wraps the native Rust runtime and scheduler for browser use.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct VibelangRuntime {
    /// Script engine for parsing
    script_engine: ScriptEngine,
    /// Native runtime (created on init)
    runtime: Option<Runtime<WebScsynthBackend>>,
    /// Whether runtime is initialized
    initialized: bool,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl VibelangRuntime {
    /// Create a new VibeLang runtime.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        // Clear registries for fresh start
        clear_synthdef_registry();
        clear_effect_registry();

        Self {
            script_engine: ScriptEngine::new(),
            runtime: None,
            initialized: false,
        }
    }

    /// Initialize the runtime with the WebScsynth backend.
    ///
    /// This must be called after SuperSonic is ready.
    #[wasm_bindgen]
    pub async fn init(&mut self) -> Result<(), JsValue> {
        web_sys::console::log_1(&JsValue::from_str("VibelangRuntime.init() called"));

        if self.initialized {
            web_sys::console::log_1(&JsValue::from_str("Already initialized, returning"));
            return Ok(());
        }

        // Create the WebScsynth backend
        web_sys::console::log_1(&JsValue::from_str("Creating WebScsynth backend..."));
        let backend = WebScsynthBackend::new();

        // Initialize it (checks if JS side is ready)
        web_sys::console::log_1(&JsValue::from_str("Initializing backend..."));
        backend
            .init()
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to init backend: {}", e)))?;

        // Create the runtime
        web_sys::console::log_1(&JsValue::from_str("Creating Runtime..."));
        let runtime = Runtime::new(backend);

        // Load built-in synthdefs
        web_sys::console::log_1(&JsValue::from_str("Loading builtins..."));
        runtime
            .load_builtins()
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to load builtins: {}", e)))?;

        self.runtime = Some(runtime);
        self.initialized = true;

        web_sys::console::log_1(&JsValue::from_str(
            "VibeLang runtime initialized successfully",
        ));

        Ok(())
    }

    /// Execute a VibeLang script.
    ///
    /// This parses the script, loads synthdefs, and applies the state to the runtime.
    #[wasm_bindgen]
    pub async fn execute(&mut self, script: &str) -> JsValue {
        web_sys::console::log_1(&JsValue::from_str("VibelangRuntime.execute() called"));

        // Clear old synthdefs
        clear_synthdef_registry();
        clear_effect_registry();

        // Parse and execute the script
        web_sys::console::log_1(&JsValue::from_str("Parsing script..."));
        let result = self.script_engine.execute(script);

        let execution_result = match result {
            Ok(state) => {
                let result = ExecutionResult {
                    success: true,
                    error: None,
                    groups: state.groups.len(),
                    voices: state.voices.len(),
                    patterns: state.patterns.len(),
                    melodies: state.melodies.len(),
                    tempo: state.tempo,
                };

                // If runtime is initialized, apply the state
                if let Some(runtime) = &self.runtime {
                    web_sys::console::log_1(&JsValue::from_str(
                        "Runtime present, applying state...",
                    ));

                    // Load user synthdefs to SuperSonic
                    let synthdefs = get_all_synthdefs_encoded();
                    web_sys::console::log_1(&JsValue::from_str(&format!(
                        "Found {} synthdefs to load",
                        synthdefs.len()
                    )));
                    for (name, data) in synthdefs {
                        web_sys::console::log_1(&JsValue::from_str(&format!(
                            "Loading synthdef: {} ({} bytes)",
                            name,
                            data.len()
                        )));
                        if let Err(e) = load_synthdef_to_supersonic(&name, &data).await {
                            web_sys::console::warn_1(&JsValue::from_str(&format!(
                                "Failed to load synthdef {}: {:?}",
                                name, e
                            )));
                        }
                    }

                    // Load user effects
                    for (name, data) in get_all_effects_encoded() {
                        if let Err(e) = load_synthdef_to_supersonic(&name, &data).await {
                            web_sys::console::warn_1(&JsValue::from_str(&format!(
                                "Failed to load effect {}: {:?}",
                                name, e
                            )));
                        }
                    }

                    // Send reload message to apply the new state
                    let handle = runtime.handle();
                    let reload_msg = vibelang_core::message::ReloadMessage::Apply { state };
                    if let Err(e) = handle.send(Message::Reload(Box::new(reload_msg))).await {
                        web_sys::console::warn_1(&JsValue::from_str(&format!(
                            "Failed to apply state: {}",
                            e
                        )));
                    }
                }

                result
            }
            Err(e) => ExecutionResult {
                success: false,
                error: Some(format!("{}", e)),
                groups: 0,
                voices: 0,
                patterns: 0,
                melodies: 0,
                tempo: 120.0,
            },
        };

        serde_wasm_bindgen::to_value(&execution_result).unwrap_or(JsValue::NULL)
    }

    /// Tick the runtime.
    ///
    /// Call this function regularly (e.g., 60 times per second via requestAnimationFrame)
    /// to drive the scheduler and process messages.
    #[wasm_bindgen]
    pub async fn tick(&mut self) {
        if let Some(runtime) = &mut self.runtime {
            runtime.tick().await;
        }
    }

    /// Start transport (begin playback).
    #[wasm_bindgen]
    pub async fn start(&self) -> Result<(), JsValue> {
        web_sys::console::log_1(&JsValue::from_str("VibelangRuntime.start() called"));
        if let Some(runtime) = &self.runtime {
            web_sys::console::log_1(&JsValue::from_str("Sending TransportMessage::Start"));
            let handle = runtime.handle();
            handle
                .send(TransportMessage::Start.into())
                .await
                .map_err(|e| JsValue::from_str(&format!("Failed to start: {}", e)))?;
            web_sys::console::log_1(&JsValue::from_str(
                "TransportMessage::Start sent successfully",
            ));
        } else {
            web_sys::console::warn_1(&JsValue::from_str("Runtime not initialized - cannot start"));
        }
        Ok(())
    }

    /// Stop transport (stop playback).
    #[wasm_bindgen]
    pub async fn stop(&self) -> Result<(), JsValue> {
        if let Some(runtime) = &self.runtime {
            let handle = runtime.handle();
            handle
                .send(TransportMessage::Stop.into())
                .await
                .map_err(|e| JsValue::from_str(&format!("Failed to stop: {}", e)))?;
        }
        Ok(())
    }

    /// Stop all audio (free all synths).
    #[wasm_bindgen(js_name = stopAll)]
    pub async fn stop_all(&self) -> Result<(), JsValue> {
        // Stop transport first
        self.stop().await?;

        // The native runtime already stopped via self.stop()
        // Additional cleanup could be done here if needed

        Ok(())
    }

    /// Get all built-in system synthdefs.
    ///
    /// Returns an array of compiled synthdefs needed for basic operation.
    #[wasm_bindgen(js_name = getSystemSynthdefs)]
    pub fn get_system_synthdefs() -> JsValue {
        let mut synthdefs: Vec<CompiledSynthdef> = Vec::new();

        // Get system synthdefs
        for (name, data) in system_synthdefs::create_all_system_synthdefs() {
            synthdefs.push(CompiledSynthdef { name, data });
        }

        // Add system_link_audio
        if let Ok(link_bytes) = system_synthdefs::create_system_link_audio_bytes() {
            synthdefs.push(CompiledSynthdef {
                name: "system_link_audio".to_string(),
                data: link_bytes,
            });
        }

        serde_wasm_bindgen::to_value(&synthdefs).unwrap_or(JsValue::NULL)
    }

    /// Check if runtime is initialized.
    #[wasm_bindgen(js_name = isInitialized)]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Parse a note name to MIDI number.
    #[wasm_bindgen(js_name = parseNote)]
    pub fn parse_note(note: &str) -> i32 {
        parse_note_name(note).map(|n| n as i32).unwrap_or(-1)
    }

    /// Convert decibels to linear amplitude.
    #[wasm_bindgen(js_name = dbToAmp)]
    pub fn db_to_amp(db: f64) -> f64 {
        10.0_f64.powf(db / 20.0)
    }

    /// Convert linear amplitude to decibels.
    #[wasm_bindgen(js_name = ampToDb)]
    pub fn amp_to_db(amp: f64) -> f64 {
        20.0 * amp.log10()
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for VibelangRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to load a synthdef to SuperSonic via JS bridge.
#[cfg(target_arch = "wasm32")]
async fn load_synthdef_to_supersonic(name: &str, data: &[u8]) -> Result<(), JsValue> {
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = ["window", "vibelangBridge"])]
        async fn loadSynthdef(name: &str, data: js_sys::Uint8Array) -> JsValue;
    }

    let array = js_sys::Uint8Array::new_with_length(data.len() as u32);
    array.copy_from(data);

    if let Some(bridge) =
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("vibelangBridge")).ok()
    {
        if !bridge.is_undefined() {
            loadSynthdef(name, array).await;
        }
    }

    Ok(())
}

// Keep the old VibelangEngine for backwards compatibility during transition
// TODO: Remove this once the new runtime is fully working

/// Legacy VibeLang engine (parse-only, for backwards compatibility).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct VibelangEngine {
    engine: ScriptEngine,
    last_state: Option<vibelang_core::reload::ScriptState>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl VibelangEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        clear_synthdef_registry();
        clear_effect_registry();
        Self {
            engine: ScriptEngine::new(),
            last_state: None,
        }
    }

    #[wasm_bindgen]
    pub fn execute(&mut self, script: &str) -> JsValue {
        let result = self.engine.execute(script);
        let execution_result = match result {
            Ok(state) => {
                let result = ExecutionResult {
                    success: true,
                    error: None,
                    groups: state.groups.len(),
                    voices: state.voices.len(),
                    patterns: state.patterns.len(),
                    melodies: state.melodies.len(),
                    tempo: state.tempo,
                };
                self.last_state = Some(state);
                result
            }
            Err(e) => {
                self.last_state = None;
                ExecutionResult {
                    success: false,
                    error: Some(format!("{}", e)),
                    groups: 0,
                    voices: 0,
                    patterns: 0,
                    melodies: 0,
                    tempo: 120.0,
                }
            }
        };
        serde_wasm_bindgen::to_value(&execution_result).unwrap_or(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = getSynthdefs)]
    pub fn get_synthdefs(&self) -> JsValue {
        let mut synthdefs: Vec<CompiledSynthdef> = Vec::new();
        for (name, data) in get_all_synthdefs_encoded() {
            synthdefs.push(CompiledSynthdef { name, data });
        }
        for (name, data) in get_all_effects_encoded() {
            synthdefs.push(CompiledSynthdef { name, data });
        }
        serde_wasm_bindgen::to_value(&synthdefs).unwrap_or(JsValue::NULL)
    }

    #[wasm_bindgen(js_name = getSystemSynthdefs)]
    pub fn get_system_synthdefs() -> JsValue {
        VibelangRuntime::get_system_synthdefs()
    }

    #[wasm_bindgen(js_name = clearSynthdefs)]
    pub fn clear_synthdefs(&mut self) {
        clear_synthdef_registry();
        clear_effect_registry();
    }

    #[wasm_bindgen(js_name = parseNote)]
    pub fn parse_note(note: &str) -> i32 {
        VibelangRuntime::parse_note(note)
    }

    #[wasm_bindgen(js_name = dbToAmp)]
    pub fn db_to_amp(db: f64) -> f64 {
        VibelangRuntime::db_to_amp(db)
    }

    #[wasm_bindgen(js_name = ampToDb)]
    pub fn amp_to_db(amp: f64) -> f64 {
        VibelangRuntime::amp_to_db(amp)
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for VibelangEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Log a message to the browser console.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn log(message: &str) {
    web_sys::console::log_1(&JsValue::from_str(message));
}

/// Get the VibeLang version.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::parse_note_name;

    #[test]
    fn parse_note_name_handles_browser_safe_notes_without_midi_feature() {
        assert_eq!(parse_note_name("C4"), Some(60));
        assert_eq!(parse_note_name("A4"), Some(69));
        assert_eq!(parse_note_name("C#4"), Some(61));
        assert_eq!(parse_note_name("Db4"), Some(61));
        assert_eq!(parse_note_name("C##4"), Some(62));
        assert_eq!(parse_note_name("C\u{266f}4"), Some(61));
        assert_eq!(parse_note_name("D\u{266d}4"), Some(61));
        assert_eq!(parse_note_name("C-1"), Some(0));
        assert_eq!(parse_note_name("G9"), Some(127));
        assert_eq!(parse_note_name("C"), Some(60));
        assert_eq!(parse_note_name("c4"), Some(60));
    }

    #[test]
    fn parse_note_name_rejects_invalid_browser_notes_without_midi_feature() {
        assert_eq!(parse_note_name(""), None);
        assert_eq!(parse_note_name("H4"), None);
        assert_eq!(parse_note_name("C-2"), None);
        assert_eq!(parse_note_name("G#9"), None);
    }
}
