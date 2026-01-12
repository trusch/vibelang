//! MIDI device hot-plug detection.
//!
//! This module provides detection of MIDI device connection and disconnection
//! by periodically polling the system for device changes.
//!
//! Note: True hot-plug notification requires platform-specific APIs that
//! midir doesn't expose. This implementation uses polling as a portable solution.
//!
//! ## Thread Safety
//!
//! The `HotPlugWatcher` uses a standard OS thread for polling because midir
//! types contain platform-specific handles. The watcher creates its own midir
//! instances for enumeration, keeping them isolated from the main device manager.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::RwLock;

#[cfg(feature = "native")]
use midir::{MidiInput, MidiOutput};

use super::devices::{MidiInputId, MidiInputInfo, MidiOutputId, MidiOutputInfo};

// ============================================================================
// Hot-Plug Events
// ============================================================================

/// A hot-plug event.
#[derive(Clone, Debug)]
pub enum HotPlugEvent {
    /// A new input device was connected.
    InputConnected(MidiInputInfo),
    /// An input device was disconnected.
    InputDisconnected(MidiInputInfo),
    /// A new output device was connected.
    OutputConnected(MidiOutputInfo),
    /// An output device was disconnected.
    OutputDisconnected(MidiOutputInfo),
}

// ============================================================================
// Hot-Plug Callback
// ============================================================================

/// Callback type for hot-plug events.
pub type HotPlugCallback = Box<dyn Fn(HotPlugEvent) + Send + Sync>;

// ============================================================================
// Hot-Plug Watcher
// ============================================================================

/// Watches for MIDI device hot-plug events.
///
/// This struct is Send+Sync and can be safely shared across threads.
pub struct HotPlugWatcher {
    /// Known input device names.
    known_inputs: RwLock<HashSet<String>>,
    /// Known output device names.
    known_outputs: RwLock<HashSet<String>>,
    /// Callbacks for events.
    callbacks: RwLock<Vec<HotPlugCallback>>,
}

impl HotPlugWatcher {
    /// Default poll interval.
    pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);

    /// Create a new hot-plug watcher.
    pub fn new() -> Self {
        Self {
            known_inputs: RwLock::new(HashSet::new()),
            known_outputs: RwLock::new(HashSet::new()),
            callbacks: RwLock::new(Vec::new()),
        }
    }

    /// Add a callback for hot-plug events.
    pub fn add_callback<F>(&self, callback: F)
    where
        F: Fn(HotPlugEvent) + Send + Sync + 'static,
    {
        self.callbacks.write().push(Box::new(callback));
    }

    /// Initialize with current devices.
    ///
    /// Call this before starting to poll to avoid spurious "connected" events.
    #[cfg(feature = "native")]
    pub fn initialize(&self) {
        let inputs = enumerate_inputs();
        let outputs = enumerate_outputs();

        let mut known_inputs = self.known_inputs.write();
        for input in inputs {
            known_inputs.insert(input.name);
        }

        let mut known_outputs = self.known_outputs.write();
        for output in outputs {
            known_outputs.insert(output.name);
        }

        tracing::debug!(
            "[HOTPLUG] Initialized with {} inputs, {} outputs",
            known_inputs.len(),
            known_outputs.len()
        );
    }

    #[cfg(not(feature = "native"))]
    pub fn initialize(&self) {
        // No-op on non-native platforms
    }

    /// Poll for device changes.
    ///
    /// Returns a list of events that occurred since the last poll.
    #[cfg(feature = "native")]
    pub fn poll(&self) -> Vec<HotPlugEvent> {
        let mut events = Vec::new();

        // Check inputs
        let current_inputs = enumerate_inputs();
        let current_input_names: HashSet<_> =
            current_inputs.iter().map(|d| d.name.clone()).collect();

        {
            let mut known = self.known_inputs.write();

            // Check for new devices
            for input in &current_inputs {
                if !known.contains(&input.name) {
                    events.push(HotPlugEvent::InputConnected(input.clone()));
                    known.insert(input.name.clone());
                }
            }

            // Check for removed devices
            let removed: Vec<_> = known
                .iter()
                .filter(|name| !current_input_names.contains(*name))
                .cloned()
                .collect();

            for name in removed {
                known.remove(&name);
                events.push(HotPlugEvent::InputDisconnected(MidiInputInfo {
                    id: MidiInputId::new(0), // ID unknown after removal
                    name,
                    connected: false,
                }));
            }
        }

        // Check outputs
        let current_outputs = enumerate_outputs();
        let current_output_names: HashSet<_> =
            current_outputs.iter().map(|d| d.name.clone()).collect();

        {
            let mut known = self.known_outputs.write();

            // Check for new devices
            for output in &current_outputs {
                if !known.contains(&output.name) {
                    events.push(HotPlugEvent::OutputConnected(output.clone()));
                    known.insert(output.name.clone());
                }
            }

            // Check for removed devices
            let removed: Vec<_> = known
                .iter()
                .filter(|name| !current_output_names.contains(*name))
                .cloned()
                .collect();

            for name in removed {
                known.remove(&name);
                events.push(HotPlugEvent::OutputDisconnected(MidiOutputInfo {
                    id: MidiOutputId::new(0),
                    name,
                    connected: false,
                }));
            }
        }

        // Invoke callbacks
        if !events.is_empty() {
            let callbacks = self.callbacks.read();
            for event in &events {
                for callback in callbacks.iter() {
                    callback(event.clone());
                }
            }
        }

        events
    }

    #[cfg(not(feature = "native"))]
    pub fn poll(&self) -> Vec<HotPlugEvent> {
        Vec::new()
    }

    /// Get the current known input devices.
    pub fn known_inputs(&self) -> Vec<String> {
        self.known_inputs.read().iter().cloned().collect()
    }

    /// Get the current known output devices.
    pub fn known_outputs(&self) -> Vec<String> {
        self.known_outputs.read().iter().cloned().collect()
    }

    /// Spawn a background polling thread.
    ///
    /// Returns a shutdown flag and join handle.
    /// Set the flag to `true` to stop the polling thread.
    pub fn spawn(self: Arc<Self>, interval: Duration) -> (Arc<AtomicBool>, JoinHandle<()>) {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();
        let watcher = self;

        let handle = thread::spawn(move || {
            tracing::info!("[HOTPLUG] Started polling every {:?}", interval);

            while !shutdown_clone.load(Ordering::Relaxed) {
                thread::sleep(interval);

                if shutdown_clone.load(Ordering::Relaxed) {
                    break;
                }

                let events = watcher.poll();
                for event in &events {
                    match event {
                        HotPlugEvent::InputConnected(info) => {
                            tracing::info!("[HOTPLUG] Input connected: {}", info.name);
                        }
                        HotPlugEvent::InputDisconnected(info) => {
                            tracing::info!("[HOTPLUG] Input disconnected: {}", info.name);
                        }
                        HotPlugEvent::OutputConnected(info) => {
                            tracing::info!("[HOTPLUG] Output connected: {}", info.name);
                        }
                        HotPlugEvent::OutputDisconnected(info) => {
                            tracing::info!("[HOTPLUG] Output disconnected: {}", info.name);
                        }
                    }
                }
            }

            tracing::info!("[HOTPLUG] Stopped");
        });

        (shutdown, handle)
    }
}

impl Default for HotPlugWatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Device Enumeration (internal)
// ============================================================================

/// Enumerate input devices using a fresh midir instance.
#[cfg(feature = "native")]
fn enumerate_inputs() -> Vec<MidiInputInfo> {
    let midi_in = match MidiInput::new("vibelang-hotplug") {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                "[HOTPLUG] Failed to create MIDI input for enumeration: {}",
                e
            );
            return Vec::new();
        }
    };

    midi_in
        .ports()
        .iter()
        .enumerate()
        .filter_map(|(idx, port)| {
            let name = midi_in.port_name(port).ok()?;
            Some(MidiInputInfo {
                id: MidiInputId::new(idx as u32),
                name,
                connected: true,
            })
        })
        .collect()
}

/// Enumerate output devices using a fresh midir instance.
#[cfg(feature = "native")]
fn enumerate_outputs() -> Vec<MidiOutputInfo> {
    let midi_out = match MidiOutput::new("vibelang-hotplug") {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                "[HOTPLUG] Failed to create MIDI output for enumeration: {}",
                e
            );
            return Vec::new();
        }
    };

    midi_out
        .ports()
        .iter()
        .enumerate()
        .filter_map(|(idx, port)| {
            let name = midi_out.port_name(port).ok()?;
            Some(MidiOutputInfo {
                id: MidiOutputId::new(idx as u32),
                name,
                connected: true,
            })
        })
        .collect()
}

// ============================================================================
// Auto-Reconnect
// ============================================================================

/// Configuration for automatic reconnection.
#[derive(Clone, Debug, Default)]
pub struct AutoReconnectConfig {
    /// Device names to auto-reconnect (inputs).
    pub auto_reconnect_inputs: Vec<String>,
    /// Device names to auto-reconnect (outputs).
    pub auto_reconnect_outputs: Vec<String>,
}

/// Manages automatic reconnection of devices.
///
/// This struct stores the auto-reconnect configuration. The actual reconnection
/// is triggered by hot-plug events, which should be handled by the application.
pub struct AutoReconnect {
    config: RwLock<AutoReconnectConfig>,
}

impl AutoReconnect {
    /// Create a new auto-reconnect manager.
    pub fn new() -> Self {
        Self {
            config: RwLock::new(AutoReconnectConfig::default()),
        }
    }

    /// Add an input device to auto-reconnect list.
    pub fn add_input(&self, name: impl Into<String>) {
        self.config.write().auto_reconnect_inputs.push(name.into());
    }

    /// Add an output device to auto-reconnect list.
    pub fn add_output(&self, name: impl Into<String>) {
        self.config.write().auto_reconnect_outputs.push(name.into());
    }

    /// Check if an input should be auto-reconnected.
    pub fn should_reconnect_input(&self, name: &str) -> bool {
        self.config
            .read()
            .auto_reconnect_inputs
            .contains(&name.to_string())
    }

    /// Check if an output should be auto-reconnected.
    pub fn should_reconnect_output(&self, name: &str) -> bool {
        self.config
            .read()
            .auto_reconnect_outputs
            .contains(&name.to_string())
    }

    /// Handle a hot-plug event.
    ///
    /// Returns `true` if the device should be auto-reconnected.
    pub fn handle_event(&self, event: &HotPlugEvent) -> bool {
        match event {
            HotPlugEvent::InputConnected(info) => {
                if self.should_reconnect_input(&info.name) {
                    tracing::info!("[AUTO_RECONNECT] Should reconnect input: {}", info.name);
                    return true;
                }
            }
            HotPlugEvent::OutputConnected(info) => {
                if self.should_reconnect_output(&info.name) {
                    tracing::info!("[AUTO_RECONNECT] Should reconnect output: {}", info.name);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    /// Clear the auto-reconnect configuration.
    pub fn clear(&self) {
        let mut config = self.config.write();
        config.auto_reconnect_inputs.clear();
        config.auto_reconnect_outputs.clear();
    }
}

impl Default for AutoReconnect {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotplug_watcher_creation() {
        let watcher = HotPlugWatcher::new();

        assert!(watcher.known_inputs().is_empty());
        assert!(watcher.known_outputs().is_empty());
    }

    #[test]
    fn test_auto_reconnect_config() {
        let ar = AutoReconnect::new();

        ar.add_input("Test Input");
        ar.add_output("Test Output");

        assert!(ar.should_reconnect_input("Test Input"));
        assert!(ar.should_reconnect_output("Test Output"));
        assert!(!ar.should_reconnect_input("Other Input"));
    }
}
