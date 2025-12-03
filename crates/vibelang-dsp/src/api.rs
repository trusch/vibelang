//! Rhai API for SynthDef definition.
//!
//! This module provides the `define_synthdef` and `define_fx` functions
//! for the Rhai scripting environment.
//!
//! Note: This module uses a callback function to deploy synthdefs to scsynth.
//! The callback must be set by the host application (CLI) before using these functions.

use crate::builder::SynthDef;
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
// Callback for deploying synthdef bytes to scsynth
static DEPLOY_CALLBACK: OnceLock<Mutex<Option<DeployCallback>>> = OnceLock::new();

fn get_synthdef_registry() -> &'static Mutex<HashMap<String, GraphIR>> {
    SYNTHDEF_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_effect_registry() -> &'static Mutex<HashMap<String, GraphIR>> {
    EFFECT_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
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
        Err(SynthDefError::OscError("No deploy callback set. Call set_deploy_callback first.".to_string()))
    }
}

fn synthdef_error_to_eval(err: SynthDefError) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        err.to_string().into(),
        Position::NONE,
    ))
}

fn deploy_synthdef_ir(name: &str, ir: GraphIR) -> crate::errors::Result<()> {
    {
        let mut registry = get_synthdef_registry().lock().unwrap();
        registry.insert(name.to_string(), ir.clone());
    }

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
    log::debug!(
        "[SYNTHDEF] Encoded synthdef '{}' ({} bytes)",
        name,
        bytes.len()
    );

    let filename = format!("/tmp/{}.scsyndef", name);
    std::fs::write(&filename, &bytes).ok();

    log::debug!("[SYNTHDEF] Sending '{}' to scsynth...", name);
    deploy_bytes(bytes)?;
    log::info!("[SYNTHDEF] ✓ SynthDef '{}' loaded successfully", name);

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(())
}

fn deploy_fx_ir(name: &str, ir: GraphIR) -> crate::errors::Result<()> {
    {
        let mut registry = get_effect_registry().lock().unwrap();
        registry.insert(name.to_string(), ir.clone());
    }

    let bytes = encode_synthdef(&ir)?;
    deploy_bytes(bytes)?;

    std::thread::sleep(std::time::Duration::from_millis(50));
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

    fn build(self, closure: rhai::FnPtr) -> crate::errors::Result<GraphIR> {
        self.synthdef.build_body_closure_with_options(closure, true)
    }

    pub fn body(self, closure: rhai::FnPtr) -> Result<(), Box<EvalAltResult>> {
        let name = self.synthdef.name.clone();
        let ir = self.build(closure).map_err(synthdef_error_to_eval)?;
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
}

/// Check if a SynthDef exists in the registry.
pub fn synthdef_exists(name: &str) -> bool {
    get_synthdef_registry().lock().unwrap().contains_key(name)
}

/// Check if an Effect exists in the registry.
pub fn effect_exists(name: &str) -> bool {
    get_effect_registry().lock().unwrap().contains_key(name)
}

/// Check if a name exists as either a synthdef or effect.
pub fn synthdef_or_effect_exists(name: &str) -> bool {
    synthdef_exists(name) || effect_exists(name)
}

/// Register a SynthDef IR in the registry (for auto-generated synthdefs).
pub fn register_synthdef_ir(name: String, ir: GraphIR) {
    let mut registry = get_synthdef_registry().lock().unwrap();
    registry.insert(name, ir);
}

/// Get default parameter values for a synthdef.
pub fn get_synthdef_param_defaults(name: &str) -> HashMap<String, f32> {
    let registry = get_synthdef_registry().lock().unwrap();
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

/// Register the SynthDef and FX builder types and functions with a Rhai engine.
pub fn register_synthdef_api(engine: &mut Engine) {
    // Register builder types
    engine
        .register_type::<SynthDefBuilderHandle>()
        .register_fn("param", SynthDefBuilderHandle::param)
        .register_fn("glide_ms", SynthDefBuilderHandle::glide_ms)
        .register_fn("out_bus", SynthDefBuilderHandle::out_bus)
        .register_fn("body", SynthDefBuilderHandle::body);

    engine
        .register_type::<FxBuilderHandle>()
        .register_fn("param", FxBuilderHandle::param)
        .register_fn("glide_ms", FxBuilderHandle::glide_ms)
        .register_fn("channels", FxBuilderHandle::channels)
        .register_fn("body", FxBuilderHandle::body);

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
        |ctx: NativeCallContext, name: String, closure: rhai::FnPtr| -> Result<(), Box<EvalAltResult>> {
            let builder = SynthDefBuilderHandle::new(name);
            closure
                .call_within_context::<Dynamic>(&ctx, (builder,))
                .map(|_| ())
        },
    );

    engine.register_fn(
        "define_fx",
        |ctx: NativeCallContext, name: String, closure: rhai::FnPtr| -> Result<(), Box<EvalAltResult>> {
            let builder = FxBuilderHandle::new(name);
            closure
                .call_within_context::<Dynamic>(&ctx, (builder,))
                .map(|_| ())
        },
    );
}
