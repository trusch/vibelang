//! Dedicated high-priority MIDI realtime thread with independent OSC client.
//!
//! This module provides a separate thread for processing MIDI triggers from
//! SuperCollider with minimal latency. The thread has its own OSC client that
//! listens directly to scsynth, completely independent from the main runtime.
//!
//! ## Architecture
//!
//! ```text
//! SuperCollider (scsynth)
//!     │
//!     │ SendTrig /tr messages
//!     │
//!     ├──────────────────────────────────────────┐
//!     ▼                                          ▼
//! ┌─────────────────────────────────┐   ┌─────────────────────────────────┐
//! │    Main Thread OSC Client       │   │    MIDI Realtime OSC Client     │
//! │    (state management, heavy)    │   │    (dedicated, high priority)   │
//! └─────────────────────────────────┘   └─────────────────────────────────┘
//!                                                       │
//!                                                       ▼
//!                                       ┌─────────────────────────────────┐
//!                                       │         MIDI Devices            │
//!                                       └─────────────────────────────────┘
//! ```

use crate::midi::constants::{decode_packed_midi, MidiData};
use crossbeam_channel::Sender;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(feature = "native")]
use rosc::{OscMessage, OscPacket, OscType};

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
    /// Create new stats tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Log current statistics.
    pub fn log_stats(&self) {
        tracing::info!(
            "[MIDI_RT] Stats: triggers={} sent={} errors={} dropped={}",
            self.triggers_received.load(Ordering::Relaxed),
            self.midi_messages_sent.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
            self.dropped.load(Ordering::Relaxed),
        );
    }
}

/// A queued MIDI event to send to a device.
#[derive(Debug, Clone)]
pub enum QueuedMidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    ControlChange { channel: u8, cc: u8, value: u8 },
    PitchBend { channel: u8, value: i16 },
    Clock,
    Start,
    Stop,
    Continue,
}

impl QueuedMidiEvent {
    /// Convert to raw MIDI bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            QueuedMidiEvent::NoteOn {
                channel,
                note,
                velocity,
            } => vec![0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F],
            QueuedMidiEvent::NoteOff { channel, note } => {
                vec![0x80 | (channel & 0x0F), note & 0x7F, 0]
            }
            QueuedMidiEvent::ControlChange { channel, cc, value } => {
                vec![0xB0 | (channel & 0x0F), cc & 0x7F, value & 0x7F]
            }
            QueuedMidiEvent::PitchBend { channel, value } => {
                // Convert from -8192..+8191 to 0..16383
                let pb = (*value + 8192) as u16;
                let lsb = (pb & 0x7F) as u8;
                let msb = ((pb >> 7) & 0x7F) as u8;
                vec![0xE0 | (channel & 0x0F), lsb, msb]
            }
            QueuedMidiEvent::Clock => vec![0xF8],
            QueuedMidiEvent::Start => vec![0xFA],
            QueuedMidiEvent::Stop => vec![0xFC],
            QueuedMidiEvent::Continue => vec![0xFB],
        }
    }
}

/// A lightweight MIDI event sender that can be shared across threads.
#[derive(Clone)]
pub struct MidiDeviceSender {
    /// Device ID.
    pub device_id: u32,
    /// Channel for sending events.
    pub event_tx: Sender<QueuedMidiEvent>,
}

impl MidiDeviceSender {
    /// Create a new device sender.
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
    /// Whether to set thread priority to realtime.
    pub realtime_priority: bool,
    /// SuperCollider server address.
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
    /// Configuration.
    config: MidiRealtimeConfig,
    /// Registered MIDI devices: device_id -> sender.
    devices: Arc<RwLock<HashMap<u32, MidiDeviceSender>>>,
    /// Running flag.
    running: Arc<AtomicBool>,
    /// Thread handle.
    thread_handle: Option<JoinHandle<()>>,
    /// Statistics.
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
        tracing::info!(
            "[MIDI_RT] Registered device {} for realtime processing",
            device_id
        );
    }

    /// Unregister a MIDI output device.
    pub fn unregister_device(&self, device_id: u32) {
        self.devices.write().remove(&device_id);
        tracing::info!("[MIDI_RT] Unregistered device {}", device_id);
    }

    /// Get a reference to the statistics.
    pub fn stats(&self) -> Arc<MidiRealtimeStats> {
        Arc::clone(&self.stats)
    }

    /// Check if the service is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Start the MIDI realtime thread with its own OSC client.
    #[cfg(feature = "native")]
    pub fn start(&mut self) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("MIDI realtime service already running".to_string());
        }

        // Create a new UDP socket for the MIDI thread
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| format!("Failed to bind MIDI OSC socket: {}", e))?;

        // Set a read timeout so we can check the running flag periodically
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(|e| format!("Failed to set socket timeout: {}", e))?;

        let local_addr = socket
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?;
        tracing::info!("[MIDI_RT] OSC client bound to {}", local_addr);

        // Register with scsynth for notifications
        let scsynth_addr = &self.config.scsynth_addr;
        let notify_msg = rosc::encoder::encode(&OscPacket::Message(OscMessage {
            addr: "/notify".to_string(),
            args: vec![OscType::Int(1)],
        }))
        .map_err(|e| format!("Failed to encode /notify: {}", e))?;

        socket
            .send_to(&notify_msg, scsynth_addr)
            .map_err(|e| format!("Failed to send /notify to scsynth: {}", e))?;
        tracing::info!(
            "[MIDI_RT] Registered with scsynth at {} for notifications",
            scsynth_addr
        );

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
        tracing::info!("[MIDI_RT] Started MIDI realtime thread with independent OSC client");

        Ok(())
    }

    /// Start the MIDI realtime thread (WASM stub - not supported).
    #[cfg(not(feature = "native"))]
    pub fn start(&mut self) -> Result<(), String> {
        Err("MIDI realtime service not supported on this platform".to_string())
    }

    /// Stop the MIDI realtime thread.
    pub fn stop(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        tracing::info!("[MIDI_RT] Stopping MIDI realtime thread...");
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.thread_handle.take() {
            if let Err(e) = handle.join() {
                tracing::error!("[MIDI_RT] Thread join error: {:?}", e);
            }
        }

        self.stats.log_stats();
        tracing::info!("[MIDI_RT] Stopped");
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
#[cfg(feature = "native")]
fn run_midi_realtime_thread(
    socket: UdpSocket,
    running: Arc<AtomicBool>,
    devices: Arc<RwLock<HashMap<u32, MidiDeviceSender>>>,
    stats: Arc<MidiRealtimeStats>,
    realtime_priority: bool,
) {
    // Try to set realtime priority on Linux
    if realtime_priority {
        #[cfg(target_os = "linux")]
        {
            unsafe {
                let param = libc::sched_param { sched_priority: 50 };
                let result = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param);
                if result == 0 {
                    tracing::info!("[MIDI_RT] Set realtime priority (SCHED_FIFO, priority 50)");
                } else {
                    tracing::warn!(
                        "[MIDI_RT] Could not set realtime priority (run as root or set CAP_SYS_NICE)"
                    );
                }
            }
        }
    }

    tracing::info!("[MIDI_RT] Thread started, listening for /tr messages from scsynth");

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
                tracing::warn!("[MIDI_RT] Socket error: {}", e);
                // Brief sleep before retrying
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    tracing::info!("[MIDI_RT] Thread exiting");
}

/// Process an incoming OSC packet.
#[cfg(feature = "native")]
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
#[cfg(feature = "native")]
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

    // Decode the packed MIDI data
    let midi_data = match decode_packed_midi(trigger_id, value) {
        Some(data) => data,
        None => return, // Not a MIDI trigger
    };

    stats.triggers_received.fetch_add(1, Ordering::Relaxed);

    // Convert to queued event and send
    match midi_data {
        MidiData::NoteOn {
            device,
            channel,
            note,
            velocity,
        } => {
            tracing::debug!(
                "[MIDI_RT] Note ON: dev={} ch={} note={} vel={}",
                device,
                channel + 1,
                note,
                velocity
            );
            send_midi_event(
                device as u32,
                QueuedMidiEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                },
                devices,
                stats,
            );
        }
        MidiData::NoteOff {
            device,
            channel,
            note,
        } => {
            tracing::debug!(
                "[MIDI_RT] Note OFF: dev={} ch={} note={}",
                device,
                channel + 1,
                note
            );
            send_midi_event(
                device as u32,
                QueuedMidiEvent::NoteOff { channel, note },
                devices,
                stats,
            );
        }
        MidiData::ControlChange {
            device,
            channel,
            cc,
            value,
        } => {
            tracing::debug!(
                "[MIDI_RT] CC: dev={} ch={} cc={} val={}",
                device,
                channel + 1,
                cc,
                value
            );
            send_midi_event(
                device as u32,
                QueuedMidiEvent::ControlChange { channel, cc, value },
                devices,
                stats,
            );
        }
        MidiData::PitchBend {
            device,
            channel,
            value,
        } => {
            tracing::debug!(
                "[MIDI_RT] Pitch Bend: dev={} ch={} val={}",
                device,
                channel + 1,
                value
            );
            send_midi_event(
                device as u32,
                QueuedMidiEvent::PitchBend { channel, value },
                devices,
                stats,
            );
        }
        MidiData::Clock { device } => {
            send_midi_event(device as u32, QueuedMidiEvent::Clock, devices, stats);
        }
        MidiData::Start { device } => {
            tracing::debug!("[MIDI_RT] MIDI Start: dev={}", device);
            send_midi_event(device as u32, QueuedMidiEvent::Start, devices, stats);
        }
        MidiData::Stop { device } => {
            tracing::debug!("[MIDI_RT] MIDI Stop: dev={}", device);
            send_midi_event(device as u32, QueuedMidiEvent::Stop, devices, stats);
        }
        MidiData::Continue { device } => {
            send_midi_event(device as u32, QueuedMidiEvent::Continue, devices, stats);
        }
    }
}

/// Send a MIDI event to a device.
#[cfg(feature = "native")]
#[inline]
fn send_midi_event(
    device_id: u32,
    event: QueuedMidiEvent,
    devices: &Arc<RwLock<HashMap<u32, MidiDeviceSender>>>,
    stats: &Arc<MidiRealtimeStats>,
) {
    // Fast path: try to get device without blocking
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
    fn test_noop_service() {
        let service = MidiRealtimeService::noop();
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
    fn test_device_unregistration() {
        let service = MidiRealtimeService::new();
        let (tx, _rx) = crossbeam_channel::unbounded();

        service.register_device(1, tx);
        service.unregister_device(1);

        let devices = service.devices.read();
        assert!(!devices.contains_key(&1));
    }

    #[test]
    fn test_queued_event_to_bytes() {
        // Note On
        let event = QueuedMidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        };
        assert_eq!(event.to_bytes(), vec![0x90, 60, 100]);

        // Note Off
        let event = QueuedMidiEvent::NoteOff {
            channel: 1,
            note: 72,
        };
        assert_eq!(event.to_bytes(), vec![0x81, 72, 0]);

        // CC
        let event = QueuedMidiEvent::ControlChange {
            channel: 2,
            cc: 74,
            value: 64,
        };
        assert_eq!(event.to_bytes(), vec![0xB2, 74, 64]);

        // Pitch Bend (center)
        let event = QueuedMidiEvent::PitchBend {
            channel: 0,
            value: 0,
        };
        assert_eq!(event.to_bytes(), vec![0xE0, 0x00, 0x40]); // 8192 = 0x2000 -> LSB=0, MSB=0x40

        // Transport
        assert_eq!(QueuedMidiEvent::Clock.to_bytes(), vec![0xF8]);
        assert_eq!(QueuedMidiEvent::Start.to_bytes(), vec![0xFA]);
        assert_eq!(QueuedMidiEvent::Stop.to_bytes(), vec![0xFC]);
        assert_eq!(QueuedMidiEvent::Continue.to_bytes(), vec![0xFB]);
    }
}
