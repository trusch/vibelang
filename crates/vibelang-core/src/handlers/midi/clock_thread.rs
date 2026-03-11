//! Dedicated MIDI clock output thread.
//!
//! This module provides a high-precision thread for sending MIDI clock
//! signals (24 PPQN) independently from the main runtime loop.
//!
//! ## Why a Dedicated Thread?
//!
//! The main runtime loop runs at 500 Hz (2ms tick). While this is sufficient
//! for most operations, MIDI clock output can block if external devices are
//! slow. Moving clock output to a dedicated thread:
//!
//! - Prevents slow MIDI devices from blocking pattern/melody playback
//! - Provides tighter timing precision (1kHz vs 500Hz)
//! - Isolates clock jitter from main loop jitter
//!
//! ## Architecture
//!
//! ```text
//! Main Loop (500 Hz)          Clock Thread (1kHz)
//! ┌─────────────────┐         ┌─────────────────┐
//! │ Transport tick  │         │ Read snapshot   │
//! │ Update snapshot │────────►│ Calculate ticks │
//! └─────────────────┘         │ Send via channel│
//!                             └────────┬────────┘
//!                                      │
//!                             Output Thread (10kHz)
//!                             ┌────────▼────────┐
//!                             │ Send to device  │
//!                             └─────────────────┘
//! ```

use crate::midi::{QueuedMidiEvent, ScheduledMidiEvent};
use crate::transport_snapshot::TransportSnapshot;
use crate::types::ids::MidiDeviceId;
use crossbeam_channel::Sender;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// How often the clock thread checks for clock ticks (microseconds).
/// 1000μs = 1kHz polling rate for ~1ms precision.
const CLOCK_THREAD_POLL_INTERVAL_US: u64 = 1000;

/// MIDI clock pulses per quarter note (standard).
const PPQN: f64 = 24.0;

/// Dedicated thread for MIDI clock output.
///
/// Reads transport state from a lock-free `TransportSnapshot` and sends
/// 24 PPQN clock messages to registered devices.
pub struct MidiClockThread {
    /// Thread handle.
    handle: Option<JoinHandle<()>>,
    /// Running flag - set to false to stop the thread.
    running: Arc<AtomicBool>,
    /// Transport snapshot (shared with main loop).
    transport: Arc<TransportSnapshot>,
    /// Devices with clock output enabled (uses parking_lot for sync access).
    clock_devices: Arc<RwLock<HashSet<MidiDeviceId>>>,
    /// Output channels for sending MIDI events.
    output_channels: Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>>,
    /// Flag indicating transport was playing in last tick.
    was_playing: Arc<AtomicBool>,
    /// Devices waiting for a quantized Start at the next bar boundary.
    pending_starts: Arc<RwLock<HashSet<MidiDeviceId>>>,
    /// Beats per bar (from time signature, default 4).
    beats_per_bar: Arc<std::sync::atomic::AtomicU8>,
}

impl MidiClockThread {
    /// Create a new clock thread (but don't start it yet).
    pub fn new(
        transport: Arc<TransportSnapshot>,
        output_channels: Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>>,
    ) -> Self {
        Self {
            handle: None,
            running: Arc::new(AtomicBool::new(false)),
            transport,
            clock_devices: Arc::new(RwLock::new(HashSet::new())),
            output_channels,
            was_playing: Arc::new(AtomicBool::new(false)),
            pending_starts: Arc::new(RwLock::new(HashSet::new())),
            beats_per_bar: Arc::new(std::sync::atomic::AtomicU8::new(4)),
        }
    }

    /// Set the beats per bar (from time signature numerator).
    pub fn set_beats_per_bar(&self, beats: u8) {
        self.beats_per_bar
            .store(beats.max(1), std::sync::atomic::Ordering::Relaxed);
    }

    /// Queue a device for a quantized Start at the next bar boundary.
    pub fn queue_quantized_start(&self, device: MidiDeviceId) {
        self.pending_starts.write().insert(device);
        tracing::debug!(
            "[MIDI_CLOCK] Queued quantized Start for device {} (next bar)",
            device.0
        );
    }

    /// Start the clock thread.
    ///
    /// The thread will run until `stop()` is called.
    pub fn start(&mut self) {
        if self.handle.is_some() {
            tracing::warn!("MIDI clock thread already running");
            return;
        }

        self.running.store(true, Ordering::SeqCst);

        let running = Arc::clone(&self.running);
        let transport = Arc::clone(&self.transport);
        let clock_devices = Arc::clone(&self.clock_devices);
        let output_channels = Arc::clone(&self.output_channels);
        let was_playing = Arc::clone(&self.was_playing);
        let pending_starts = Arc::clone(&self.pending_starts);
        let beats_per_bar = Arc::clone(&self.beats_per_bar);

        let handle = thread::Builder::new()
            .name("midi-clock".to_string())
            .spawn(move || {
                run_clock_thread(
                    running,
                    transport,
                    clock_devices,
                    output_channels,
                    was_playing,
                    pending_starts,
                    beats_per_bar,
                );
            })
            .expect("Failed to spawn MIDI clock thread");

        self.handle = Some(handle);
        tracing::info!("MIDI clock thread started");
    }

    /// Stop the clock thread and wait for it to exit.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.handle.take() {
            if let Err(e) = handle.join() {
                tracing::warn!("MIDI clock thread panicked: {:?}", e);
            } else {
                tracing::info!("MIDI clock thread stopped");
            }
        }
    }

    /// Check if the thread is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Enable clock output for a device.
    ///
    /// If the transport is already playing and this is a *newly* enabled device,
    /// sends a Start message so it syncs with the current playback.
    /// Already-enabled devices are left untouched (idempotent).
    pub fn enable_clock_output(&self, device: MidiDeviceId) {
        // Check if already enabled — if so, skip to avoid re-sending Start on reload
        {
            let devices = self.clock_devices.read();
            if devices.contains(&device) {
                tracing::trace!("[MIDI_CLOCK] Clock output already enabled for device {}, skipping", device.0);
                return;
            }
        }

        self.clock_devices.write().insert(device);
        tracing::debug!("[MIDI_CLOCK] Enabled clock output for device {}", device.0);

        // Only send Start for newly enabled devices while transport is playing
        let (_beat, _tempo, playing) = self.transport.read();
        if playing {
            tracing::debug!(
                "[MIDI_CLOCK] Transport already playing, sending Start to device {}",
                device.0
            );
            if let Ok(channels) = self.output_channels.lock() {
                if let Some(sender) = channels.get(&device) {
                    let scheduled = QueuedMidiEvent::Start.immediate();
                    if let Err(e) = sender.try_send(scheduled) {
                        tracing::warn!(
                            "[MIDI_CLOCK] Failed to send Start to device {}: {}",
                            device.0,
                            e
                        );
                    }
                }
            }
        }
    }

    /// Disable clock output for a device.
    pub fn disable_clock_output(&self, device: MidiDeviceId) {
        self.clock_devices.write().remove(&device);
        tracing::debug!("[MIDI_CLOCK] Disabled clock output for device {}", device.0);
    }

    /// Check if clock output is enabled for a device.
    #[allow(dead_code)]
    pub fn is_clock_enabled(&self, device: MidiDeviceId) -> bool {
        self.clock_devices.read().contains(&device)
    }

    /// Get the set of devices with clock output enabled.
    #[allow(dead_code)]
    pub fn clock_devices(&self) -> Arc<RwLock<HashSet<MidiDeviceId>>> {
        Arc::clone(&self.clock_devices)
    }
}

impl Drop for MidiClockThread {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Tolerance for bar boundary detection (in beats).
/// At 1kHz polling and 120 BPM, we advance ~0.002 beats per poll.
const BAR_BOUNDARY_TOLERANCE: f64 = 0.02;

/// Main clock thread function.
fn run_clock_thread(
    running: Arc<AtomicBool>,
    transport: Arc<TransportSnapshot>,
    clock_devices: Arc<RwLock<HashSet<MidiDeviceId>>>,
    output_channels: Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>>,
    was_playing: Arc<AtomicBool>,
    pending_starts: Arc<RwLock<HashSet<MidiDeviceId>>>,
    beats_per_bar: Arc<std::sync::atomic::AtomicU8>,
) {
    tracing::info!(
        "[MIDI_CLOCK] Thread started with {}μs poll interval",
        CLOCK_THREAD_POLL_INTERVAL_US
    );

    let poll_duration = Duration::from_micros(CLOCK_THREAD_POLL_INTERVAL_US);
    let mut last_clock_beat: f64 = 0.0;

    while running.load(Ordering::Relaxed) {
        // Read transport state (lock-free)
        let (beat, _tempo, playing) = transport.read();

        // Check for seek - reset clock position
        if transport.check_and_clear_seek() {
            last_clock_beat = beat;
            tracing::debug!("[MIDI_CLOCK] Seek detected, reset to beat {}", beat);
        }

        // Handle transport start/stop
        let prev_playing = was_playing.swap(playing, Ordering::Relaxed);
        if playing && !prev_playing {
            // Transport started - send MIDI Start
            send_to_all_devices(&clock_devices, &output_channels, QueuedMidiEvent::Start);
            last_clock_beat = beat;
            tracing::debug!("[MIDI_CLOCK] Transport started at beat {}", beat);
        } else if !playing && prev_playing {
            // Transport stopped - send MIDI Stop
            send_to_all_devices(&clock_devices, &output_channels, QueuedMidiEvent::Stop);
            tracing::debug!("[MIDI_CLOCK] Transport stopped");
        }

        // Check for pending quantized starts at bar boundaries
        if playing && !pending_starts.read().is_empty() {
            let bar_len = beats_per_bar.load(std::sync::atomic::Ordering::Relaxed) as f64;
            let position_in_bar = beat % bar_len;

            // Check if we're at a bar boundary
            if position_in_bar < BAR_BOUNDARY_TOLERANCE
                || position_in_bar > (bar_len - BAR_BOUNDARY_TOLERANCE)
            {
                // Send Start to all pending devices
                let devices: Vec<MidiDeviceId> =
                    pending_starts.write().drain().collect();
                if !devices.is_empty() {
                    if let Ok(channels) = output_channels.lock() {
                        for device in devices {
                            if let Some(sender) = channels.get(&device) {
                                let scheduled = QueuedMidiEvent::Start.immediate();
                                if let Err(e) = sender.try_send(scheduled) {
                                    tracing::warn!(
                                        "[MIDI_CLOCK] Failed to send quantized Start to device {}: {}",
                                        device.0,
                                        e
                                    );
                                } else {
                                    tracing::info!(
                                        "[MIDI_CLOCK] Sent quantized Start to device {} at bar boundary (beat {})",
                                        device.0,
                                        beat
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Only send clock when playing
        if playing {
            let beat_diff = beat - last_clock_beat;

            // Handle negative beat diff (backward seek already handled above)
            if beat_diff >= 0.0 {
                let ticks_to_send = (beat_diff * PPQN).floor() as u32;

                if ticks_to_send > 0 {
                    // Send clock ticks
                    for _ in 0..ticks_to_send {
                        send_to_all_devices(
                            &clock_devices,
                            &output_channels,
                            QueuedMidiEvent::Clock,
                        );
                    }

                    // Update last beat position
                    last_clock_beat += (ticks_to_send as f64) / PPQN;
                }
            } else {
                // Negative diff without seek flag - reset position
                last_clock_beat = beat;
            }
        } else {
            // Not playing - keep position in sync for when we start
            last_clock_beat = beat;
        }

        // Sleep until next poll
        thread::sleep(poll_duration);
    }

    tracing::info!("[MIDI_CLOCK] Thread exiting");
}

/// Send a MIDI event to all clock-enabled devices.
fn send_to_all_devices(
    clock_devices: &Arc<RwLock<HashSet<MidiDeviceId>>>,
    output_channels: &Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>>,
    event: QueuedMidiEvent,
) {
    let devices = clock_devices.read();

    if devices.is_empty() {
        return;
    }

    let Ok(channels) = output_channels.lock() else {
        tracing::warn!("[MIDI_CLOCK] Failed to lock output channels (mutex poisoned)");
        return;
    };

    for device_id in devices.iter() {
        if let Some(sender) = channels.get(device_id) {
            // Send immediately (timestamp = now)
            let scheduled = event.clone().immediate();
            if let Err(e) = sender.try_send(scheduled) {
                tracing::trace!(
                    "[MIDI_CLOCK] Failed to send to device {}: {}",
                    device_id.0,
                    e
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_thread_lifecycle() {
        let transport = Arc::new(TransportSnapshot::new());
        let output_channels = Arc::new(Mutex::new(HashMap::new()));

        let mut clock_thread = MidiClockThread::new(transport, output_channels);

        assert!(!clock_thread.is_running());

        clock_thread.start();
        assert!(clock_thread.is_running());

        // Give thread time to start
        std::thread::sleep(Duration::from_millis(10));

        clock_thread.stop();
        assert!(!clock_thread.is_running());
    }

    #[test]
    fn test_enable_disable_clock() {
        let transport = Arc::new(TransportSnapshot::new());
        let output_channels = Arc::new(Mutex::new(HashMap::new()));

        let clock_thread = MidiClockThread::new(transport, output_channels);

        let device = MidiDeviceId(1);

        assert!(!clock_thread.is_clock_enabled(device));

        clock_thread.enable_clock_output(device);
        assert!(clock_thread.is_clock_enabled(device));

        clock_thread.disable_clock_output(device);
        assert!(!clock_thread.is_clock_enabled(device));
    }
}
