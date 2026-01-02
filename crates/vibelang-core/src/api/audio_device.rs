//! Audio device API for Rhai scripts.
//!
//! This module provides functions to list available audio devices from within .vibe scripts.

use crate::audio_device::{list_audio_devices, AudioDeviceInfo};
use rhai::{Array, Dynamic, Engine, Map};

/// Register audio device API functions with a Rhai engine.
pub fn register(engine: &mut Engine) {
    engine.register_fn("list_audio_devices", rhai_list_audio_devices);
}

/// List available audio devices.
///
/// Returns an array of maps, where each map contains:
/// - `name`: Device name (string)
/// - `max_input_channels`: Maximum input channels (int)
/// - `max_output_channels`: Maximum output channels (int)
/// - `sample_rates`: Array of supported sample rates (array of ints)
/// - `is_default_input`: Whether this is the default input device (bool)
/// - `is_default_output`: Whether this is the default output device (bool)
///
/// # Example
///
/// ```rhai
/// let devices = list_audio_devices();
/// for device in devices {
///     print(`${device.name}: ${device.max_output_channels} outputs`);
/// }
/// ```
fn rhai_list_audio_devices() -> Array {
    match list_audio_devices() {
        Ok(devices) => devices.into_iter().map(device_to_dynamic).collect(),
        Err(e) => {
            log::error!("Failed to list audio devices: {}", e);
            Array::new()
        }
    }
}

/// Convert an AudioDeviceInfo to a Rhai Dynamic map.
fn device_to_dynamic(info: AudioDeviceInfo) -> Dynamic {
    let mut map = Map::new();

    map.insert("name".into(), Dynamic::from(info.name));
    map.insert("max_input_channels".into(), Dynamic::from(info.max_input_channels as i64));
    map.insert("max_output_channels".into(), Dynamic::from(info.max_output_channels as i64));
    map.insert("is_default_input".into(), Dynamic::from(info.is_default_input));
    map.insert("is_default_output".into(), Dynamic::from(info.is_default_output));

    // Convert sample rates to a Rhai array
    let sample_rates: Array = info
        .sample_rates
        .into_iter()
        .map(|r| Dynamic::from(r as i64))
        .collect();
    map.insert("sample_rates".into(), Dynamic::from(sample_rates));

    Dynamic::from_map(map)
}
