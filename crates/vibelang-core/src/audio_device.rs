//! Audio device enumeration and configuration.
//!
//! This module provides cross-platform audio device discovery using the cpal library,
//! and configuration structures for selecting audio devices when starting SuperCollider.

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

/// Information about an audio device.
#[derive(Clone, Debug)]
pub struct AudioDeviceInfo {
    /// Device name (as reported by the system)
    pub name: String,
    /// Maximum supported input channels
    pub max_input_channels: u32,
    /// Maximum supported output channels
    pub max_output_channels: u32,
    /// Supported sample rates (sorted)
    pub sample_rates: Vec<u32>,
    /// Whether this is the default input device
    pub is_default_input: bool,
    /// Whether this is the default output device
    pub is_default_output: bool,
}

/// Audio configuration for scsynth.
#[derive(Clone, Debug)]
pub struct AudioConfig {
    /// Input device name (None = default)
    pub input_device: Option<String>,
    /// Output device name (None = default)
    pub output_device: Option<String>,
    /// Number of input channels
    pub input_channels: u32,
    /// Number of output channels
    pub output_channels: u32,
    /// Sample rate (None = use device default)
    pub sample_rate: Option<u32>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            input_channels: 2,
            output_channels: 2,
            sample_rate: None,
        }
    }
}

impl AudioConfig {
    /// Create a new AudioConfig with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the input device.
    pub fn with_input_device(mut self, device: Option<String>) -> Self {
        self.input_device = device;
        self
    }

    /// Set the output device.
    pub fn with_output_device(mut self, device: Option<String>) -> Self {
        self.output_device = device;
        self
    }

    /// Set the number of input channels.
    pub fn with_input_channels(mut self, channels: u32) -> Self {
        self.input_channels = channels;
        self
    }

    /// Set the number of output channels.
    pub fn with_output_channels(mut self, channels: u32) -> Self {
        self.output_channels = channels;
        self
    }

    /// Set the sample rate.
    pub fn with_sample_rate(mut self, rate: Option<u32>) -> Self {
        self.sample_rate = rate;
        self
    }
}

/// List all available audio devices.
///
/// Returns a list of audio devices with their names, channel counts, and sample rates.
/// Uses cpal for cross-platform device enumeration.
pub fn list_audio_devices() -> Result<Vec<AudioDeviceInfo>> {
    let host = cpal::default_host();

    // Get default device names for comparison
    let default_input_name = host
        .default_input_device()
        .and_then(|d| d.name().ok());
    let default_output_name = host
        .default_output_device()
        .and_then(|d| d.name().ok());

    let mut devices = Vec::new();

    // Enumerate all devices
    let all_devices = host.devices()?;

    for device in all_devices {
        let name = match device.name() {
            Ok(n) => n,
            Err(_) => continue, // Skip devices we can't get a name for
        };

        // Get input channel count
        let max_input_channels = device
            .supported_input_configs()
            .map(|configs| {
                configs
                    .map(|c| c.channels() as u32)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        // Get output channel count
        let max_output_channels = device
            .supported_output_configs()
            .map(|configs| {
                configs
                    .map(|c| c.channels() as u32)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        // Skip devices with no channels
        if max_input_channels == 0 && max_output_channels == 0 {
            continue;
        }

        // Get supported sample rates from either output or input configs
        let mut sample_rates: Vec<u32> = Vec::new();

        // Helper to extract sample rates from configs
        let extract_sample_rates = |configs: cpal::SupportedOutputConfigs| {
            let mut rates = Vec::new();
            for config in configs {
                let min = config.min_sample_rate();
                let max = config.max_sample_rate();
                for rate in [22050, 44100, 48000, 88200, 96000, 176400, 192000] {
                    if rate >= min && rate <= max && !rates.contains(&rate) {
                        rates.push(rate);
                    }
                }
            }
            rates
        };

        // Try output configs first
        if let Ok(configs) = device.supported_output_configs() {
            sample_rates = extract_sample_rates(configs);
        }

        // If no output sample rates, try input configs
        if sample_rates.is_empty() {
            if let Ok(configs) = device.supported_input_configs() {
                for config in configs {
                    let min = config.min_sample_rate();
                    let max = config.max_sample_rate();
                    for rate in [22050, 44100, 48000, 88200, 96000, 176400, 192000] {
                        if rate >= min && rate <= max && !sample_rates.contains(&rate) {
                            sample_rates.push(rate);
                        }
                    }
                }
            }
        }
        sample_rates.sort();

        let is_default_input = default_input_name
            .as_ref()
            .is_some_and(|default| default == &name);
        let is_default_output = default_output_name
            .as_ref()
            .is_some_and(|default| default == &name);

        devices.push(AudioDeviceInfo {
            name,
            max_input_channels,
            max_output_channels,
            sample_rates,
            is_default_input,
            is_default_output,
        });
    }

    // Sort: default devices first, then by name
    devices.sort_by(|a, b| {
        let a_default = a.is_default_input || a.is_default_output;
        let b_default = b.is_default_input || b.is_default_output;
        b_default.cmp(&a_default).then_with(|| a.name.cmp(&b.name))
    });

    Ok(devices)
}

/// Get the default audio device names.
///
/// Returns (default_input_name, default_output_name).
pub fn get_default_devices() -> Result<(Option<String>, Option<String>)> {
    let host = cpal::default_host();

    let default_input = host
        .default_input_device()
        .and_then(|d| d.name().ok());
    let default_output = host
        .default_output_device()
        .and_then(|d| d.name().ok());

    Ok((default_input, default_output))
}

/// Print a formatted list of audio devices to stdout.
pub fn print_audio_devices() -> Result<()> {
    let devices = list_audio_devices()?;

    println!("Available Audio Devices:");
    println!("========================\n");

    if devices.is_empty() {
        println!("  No audio devices found.");
        return Ok(());
    }

    for device in devices {
        let default_markers = match (device.is_default_input, device.is_default_output) {
            (true, true) => " [default input/output]",
            (true, false) => " [default input]",
            (false, true) => " [default output]",
            (false, false) => "",
        };

        println!("  {}{}", device.name, default_markers);

        if device.max_input_channels > 0 {
            println!("    Input channels:  {}", device.max_input_channels);
        }
        if device.max_output_channels > 0 {
            println!("    Output channels: {}", device.max_output_channels);
        }
        if !device.sample_rates.is_empty() {
            let rates_str: Vec<String> = device.sample_rates.iter().map(|r| r.to_string()).collect();
            println!("    Sample rates:    {}", rates_str.join(", "));
        }
        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_config_default() {
        let config = AudioConfig::default();
        assert!(config.input_device.is_none());
        assert!(config.output_device.is_none());
        assert_eq!(config.input_channels, 2);
        assert_eq!(config.output_channels, 2);
        assert!(config.sample_rate.is_none());
    }

    #[test]
    fn test_audio_config_builder() {
        let config = AudioConfig::new()
            .with_input_device(Some("hw:0".to_string()))
            .with_output_device(Some("hw:1".to_string()))
            .with_input_channels(4)
            .with_output_channels(8)
            .with_sample_rate(Some(48000));

        assert_eq!(config.input_device, Some("hw:0".to_string()));
        assert_eq!(config.output_device, Some("hw:1".to_string()));
        assert_eq!(config.input_channels, 4);
        assert_eq!(config.output_channels, 8);
        assert_eq!(config.sample_rate, Some(48000));
    }
}
