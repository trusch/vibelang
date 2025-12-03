//! SynthDef API for Rhai scripts.
//!
//! This module provides the `define_synthdef` and `define_fx` functions
//! that allow users to create SuperCollider SynthDefs from Rhai closures.
//!
//! Note: This module requires vibelang-dsp for actual SynthDef generation.
//! The actual DSP registration (UGens, NodeRef, etc.) must be done by the
//! CLI or host application that imports both vibelang-core and vibelang-dsp.

use rhai::Engine;

use super::require_handle;

/// Register synthdef placeholder functions.
///
/// Note: The actual `define_synthdef` and `define_fx` functions that work
/// with DSP closures must be registered by the host application using
/// vibelang-dsp's registration functions.
///
/// This module only registers the message-sending parts.
pub fn register(engine: &mut Engine) {
    // The actual define_synthdef/define_fx functions must be registered
    // by the host that has access to vibelang-dsp.
    //
    // This is intentional - we don't want vibelang-core to depend on vibelang-dsp.
    //
    // The CLI should call vibelang_dsp::register_dsp_api(engine) to add:
    // - define_synthdef(name, closure)
    // - define_fx(name, closure)
    // - All UGen functions (SinOsc, LPF, etc.)
    // - NodeRef type and operators

    // We can register some utility functions here
    engine.register_fn("load_synthdef_bytes", load_synthdef_bytes);
}

/// Load a pre-compiled synthdef from bytes.
pub fn load_synthdef_bytes(bytes: rhai::Blob) {
    let handle = require_handle();
    if let Err(e) = handle.scsynth().d_recv_bytes(bytes.to_vec()) {
        log::error!("Failed to load synthdef: {}", e);
    }
}
