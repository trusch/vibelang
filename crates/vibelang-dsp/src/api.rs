//! Rhai API for SynthDef definition.
//!
//! This module provides the `define_synthdef` and `define_fx` functions
//! for the Rhai scripting environment.
//!
//! Note: This module uses a callback function to deploy synthdefs to scsynth.
//! The callback must be set by the host application (CLI) before using these functions.

use crate::builder::{InputPort, OutputPort, SynthDef};
use crate::encoder::encode_synthdef;
use crate::errors::SynthDefError;
use crate::graph::GraphIR;
use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, NativeCallContext, Position};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Type alias for the deploy callback function
type DeployCallback = Arc<dyn Fn(Vec<u8>) -> Result<(), String> + Send + Sync>;

// Global registry of synthdefs
static SYNTHDEF_REGISTRY: OnceLock<Mutex<HashMap<String, GraphIR>>> = OnceLock::new();
// Global registry of effects (separate from regular synthdefs)
static EFFECT_REGISTRY: OnceLock<Mutex<HashMap<String, GraphIR>>> = OnceLock::new();
// Per-synthdef declared output port set (script-side; populated alongside the IR
// at deploy time so the Rhai surface can resolve `voice.output(name|idx)`).
static SYNTHDEF_OUTPUTS_REGISTRY: OnceLock<Mutex<HashMap<String, Vec<OutputPort>>>> =
    OnceLock::new();
// Per-synthdef declared input port set. Populated at deploy time alongside the
// outputs registry so the runtime can allocate / route input buses without
// re-running the builder.
static SYNTHDEF_INPUTS_REGISTRY: OnceLock<Mutex<HashMap<String, Vec<InputPort>>>> = OnceLock::new();
// Per-synthdef content hash of the encoded SCgf bytes, keyed by name.
// Populated at deploy time (both `define_synthdef` and `define_fx` paths) so
// the reload differ can detect body-only edits: same name, same params, but
// a different compiled graph. Names not in this map (builtins, auto-generated
// sample voices) have no hash and are never treated as body-changed.
static SYNTHDEF_HASH_REGISTRY: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
// Callback for deploying synthdef bytes to scsynth
static DEPLOY_CALLBACK: OnceLock<Mutex<Option<DeployCallback>>> = OnceLock::new();
// Per-synthdef memoized single-value param defaults (Fix B). Built lazily from
// SYNTHDEF_REGISTRY on first request and shared as an `Arc` so the note-on hot
// path (called several times per trigger under the global State lock) clones a
// pointer instead of rebuilding a `HashMap`. Invalidated wherever the synthdef
// IR registry is mutated (`deploy_synthdef_ir`, `register_synthdef_ir`,
// `clear_synthdef_registry`) and dropped wholesale on scsynth (re)connect
// (`clear_synthdef_hash_registry`) so a redefined def can't serve stale values.
static SYNTHDEF_PARAM_DEFAULTS_MEMO: OnceLock<Mutex<HashMap<String, Arc<HashMap<String, f32>>>>> =
    OnceLock::new();

fn get_synthdef_registry() -> &'static Mutex<HashMap<String, GraphIR>> {
    SYNTHDEF_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_effect_registry() -> &'static Mutex<HashMap<String, GraphIR>> {
    EFFECT_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_synthdef_outputs_registry() -> &'static Mutex<HashMap<String, Vec<OutputPort>>> {
    SYNTHDEF_OUTPUTS_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_synthdef_inputs_registry() -> &'static Mutex<HashMap<String, Vec<InputPort>>> {
    SYNTHDEF_INPUTS_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record the declared output port set for a synthdef name.
///
/// Called by [`SynthDefBuilderHandle::body`] / `body_map` at deploy time so
/// later script-side code (e.g. the Rhai `voice.output(...)` surface) can
/// resolve port names and indices without re-running the builder.
pub fn register_synthdef_outputs(name: String, outputs: Vec<OutputPort>) {
    let mut registry = get_synthdef_outputs_registry().lock().unwrap();
    registry.insert(name, outputs);
}

/// Look up the declared output port set for a registered synthdef.
///
/// Returns `None` for unknown synthdefs — callers should treat that as the
/// implicit legacy `[("out", 2)]` set (matches [`crate::builder::SynthDef::new`]
/// defaults).
pub fn get_synthdef_outputs(name: &str) -> Option<Vec<OutputPort>> {
    get_synthdef_outputs_registry()
        .lock()
        .unwrap()
        .get(name)
        .cloned()
}

/// Clear the synthdef output-port registry. Useful for tests and reload.
pub fn clear_synthdef_outputs_registry() {
    get_synthdef_outputs_registry().lock().unwrap().clear();
}

/// Record the declared input port set for a synthdef name.
///
/// Mirror of [`register_synthdef_outputs`] for the input side. Called by
/// [`SynthDefBuilderHandle::body`] / `body_map` at deploy time so the runtime
/// can resolve per-synthdef input ports when allocating / routing input buses.
pub fn register_synthdef_inputs(name: String, inputs: Vec<InputPort>) {
    let mut registry = get_synthdef_inputs_registry().lock().unwrap();
    registry.insert(name, inputs);
}

/// Look up the declared input port set for a registered synthdef.
///
/// Returns `None` for unknown synthdefs and for synthdefs that declared no
/// inputs — callers should treat both as "no inputs".
pub fn get_synthdef_inputs(name: &str) -> Option<Vec<InputPort>> {
    get_synthdef_inputs_registry()
        .lock()
        .unwrap()
        .get(name)
        .cloned()
}

/// Clear the synthdef input-port registry. Useful for tests and reload.
pub fn clear_synthdef_inputs_registry() {
    get_synthdef_inputs_registry().lock().unwrap().clear();
}

fn get_synthdef_hash_registry() -> &'static Mutex<HashMap<String, u64>> {
    SYNTHDEF_HASH_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Content hash of encoded SCgf bytes (stable within a process run).
fn hash_synthdef_bytes(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Record the content hash for a deployed synthdef / effect body.
///
/// Called by the deploy paths with a hash of the encoded SCgf bytes. Tests
/// may call it directly to simulate body edits without running the builder.
pub fn register_synthdef_hash(name: String, hash: u64) {
    get_synthdef_hash_registry()
        .lock()
        .unwrap()
        .insert(name, hash);
}

/// Look up the content hash for a deployed synthdef / effect body.
///
/// Returns `None` for names that never went through the script deploy path
/// (builtins, auto-generated voices) — callers must treat that as "unknown,
/// assume unchanged".
pub fn get_synthdef_hash(name: &str) -> Option<u64> {
    get_synthdef_hash_registry()
        .lock()
        .unwrap()
        .get(name)
        .copied()
}

/// Clear the synthdef content-hash registry. Useful for tests.
///
/// Called on scsynth (re)connect to force a full re-send of every def. Also
/// drops the param-defaults memo (Fix B) so a def redefined against a fresh
/// server can never serve stale defaults out of the memo.
pub fn clear_synthdef_hash_registry() {
    get_synthdef_hash_registry().lock().unwrap().clear();
    clear_param_defaults_memo();
}

fn get_param_defaults_memo() -> &'static Mutex<HashMap<String, Arc<HashMap<String, f32>>>> {
    SYNTHDEF_PARAM_DEFAULTS_MEMO.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drop the memoized param defaults for a single synthdef.
///
/// Must be called whenever that synthdef's IR is (re)inserted so a redefined
/// body with different params never serves stale defaults from the memo.
fn invalidate_param_defaults_memo(name: &str) {
    get_param_defaults_memo().lock().unwrap().remove(name);
}

/// Drop the entire param-defaults memo (registry clear / server reconnect).
fn clear_param_defaults_memo() {
    get_param_defaults_memo().lock().unwrap().clear();
}

fn get_deploy_callback() -> &'static Mutex<Option<DeployCallback>> {
    DEPLOY_CALLBACK.get_or_init(|| Mutex::new(None))
}

/// Set the callback function for deploying synthdef bytes to scsynth.
/// This must be called by the host application before any synthdefs are created.
pub fn set_deploy_callback<F>(callback: F)
where
    F: Fn(Vec<u8>) -> Result<(), String> + Send + Sync + 'static,
{
    let mut cb = get_deploy_callback().lock().unwrap();
    *cb = Some(Arc::new(callback));
}

fn deploy_bytes(bytes: Vec<u8>) -> Result<(), SynthDefError> {
    let callback = get_deploy_callback().lock().unwrap();
    if let Some(ref cb) = *callback {
        cb(bytes).map_err(SynthDefError::OscError)
    } else {
        Err(SynthDefError::OscError(
            "No deploy callback set. Call set_deploy_callback first.".to_string(),
        ))
    }
}

fn synthdef_error_to_eval(err: SynthDefError) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        err.to_string().into(),
        Position::NONE,
    ))
}

/// Whether encoded defs are dumped to `/tmp/<name>.scsyndef` for debugging.
///
/// Gated behind a non-empty `VIBELANG_DUMP_SYNTHDEFS`, read once per process.
#[cfg(not(feature = "wasm"))]
fn dump_synthdefs_enabled() -> bool {
    static DUMP: OnceLock<bool> = OnceLock::new();
    *DUMP.get_or_init(|| {
        std::env::var_os("VIBELANG_DUMP_SYNTHDEFS")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    })
}

fn deploy_synthdef_ir(name: &str, ir: GraphIR) -> crate::errors::Result<()> {
    {
        let mut registry = get_synthdef_registry().lock().unwrap();
        registry.insert(name.to_string(), ir.clone());
    }
    // The IR just changed — drop any memoized param defaults for this name so a
    // redefined body with different params can't serve stale values (Fix B).
    invalidate_param_defaults_memo(name);

    log::debug!(
        "[SYNTHDEF] Building synthdef '{}' with {} nodes",
        name,
        ir.nodes.len()
    );
    log::debug!(
        "[SYNTHDEF] Parameters: {:?}",
        ir.params.iter().map(|p| &p.name).collect::<Vec<_>>()
    );

    let bytes = encode_synthdef(&ir)?;
    let hash = hash_synthdef_bytes(&bytes);

    // Skip re-sending a byte-identical body. The hash registry is process-global
    // and cleared on scsynth (re)connect, so a fresh server always gets a full
    // re-send; within a run this collapses redundant redeploys on reload.
    if get_synthdef_hash(name) == Some(hash) {
        log::debug!(
            "[SYNTHDEF] '{}' unchanged (hash {:016x}) — skip redeploy",
            name,
            hash
        );
        return Ok(());
    }

    log::debug!(
        "[SYNTHDEF] Encoded synthdef '{}' ({} bytes)",
        name,
        bytes.len()
    );

    // Debug dump, gated behind VIBELANG_DUMP_SYNTHDEFS. Skipped in WASM (no fs).
    #[cfg(not(feature = "wasm"))]
    if dump_synthdefs_enabled() {
        let filename = format!("/tmp/{}.scsyndef", name);
        std::fs::write(&filename, &bytes).ok();
    }

    log::debug!("[SYNTHDEF] Sending '{}' to scsynth...", name);
    deploy_bytes(bytes)?;
    // Register the hash only after a successful deploy so a failed send is
    // retried (not hash-skipped) on the next eval.
    register_synthdef_hash(name.to_string(), hash);
    log::debug!("[SYNTHDEF] ✓ SynthDef '{}' loaded successfully", name);
    Ok(())
}

fn deploy_fx_ir(name: &str, ir: GraphIR) -> crate::errors::Result<()> {
    {
        let mut registry = get_effect_registry().lock().unwrap();
        registry.insert(name.to_string(), ir.clone());
    }

    let bytes = encode_synthdef(&ir)?;
    let hash = hash_synthdef_bytes(&bytes);
    if get_synthdef_hash(name) == Some(hash) {
        log::debug!(
            "[FX] '{}' unchanged (hash {:016x}) — skip redeploy",
            name,
            hash
        );
        return Ok(());
    }
    deploy_bytes(bytes)?;
    register_synthdef_hash(name.to_string(), hash);
    log::debug!("[FX] ✓ Effect '{}' loaded successfully", name);
    Ok(())
}

/// Builder handle for SynthDef creation via method chaining.
#[derive(Clone, Debug)]
pub struct SynthDefBuilderHandle {
    synthdef: SynthDef,
}

impl SynthDefBuilderHandle {
    pub fn new(name: String) -> Self {
        Self {
            synthdef: SynthDef::new(name),
        }
    }

    pub fn param(mut self, name: ImmutableString, default: f64) -> Self {
        self.synthdef.arg_f(name.into_owned(), default);
        self
    }

    pub fn glide_ms(mut self, name: ImmutableString, ms: f64) -> Self {
        self.synthdef.glide_ms(name.into_owned(), ms);
        self
    }

    pub fn out_bus(mut self, tag: ImmutableString) -> Self {
        self.synthdef.out_bus(tag.into_owned());
        self
    }

    /// Declare a named audio-rate input port (1 channel by default).
    pub fn input(mut self, name: ImmutableString) -> Result<Self, Box<EvalAltResult>> {
        self.synthdef
            .input(name.into_owned(), 1)
            .map_err(synthdef_error_to_eval)?;
        Ok(self)
    }

    /// Declare a named audio-rate input port with explicit channel count.
    pub fn input_with_channels(
        mut self,
        name: ImmutableString,
        channels: i64,
    ) -> Result<Self, Box<EvalAltResult>> {
        if channels != 1 && channels != 2 {
            return Err(synthdef_error_to_eval(SynthDefError::ValidationError(
                format!(
                    "Input port channels must be 1 (mono) or 2 (stereo), got {}; named-input routing currently supports audio-rate mono/stereo only",
                    channels
                ),
            )));
        }
        self.synthdef
            .input(name.into_owned(), channels as u8)
            .map_err(synthdef_error_to_eval)?;
        Ok(self)
    }

    /// Declare a named audio-rate output port (1 channel by default).
    pub fn output(mut self, name: ImmutableString) -> Result<Self, Box<EvalAltResult>> {
        self.synthdef
            .output(name.into_owned(), 1)
            .map_err(synthdef_error_to_eval)?;
        Ok(self)
    }

    /// Declare a named audio-rate output port with explicit channel count.
    pub fn output_with_channels(
        mut self,
        name: ImmutableString,
        channels: i64,
    ) -> Result<Self, Box<EvalAltResult>> {
        if !(1..=255).contains(&channels) {
            return Err(synthdef_error_to_eval(SynthDefError::ValidationError(
                format!("Output port channels must be in 1..=255, got {}", channels),
            )));
        }
        self.synthdef
            .output(name.into_owned(), channels as u8)
            .map_err(synthdef_error_to_eval)?;
        Ok(self)
    }

    /// Declare a named control-rate output port (1 channel by default).
    ///
    /// Control-rate ports drive `Out.kr` codegen and feed control buses for
    /// CV-to-param routing (`MapN`).
    pub fn output_kr(mut self, name: ImmutableString) -> Result<Self, Box<EvalAltResult>> {
        self.synthdef
            .output_kr(name.into_owned(), 1)
            .map_err(synthdef_error_to_eval)?;
        Ok(self)
    }

    /// Declare a named control-rate output port with explicit channel count.
    pub fn output_kr_with_channels(
        mut self,
        name: ImmutableString,
        channels: i64,
    ) -> Result<Self, Box<EvalAltResult>> {
        if !(1..=255).contains(&channels) {
            return Err(synthdef_error_to_eval(SynthDefError::ValidationError(
                format!("Output port channels must be in 1..=255, got {}", channels),
            )));
        }
        self.synthdef
            .output_kr(name.into_owned(), channels as u8)
            .map_err(synthdef_error_to_eval)?;
        Ok(self)
    }

    /// Declare a named trigger-rate output port (1 channel by default).
    ///
    /// Trigger ports share the control-bus / `Out.kr` codegen path with kr
    /// ports; the `Tr` rate tag drives v3 trigger routing
    /// (`.to_trigger`, trigger-mixer synthdef) instead of CV-to-param `MapN`.
    pub fn output_tr(mut self, name: ImmutableString) -> Result<Self, Box<EvalAltResult>> {
        self.synthdef
            .output_tr(name.into_owned(), 1)
            .map_err(synthdef_error_to_eval)?;
        Ok(self)
    }

    /// Declare a named trigger-rate output port with explicit channel count.
    pub fn output_tr_with_channels(
        mut self,
        name: ImmutableString,
        channels: i64,
    ) -> Result<Self, Box<EvalAltResult>> {
        if !(1..=255).contains(&channels) {
            return Err(synthdef_error_to_eval(SynthDefError::ValidationError(
                format!("Output port channels must be in 1..=255, got {}", channels),
            )));
        }
        self.synthdef
            .output_tr(name.into_owned(), channels as u8)
            .map_err(synthdef_error_to_eval)?;
        Ok(self)
    }

    /// Declare an explicit output-port list.
    ///
    /// Currently this is intentionally narrow: `outputs([])` is the spelling
    /// for side-effect-only synthdefs with no output ports. Named output ports
    /// still use repeated `.output(...)`, `.output_kr(...)`, or `.output_tr(...)`
    /// calls so rate/channel metadata stays explicit.
    pub fn outputs(mut self, outputs: rhai::Array) -> Result<Self, Box<EvalAltResult>> {
        if !outputs.is_empty() {
            return Err(synthdef_error_to_eval(SynthDefError::ValidationError(
                "outputs(...) currently only accepts an empty list: use outputs([]) for zero-output synthdefs, or repeated .output/.output_kr/.output_tr calls for named ports".to_string(),
            )));
        }
        self.synthdef
            .set_outputs(Vec::new())
            .map_err(synthdef_error_to_eval)?;
        Ok(self)
    }

    fn build(self, closure: rhai::FnPtr) -> crate::errors::Result<GraphIR> {
        self.synthdef.build_body_closure_with_options(closure, true)
    }

    pub fn body(self, closure: rhai::FnPtr) -> Result<(), Box<EvalAltResult>> {
        let name = self.synthdef.name.clone();
        let outputs = self.synthdef.outputs.clone();
        let inputs = self.synthdef.inputs.clone();
        let ir = self.build(closure).map_err(synthdef_error_to_eval)?;
        register_synthdef_outputs(name.clone(), outputs);
        register_synthdef_inputs(name.clone(), inputs);
        deploy_synthdef_ir(&name, ir).map_err(synthdef_error_to_eval)
    }

    pub fn body_map(self, closure: rhai::FnPtr) -> Result<(), Box<EvalAltResult>> {
        let name = self.synthdef.name.clone();
        let outputs = self.synthdef.outputs.clone();
        let inputs = self.synthdef.inputs.clone();
        let ir = self
            .synthdef
            .build_body_map_closure_with_options(closure, true)
            .map_err(synthdef_error_to_eval)?;
        register_synthdef_outputs(name.clone(), outputs);
        register_synthdef_inputs(name.clone(), inputs);
        deploy_synthdef_ir(&name, ir).map_err(synthdef_error_to_eval)
    }
}

/// Builder handle for FX creation via method chaining.
#[derive(Clone, Debug)]
pub struct FxBuilderHandle {
    synthdef: SynthDef,
    num_channels: usize,
}

impl FxBuilderHandle {
    pub fn new(name: String) -> Self {
        Self {
            synthdef: SynthDef::new(name),
            num_channels: 2,
        }
    }

    pub fn param(mut self, name: ImmutableString, default: f64) -> Self {
        self.synthdef.arg_f(name.into_owned(), default);
        self
    }

    pub fn glide_ms(mut self, name: ImmutableString, ms: f64) -> Self {
        self.synthdef.glide_ms(name.into_owned(), ms);
        self
    }

    pub fn channels(mut self, channels: i64) -> Self {
        if channels > 0 {
            self.num_channels = channels as usize;
        }
        self
    }

    pub fn body(self, closure: rhai::FnPtr) -> Result<(), Box<EvalAltResult>> {
        if self.num_channels == 0 {
            return Err(synthdef_error_to_eval(SynthDefError::ValidationError(
                "FX must use at least one channel".to_string(),
            )));
        }
        let name = self.synthdef.name.clone();
        let ir = self
            .synthdef
            .build_effect_closure(closure, self.num_channels)
            .map_err(synthdef_error_to_eval)?;
        deploy_fx_ir(&name, ir).map_err(synthdef_error_to_eval)
    }

    pub fn body_map(self, closure: rhai::FnPtr) -> Result<(), Box<EvalAltResult>> {
        if self.num_channels == 0 {
            return Err(synthdef_error_to_eval(SynthDefError::ValidationError(
                "FX must use at least one channel".to_string(),
            )));
        }
        let name = self.synthdef.name.clone();
        let ir = self
            .synthdef
            .build_effect_map_closure(closure, self.num_channels)
            .map_err(synthdef_error_to_eval)?;
        deploy_fx_ir(&name, ir).map_err(synthdef_error_to_eval)
    }
}

/// Check if a SynthDef exists in the registry.
pub fn synthdef_exists(name: &str) -> bool {
    get_synthdef_registry().lock().unwrap().contains_key(name)
}

/// Check if an Effect exists in the registry.
pub fn effect_exists(name: &str) -> bool {
    get_effect_registry().lock().unwrap().contains_key(name)
}

/// Get the names of all registered FX synthdefs.
///
/// Snapshot of the EFFECT_REGISTRY name set at call time. Used by Rhai-side
/// validators to render "did-you-mean" hints when the script names an FX that
/// isn't registered.
pub fn get_all_effect_names() -> Vec<String> {
    get_effect_registry()
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

/// Check if a name exists as either a synthdef or effect.
pub fn synthdef_or_effect_exists(name: &str) -> bool {
    synthdef_exists(name) || effect_exists(name)
}

/// Register a SynthDef IR in the registry (for auto-generated synthdefs).
pub fn register_synthdef_ir(name: String, ir: GraphIR) {
    invalidate_param_defaults_memo(&name);
    let mut registry = get_synthdef_registry().lock().unwrap();
    registry.insert(name, ir);
}

/// Register an Effect IR in the registry (for auto-generated effects and tests).
///
/// Mirror of [`register_synthdef_ir`] for the FX side. Used by Rhai-surface
/// tests that need to seed the effect registry without running a full
/// `define_fx(...).body(...)` builder pipeline.
pub fn register_effect_ir(name: String, ir: GraphIR) {
    let mut registry = get_effect_registry().lock().unwrap();
    registry.insert(name, ir);
}

/// Get default parameter values for a synthdef.
///
/// Convenience wrapper over [`get_synthdef_param_defaults_arc`] that hands back
/// an owned map (empty for unknown synthdefs), for callers that aren't on a hot
/// path. Hot-path callers should prefer the `_arc` variant and clone the `Arc`.
pub fn get_synthdef_param_defaults(name: &str) -> HashMap<String, f32> {
    get_synthdef_param_defaults_arc(name)
        .map(|arc| (*arc).clone())
        .unwrap_or_default()
}

/// Memoized single-value param defaults for a synthdef, shared as an `Arc`.
///
/// Built lazily from the synthdef IR registry on first request and cached until
/// the synthdef is redeployed or the registries are cleared (see
/// [`invalidate_param_defaults_memo`] / [`clear_param_defaults_memo`]). Returns
/// `None` for unknown synthdefs. The note-on hot path clones the `Arc`
/// (pointer bump) instead of rebuilding the map under the global State lock.
pub fn get_synthdef_param_defaults_arc(name: &str) -> Option<Arc<HashMap<String, f32>>> {
    if let Some(defaults) = get_param_defaults_memo().lock().unwrap().get(name).cloned() {
        return Some(defaults);
    }
    // Build outside the memo lock; never hold the memo and IR-registry locks at
    // the same time. A benign race can build twice — both maps are identical.
    let arc = {
        let registry = get_synthdef_registry().lock().unwrap();
        let ir = registry.get(name)?;
        let mut defaults = HashMap::new();
        for param in &ir.params {
            if param.default.len() == 1 {
                defaults.insert(param.name.clone(), param.default[0]);
            }
        }
        Arc::new(defaults)
    };
    get_param_defaults_memo()
        .lock()
        .unwrap()
        .insert(name.to_string(), arc.clone());
    Some(arc)
}

/// Get default parameter values for an effect.
pub fn get_effect_param_defaults(name: &str) -> HashMap<String, f32> {
    let registry = get_effect_registry().lock().unwrap();
    if let Some(ir) = registry.get(name) {
        let mut defaults = HashMap::new();
        for param in &ir.params {
            if param.default.len() == 1 {
                defaults.insert(param.name.clone(), param.default[0]);
            }
        }
        defaults
    } else {
        HashMap::new()
    }
}

/// Get all registered synthdefs as encoded bytes.
///
/// Returns a vector of (name, encoded_bytes) pairs.
pub fn get_all_synthdefs_encoded() -> Vec<(String, Vec<u8>)> {
    let registry = get_synthdef_registry().lock().unwrap();
    let mut result = Vec::new();
    for (name, ir) in registry.iter() {
        if let Ok(bytes) = encode_synthdef(ir) {
            result.push((name.clone(), bytes));
        }
    }
    result
}

/// Get all registered effects as encoded bytes.
///
/// Returns a vector of (name, encoded_bytes) pairs.
pub fn get_all_effects_encoded() -> Vec<(String, Vec<u8>)> {
    let registry = get_effect_registry().lock().unwrap();
    let mut result = Vec::new();
    for (name, ir) in registry.iter() {
        if let Ok(bytes) = encode_synthdef(ir) {
            result.push((name.clone(), bytes));
        }
    }
    result
}

/// Clear all registered synthdefs from the registry.
///
/// Useful for testing or when reloading scripts.
pub fn clear_synthdef_registry() {
    clear_param_defaults_memo();
    let mut registry = get_synthdef_registry().lock().unwrap();
    registry.clear();
}

/// Clear all registered effects from the registry.
///
/// Useful for testing or when reloading scripts.
pub fn clear_effect_registry() {
    let mut registry = get_effect_registry().lock().unwrap();
    registry.clear();
}

/// Register the SynthDef and FX builder types and functions with a Rhai engine.
pub fn register_synthdef_api(engine: &mut Engine) {
    // Register builder types
    engine
        .register_type::<SynthDefBuilderHandle>()
        .register_fn("param", SynthDefBuilderHandle::param)
        .register_fn("glide_ms", SynthDefBuilderHandle::glide_ms)
        .register_fn("out_bus", SynthDefBuilderHandle::out_bus)
        .register_fn("input", SynthDefBuilderHandle::input)
        .register_fn("input", SynthDefBuilderHandle::input_with_channels)
        .register_fn("output", SynthDefBuilderHandle::output)
        .register_fn("output", SynthDefBuilderHandle::output_with_channels)
        .register_fn("output_kr", SynthDefBuilderHandle::output_kr)
        .register_fn("output_kr", SynthDefBuilderHandle::output_kr_with_channels)
        .register_fn("output_tr", SynthDefBuilderHandle::output_tr)
        .register_fn("output_tr", SynthDefBuilderHandle::output_tr_with_channels)
        .register_fn("outputs", SynthDefBuilderHandle::outputs)
        .register_fn("body", SynthDefBuilderHandle::body)
        .register_fn("body_map", SynthDefBuilderHandle::body_map);

    engine
        .register_type::<FxBuilderHandle>()
        .register_fn("param", FxBuilderHandle::param)
        .register_fn("glide_ms", FxBuilderHandle::glide_ms)
        .register_fn("channels", FxBuilderHandle::channels)
        .register_fn("body", FxBuilderHandle::body)
        .register_fn("body_map", FxBuilderHandle::body_map);

    // Register entry point functions
    engine.register_fn("define_synthdef", |name: String| -> SynthDefBuilderHandle {
        SynthDefBuilderHandle::new(name)
    });

    engine.register_fn("define_fx", |name: String| -> FxBuilderHandle {
        FxBuilderHandle::new(name)
    });

    // Backward-compatible overload that accepts a closure receiving the builder
    engine.register_fn(
        "define_synthdef",
        |ctx: NativeCallContext,
         name: String,
         closure: rhai::FnPtr|
         -> Result<(), Box<EvalAltResult>> {
            let builder = SynthDefBuilderHandle::new(name);
            closure
                .call_within_context::<Dynamic>(&ctx, (builder,))
                .map(|_| ())
        },
    );

    engine.register_fn(
        "define_fx",
        |ctx: NativeCallContext,
         name: String,
         closure: rhai::FnPtr|
         -> Result<(), Box<EvalAltResult>> {
            let builder = FxBuilderHandle::new(name);
            closure
                .call_within_context::<Dynamic>(&ctx, (builder,))
                .map(|_| ())
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{register_dsp_api, Input, PortRate, Rate};

    const INPUT_CHANNEL_ERROR: &str = "Input port channels must be 1 (mono) or 2 (stereo)";

    fn test_engine() -> Engine {
        let mut engine = Engine::new();
        register_dsp_api(&mut engine);
        engine
    }

    fn reset_registries() {
        clear_synthdef_registry();
        clear_synthdef_outputs_registry();
        clear_synthdef_inputs_registry();
        set_deploy_callback(|_| Ok(()));
    }

    fn eval_err(script: &str) -> String {
        reset_registries();
        test_engine()
            .eval::<Dynamic>(script)
            .expect_err("script should fail")
            .to_string()
    }

    fn registered_ir(name: &str) -> GraphIR {
        get_synthdef_registry()
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .expect("registered synthdef")
    }

    #[test]
    fn rhai_synthdef_outputs_empty_registers_zero_output_manifest() {
        reset_registries();
        let _ = test_engine()
            .eval::<Dynamic>(
                r#"
                define_synthdef("rhai_zero_output")
                    .outputs([])
                    .body(|| []);
                "#,
            )
            .expect("define zero-output synthdef");

        assert_eq!(get_synthdef_outputs("rhai_zero_output"), Some(Vec::new()));
        let ir = registered_ir("rhai_zero_output");
        assert!(ir.params.is_empty(), "no out params: {:?}", ir.params);
        assert!(ir.nodes.iter().all(|n| n.name != "Out"));
    }

    #[test]
    fn rhai_synthdef_input_mono_body_map_registers_input_manifest() {
        reset_registries();
        let _ = test_engine()
            .eval::<Dynamic>(
                r#"
                define_synthdef("rhai_input_mono")
                    .input("audio")
                    .body_map(|p| p.inputs.audio);
                "#,
            )
            .expect("define mono input synthdef");

        assert_eq!(
            get_synthdef_inputs("rhai_input_mono"),
            Some(vec![InputPort {
                name: "audio".to_string(),
                channels: 1,
                rate: PortRate::Ar,
            }])
        );

        let ir = registered_ir("rhai_input_mono");
        assert!(
            ir.params.iter().any(|p| p.name == "__in0"),
            "hidden input bus param not found: {:?}",
            ir.params
        );
        let in_nodes: Vec<_> = ir.nodes.iter().filter(|n| n.name == "In").collect();
        assert_eq!(in_nodes.len(), 1, "expected one input reader");
        assert_eq!(in_nodes[0].rate, Rate::Audio);
        assert_eq!(in_nodes[0].num_outputs, 1);
    }

    #[test]
    fn rhai_synthdef_input_stereo_body_map_access() {
        reset_registries();
        let _ = test_engine()
            .eval::<Dynamic>(
                r#"
                define_synthdef("rhai_input_stereo")
                    .input("wide", 2)
                    .body_map(|p| [p.inputs.wide[0], p.inputs.wide[1]]);
                "#,
            )
            .expect("define stereo input synthdef");

        assert_eq!(
            get_synthdef_inputs("rhai_input_stereo"),
            Some(vec![InputPort {
                name: "wide".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }])
        );

        let ir = registered_ir("rhai_input_stereo");
        let in_nodes: Vec<_> = ir.nodes.iter().filter(|n| n.name == "In").collect();
        assert_eq!(in_nodes.len(), 1, "expected one stereo input reader");
        assert_eq!(in_nodes[0].rate, Rate::Audio);
        assert_eq!(in_nodes[0].num_outputs, 2);
        assert!(matches!(
            in_nodes[0].inputs.first(),
            Some(Input::Node {
                output_index: 0,
                ..
            })
        ));
    }

    #[test]
    fn rhai_synthdef_input_rejects_bad_width() {
        for channels in [0, 3] {
            let err = eval_err(&format!(
                r#"
                define_synthdef("rhai_input_bad_width_{channels}")
                    .input("audio", {channels})
                    .body_map(|p| p.inputs.audio);
                "#
            ));
            assert!(err.contains(INPUT_CHANNEL_ERROR), "err = {}", err);
            assert!(err.contains(&format!("got {}", channels)), "err = {}", err);
        }
    }

    #[test]
    fn rhai_synthdef_input_duplicate_name_errors() {
        let err = eval_err(
            r#"
            define_synthdef("rhai_input_dup")
                .input("audio")
                .input("audio", 2)
                .body_map(|p| p.inputs.audio);
            "#,
        );

        assert!(err.contains("Duplicate input port name"), "err = {}", err);
        assert!(err.contains("audio"), "err = {}", err);
    }

    #[test]
    fn rhai_synthdef_input_reserved_inputs_param_errors() {
        let err = eval_err(
            r#"
            define_synthdef("rhai_input_reserved_param")
                .input("audio")
                .param("inputs", 0.0)
                .body_map(|p| p.inputs.audio);
            "#,
        );

        assert!(err.contains("inputs"), "err = {}", err);
        assert!(err.contains("reserved"), "err = {}", err);
    }

    #[test]
    fn rhai_synthdef_input_requires_body_map() {
        let err = eval_err(
            r#"
            define_synthdef("rhai_input_requires_body_map")
                .input("audio")
                .body(|| 0.0);
            "#,
        );

        assert!(err.contains("body_map"), "err = {}", err);
        assert!(err.contains("p.inputs"), "err = {}", err);
    }
}
