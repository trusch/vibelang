//! WebScsynth backend for WASM.
//!
//! This backend communicates with SuperSonic (scsynth compiled to WASM)
//! via JavaScript bindings. OSC messages are serialized in Rust and sent
//! to JavaScript which forwards them to the SuperSonic AudioWorklet.

use crate::backend::{AddAction, Backend, BufferInfo};
use crate::compat::Instant;
use crate::types::{BufferId, NodeId, ParamMap};
use async_trait::async_trait;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use wasm_bindgen::prelude::*;

/// Error type for WebScsynth operations.
#[derive(Error, Debug)]
pub enum WebScsynthError {
    #[error("JavaScript error: {0}")]
    JsError(String),
    #[error("Backend not ready")]
    NotReady,
    #[error("Buffer operation failed: {0}")]
    BufferError(String),
}

impl From<JsValue> for WebScsynthError {
    fn from(value: JsValue) -> Self {
        let msg = value
            .as_string()
            .unwrap_or_else(|| format!("{:?}", value));
        WebScsynthError::JsError(msg)
    }
}

// JavaScript bindings - these functions are implemented in JavaScript
// and called from Rust to communicate with SuperSonic.

#[wasm_bindgen]
extern "C" {
    /// Send a raw OSC message to SuperSonic.
    #[wasm_bindgen(js_namespace = vibelangBridge, js_name = sendOsc)]
    fn js_send_osc(address: &str, args: &JsValue) -> js_sys::Promise;

    /// Load a synthdef from bytes.
    #[wasm_bindgen(js_namespace = vibelangBridge, js_name = loadSynthdef)]
    fn js_load_synthdef(name: &str, data: &[u8]) -> js_sys::Promise;

    /// Load a buffer from a URL.
    #[wasm_bindgen(js_namespace = vibelangBridge, js_name = loadBuffer)]
    fn js_load_buffer(id: i32, url: &str) -> js_sys::Promise;

    /// Allocate an empty buffer.
    #[wasm_bindgen(js_namespace = vibelangBridge, js_name = allocBuffer)]
    fn js_alloc_buffer(id: i32, frames: u32, channels: u16) -> js_sys::Promise;

    /// Free a buffer.
    #[wasm_bindgen(js_namespace = vibelangBridge, js_name = freeBuffer)]
    fn js_free_buffer(id: i32) -> js_sys::Promise;

    /// Check if SuperSonic is ready.
    #[wasm_bindgen(js_namespace = vibelangBridge, js_name = isReady)]
    fn js_is_ready() -> bool;

    /// Get the sample rate.
    #[wasm_bindgen(js_namespace = vibelangBridge, js_name = getSampleRate)]
    fn js_get_sample_rate() -> f64;
}

/// WebScsynth backend for WASM.
///
/// This backend communicates with SuperSonic (scsynth WASM) through
/// JavaScript bindings. It serializes OSC messages and sends them
/// to the SuperSonic AudioWorklet via MessagePort.
pub struct WebScsynthBackend {
    ready: AtomicBool,
    sample_rate: f64,
}

impl WebScsynthBackend {
    /// Create a new WebScsynth backend.
    ///
    /// Note: The SuperSonic AudioWorklet must be initialized in JavaScript
    /// before creating this backend.
    pub fn new() -> Self {
        Self {
            ready: AtomicBool::new(false),
            sample_rate: 44100.0,
        }
    }

    /// Initialize the backend.
    ///
    /// This should be called after SuperSonic has been set up in JavaScript.
    pub async fn init(&self) -> Result<(), WebScsynthError> {
        // Check if JavaScript side is ready
        if js_is_ready() {
            self.ready.store(true, Ordering::SeqCst);
            Ok(())
        } else {
            Err(WebScsynthError::NotReady)
        }
    }

    /// Helper to convert a JsValue Promise to a Result.
    async fn await_promise(promise: js_sys::Promise) -> Result<JsValue, WebScsynthError> {
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(WebScsynthError::from)
    }

    /// Build OSC args array for s_new (synth creation).
    fn build_synth_args(
        def: &str,
        node: NodeId,
        action: AddAction,
        target: NodeId,
        params: &ParamMap,
    ) -> JsValue {
        let args = js_sys::Array::new();
        args.push(&JsValue::from_str(def));
        args.push(&JsValue::from_f64(node.0 as f64));
        args.push(&JsValue::from_f64(action as i32 as f64));
        args.push(&JsValue::from_f64(target.0 as f64));

        // Add params as key-value pairs
        for (key, value) in params {
            args.push(&JsValue::from_str(key));
            args.push(&JsValue::from_f64(*value as f64));
        }

        args.into()
    }

    /// Build OSC args array for g_new (group creation).
    fn build_group_args(node: NodeId, action: AddAction, target: NodeId) -> JsValue {
        let args = js_sys::Array::new();
        args.push(&JsValue::from_f64(node.0 as f64));
        args.push(&JsValue::from_f64(action as i32 as f64));
        args.push(&JsValue::from_f64(target.0 as f64));
        args.into()
    }
}

impl Default for WebScsynthBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Backend for WebScsynthBackend {
    type Error = WebScsynthError;

    async fn load_synthdef(&self, name: &str, data: &[u8]) -> Result<(), Self::Error> {
        let promise = js_load_synthdef(name, data);
        Self::await_promise(promise).await?;
        Ok(())
    }

    async fn create_synth(
        &self,
        def: &str,
        node: NodeId,
        target: NodeId,
        action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        let args = Self::build_synth_args(def, node, action, target, params);
        let promise = js_send_osc("/s_new", &args);
        Self::await_promise(promise).await?;
        Ok(())
    }

    async fn create_group(
        &self,
        node: NodeId,
        target: NodeId,
        action: AddAction,
    ) -> Result<(), Self::Error> {
        let args = Self::build_group_args(node, action, target);
        let promise = js_send_osc("/g_new", &args);
        Self::await_promise(promise).await?;
        Ok(())
    }

    async fn free_node(&self, node: NodeId) -> Result<(), Self::Error> {
        let args = js_sys::Array::new();
        args.push(&JsValue::from_f64(node.0 as f64));
        let promise = js_send_osc("/n_free", &args.into());
        Self::await_promise(promise).await?;
        Ok(())
    }

    async fn run_node(&self, node: NodeId, running: bool) -> Result<(), Self::Error> {
        let args = js_sys::Array::new();
        args.push(&JsValue::from_f64(node.0 as f64));
        args.push(&JsValue::from_f64(if running { 1.0 } else { 0.0 }));
        let promise = js_send_osc("/n_run", &args.into());
        Self::await_promise(promise).await?;
        Ok(())
    }

    async fn set_param(&self, node: NodeId, param: &str, value: f32) -> Result<(), Self::Error> {
        let args = js_sys::Array::new();
        args.push(&JsValue::from_f64(node.0 as f64));
        args.push(&JsValue::from_str(param));
        args.push(&JsValue::from_f64(value as f64));
        let promise = js_send_osc("/n_set", &args.into());
        Self::await_promise(promise).await?;
        Ok(())
    }

    async fn map_param_to_bus(
        &self,
        node: NodeId,
        param: &str,
        bus: u32,
    ) -> Result<(), Self::Error> {
        // /n_map: node_id, param_name, bus_index
        let args = js_sys::Array::new();
        args.push(&JsValue::from_f64(node.0 as f64));
        args.push(&JsValue::from_str(param));
        args.push(&JsValue::from_f64(bus as f64));
        let promise = js_send_osc("/n_map", &args.into());
        Self::await_promise(promise).await?;
        Ok(())
    }

    async fn load_buffer(&self, id: BufferId, path: &Path) -> Result<BufferInfo, Self::Error> {
        // In WASM, the path is treated as a URL
        let url = path.to_string_lossy();
        let promise = js_load_buffer(id.0 as i32, &url);
        let result = Self::await_promise(promise).await?;

        // Parse the result (JavaScript returns {frames, channels, sampleRate})
        let frames = js_sys::Reflect::get(&result, &JsValue::from_str("frames"))
            .map_err(|_| WebScsynthError::BufferError("missing frames".into()))?
            .as_f64()
            .ok_or_else(|| WebScsynthError::BufferError("invalid frames".into()))?
            as u32;

        let channels = js_sys::Reflect::get(&result, &JsValue::from_str("channels"))
            .map_err(|_| WebScsynthError::BufferError("missing channels".into()))?
            .as_f64()
            .ok_or_else(|| WebScsynthError::BufferError("invalid channels".into()))?
            as u16;

        let sample_rate = js_sys::Reflect::get(&result, &JsValue::from_str("sampleRate"))
            .map_err(|_| WebScsynthError::BufferError("missing sampleRate".into()))?
            .as_f64()
            .ok_or_else(|| WebScsynthError::BufferError("invalid sampleRate".into()))?;

        Ok(BufferInfo {
            frames,
            channels,
            sample_rate,
        })
    }

    async fn alloc_buffer(
        &self,
        id: BufferId,
        frames: u32,
        channels: u16,
    ) -> Result<BufferInfo, Self::Error> {
        let promise = js_alloc_buffer(id.0 as i32, frames, channels);
        Self::await_promise(promise).await?;

        Ok(BufferInfo {
            frames,
            channels,
            sample_rate: self.sample_rate,
        })
    }

    async fn write_buffer(&self, _id: BufferId, _path: &Path) -> Result<(), Self::Error> {
        // Writing buffers to files is not supported in WASM
        // Could potentially download via JavaScript, but for now just no-op
        Ok(())
    }

    async fn free_buffer(&self, id: BufferId) -> Result<(), Self::Error> {
        let promise = js_free_buffer(id.0 as i32);
        Self::await_promise(promise).await?;
        Ok(())
    }

    fn current_time(&self) -> Instant {
        Instant::now()
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst) && js_is_ready()
    }

    fn sample_rate(&self) -> f64 {
        js_get_sample_rate()
    }

    fn block_size(&self) -> u32 {
        128 // SuperSonic default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_scsynth_error_from_string() {
        let err = WebScsynthError::JsError("test error".to_string());
        assert_eq!(err.to_string(), "JavaScript error: test error");
    }

    #[test]
    fn test_web_scsynth_not_ready_error() {
        let err = WebScsynthError::NotReady;
        assert_eq!(err.to_string(), "Backend not ready");
    }
}
