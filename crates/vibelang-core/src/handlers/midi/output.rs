//! MIDI output thread and channel management.
//!
//! This module handles background threads for sending MIDI events
//! with low latency and proper timing.

use crate::compat::Instant;
use crate::midi::{QueuedMidiEvent, ScheduledMidiEvent};
use crate::traits::MidiOutputCapability;
use crate::types::ids::MidiDeviceId;
use crate::Error;
use crossbeam_channel::{Receiver, Sender};
use midir::{MidiOutput, MidiOutputConnection};
use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// How often the output thread checks for due events (microseconds).
/// 100μs = 10kHz check rate, ~0.1ms precision.
const MIDI_THREAD_POLL_INTERVAL_US: u64 = 100;
const MIDI_CLOCK_CHANNEL_CAPACITY: usize = 128;

pub type ClockOutputChannels = Arc<Mutex<HashMap<MidiDeviceId, Sender<QueuedMidiEvent>>>>;

/// State for a background MIDI output thread.
pub struct OutputThreadState {
    /// Thread handle.
    pub handle: JoinHandle<()>,
    /// Running flag.
    pub running: Arc<AtomicBool>,
    /// Device capability.
    #[allow(dead_code)]
    pub capability: MidiOutputCapability,
}

/// Manager for MIDI output channels and threads.
pub struct MidiOutputManager {
    /// Output channel senders (for realtime service integration).
    /// Maps device ID to the sender channel.
    pub output_channels: Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>>,

    /// Isolated high-priority clock/control senders.
    pub clock_channels: ClockOutputChannels,

    /// Background output threads (for realtime service integration).
    pub output_threads: Arc<Mutex<HashMap<MidiDeviceId, OutputThreadState>>>,

    /// Device capabilities cache.
    pub device_capabilities: Arc<Mutex<HashMap<MidiDeviceId, MidiOutputCapability>>>,
}

impl Default for MidiOutputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiOutputManager {
    /// Create a new output manager.
    pub fn new() -> Self {
        Self {
            output_channels: Arc::new(Mutex::new(HashMap::new())),
            clock_channels: Arc::new(Mutex::new(HashMap::new())),
            output_threads: Arc::new(Mutex::new(HashMap::new())),
            device_capabilities: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the capability of a device (if channel exists).
    #[allow(dead_code)]
    pub fn get_capability(&self, id: MidiDeviceId) -> Option<MidiOutputCapability> {
        self.device_capabilities.lock().ok()?.get(&id).copied()
    }

    /// Create an output channel for a MIDI device.
    ///
    /// This opens the device and spawns a background thread to send MIDI events.
    /// The thread uses a timed queue for sub-millisecond scheduling precision.
    ///
    /// # Arguments
    ///
    /// * `id` - The MIDI device ID
    ///
    /// # Returns
    ///
    /// A tuple of (`Sender<ScheduledMidiEvent>`, `MidiOutputCapability`).
    /// The sender can be passed to `MidiRealtimeService::register_device_with_capability()`.
    pub fn create_output_channel(
        &self,
        id: MidiDeviceId,
    ) -> crate::Result<(Sender<ScheduledMidiEvent>, MidiOutputCapability)> {
        // Check if we already have a channel for this device
        {
            let channels = self
                .output_channels
                .lock()
                .map_err(|e| Error::MidiError(format!("Output channels lock poisoned: {e}")))?;
            if let Some(sender) = channels.get(&id) {
                let capability = self
                    .device_capabilities
                    .lock()
                    .map_err(|e| {
                        Error::MidiError(format!("Device capabilities lock poisoned: {e}"))
                    })?
                    .get(&id)
                    .copied()
                    .unwrap_or(MidiOutputCapability::Midi2ViaTranslation);
                return Ok((sender.clone(), capability));
            }
        }

        // Open the MIDI output device
        let midi_out = MidiOutput::new("vibelang-core2-rt")
            .map_err(|e| Error::MidiError(format!("Failed to create MIDI output: {}", e)))?;

        let ports = midi_out.ports();
        let port = ports
            .get(id.0 as usize)
            .ok_or_else(|| Error::MidiError(format!("MIDI device {} not found", id.0)))?;

        let port_name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| format!("Unknown {}", id.0));

        // Detect MIDI 2.0 capability based on device name
        let capability = detect_midi2_capability(&port_name);

        let conn = midi_out
            .connect(port, "vibelang-rt-output")
            .map_err(|e| Error::MidiError(format!("Failed to connect MIDI output: {}", e)))?;

        // Create independent channels for scheduled notes and realtime clock/control.
        let (tx, rx) = crossbeam_channel::bounded::<ScheduledMidiEvent>(1024);
        let (clock_tx, clock_rx) =
            crossbeam_channel::bounded::<QueuedMidiEvent>(MIDI_CLOCK_CHANNEL_CAPACITY);

        // Spawn the background thread with capability awareness
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let device_id = id.0;

        let handle = thread::Builder::new()
            .name(format!("midi-out-{}", device_id))
            .spawn(move || {
                run_output_thread(conn, rx, clock_rx, running_clone, device_id, capability);
            })
            .map_err(|e| Error::MidiError(format!("Failed to spawn output thread: {}", e)))?;

        // Store the sender, thread state, and capability
        self.output_channels
            .lock()
            .map_err(|e| Error::MidiError(format!("Output channels lock poisoned: {e}")))?
            .insert(id, tx.clone());
        self.clock_channels
            .lock()
            .map_err(|e| Error::MidiError(format!("Clock channels lock poisoned: {e}")))?
            .insert(id, clock_tx);
        self.device_capabilities
            .lock()
            .map_err(|e| Error::MidiError(format!("Device capabilities lock poisoned: {e}")))?
            .insert(id, capability);
        self.output_threads
            .lock()
            .map_err(|e| Error::MidiError(format!("Output threads lock poisoned: {e}")))?
            .insert(
                id,
                OutputThreadState {
                    handle,
                    running,
                    capability,
                },
            );

        let cap_str = match capability {
            MidiOutputCapability::Midi2Native => "MIDI 2.0 Native",
            MidiOutputCapability::Midi2ViaTranslation => "MIDI 2.0 via translation",
            MidiOutputCapability::Midi1Only => "MIDI 1.0 only",
        };

        tracing::info!(
            "Created MIDI output channel for {} (id={}, {})",
            port_name,
            id.0,
            cap_str
        );

        Ok((tx, capability))
    }

    /// Close an output channel and stop its background thread.
    pub fn close_output_channel(&self, id: MidiDeviceId) {
        // Remove the sender
        if let Ok(mut channels) = self.output_channels.lock() {
            channels.remove(&id);
        }
        if let Ok(mut channels) = self.clock_channels.lock() {
            channels.remove(&id);
        }

        // Stop the thread
        if let Some(state) = self
            .output_threads
            .lock()
            .ok()
            .and_then(|mut t| t.remove(&id))
        {
            state.running.store(false, Ordering::SeqCst);
            // The thread will exit when it sees the running flag is false
            // or when the channel is disconnected
            let _ = state.handle.join();
            tracing::info!("Closed MIDI output channel for device {}", id.0);
        }
    }

    /// Get the output channel sender for a device (if it exists).
    pub fn get_output_channel(&self, id: MidiDeviceId) -> Option<Sender<ScheduledMidiEvent>> {
        self.output_channels.lock().ok()?.get(&id).cloned()
    }
}

/// Background thread for sending MIDI events to a device with precise timing.
///
/// This thread receives `ScheduledMidiEvent`s from the channel and sends them
/// to the MIDI device at their scheduled timestamps. Events are held in a
/// priority queue (min-heap by timestamp) and sent when their time arrives.
///
/// ## Timing Precision
///
/// The thread polls at 100μs intervals (10kHz) to achieve ~0.1ms timing precision.
/// This is sufficient for professional MIDI timing requirements.
///
/// ## MIDI 2.0 Handling
///
/// For `Midi2Native` devices, high-resolution values are preserved and encoded
/// as UMP packets (when the transport supports it).
///
/// For `Midi2ViaTranslation` devices, MIDI 1.0 bytes are sent and the OS
/// translation layer handles upscaling.
///
/// For `Midi1Only` devices, values are downscaled to 7-bit MIDI 1.0.
fn run_output_thread(
    mut conn: MidiOutputConnection,
    rx: Receiver<ScheduledMidiEvent>,
    clock_rx: Receiver<QueuedMidiEvent>,
    running: Arc<AtomicBool>,
    device_id: u32,
    capability: MidiOutputCapability,
) {
    let cap_str = match capability {
        MidiOutputCapability::Midi2Native => "MIDI 2.0 Native",
        MidiOutputCapability::Midi2ViaTranslation => "MIDI 2.0 via translation",
        MidiOutputCapability::Midi1Only => "MIDI 1.0 only",
    };
    tracing::info!(
        "[MIDI_OUT_{}] Background thread started with timed queue ({})",
        device_id,
        cap_str
    );

    // Priority queue for scheduled events (min-heap by timestamp)
    let mut scheduled: BinaryHeap<ScheduledMidiEvent> = BinaryHeap::new();
    let poll_duration = std::time::Duration::from_micros(MIDI_THREAD_POLL_INTERVAL_US);

    while running.load(Ordering::Relaxed) {
        // 1. Clock/control messages have priority and independent coalescing.
        send_clock_channel_events(&mut conn, &clock_rx, device_id, capability);

        // 2. Receive all pending normal events from the channel (non-blocking)
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    scheduled.push(event);
                }
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    tracing::info!("[MIDI_OUT_{}] Channel disconnected, exiting", device_id);
                    return;
                }
            }
        }

        // 3. Send all normal events that are due, checking clock between sends.
        let now = Instant::now();
        while let Some((event, dropped_clocks)) = pop_due_event(&mut scheduled, now) {
            if dropped_clocks > 0 {
                tracing::warn!(
                    "[MIDI_OUT_{}] Dropped {} coalesced late clock event(s) from normal queue",
                    device_id,
                    dropped_clocks
                );
            }
            send_event_to_connection(&mut conn, &event.event, device_id, capability);
            send_clock_channel_events(&mut conn, &clock_rx, device_id, capability);
        }

        // 4. Sleep briefly before next poll (100μs for ~0.1ms precision)
        std::thread::sleep(poll_duration);
    }

    tracing::info!("[MIDI_OUT_{}] Background thread exiting", device_id);
}

fn send_clock_channel_events(
    conn: &mut MidiOutputConnection,
    clock_rx: &Receiver<QueuedMidiEvent>,
    device_id: u32,
    capability: MidiOutputCapability,
) {
    let batch = drain_clock_channel(clock_rx);
    if batch.is_empty() {
        return;
    }

    let (events, dropped_clocks) = coalesce_clock_events(batch);
    if dropped_clocks > 0 {
        tracing::warn!(
            "[MIDI_OUT_{}] Dropped {} coalesced clock event(s)",
            device_id,
            dropped_clocks
        );
    }

    for event in events {
        send_event_to_connection(conn, &event, device_id, capability);
    }
}

fn drain_clock_channel(clock_rx: &Receiver<QueuedMidiEvent>) -> Vec<QueuedMidiEvent> {
    let mut batch = Vec::new();
    loop {
        match clock_rx.try_recv() {
            Ok(event) => batch.push(event),
            Err(crossbeam_channel::TryRecvError::Empty)
            | Err(crossbeam_channel::TryRecvError::Disconnected) => break,
        }
    }
    batch
}

fn coalesce_clock_events(events: Vec<QueuedMidiEvent>) -> (Vec<QueuedMidiEvent>, u64) {
    let mut coalesced = Vec::with_capacity(events.len());
    let mut pending_clock = false;
    let mut dropped = 0;

    for event in events {
        if matches!(event, QueuedMidiEvent::Clock) {
            if pending_clock {
                dropped += 1;
            } else {
                pending_clock = true;
            }
            continue;
        }

        if pending_clock {
            coalesced.push(QueuedMidiEvent::Clock);
            pending_clock = false;
        }
        coalesced.push(event);
    }

    if pending_clock {
        coalesced.push(QueuedMidiEvent::Clock);
    }

    (coalesced, dropped)
}

fn pop_due_event(
    scheduled: &mut BinaryHeap<ScheduledMidiEvent>,
    now: Instant,
) -> Option<(ScheduledMidiEvent, u64)> {
    if scheduled.peek()?.timestamp > now {
        return None;
    }

    let event = scheduled.pop()?;
    if !matches!(event.event, QueuedMidiEvent::Clock) {
        return Some((event, 0));
    }

    let mut dropped = 0;
    while matches!(
        scheduled.peek(),
        Some(next)
            if next.timestamp <= now && matches!(next.event, QueuedMidiEvent::Clock)
    ) {
        scheduled.pop();
        dropped += 1;
    }

    Some((event, dropped))
}

fn send_event_to_connection(
    conn: &mut MidiOutputConnection,
    event: &QueuedMidiEvent,
    device_id: u32,
    capability: MidiOutputCapability,
) {
    let bytes = encode_event_for_capability(event, capability);

    if !bytes.is_empty() {
        tracing::debug!(
            "[MIDI_OUT_{}] Sending {:?} ({} bytes: {:02x?})",
            device_id,
            event,
            bytes.len(),
            bytes
        );
        if let Err(e) = conn.send(&bytes) {
            tracing::warn!("[MIDI_OUT_{}] Failed to send MIDI: {}", device_id, e);
        }
    }
}

fn encode_event_for_capability(
    event: &QueuedMidiEvent,
    capability: MidiOutputCapability,
) -> Vec<u8> {
    match capability {
        MidiOutputCapability::Midi2Native => event.to_ump_bytes(),
        MidiOutputCapability::Midi2ViaTranslation | MidiOutputCapability::Midi1Only => {
            event.to_bytes()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_channel_coalesces_pending_clock_events() {
        let (events, dropped) = coalesce_clock_events(vec![
            QueuedMidiEvent::Clock,
            QueuedMidiEvent::Clock,
            QueuedMidiEvent::Start,
            QueuedMidiEvent::Clock,
            QueuedMidiEvent::Clock,
        ]);

        assert_eq!(dropped, 2);
        assert!(matches!(events[0], QueuedMidiEvent::Clock));
        assert!(matches!(events[1], QueuedMidiEvent::Start));
        assert!(matches!(events[2], QueuedMidiEvent::Clock));
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn output_thread_coalesces_late_clock_events_from_normal_queue() {
        let now = Instant::now();
        let mut scheduled = BinaryHeap::new();
        scheduled.push(ScheduledMidiEvent::new(
            QueuedMidiEvent::Clock,
            now - std::time::Duration::from_millis(3),
        ));
        scheduled.push(ScheduledMidiEvent::new(
            QueuedMidiEvent::Clock,
            now - std::time::Duration::from_millis(2),
        ));
        scheduled.push(ScheduledMidiEvent::new(
            QueuedMidiEvent::Clock,
            now - std::time::Duration::from_millis(1),
        ));

        let Some((event, dropped)) = pop_due_event(&mut scheduled, now) else {
            panic!("expected one coalesced clock event");
        };

        assert!(matches!(event.event, QueuedMidiEvent::Clock));
        assert_eq!(dropped, 2);
        assert!(scheduled.is_empty());
    }

    #[test]
    fn channel_pressure_does_not_block_isolated_clock_channel() {
        let (normal_tx, _normal_rx) = crossbeam_channel::bounded::<ScheduledMidiEvent>(1);
        let pressure = QueuedMidiEvent::Midi2ChannelPressure {
            group: 0,
            channel: 0,
            pressure: u32::MAX,
        };

        normal_tx
            .try_send(ScheduledMidiEvent::immediate(pressure.clone()))
            .unwrap();
        assert!(normal_tx
            .try_send(ScheduledMidiEvent::immediate(pressure))
            .is_err());

        let (clock_tx, clock_rx) = crossbeam_channel::bounded::<QueuedMidiEvent>(1);
        assert!(clock_tx.try_send(QueuedMidiEvent::Clock).is_ok());

        let drained = drain_clock_channel(&clock_rx);
        assert_eq!(drained.len(), 1);
        assert!(matches!(drained[0], QueuedMidiEvent::Clock));
    }
}

/// Detect MIDI 2.0 output capability of a device.
///
/// **Philosophy: MIDI 2.0 by default, fallback to MIDI 1.0 only when necessary.**
///
/// Modern systems (Linux 6.5+, macOS 11+) have native MIDI 2.0 support via
/// ALSA UMP or CoreMIDI. Even when midir sends MIDI 1.0 bytes, the OS can
/// translate to MIDI 2.0 UMP format for devices that support it.
///
/// We only fall back to explicit MIDI 1.0 mode for:
/// - Known legacy-only devices
/// - Systems without MIDI 2.0 infrastructure
pub fn detect_midi2_capability(device_name: &str) -> MidiOutputCapability {
    let name_lower = device_name.to_lowercase();

    // Known MIDI 2.0 native devices (high-resolution controllers)
    let midi2_native_keywords = [
        "seaboard",
        "linnstrument",
        "continuum",
        "sensel",
        "roli",
        "osmose",
        "lumatone",
        "k-board",
        "erae touch",
        "lightpad",
        "blocks",
    ];

    // Known legacy MIDI 1.0 only devices (older hardware)
    let midi1_only_keywords = [
        "usb midi",       // Generic USB-MIDI adapters
        "midi 1",         // Explicitly marked MIDI 1
        "um-one",         // Roland UM-ONE adapters
        "midisport",      // M-Audio MIDISPORT adapters
        "midi interface", // Generic interfaces
    ];

    // Check for known MIDI 1.0 only devices first
    if midi1_only_keywords.iter().any(|k| name_lower.contains(k)) {
        tracing::debug!(
            "Device '{}' detected as MIDI 1.0 only (legacy device)",
            device_name
        );
        return MidiOutputCapability::Midi1Only;
    }

    // Check for known MIDI 2.0 native devices
    let is_midi2_native = midi2_native_keywords.iter().any(|k| name_lower.contains(k));

    // Check OS-level MIDI 2.0 support
    #[cfg(target_os = "linux")]
    {
        // Check kernel version for UMP support (6.5+)
        let has_modern_kernel = std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .and_then(|o| {
                let version = String::from_utf8_lossy(&o.stdout);
                let parts: Vec<_> = version.split('.').take(2).collect();
                if parts.len() >= 2 {
                    let major = parts[0].parse::<u32>().ok()?;
                    let minor = parts[1]
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<u32>()
                        .ok()?;
                    Some(major > 6 || (major == 6 && minor >= 5))
                } else {
                    None
                }
            })
            .unwrap_or(false);

        // Check if PipeWire is running (provides MIDI 2.0 translation layer)
        let has_pipewire = std::process::Command::new("pgrep")
            .arg("-x")
            .arg("pipewire")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if is_midi2_native {
            if has_modern_kernel {
                tracing::debug!(
                    "Device '{}' detected as MIDI 2.0 native (Linux 6.5+ kernel)",
                    device_name
                );
                return MidiOutputCapability::Midi2Native;
            }
            tracing::debug!(
                "Device '{}' detected as MIDI 2.0 via translation",
                device_name
            );
            return MidiOutputCapability::Midi2ViaTranslation;
        }

        // Default: Modern Linux systems support MIDI 2.0 via translation
        if has_modern_kernel || has_pipewire {
            tracing::debug!(
                "Device '{}' defaulting to MIDI 2.0 via translation (modern Linux)",
                device_name
            );
            return MidiOutputCapability::Midi2ViaTranslation;
        }
    }

    #[cfg(target_os = "macos")]
    {
        // macOS 11+ (Big Sur) has CoreMIDI 2.0 support
        // All modern macOS systems support MIDI 2.0 translation
        if is_midi2_native {
            tracing::debug!(
                "Device '{}' detected as MIDI 2.0 native (macOS CoreMIDI)",
                device_name
            );
            return MidiOutputCapability::Midi2Native;
        }

        // Default: macOS supports MIDI 2.0 via CoreMIDI translation
        tracing::debug!(
            "Device '{}' defaulting to MIDI 2.0 via translation (macOS CoreMIDI)",
            device_name
        );
        return MidiOutputCapability::Midi2ViaTranslation;
    }

    #[cfg(target_os = "windows")]
    {
        // Windows MIDI 2.0 support varies - be conservative
        if is_midi2_native {
            tracing::debug!(
                "Device '{}' detected as MIDI 2.0 via translation (Windows)",
                device_name
            );
            return MidiOutputCapability::Midi2ViaTranslation;
        }
    }

    // Default for unknown systems: assume MIDI 2.0 translation support
    // Modern MIDI infrastructure handles translation automatically
    tracing::debug!(
        "Device '{}' defaulting to MIDI 2.0 via translation",
        device_name
    );
    MidiOutputCapability::Midi2ViaTranslation
}

/// Send a MIDI 2.0 event to a device, handling capability differences.
///
/// **Behavior by capability:**
/// - `Midi2Native`: Send with full resolution (OS/device handles UMP natively)
/// - `Midi2ViaTranslation`: Send with full resolution (OS translates to device)
/// - `Midi1Only`: Downscale to 7-bit MIDI 1.0 (lossy)
///
/// Note: midir currently only supports byte-level output, so we send MIDI 1.0 bytes
/// in all cases. However, for MIDI 2.0 capable systems, the OS translation layer
/// (PipeWire, CoreMIDI) can upscale these back to MIDI 2.0 for the device.
/// The key difference is semantic: we preserve high-resolution intent internally.
pub fn send_midi2_event_to_device(
    conn: &mut MidiOutputConnection,
    event: &QueuedMidiEvent,
    capability: MidiOutputCapability,
) -> std::result::Result<(), midir::SendError> {
    match capability {
        MidiOutputCapability::Midi2Native | MidiOutputCapability::Midi2ViaTranslation => {
            // MIDI 2.0 capable device/system
            // The OS translation layer will handle conversion if needed
            // We preserve the high-resolution data in our internal representation
            let bytes = event.to_bytes();
            tracing::trace!(
                "MIDI 2.0 output: {:?} -> {} bytes (OS translation available)",
                event,
                bytes.len()
            );
            conn.send(&bytes)
        }
        MidiOutputCapability::Midi1Only => {
            // Legacy MIDI 1.0 device - explicit downscaling with warning for high-res events
            let bytes = event.to_bytes();

            // Log when we're losing precision
            match event {
                QueuedMidiEvent::Midi2NoteOn { velocity, .. } if *velocity > 127 => {
                    tracing::debug!(
                        "Downscaling MIDI 2.0 velocity {} to 7-bit for MIDI 1.0 device",
                        velocity
                    );
                }
                QueuedMidiEvent::Midi2ControlChange { value, .. } if *value > 127 => {
                    tracing::debug!(
                        "Downscaling MIDI 2.0 CC value {} to 7-bit for MIDI 1.0 device",
                        value
                    );
                }
                QueuedMidiEvent::Midi2PitchBend { value, .. } => {
                    tracing::debug!(
                        "Downscaling MIDI 2.0 pitch bend {} to 14-bit for MIDI 1.0 device",
                        value
                    );
                }
                QueuedMidiEvent::Midi2PerNotePitchBend { .. }
                | QueuedMidiEvent::Midi2PerNoteController { .. } => {
                    tracing::debug!("Dropping MIDI 2.0 per-note message (no MIDI 1.0 equivalent)");
                    return Ok(()); // Skip - no equivalent in MIDI 1.0
                }
                _ => {}
            }

            conn.send(&bytes)
        }
    }
}
