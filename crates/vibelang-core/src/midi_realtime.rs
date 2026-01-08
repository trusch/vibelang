//! Dedicated high-priority MIDI realtime thread with independent OSC client.
//!
//! This module provides a separate thread for processing MIDI triggers from
//! SuperCollider with minimal latency. The thread has its own OSC client that
//! listens directly to scsynth, completely independent from the main thread.
//!
//! ## Architecture
//!
//! ```text
//! SuperCollider (scsynth)
//!     │
//!     │ SendTrig /tr messages (sent to ALL registered clients)
//!     │
//!     ├──────────────────────────────────────────┐
//!     ▼                                          ▼
//! ┌─────────────────────────────────┐   ┌─────────────────────────────────┐
//! │    Main Thread OSC Client       │   │    MIDI Realtime OSC Client     │
//! │    (port N)                     │   │    (port M)                     │
//! │                                 │   │                                 │
//! │  • State management             │   │  • Dedicated high priority      │
//! │  • Heavy operations             │   │  • Blocking OSC recv            │
//! │  • May have latency spikes      │   │  • Direct MIDI output           │
//! └─────────────────────────────────┘   └─────────────────────────────────┘
//!                                                       │
//!                                                       ▼
//!                                       ┌─────────────────────────────────┐
//!                                       │         MIDI Devices            │
//!                                       │   (ALSA / JACK)                 │
//!                                       └─────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! // Create the MIDI realtime service with scsynth address
//! let midi_rt = MidiRealtimeService::new("127.0.0.1:57110");
//!
//! // Register MIDI output devices
//! midi_rt.register_device(device_id, event_tx);
//!
//! // Start processing (spawns dedicated thread with own OSC client)
//! midi_rt.start()?;
//!
//! // When done
//! midi_rt.stop();
//! ```

use crate::midi::{MidiOutputHandle, QueuedMidiEvent};
use vibelang_dsp::system_synthdefs::{trigger_ids, MidiTriggerType};
use crossbeam_channel::Sender;
use parking_lot::RwLock;
use rosc::{OscMessage, OscPacket, OscType};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(target_os = "linux")]
use libc;

/// Statistics for the MIDI realtime thread.
#[derive(Debug, Default)]
pub struct MidiRealtimeStats {
    /// Total /tr messages received
    pub triggers_received: AtomicU64,
    /// MIDI messages sent to devices
    pub midi_messages_sent: AtomicU64,
    /// Processing errors
    pub errors: AtomicU64,
    /// Messages dropped (device not found)
    pub dropped: AtomicU64,
}

impl MidiRealtimeStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log_stats(&self) {
        log::info!(
            "[MIDI_RT] Stats: triggers={} sent={} errors={} dropped={}",
            self.triggers_received.load(Ordering::Relaxed),
            self.midi_messages_sent.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
            self.dropped.load(Ordering::Relaxed),
        );
    }
}

/// A lightweight MIDI event sender that can be shared across threads.
#[derive(Clone)]
pub struct MidiDeviceSender {
    pub device_id: u32,
    pub event_tx: Sender<QueuedMidiEvent>,
}

impl MidiDeviceSender {
    pub fn new(device_id: u32, event_tx: Sender<QueuedMidiEvent>) -> Self {
        Self { device_id, event_tx }
    }

    /// Send a MIDI event with minimal overhead.
    #[inline]
    pub fn try_send(&self, event: QueuedMidiEvent) -> bool {
        self.event_tx.try_send(event).is_ok()
    }
}

/// Configuration for the MIDI realtime service.
#[derive(Clone, Debug)]
pub struct MidiRealtimeConfig {
    /// Whether to set thread priority to realtime
    pub realtime_priority: bool,
    /// SuperCollider server address
    pub scsynth_addr: String,
}

impl Default for MidiRealtimeConfig {
    fn default() -> Self {
        Self {
            realtime_priority: true,
            scsynth_addr: "127.0.0.1:57110".to_string(),
        }
    }
}

/// The MIDI realtime service.
///
/// This manages a dedicated high-priority thread with its own OSC client
/// that listens directly to SuperCollider for MIDI triggers.
pub struct MidiRealtimeService {
    /// Configuration
    config: MidiRealtimeConfig,
    /// Registered MIDI devices: device_id -> sender
    devices: Arc<RwLock<HashMap<u32, MidiDeviceSender>>>,
    /// Running flag
    running: Arc<AtomicBool>,
    /// Thread handle
    thread_handle: Option<JoinHandle<()>>,
    /// Statistics
    stats: Arc<MidiRealtimeStats>,
}

impl MidiRealtimeService {
    /// Create a new MIDI realtime service with default configuration.
    pub fn new() -> Self {
        Self::with_config(MidiRealtimeConfig::default())
    }

    /// Create a no-op MIDI realtime service for testing.
    ///
    /// This creates a service that won't connect to any scsynth instance
    /// and won't spawn any threads. Useful for unit tests.
    pub fn noop() -> Self {
        Self {
            config: MidiRealtimeConfig {
                scsynth_addr: "127.0.0.1:0".to_string(),
                ..Default::default()
            },
            devices: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            stats: Arc::new(MidiRealtimeStats::new()),
        }
    }

    /// Create a new MIDI realtime service for a specific scsynth address.
    pub fn with_scsynth_addr(addr: &str) -> Self {
        Self::with_config(MidiRealtimeConfig {
            scsynth_addr: addr.to_string(),
            ..Default::default()
        })
    }

    /// Create a new MIDI realtime service with custom configuration.
    pub fn with_config(config: MidiRealtimeConfig) -> Self {
        Self {
            config,
            devices: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            stats: Arc::new(MidiRealtimeStats::new()),
        }
    }

    /// Register a MIDI output device.
    pub fn register_device(&self, device_id: u32, event_tx: Sender<QueuedMidiEvent>) {
        let sender = MidiDeviceSender::new(device_id, event_tx);
        self.devices.write().insert(device_id, sender);
        log::info!("[MIDI_RT] Registered device {} for realtime processing", device_id);
    }

    /// Register a device from a MidiOutputHandle.
    pub fn register_device_from_handle(&self, handle: &MidiOutputHandle) {
        self.register_device(handle.device_id, handle.event_tx().clone());
    }

    /// Unregister a MIDI output device.
    pub fn unregister_device(&self, device_id: u32) {
        self.devices.write().remove(&device_id);
        log::info!("[MIDI_RT] Unregistered device {}", device_id);
    }

    /// Get a reference to the statistics.
    pub fn stats(&self) -> Arc<MidiRealtimeStats> {
        Arc::clone(&self.stats)
    }

    /// Check if a trigger ID is a MIDI trigger.
    #[inline]
    pub fn is_midi_trigger(trigger_id: i32) -> bool {
        trigger_ids::message_type(trigger_id).is_some()
    }

    /// Start the MIDI realtime thread with its own OSC client.
    pub fn start(&mut self) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("MIDI realtime service already running".to_string());
        }

        // Create a new UDP socket for the MIDI thread
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| format!("Failed to bind MIDI OSC socket: {}", e))?;

        // Set a read timeout so we can check the running flag periodically
        socket.set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(|e| format!("Failed to set socket timeout: {}", e))?;

        let local_addr = socket.local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?;
        log::info!("[MIDI_RT] OSC client bound to {}", local_addr);

        // Register with scsynth for notifications
        let scsynth_addr = &self.config.scsynth_addr;
        let notify_msg = rosc::encoder::encode(&OscPacket::Message(OscMessage {
            addr: "/notify".to_string(),
            args: vec![OscType::Int(1)],
        })).map_err(|e| format!("Failed to encode /notify: {}", e))?;

        socket.send_to(&notify_msg, scsynth_addr)
            .map_err(|e| format!("Failed to send /notify to scsynth: {}", e))?;
        log::info!("[MIDI_RT] Registered with scsynth at {} for notifications", scsynth_addr);

        // Set running flag
        self.running.store(true, Ordering::SeqCst);

        // Clone what we need for the thread
        let running = Arc::clone(&self.running);
        let devices = Arc::clone(&self.devices);
        let stats = Arc::clone(&self.stats);
        let realtime_priority = self.config.realtime_priority;

        // Spawn the realtime thread
        let handle = thread::Builder::new()
            .name("midi-realtime".to_string())
            .spawn(move || {
                run_midi_realtime_thread(socket, running, devices, stats, realtime_priority);
            })
            .map_err(|e| format!("Failed to spawn MIDI realtime thread: {}", e))?;

        self.thread_handle = Some(handle);
        log::info!("[MIDI_RT] Started MIDI realtime thread with independent OSC client");

        Ok(())
    }

    /// Stop the MIDI realtime thread.
    pub fn stop(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        log::info!("[MIDI_RT] Stopping MIDI realtime thread...");
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.thread_handle.take() {
            if let Err(e) = handle.join() {
                log::error!("[MIDI_RT] Thread join error: {:?}", e);
            }
        }

        self.stats.log_stats();
        log::info!("[MIDI_RT] Stopped");
    }

    /// Check if the service is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for MidiRealtimeService {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Default for MidiRealtimeService {
    fn default() -> Self {
        Self::new()
    }
}

/// The main loop for the MIDI realtime thread.
fn run_midi_realtime_thread(
    socket: UdpSocket,
    running: Arc<AtomicBool>,
    devices: Arc<RwLock<HashMap<u32, MidiDeviceSender>>>,
    stats: Arc<MidiRealtimeStats>,
    realtime_priority: bool,
) {
    // Try to set realtime priority
    if realtime_priority {
        #[cfg(target_os = "linux")]
        {
            unsafe {
                let param = libc::sched_param {
                    sched_priority: 50,
                };
                let result = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param);
                if result == 0 {
                    log::info!("[MIDI_RT] Set realtime priority (SCHED_FIFO, priority 50)");
                } else {
                    log::warn!(
                        "[MIDI_RT] Could not set realtime priority (run as root or set CAP_SYS_NICE)"
                    );
                }
            }
        }
    }

    log::info!("[MIDI_RT] Thread started, listening for /tr messages from scsynth");

    let mut buf = [0u8; 65536];

    // Main processing loop - blocking recv on our own OSC socket
    while running.load(Ordering::Relaxed) {
        match socket.recv_from(&mut buf) {
            Ok((size, _)) => {
                // Decode the OSC packet
                if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                    process_osc_packet(&packet, &devices, &stats);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Timeout - check running flag and continue
                continue;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // Timeout on Windows
                continue;
            }
            Err(e) => {
                log::warn!("[MIDI_RT] Socket error: {}", e);
                // Brief sleep before retrying
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    log::info!("[MIDI_RT] Thread exiting");
}

/// Process an incoming OSC packet.
fn process_osc_packet(
    packet: &OscPacket,
    devices: &Arc<RwLock<HashMap<u32, MidiDeviceSender>>>,
    stats: &Arc<MidiRealtimeStats>,
) {
    match packet {
        OscPacket::Message(msg) => {
            if msg.addr == "/tr" {
                process_trigger_message(msg, devices, stats);
            }
            // Ignore all other messages - we only care about /tr
        }
        OscPacket::Bundle(bundle) => {
            // Process all messages in the bundle
            for content in &bundle.content {
                process_osc_packet(content, devices, stats);
            }
        }
    }
}

/// Process a /tr message.
fn process_trigger_message(
    msg: &OscMessage,
    devices: &Arc<RwLock<HashMap<u32, MidiDeviceSender>>>,
    stats: &Arc<MidiRealtimeStats>,
) {
    // /tr format: [node_id: i32, trigger_id: i32, value: f32]
    if msg.args.len() < 3 {
        return;
    }

    let trigger_id = match &msg.args[1] {
        OscType::Int(n) => *n,
        OscType::Float(f) => *f as i32,
        _ => return,
    };

    let value = match &msg.args[2] {
        OscType::Float(f) => *f,
        OscType::Int(n) => *n as f32,
        _ => return,
    };

    // Determine message type and process
    let midi_type = match trigger_ids::message_type(trigger_id) {
        Some(t) => t,
        None => return, // Not a MIDI trigger
    };

    stats.triggers_received.fetch_add(1, Ordering::Relaxed);

    match midi_type {
        MidiTriggerType::NoteOn => {
            // Decode packed value: (device << 18) | (channel << 14) | (note << 7) | velocity
            let packed = value as u32;
            let device_id = (packed >> 18) & 0x3F;
            let channel = ((packed >> 14) & 0xF) as u8;
            let note = ((packed >> 7) & 0x7F) as u8;
            let velocity = (packed & 0x7F) as u8;

            log::debug!(
                "[MIDI_RT] Note ON: dev={} ch={} note={} vel={}",
                device_id, channel + 1, note, velocity
            );

            send_midi_event(
                device_id,
                QueuedMidiEvent::note_on(channel, note, velocity),
                devices,
                stats,
            );
        }
        MidiTriggerType::NoteOff => {
            // Decode packed value: (device << 14) | (channel << 7) | note
            let packed = value as u32;
            let device_id = (packed >> 14) & 0x3FFF;
            let channel = ((packed >> 7) & 0x7F) as u8;
            let note = (packed & 0x7F) as u8;

            log::debug!(
                "[MIDI_RT] Note OFF: dev={} ch={} note={}",
                device_id, channel + 1, note
            );

            send_midi_event(
                device_id,
                QueuedMidiEvent::note_off(channel, note),
                devices,
                stats,
            );
        }
        MidiTriggerType::CC => {
            // Decode packed value: (device << 18) | (channel << 14) | (cc_num << 7) | value
            let packed = value as u32;
            let device_id = (packed >> 18) & 0x3F;
            let channel = ((packed >> 14) & 0xF) as u8;
            let cc_num = ((packed >> 7) & 0x7F) as u8;
            let cc_value = (packed & 0x7F) as u8;

            log::debug!(
                "[MIDI_RT] CC: dev={} ch={} cc={} val={}",
                device_id, channel + 1, cc_num, cc_value
            );

            send_midi_event(
                device_id,
                QueuedMidiEvent::control_change(channel, cc_num, cc_value),
                devices,
                stats,
            );
        }
        MidiTriggerType::PitchBend => {
            // Decode packed value: (device << 18) | (channel << 14) | value
            let packed = value as u32;
            let device_id = (packed >> 18) & 0x3F;
            let channel = ((packed >> 14) & 0xF) as u8;
            let pb_value = (packed & 0x3FFF) as u16; // 14-bit value (0-16383)

            // Convert from 0-16383 to -8192..+8191
            let pb_centered = (pb_value as i16) - 8192;

            log::debug!(
                "[MIDI_RT] Pitch Bend: dev={} ch={} val={}",
                device_id, channel + 1, pb_centered
            );

            send_midi_event(
                device_id,
                QueuedMidiEvent::pitch_bend(channel, pb_centered),
                devices,
                stats,
            );
        }
        MidiTriggerType::Clock => {
            let device_id = value as u32;
            send_midi_event(device_id, QueuedMidiEvent::clock(), devices, stats);
        }
        MidiTriggerType::Start => {
            let device_id = value as u32;
            log::debug!("[MIDI_RT] MIDI Start: dev={}", device_id);
            send_midi_event(device_id, QueuedMidiEvent::start(), devices, stats);
        }
        MidiTriggerType::Stop => {
            let device_id = value as u32;
            log::debug!("[MIDI_RT] MIDI Stop: dev={}", device_id);
            send_midi_event(device_id, QueuedMidiEvent::stop(), devices, stats);
        }
        MidiTriggerType::Continue => {
            let device_id = value as u32;
            send_midi_event(device_id, QueuedMidiEvent::continue_msg(), devices, stats);
        }
    }
}

/// Send a MIDI event to a device.
#[inline]
fn send_midi_event(
    device_id: u32,
    event: QueuedMidiEvent,
    devices: &Arc<RwLock<HashMap<u32, MidiDeviceSender>>>,
    stats: &Arc<MidiRealtimeStats>,
) {
    // Fast path: try to get device without blocking
    // parking_lot's try_read returns Option<RwLockReadGuard> instead of Result
    let devices_guard = devices.try_read().unwrap_or_else(|| devices.read());

    if let Some(sender) = devices_guard.get(&device_id) {
        if sender.try_send(event) {
            stats.midi_messages_sent.fetch_add(1, Ordering::Relaxed);
        } else {
            stats.dropped.fetch_add(1, Ordering::Relaxed);
        }
    } else {
        stats.errors.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_creation() {
        let service = MidiRealtimeService::new();
        assert!(!service.is_running());
    }

    #[test]
    fn test_device_registration() {
        let service = MidiRealtimeService::new();
        let (tx, _rx) = crossbeam_channel::unbounded();

        service.register_device(1, tx);

        let devices = service.devices.read();
        assert!(devices.contains_key(&1));
    }

    #[test]
    fn test_is_midi_trigger() {
        // Packed note on (100)
        assert!(MidiRealtimeService::is_midi_trigger(100));
        // Packed note off (110)
        assert!(MidiRealtimeService::is_midi_trigger(110));
        // Clock (140)
        assert!(MidiRealtimeService::is_midi_trigger(140));
        // Not a MIDI trigger
        assert!(!MidiRealtimeService::is_midi_trigger(0));
        assert!(!MidiRealtimeService::is_midi_trigger(50));
    }

    #[test]
    fn test_note_on_packed_decode() {
        let device_id: u32 = 1;
        let channel: u32 = 5;
        let note: u32 = 60;
        let velocity: u32 = 100;

        let packed = (device_id << 18) | (channel << 14) | (note << 7) | velocity;

        let dec_device = (packed >> 18) & 0x3F;
        let dec_channel = (packed >> 14) & 0xF;
        let dec_note = (packed >> 7) & 0x7F;
        let dec_velocity = packed & 0x7F;

        assert_eq!(dec_device, device_id);
        assert_eq!(dec_channel, channel);
        assert_eq!(dec_note, note);
        assert_eq!(dec_velocity, velocity);
    }
}
