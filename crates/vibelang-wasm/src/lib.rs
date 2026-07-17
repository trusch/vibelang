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
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
#[cfg(target_arch = "wasm32")]
use vibelang_core::backends::WebScsynthBackend;
#[cfg(target_arch = "wasm32")]
use vibelang_core::message::TransportMessage;
use vibelang_core::mutation::{
    Atomicity, CandidateOrigin, MutationEventSink, MutationKind, MutationReceipt,
    MutationReplySink, MutationSource, ReceiptState, RequestMaterial, Submission,
    SupersessionPolicy, TerminalOutcome,
};
#[cfg(target_arch = "wasm32")]
use vibelang_core::{Message, Runtime};
use vibelang_dsp::{
    clear_effect_registry, clear_synthdef_registry, get_all_effects_encoded,
    get_all_synthdefs_encoded, notes::parse_note_name, set_deploy_callback, system_synthdefs,
};
use vibelang_rhai::ScriptEngine;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

// Initialize panic hook for better error messages in browser console
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();

    // Set a no-op deploy callback since we handle synthdefs manually in WASM
    set_deploy_callback(|_| Ok(()));
}

/// Result from executing a VibeLang script.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Evaluation-only compatibility boolean. It is false for any delivery failure
    /// known before return and is never a terminal applied claim.
    pub success: bool,
    /// Fixed scope for the legacy success boolean.
    pub legacy_success_scope: String,
    /// Error message if failed.
    pub error: Option<String>,
    /// Structured evaluation, initialization, bridge, or dispatch failure.
    pub failure: Option<ExecutionFailure>,
    /// Canonical queue/runtime truth when execution reached runtime submission.
    pub receipt: Option<MutationReceipt>,
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

/// Structured failure returned by [`ExecutionResult`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionFailure {
    pub phase: ExecutionFailurePhase,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionFailurePhase {
    Evaluate,
    Initialize,
    Bridge,
    Dispatch,
    Runtime,
}

impl ExecutionResult {
    fn evaluated(state: &vibelang_core::reload::ScriptState) -> Self {
        Self {
            success: true,
            legacy_success_scope: "evaluation_only".into(),
            error: None,
            failure: None,
            receipt: None,
            groups: state.groups.len(),
            voices: state.voices.len(),
            patterns: state.patterns.len(),
            melodies: state.melodies.len(),
            tempo: state.tempo,
        }
    }

    fn failed(
        phase: ExecutionFailurePhase,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let message = message.into();
        Self {
            success: false,
            legacy_success_scope: "evaluation_only".into(),
            error: Some(message.clone()),
            failure: Some(ExecutionFailure {
                phase,
                code: code.into(),
                message,
            }),
            receipt: None,
            groups: 0,
            voices: 0,
            patterns: 0,
            melodies: 0,
            tempo: 120.0,
        }
    }

    fn with_receipt(mut self, receipt: MutationReceipt) -> Self {
        if let Some(failure) = receipt_failure(&receipt) {
            self.success = false;
            self.error = Some(failure.message.clone());
            self.failure = Some(failure);
        }
        self.receipt = Some(receipt);
        self
    }

    fn delivery_failed(
        mut self,
        phase: ExecutionFailurePhase,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let message = message.into();
        self.success = false;
        self.error = Some(message.clone());
        self.failure = Some(ExecutionFailure {
            phase,
            code: code.into(),
            message,
        });
        self
    }
}

fn receipt_failure(receipt: &MutationReceipt) -> Option<ExecutionFailure> {
    match &receipt.state {
        ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) => Some(ExecutionFailure {
            phase: ExecutionFailurePhase::Runtime,
            code: rejected.code.clone(),
            message: rejected.message.clone(),
        }),
        ReceiptState::Terminal(TerminalOutcome::Superseded(superseded)) => Some(ExecutionFailure {
            phase: ExecutionFailurePhase::Runtime,
            code: "superseded".into(),
            message: format!("mutation was superseded: {:?}", superseded.reason),
        }),
        ReceiptState::Terminal(TerminalOutcome::Partial(partial)) => Some(ExecutionFailure {
            phase: ExecutionFailurePhase::Runtime,
            code: partial.code.clone(),
            message: format!(
                "mutation is partial{}",
                if partial.fenced {
                    " and the runtime is fenced"
                } else {
                    ""
                }
            ),
        }),
        _ => None,
    }
}

fn wasm_submission(
    runtime_epoch: vibelang_core::mutation::RuntimeEpoch,
    instance_id: &str,
    script: &str,
) -> Submission {
    Submission {
        kind: MutationKind::Candidate {
            origin: CandidateOrigin::WasmRuntime,
        },
        source: MutationSource::Wasm {
            instance_id: instance_id.into(),
        },
        caller_namespace: format!("compat.vibelang.v1.wasm.{instance_id}"),
        idempotency_key: None,
        require_idempotency_key: false,
        retry_epoch: Some(runtime_epoch),
        expected_revision: None,
        atomicity: Atomicity::BestEffort,
        supersession: SupersessionPolicy::Fifo,
        material: RequestMaterial::from_values(
            serde_json::json!({
                "operation": "execute",
                "script": script,
            }),
            Some(serde_json::json!({
                "operation": "execute",
                "source_bytes": script.len(),
            })),
        ),
    }
}

fn receipt_sinks(
    receipts: Arc<Mutex<HashMap<String, MutationReceipt>>>,
) -> (
    Arc<Mutex<Option<MutationReceipt>>>,
    MutationReplySink,
    MutationEventSink,
) {
    let latest = Arc::new(Mutex::new(None));
    let sink_latest = Arc::clone(&latest);
    let reply_sink = MutationReplySink::new(move |receipt| {
        if let Ok(mut latest) = sink_latest.lock() {
            *latest = Some(receipt.clone());
        }
        if let Ok(mut receipts) = receipts.lock() {
            receipts.insert(receipt.attempt_id.to_string(), receipt);
        }
    });
    (latest, reply_sink, MutationEventSink::default())
}

fn latest_known_receipt(
    returned: MutationReceipt,
    observed: Option<MutationReceipt>,
) -> MutationReceipt {
    observed
        .filter(|observed| {
            observed.attempt_id == returned.attempt_id
                && observed.event_sequence >= returned.event_sequence
        })
        .unwrap_or(returned)
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
    /// Stable source identity for canonical WASM mutation receipts.
    instance_id: String,
    /// Latest canonical receipt for every execution attempt.
    receipts: Arc<Mutex<HashMap<String, MutationReceipt>>>,
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
            instance_id: vibelang_core::mutation::AttemptId::new().to_string(),
            receipts: Arc::new(Mutex::new(HashMap::new())),
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

        let state = match result {
            Ok(state) => state,
            Err(error) => {
                let result = ExecutionResult::failed(
                    ExecutionFailurePhase::Evaluate,
                    "evaluation_failed",
                    error.to_string(),
                );
                return serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL);
            }
        };
        let mut execution_result = ExecutionResult::evaluated(&state);

        let Some(runtime) = &self.runtime else {
            execution_result = execution_result.delivery_failed(
                ExecutionFailurePhase::Initialize,
                "runtime_not_initialized",
                "VibelangRuntime.init() must complete before execute can deliver a candidate",
            );
            return serde_wasm_bindgen::to_value(&execution_result).unwrap_or(JsValue::NULL);
        };

        web_sys::console::log_1(&JsValue::from_str("Runtime present, applying state..."));
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
            if let Err(error) = load_synthdef_to_supersonic(&name, &data).await {
                execution_result = execution_result.delivery_failed(
                    ExecutionFailurePhase::Bridge,
                    "synthdef_bridge_failed",
                    format!(
                        "failed to load synthdef {name}: {}",
                        js_error_message(&error)
                    ),
                );
                return serde_wasm_bindgen::to_value(&execution_result).unwrap_or(JsValue::NULL);
            }
        }

        for (name, data) in get_all_effects_encoded() {
            if let Err(error) = load_synthdef_to_supersonic(&name, &data).await {
                execution_result = execution_result.delivery_failed(
                    ExecutionFailurePhase::Bridge,
                    "effect_bridge_failed",
                    format!("failed to load effect {name}: {}", js_error_message(&error)),
                );
                return serde_wasm_bindgen::to_value(&execution_result).unwrap_or(JsValue::NULL);
            }
        }

        let handle = runtime.handle();
        let submission = wasm_submission(
            handle.mutation_status().runtime_epoch,
            &self.instance_id,
            script,
        );
        let (latest, reply_sink, event_sink) = receipt_sinks(Arc::clone(&self.receipts));
        let submission_result = handle
            .submit_with_sinks(
                Message::Reload(Box::new(vibelang_core::message::ReloadMessage::Apply {
                    state,
                })),
                submission,
                reply_sink,
                event_sink,
            )
            .await;
        let receipt = match submission_result {
            Ok(receipt) => {
                let observed = latest.lock().ok().and_then(|latest| latest.clone());
                Some(latest_known_receipt(receipt, observed))
            }
            Err(error) => {
                let receipt = latest
                    .lock()
                    .ok()
                    .and_then(|latest| latest.clone())
                    .filter(|receipt| receipt.state.is_terminal());
                if receipt.is_none() {
                    execution_result = execution_result.delivery_failed(
                        ExecutionFailurePhase::Dispatch,
                        "reload_dispatch_failed",
                        error.to_string(),
                    );
                }
                receipt
            }
        };
        if let Some(receipt) = receipt {
            execution_result = execution_result.with_receipt(receipt);
        }

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

    /// Read the latest canonical receipt for an execution attempt.
    #[wasm_bindgen(js_name = getReceipt)]
    pub fn get_receipt(&self, attempt_id: &str) -> Result<JsValue, JsValue> {
        let receipt = self
            .receipts
            .lock()
            .map_err(|_| JsValue::from_str("receipt state is unavailable"))?
            .get(attempt_id)
            .cloned()
            .ok_or_else(|| JsValue::from_str("receipt not found"))?;
        serde_wasm_bindgen::to_value(&receipt)
            .map_err(|error| JsValue::from_str(&format!("failed to serialize receipt: {error}")))
    }

    /// Explicitly acknowledge the current fenced Partial before continuing.
    #[wasm_bindgen(js_name = continueBestEffort)]
    pub fn continue_best_effort(&self, attempt_id: &str) -> Result<(), JsValue> {
        let receipt = self
            .receipts
            .lock()
            .map_err(|_| JsValue::from_str("receipt state is unavailable"))?
            .get(attempt_id)
            .cloned()
            .ok_or_else(|| JsValue::from_str("receipt not found"))?;
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| JsValue::from_str("runtime is not initialized"))?;
        runtime
            .handle()
            .continue_best_effort(receipt.attempt_id)
            .map_err(|error| JsValue::from_str(&error.to_string()))
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
    let array = js_sys::Uint8Array::new_with_length(data.len() as u32);
    array.copy_from(data);

    let bridge = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("vibelangBridge"))
        .map_err(|error| {
            JsValue::from_str(&format!(
                "failed to inspect globalThis.vibelangBridge: {}",
                js_error_message(&error)
            ))
        })?;
    if bridge.is_null() || bridge.is_undefined() {
        return Err(JsValue::from_str(
            "globalThis.vibelangBridge is not installed",
        ));
    }
    let load =
        js_sys::Reflect::get(&bridge, &JsValue::from_str("loadSynthdef")).map_err(|error| {
            JsValue::from_str(&format!(
                "failed to inspect vibelangBridge.loadSynthdef: {}",
                js_error_message(&error)
            ))
        })?;
    let load = load.dyn_into::<js_sys::Function>().map_err(|_| {
        JsValue::from_str("globalThis.vibelangBridge.loadSynthdef is not a function")
    })?;
    let pending = load
        .call2(&bridge, &JsValue::from_str(name), array.as_ref())
        .map_err(|error| {
            JsValue::from_str(&format!(
                "vibelangBridge.loadSynthdef threw: {}",
                js_error_message(&error)
            ))
        })?;
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&pending))
        .await
        .map_err(|error| {
            JsValue::from_str(&format!(
                "vibelangBridge.loadSynthdef rejected: {}",
                js_error_message(&error)
            ))
        })?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn js_error_message(error: &JsValue) -> String {
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
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
                let result = ExecutionResult::evaluated(&state);
                self.last_state = Some(state);
                result
            }
            Err(e) => {
                self.last_state = None;
                ExecutionResult::failed(
                    ExecutionFailurePhase::Evaluate,
                    "evaluation_failed",
                    e.to_string(),
                )
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
    use super::*;
    use vibelang_core::mutation::{
        AttemptId, EventSequence, FailurePhase, Partial, ReceiptTimestamps, RequestIdentity,
        RevisionId, RollbackState, RuntimeEpoch, Timestamp, MUTATION_SCHEMA_VERSION,
    };

    fn receipt(state: ReceiptState) -> MutationReceipt {
        let now = Timestamp::parse("2026-07-17T08:00:00Z").unwrap();
        MutationReceipt {
            schema_version: MUTATION_SCHEMA_VERSION,
            attempt_id: AttemptId::new(),
            runtime_epoch: RuntimeEpoch::new(),
            revision: Some(RevisionId::new(1).unwrap()),
            event_sequence: EventSequence::new(1).unwrap(),
            request: RequestIdentity {
                kind: MutationKind::Candidate {
                    origin: CandidateOrigin::WasmRuntime,
                },
                source: MutationSource::Wasm {
                    instance_id: "wasm-test".into(),
                },
                submission_digest: None,
                operation_digest: None,
                idempotency_key_present: false,
                expected_revision: None,
                atomicity: Atomicity::BestEffort,
                supersession: SupersessionPolicy::Fifo,
            },
            state,
            previous_confirmed_revision: None,
            timestamps: ReceiptTimestamps {
                submitted_at: now.clone(),
                accepted_at: Some(now.clone()),
                last_transition_at: now,
                terminal_at: None,
            },
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn accepted_receipt_keeps_success_evaluation_only_and_pending() {
        let state = vibelang_core::reload::ScriptState::default();
        let accepted = receipt(ReceiptState::Accepted {
            queue_position: Some(1),
        });
        let result = ExecutionResult::evaluated(&state).with_receipt(accepted.clone());

        assert!(result.success);
        assert_eq!(result.legacy_success_scope, "evaluation_only");
        assert_eq!(result.receipt, Some(accepted));
        assert!(result.failure.is_none());
    }

    #[test]
    fn partial_receipt_overrides_evaluation_success_and_preserves_fence_truth() {
        let state = vibelang_core::reload::ScriptState::default();
        let partial = receipt(ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: FailurePhase::BackendBarrier,
            code: "backend_acknowledgement_lost".into(),
            components: Vec::new(),
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: None,
        })));
        let result = ExecutionResult::evaluated(&state).with_receipt(partial.clone());

        assert!(!result.success);
        assert_eq!(result.receipt, Some(partial));
        let failure = result.failure.unwrap();
        assert_eq!(failure.phase, ExecutionFailurePhase::Runtime);
        assert_eq!(failure.code, "backend_acknowledgement_lost");
        assert!(failure.message.contains("fenced"));
    }

    #[test]
    fn known_terminal_transition_outranks_returned_queue_admission() {
        let accepted = receipt(ReceiptState::Accepted {
            queue_position: Some(1),
        });
        let mut partial = accepted.clone();
        partial.event_sequence = EventSequence::new(2).unwrap();
        partial.state = ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: FailurePhase::BackendBarrier,
            code: "backend_acknowledgement_lost".into(),
            components: Vec::new(),
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: None,
        }));

        assert_eq!(
            latest_known_receipt(accepted, Some(partial.clone())),
            partial
        );
    }

    #[test]
    fn known_initialization_and_bridge_failures_are_structured_non_success() {
        let state = vibelang_core::reload::ScriptState::default();
        for (phase, code) in [
            (ExecutionFailurePhase::Initialize, "runtime_not_initialized"),
            (ExecutionFailurePhase::Bridge, "synthdef_bridge_failed"),
            (ExecutionFailurePhase::Dispatch, "reload_dispatch_failed"),
        ] {
            let result =
                ExecutionResult::evaluated(&state).delivery_failed(phase, code, "delivery failed");
            assert!(!result.success);
            assert_eq!(result.failure.as_ref().unwrap().phase, phase);
            assert_eq!(result.failure.as_ref().unwrap().code, code);
            assert!(result.receipt.is_none());
        }
    }

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
