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

use crate::compat::Instant;
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
    // MIDI 1.0 messages
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    ControlChange {
        channel: u8,
        cc: u8,
        value: u8,
    },
    PitchBend {
        channel: u8,
        value: i16,
    },
    Clock,
    Start,
    Stop,
    Continue,

    // MIDI 2.0 messages (UMP Channel Voice Messages)
    Midi2NoteOn {
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
        attribute_type: u8,
        attribute_value: u16,
    },
    Midi2NoteOff {
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
        attribute_type: u8,
        attribute_value: u16,
    },
    Midi2ControlChange {
        group: u8,
        channel: u8,
        controller: u8,
        value: u32,
    },
    Midi2PitchBend {
        group: u8,
        channel: u8,
        value: u32,
    },
    Midi2PerNotePitchBend {
        group: u8,
        channel: u8,
        note: u8,
        value: u32,
    },
    Midi2PerNoteController {
        group: u8,
        channel: u8,
        note: u8,
        controller: u8,
        value: u32,
    },
    Midi2PolyPressure {
        group: u8,
        channel: u8,
        note: u8,
        pressure: u32,
    },
    Midi2ChannelPressure {
        group: u8,
        channel: u8,
        pressure: u32,
    },
    Midi2ProgramChange {
        group: u8,
        channel: u8,
        program: u8,
        bank_valid: bool,
        bank_msb: u8,
        bank_lsb: u8,
    },
}

impl QueuedMidiEvent {
    /// Create a scheduled version of this event at the given timestamp.
    pub fn at(self, timestamp: Instant) -> ScheduledMidiEvent {
        ScheduledMidiEvent::new(self, timestamp)
    }

    /// Create an immediate scheduled event (timestamp = now).
    pub fn immediate(self) -> ScheduledMidiEvent {
        ScheduledMidiEvent::immediate(self)
    }
}

/// Global monotonic sequence for [`ScheduledMidiEvent`] creation order.
///
/// The output-thread heap orders by timestamp; this breaks ties so that
/// events scheduled for the same instant are sent in the order they were
/// created (e.g. a voice-pool `NoteOff(stolen)` before the `NoteOn(new)`
/// that stole its slot, or a same-deadline retrigger's off-before-on).
static SCHEDULED_EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

/// A MIDI event scheduled for a specific wall-clock time.
///
/// Used for lookahead scheduling to achieve sub-millisecond timing precision.
/// Events are queued with their target timestamp and sent at the exact moment.
#[derive(Debug, Clone)]
pub struct ScheduledMidiEvent {
    /// When this event should be sent (wall-clock time).
    pub timestamp: Instant,
    /// The MIDI event to send.
    pub event: QueuedMidiEvent,
    /// Creation order, used to break timestamp ties in the output heap.
    seq: u64,
}

impl ScheduledMidiEvent {
    /// Create a new scheduled event.
    pub fn new(event: QueuedMidiEvent, timestamp: Instant) -> Self {
        Self {
            timestamp,
            event,
            seq: SCHEDULED_EVENT_SEQ.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Create an immediate event (timestamp = now).
    pub fn immediate(event: QueuedMidiEvent) -> Self {
        Self::new(event, Instant::now())
    }

    /// Check if this event is due (timestamp has passed).
    #[inline]
    pub fn is_due(&self) -> bool {
        Instant::now() >= self.timestamp
    }

    /// Get time until this event is due.
    pub fn time_until_due(&self) -> Option<Duration> {
        let now = Instant::now();
        if now >= self.timestamp {
            None
        } else {
            Some(self.timestamp.duration_since(now))
        }
    }
}

// Implement ordering for BinaryHeap (min-heap by timestamp, then by
// creation sequence so equal-deadline events keep their enqueue order)
impl PartialEq for ScheduledMidiEvent {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp && self.seq == other.seq
    }
}

impl Eq for ScheduledMidiEvent {}

impl PartialOrd for ScheduledMidiEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledMidiEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering for min-heap (earlier timestamps have higher
        // priority); ties broken by creation order (earlier seq first) so
        // e.g. NoteOn→NoteOff pairs at the same instant stay ordered.
        other
            .timestamp
            .cmp(&self.timestamp)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl QueuedMidiEvent {
    /// Convert to UMP (Universal MIDI Packet) bytes for MIDI 2.0 native devices.
    ///
    /// Returns 32-bit or 64-bit UMP packets encoded as bytes (big-endian).
    /// For devices that support native MIDI 2.0 transport, this preserves
    /// full resolution (16-bit velocity, 32-bit CC values, etc.).
    ///
    /// ## UMP Format
    ///
    /// - Message Type 0x1 (32-bit): System Real Time (Clock, Start, Stop, Continue)
    /// - Message Type 0x2 (32-bit): MIDI 1.0 Channel Voice (fallback for MIDI 1.0 events)
    /// - Message Type 0x4 (64-bit): MIDI 2.0 Channel Voice (high-resolution)
    ///
    /// Note: Currently midir only supports MIDI 1.0 byte streams, so this
    /// encodes UMP packets but the transport may need to handle them specially.
    /// When a native MIDI 2.0 backend is available, these bytes can be sent directly.
    pub fn to_ump_bytes(&self) -> Vec<u8> {
        match self {
            // MIDI 1.0 events -> Message Type 0x2 (32-bit MIDI 1.0 Channel Voice)
            QueuedMidiEvent::NoteOn {
                channel,
                note,
                velocity,
            } => {
                // MT=0x2 (MIDI 1.0 CV), Group=0, Status=0x9n, Note, Velocity, 0
                let word: u32 = (0x20 << 24)
                    | ((0x90 | (channel & 0x0F)) as u32) << 16
                    | ((*note & 0x7F) as u32) << 8
                    | (*velocity & 0x7F) as u32;
                word.to_be_bytes().to_vec()
            }
            QueuedMidiEvent::NoteOff { channel, note } => {
                let word: u32 = (0x20 << 24)
                    | ((0x80 | (channel & 0x0F)) as u32) << 16
                    | ((*note & 0x7F) as u32) << 8;
                word.to_be_bytes().to_vec()
            }
            QueuedMidiEvent::ControlChange { channel, cc, value } => {
                let word: u32 = (0x20 << 24)
                    | ((0xB0 | (channel & 0x0F)) as u32) << 16
                    | ((*cc & 0x7F) as u32) << 8
                    | (*value & 0x7F) as u32;
                word.to_be_bytes().to_vec()
            }
            QueuedMidiEvent::PitchBend { channel, value } => {
                let pb = (*value + 8192) as u16;
                let lsb = (pb & 0x7F) as u8;
                let msb = ((pb >> 7) & 0x7F) as u8;
                let word: u32 = (0x20 << 24)
                    | ((0xE0 | (channel & 0x0F)) as u32) << 16
                    | (lsb as u32) << 8
                    | msb as u32;
                word.to_be_bytes().to_vec()
            }

            // System Real Time -> Message Type 0x1 (32-bit System)
            QueuedMidiEvent::Clock => {
                // MT=0x1, Group=0, Status=0xF8
                let word: u32 = (0x10 << 24) | (0xF8 << 16);
                word.to_be_bytes().to_vec()
            }
            QueuedMidiEvent::Start => {
                let word: u32 = (0x10 << 24) | (0xFA << 16);
                word.to_be_bytes().to_vec()
            }
            QueuedMidiEvent::Stop => {
                let word: u32 = (0x10 << 24) | (0xFC << 16);
                word.to_be_bytes().to_vec()
            }
            QueuedMidiEvent::Continue => {
                let word: u32 = (0x10 << 24) | (0xFB << 16);
                word.to_be_bytes().to_vec()
            }

            // MIDI 2.0 events -> Message Type 0x4 (64-bit MIDI 2.0 Channel Voice)
            QueuedMidiEvent::Midi2NoteOn {
                group,
                channel,
                note,
                velocity,
                attribute_type,
                attribute_value,
            } => {
                // Word 1: MT=0x4, Group, Status=0x9n, Note
                // Word 2: Velocity (16-bit), Attribute Type, Attribute Value (16-bit)
                let word1: u32 = ((0x40 | (group & 0x0F)) as u32) << 24
                    | ((0x90 | (channel & 0x0F)) as u32) << 16
                    | ((*note & 0x7F) as u32) << 8
                    | (*attribute_type as u32);
                let word2: u32 = ((*velocity as u32) << 16) | (*attribute_value as u32);
                let mut bytes = word1.to_be_bytes().to_vec();
                bytes.extend_from_slice(&word2.to_be_bytes());
                bytes
            }
            QueuedMidiEvent::Midi2NoteOff {
                group,
                channel,
                note,
                velocity,
                attribute_type,
                attribute_value,
            } => {
                let word1: u32 = ((0x40 | (group & 0x0F)) as u32) << 24
                    | ((0x80 | (channel & 0x0F)) as u32) << 16
                    | ((*note & 0x7F) as u32) << 8
                    | (*attribute_type as u32);
                let word2: u32 = ((*velocity as u32) << 16) | (*attribute_value as u32);
                let mut bytes = word1.to_be_bytes().to_vec();
                bytes.extend_from_slice(&word2.to_be_bytes());
                bytes
            }
            QueuedMidiEvent::Midi2ControlChange {
                group,
                channel,
                controller,
                value,
            } => {
                // MIDI 2.0 CC: MT=0x4, Group, Status=0xBn, Controller
                // Word 2: 32-bit value
                let word1: u32 = ((0x40 | (group & 0x0F)) as u32) << 24
                    | ((0xB0 | (channel & 0x0F)) as u32) << 16
                    | ((*controller & 0x7F) as u32) << 8;
                let word2: u32 = *value;
                let mut bytes = word1.to_be_bytes().to_vec();
                bytes.extend_from_slice(&word2.to_be_bytes());
                bytes
            }
            QueuedMidiEvent::Midi2PitchBend {
                group,
                channel,
                value,
            } => {
                // MIDI 2.0 Pitch Bend: MT=0x4, Group, Status=0xEn
                // Word 2: 32-bit value (0x80000000 = center)
                let word1: u32 = ((0x40 | (group & 0x0F)) as u32) << 24
                    | ((0xE0 | (channel & 0x0F)) as u32) << 16;
                let word2: u32 = *value;
                let mut bytes = word1.to_be_bytes().to_vec();
                bytes.extend_from_slice(&word2.to_be_bytes());
                bytes
            }
            QueuedMidiEvent::Midi2PerNotePitchBend {
                group,
                channel,
                note,
                value,
            } => {
                // Per-Note Pitch Bend: MT=0x4, Group, Status=0x6n, Note
                // Word 2: 32-bit value
                let word1: u32 = ((0x40 | (group & 0x0F)) as u32) << 24
                    | ((0x60 | (channel & 0x0F)) as u32) << 16
                    | ((*note & 0x7F) as u32) << 8;
                let word2: u32 = *value;
                let mut bytes = word1.to_be_bytes().to_vec();
                bytes.extend_from_slice(&word2.to_be_bytes());
                bytes
            }
            QueuedMidiEvent::Midi2PerNoteController {
                group,
                channel,
                note,
                controller,
                value,
            } => {
                // Per-Note CC: MT=0x4, Group, Status=0x1n, Note
                // Word 2: Controller (8-bit) | 0 | 32-bit value (split across)
                // Actually: Word1[7:0] = note, Word2 = controller:8 + value:32 is not right
                // Correct: Per-Note CC is opcode 0x01, Index = controller
                let word1: u32 = ((0x40 | (group & 0x0F)) as u32) << 24
                    | ((0x10 | (channel & 0x0F)) as u32) << 16
                    | ((*note & 0x7F) as u32) << 8
                    | (*controller as u32);
                let word2: u32 = *value;
                let mut bytes = word1.to_be_bytes().to_vec();
                bytes.extend_from_slice(&word2.to_be_bytes());
                bytes
            }
            QueuedMidiEvent::Midi2PolyPressure {
                group,
                channel,
                note,
                pressure,
            } => {
                // Poly Pressure: MT=0x4, Group, Status=0xAn, Note
                let word1: u32 = ((0x40 | (group & 0x0F)) as u32) << 24
                    | ((0xA0 | (channel & 0x0F)) as u32) << 16
                    | ((*note & 0x7F) as u32) << 8;
                let word2: u32 = *pressure;
                let mut bytes = word1.to_be_bytes().to_vec();
                bytes.extend_from_slice(&word2.to_be_bytes());
                bytes
            }
            QueuedMidiEvent::Midi2ChannelPressure {
                group,
                channel,
                pressure,
            } => {
                // Channel Pressure: MT=0x4, Group, Status=0xDn
                let word1: u32 = ((0x40 | (group & 0x0F)) as u32) << 24
                    | ((0xD0 | (channel & 0x0F)) as u32) << 16;
                let word2: u32 = *pressure;
                let mut bytes = word1.to_be_bytes().to_vec();
                bytes.extend_from_slice(&word2.to_be_bytes());
                bytes
            }
            QueuedMidiEvent::Midi2ProgramChange {
                group,
                channel,
                program,
                bank_valid,
                bank_msb,
                bank_lsb,
            } => {
                // Program Change: MT=0x4, Group, Status=0xCn
                // Word 2: Options (bank valid flag) | Program | Bank MSB | Bank LSB
                let options = if *bank_valid { 0x01u32 } else { 0x00 };
                let word1: u32 = ((0x40 | (group & 0x0F)) as u32) << 24
                    | ((0xC0 | (channel & 0x0F)) as u32) << 16;
                let word2: u32 = (options << 24)
                    | ((*program & 0x7F) as u32) << 16
                    | ((*bank_msb & 0x7F) as u32) << 8
                    | (*bank_lsb & 0x7F) as u32;
                let mut bytes = word1.to_be_bytes().to_vec();
                bytes.extend_from_slice(&word2.to_be_bytes());
                bytes
            }
        }
    }

    /// Convert to raw MIDI 1.0 bytes.
    ///
    /// MIDI 2.0 messages are downgraded to MIDI 1.0 format.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            // MIDI 1.0 messages
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

            // MIDI 2.0 messages (downgrade to MIDI 1.0)
            QueuedMidiEvent::Midi2NoteOn {
                channel,
                note,
                velocity,
                ..
            } => {
                // Convert 16-bit velocity to 7-bit
                let vel7 = (velocity >> 9) as u8;
                vec![0x90 | (channel & 0x0F), note & 0x7F, vel7.max(1)]
            }
            QueuedMidiEvent::Midi2NoteOff { channel, note, .. } => {
                vec![0x80 | (channel & 0x0F), note & 0x7F, 0]
            }
            QueuedMidiEvent::Midi2ControlChange {
                channel,
                controller,
                value,
                ..
            } => {
                // Convert 32-bit value to 7-bit
                let val7 = (value >> 25) as u8;
                vec![0xB0 | (channel & 0x0F), controller & 0x7F, val7 & 0x7F]
            }
            QueuedMidiEvent::Midi2PitchBend { channel, value, .. } => {
                // Convert 32-bit unsigned to 14-bit signed pitch bend
                // MIDI 2.0 uses 0x80000000 as center, MIDI 1.0 uses 0x2000
                let pb14 = (value >> 18) as u16;
                let lsb = (pb14 & 0x7F) as u8;
                let msb = ((pb14 >> 7) & 0x7F) as u8;
                vec![0xE0 | (channel & 0x0F), lsb, msb]
            }
            QueuedMidiEvent::Midi2PerNotePitchBend { .. } => {
                // Per-note pitch bend has no MIDI 1.0 equivalent
                vec![]
            }
            QueuedMidiEvent::Midi2PerNoteController { .. } => {
                // Per-note controller has no MIDI 1.0 equivalent
                vec![]
            }
            QueuedMidiEvent::Midi2PolyPressure {
                channel,
                note,
                pressure,
                ..
            } => {
                // Convert 32-bit pressure to 7-bit
                let press7 = (pressure >> 25) as u8;
                vec![0xA0 | (channel & 0x0F), note & 0x7F, press7 & 0x7F]
            }
            QueuedMidiEvent::Midi2ChannelPressure {
                channel, pressure, ..
            } => {
                // Convert 32-bit pressure to 7-bit
                let press7 = (pressure >> 25) as u8;
                vec![0xD0 | (channel & 0x0F), press7 & 0x7F]
            }
            QueuedMidiEvent::Midi2ProgramChange {
                channel,
                program,
                bank_valid,
                bank_msb,
                bank_lsb,
                ..
            } => {
                if *bank_valid {
                    // Send bank select CCs first, then program change
                    vec![
                        0xB0 | (channel & 0x0F),
                        0x00,
                        bank_msb & 0x7F, // Bank MSB
                        0xB0 | (channel & 0x0F),
                        0x20,
                        bank_lsb & 0x7F, // Bank LSB
                        0xC0 | (channel & 0x0F),
                        program & 0x7F, // Program Change
                    ]
                } else {
                    vec![0xC0 | (channel & 0x0F), program & 0x7F]
                }
            }
        }
    }
}

use crate::traits::MidiOutputCapability;

/// A lightweight MIDI event sender that can be shared across threads.
///
/// Supports both immediate and scheduled event sending for lookahead timing.
#[derive(Clone)]
pub struct MidiDeviceSender {
    /// Device ID.
    pub device_id: u32,
    /// Channel for sending scheduled events (with timestamps).
    pub event_tx: Sender<ScheduledMidiEvent>,
    /// MIDI 2.0 capability of this device.
    pub capability: MidiOutputCapability,
}

impl MidiDeviceSender {
    /// Create a new device sender.
    pub fn new(device_id: u32, event_tx: Sender<ScheduledMidiEvent>) -> Self {
        Self {
            device_id,
            event_tx,
            capability: MidiOutputCapability::Midi2ViaTranslation, // Default to translation
        }
    }

    /// Create a new device sender with explicit capability.
    pub fn with_capability(
        device_id: u32,
        event_tx: Sender<ScheduledMidiEvent>,
        capability: MidiOutputCapability,
    ) -> Self {
        Self {
            device_id,
            event_tx,
            capability,
        }
    }

    /// Send an immediate MIDI event (timestamp = now).
    #[inline]
    pub fn try_send(&self, event: QueuedMidiEvent) -> bool {
        self.event_tx.try_send(event.immediate()).is_ok()
    }

    /// Send a scheduled MIDI event with a specific timestamp.
    ///
    /// The event will be held by the output thread until its timestamp arrives.
    #[inline]
    pub fn try_send_scheduled(&self, event: ScheduledMidiEvent) -> bool {
        self.event_tx.try_send(event).is_ok()
    }

    /// Send a MIDI event scheduled for a specific time.
    #[inline]
    pub fn try_send_at(&self, event: QueuedMidiEvent, timestamp: Instant) -> bool {
        self.event_tx.try_send(event.at(timestamp)).is_ok()
    }

    /// Check if this device supports native MIDI 2.0.
    #[inline]
    pub fn is_midi2_native(&self) -> bool {
        self.capability == MidiOutputCapability::Midi2Native
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
///
/// ## MIDI 2.0 Support
///
/// The service tracks each device's MIDI 2.0 capability:
/// - `Midi2Native`: Device speaks native MIDI 2.0 UMP
/// - `Midi2ViaTranslation`: OS translates MIDI 1.0 to UMP
/// - `Midi1Only`: Legacy device, no MIDI 2.0 support
///
/// For Midi2Native devices, high-resolution values are preserved.
/// For others, values are downscaled to MIDI 1.0 7-bit range.
pub struct MidiRealtimeService {
    /// Configuration.
    config: MidiRealtimeConfig,
    /// Registered MIDI devices: device_id -> sender.
    devices: Arc<RwLock<HashMap<u32, MidiDeviceSender>>>,
    /// Device capabilities: device_id -> capability.
    capabilities: Arc<RwLock<HashMap<u32, MidiOutputCapability>>>,
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
            capabilities: Arc::new(RwLock::new(HashMap::new())),
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
            capabilities: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            stats: Arc::new(MidiRealtimeStats::new()),
        }
    }

    /// Register a MIDI output device with default capability (Midi2ViaTranslation).
    pub fn register_device(&self, device_id: u32, event_tx: Sender<ScheduledMidiEvent>) {
        self.register_device_with_capability(
            device_id,
            event_tx,
            MidiOutputCapability::Midi2ViaTranslation,
        );
    }

    /// Register a MIDI output device with explicit capability.
    pub fn register_device_with_capability(
        &self,
        device_id: u32,
        event_tx: Sender<ScheduledMidiEvent>,
        capability: MidiOutputCapability,
    ) {
        let sender = MidiDeviceSender::with_capability(device_id, event_tx, capability);
        self.devices.write().insert(device_id, sender);
        self.capabilities.write().insert(device_id, capability);

        let cap_str = match capability {
            MidiOutputCapability::Midi2Native => "MIDI 2.0 Native (UMP)",
            MidiOutputCapability::Midi2ViaTranslation => "MIDI 2.0 via OS translation",
            MidiOutputCapability::Midi1Only => "MIDI 1.0 only",
        };

        tracing::info!(
            "[MIDI_RT] Registered device {} for realtime processing ({})",
            device_id,
            cap_str
        );
    }

    /// Unregister a MIDI output device.
    pub fn unregister_device(&self, device_id: u32) {
        self.devices.write().remove(&device_id);
        self.capabilities.write().remove(&device_id);
        tracing::info!("[MIDI_RT] Unregistered device {}", device_id);
    }

    /// Get the capability of a registered device.
    pub fn device_capability(&self, device_id: u32) -> Option<MidiOutputCapability> {
        self.capabilities.read().get(&device_id).copied()
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
        let (tx, _rx) = crossbeam_channel::unbounded::<ScheduledMidiEvent>();

        service.register_device(1, tx);

        let devices = service.devices.read();
        assert!(devices.contains_key(&1));
    }

    #[test]
    fn test_device_unregistration() {
        let service = MidiRealtimeService::new();
        let (tx, _rx) = crossbeam_channel::unbounded::<ScheduledMidiEvent>();

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

    // =========================================================================
    // ScheduledMidiEvent tests
    // =========================================================================

    #[test]
    fn test_scheduled_event_creation_at() {
        let future = Instant::now() + Duration::from_millis(100);
        let event = QueuedMidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        };
        let scheduled = event.at(future);

        assert_eq!(scheduled.timestamp, future);
        assert!(matches!(
            scheduled.event,
            QueuedMidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100
            }
        ));
    }

    #[test]
    fn test_scheduled_event_creation_immediate() {
        let before = Instant::now();
        let event = QueuedMidiEvent::NoteOff {
            channel: 1,
            note: 72,
        };
        let scheduled = event.immediate();
        let after = Instant::now();

        // Timestamp should be between before and after
        assert!(scheduled.timestamp >= before);
        assert!(scheduled.timestamp <= after);
    }

    #[test]
    fn test_scheduled_event_static_immediate() {
        let before = Instant::now();
        let event = QueuedMidiEvent::ControlChange {
            channel: 2,
            cc: 74,
            value: 64,
        };
        let scheduled = ScheduledMidiEvent::immediate(event);
        let after = Instant::now();

        assert!(scheduled.timestamp >= before);
        assert!(scheduled.timestamp <= after);
    }

    #[test]
    fn test_scheduled_event_static_new() {
        let timestamp = Instant::now() + Duration::from_secs(1);
        let event = QueuedMidiEvent::Clock;
        let scheduled = ScheduledMidiEvent::new(event, timestamp);

        assert_eq!(scheduled.timestamp, timestamp);
        assert!(matches!(scheduled.event, QueuedMidiEvent::Clock));
    }

    #[test]
    fn test_scheduled_event_is_due_past() {
        // Event scheduled in the past should be due
        let past = Instant::now() - Duration::from_millis(10);
        let scheduled = QueuedMidiEvent::Start.at(past);

        assert!(scheduled.is_due());
    }

    #[test]
    fn test_scheduled_event_is_due_future() {
        // Event scheduled in the future should not be due
        let future = Instant::now() + Duration::from_secs(10);
        let scheduled = QueuedMidiEvent::Stop.at(future);

        assert!(!scheduled.is_due());
    }

    #[test]
    fn test_scheduled_event_time_until_due_past() {
        let past = Instant::now() - Duration::from_millis(10);
        let scheduled = QueuedMidiEvent::Continue.at(past);

        // Event in the past has no time remaining
        assert!(scheduled.time_until_due().is_none());
    }

    #[test]
    fn test_scheduled_event_time_until_due_future() {
        let delay = Duration::from_millis(500);
        let future = Instant::now() + delay;
        let scheduled = QueuedMidiEvent::Clock.at(future);

        let remaining = scheduled.time_until_due();
        assert!(remaining.is_some());
        // Should be close to 500ms (allowing some tolerance for test execution time)
        let remaining = remaining.unwrap();
        assert!(remaining <= delay);
        assert!(remaining >= delay - Duration::from_millis(10));
    }

    #[test]
    fn test_scheduled_event_ordering_min_heap() {
        use std::collections::BinaryHeap;

        let now = Instant::now();
        let t1 = now + Duration::from_millis(100);
        let t2 = now + Duration::from_millis(50);
        let t3 = now + Duration::from_millis(200);

        let e1 = QueuedMidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        }
        .at(t1);
        let e2 = QueuedMidiEvent::NoteOn {
            channel: 0,
            note: 64,
            velocity: 100,
        }
        .at(t2);
        let e3 = QueuedMidiEvent::NoteOn {
            channel: 0,
            note: 67,
            velocity: 100,
        }
        .at(t3);

        let mut heap = BinaryHeap::new();
        heap.push(e1);
        heap.push(e2);
        heap.push(e3);

        // Due to reverse ordering, earliest event should come out first (min-heap behavior)
        let first = heap.pop().unwrap();
        assert_eq!(first.timestamp, t2); // 50ms (earliest)

        let second = heap.pop().unwrap();
        assert_eq!(second.timestamp, t1); // 100ms

        let third = heap.pop().unwrap();
        assert_eq!(third.timestamp, t3); // 200ms (latest)
    }

    #[test]
    fn test_scheduled_event_eq() {
        let t = Instant::now();
        let e1 = QueuedMidiEvent::Clock.at(t);
        let e2 = QueuedMidiEvent::Clock.at(t);

        // Distinct events at the same timestamp are NOT equal: each gets a
        // unique creation sequence, which keeps Eq consistent with the
        // heap's total order (timestamp, then enqueue order).
        assert_ne!(e1, e2);

        // A clone shares the sequence and stays equal.
        assert_eq!(e1, e1.clone());
    }

    // =========================================================================
    // MidiDeviceSender tests
    // =========================================================================

    #[test]
    fn test_midi_device_sender_creation() {
        let (tx, _rx) = crossbeam_channel::unbounded::<ScheduledMidiEvent>();
        let sender = MidiDeviceSender::new(1, tx);

        assert_eq!(sender.device_id, 1);
        assert!(!sender.is_midi2_native()); // Default is Midi2ViaTranslation
    }

    #[test]
    fn test_midi_device_sender_with_capability() {
        let (tx, _rx) = crossbeam_channel::unbounded::<ScheduledMidiEvent>();
        let sender = MidiDeviceSender::with_capability(1, tx, MidiOutputCapability::Midi2Native);

        assert!(sender.is_midi2_native());
    }

    #[test]
    fn test_midi_device_sender_try_send() {
        let (tx, rx) = crossbeam_channel::unbounded::<ScheduledMidiEvent>();
        let sender = MidiDeviceSender::new(1, tx);

        let event = QueuedMidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        };
        assert!(sender.try_send(event));

        // Verify the event was sent and received
        let received = rx.try_recv().unwrap();
        assert!(matches!(
            received.event,
            QueuedMidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100
            }
        ));
    }

    #[test]
    fn test_midi_device_sender_try_send_scheduled() {
        let (tx, rx) = crossbeam_channel::unbounded::<ScheduledMidiEvent>();
        let sender = MidiDeviceSender::new(1, tx);

        let timestamp = Instant::now() + Duration::from_millis(100);
        let scheduled = QueuedMidiEvent::NoteOff {
            channel: 0,
            note: 60,
        }
        .at(timestamp);

        assert!(sender.try_send_scheduled(scheduled));

        let received = rx.try_recv().unwrap();
        assert_eq!(received.timestamp, timestamp);
    }

    #[test]
    fn test_midi_device_sender_try_send_at() {
        let (tx, rx) = crossbeam_channel::unbounded::<ScheduledMidiEvent>();
        let sender = MidiDeviceSender::new(1, tx);

        let timestamp = Instant::now() + Duration::from_millis(200);
        let event = QueuedMidiEvent::ControlChange {
            channel: 1,
            cc: 1,
            value: 64,
        };

        assert!(sender.try_send_at(event, timestamp));

        let received = rx.try_recv().unwrap();
        assert_eq!(received.timestamp, timestamp);
        assert!(matches!(
            received.event,
            QueuedMidiEvent::ControlChange {
                channel: 1,
                cc: 1,
                value: 64
            }
        ));
    }

    #[test]
    fn test_midi_device_sender_disconnected_channel() {
        let (tx, rx) = crossbeam_channel::unbounded::<ScheduledMidiEvent>();
        let sender = MidiDeviceSender::new(1, tx);

        // Drop the receiver to simulate disconnection
        drop(rx);

        // Sending should fail gracefully
        let event = QueuedMidiEvent::Clock;
        assert!(!sender.try_send(event));
    }
}
