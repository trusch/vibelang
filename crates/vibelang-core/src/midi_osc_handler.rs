//! OSC handler for MIDI triggers from SuperCollider.
//!
//! This module handles `/tr` (SendTrig) messages from scsynth and converts them
//! into actual MIDI output. This allows MIDI events to be precisely synchronized
//! with audio events, as both are scheduled through SuperCollider's timing system.
//!
//! ## Architecture
//!
//! 1. MIDI events are scheduled as synth creations in OSC bundles (same as audio)
//! 2. The MIDI trigger synthdefs use SendTrig to fire OSC messages at sample-accurate times
//! 3. This handler receives those `/tr` messages and immediately sends MIDI bytes
//! 4. The timing is determined by scsynth, ensuring perfect sync with audio
//!
//! ## Trigger ID Scheme
//!
//! Different MIDI message types use different trigger ID ranges:
//! - 100-103: Note On (device_id, channel, note, velocity)
//! - 110-112: Note Off (device_id, channel, note)
//! - 120-123: CC (device_id, channel, cc_num, value)
//! - 130-132: Pitch Bend (device_id, channel, value)
//! - 140: Clock (device_id)
//! - 150: Start (device_id)
//! - 151: Stop (device_id)
//! - 152: Continue (device_id)

use crate::midi::{MidiOutputHandle, QueuedMidiEvent};
use crate::midi_synthdefs::{trigger_ids, MidiTriggerType};
use rosc::OscType;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Accumulator for multi-trigger MIDI messages.
///
/// Since we use multiple SendTrig calls per MIDI message (one per parameter),
/// we need to accumulate the values before sending the complete MIDI message.
///
/// The key insight is that all triggers for a single message come from the same
/// synth (same node ID), so we use node ID as the accumulation key.
#[derive(Debug, Default)]
struct MessageAccumulator {
    /// Pending note-on messages: node_id -> (device_id, channel, note, velocity)
    note_on: HashMap<i32, (Option<u32>, Option<u8>, Option<u8>, Option<u8>)>,
    /// Pending note-off messages: node_id -> (device_id, channel, note)
    note_off: HashMap<i32, (Option<u32>, Option<u8>, Option<u8>)>,
    /// Pending CC messages: node_id -> (device_id, channel, cc_num, value)
    cc: HashMap<i32, (Option<u32>, Option<u8>, Option<u8>, Option<u8>)>,
    /// Pending pitch bend messages: node_id -> (device_id, channel, value)
    pitch_bend: HashMap<i32, (Option<u32>, Option<u8>, Option<i32>)>,
}

impl MessageAccumulator {
    fn new() -> Self {
        Self::default()
    }

    /// Process a note-on trigger and return Some if the message is complete.
    fn accumulate_note_on(
        &mut self,
        node_id: i32,
        trigger_id: i32,
        value: f32,
    ) -> Option<(u32, u8, u8, u8)> {
        let entry = self.note_on.entry(node_id).or_default();

        match trigger_id {
            trigger_ids::NOTE_ON_DEVICE_ID => {
                log::debug!("[MIDI_OSC] Note ON accumulator: node={} got device_id={}", node_id, value);
                entry.0 = Some(value as u32);
            }
            trigger_ids::NOTE_ON_CHANNEL => {
                log::debug!("[MIDI_OSC] Note ON accumulator: node={} got channel={}", node_id, value);
                entry.1 = Some(value as u8);
            }
            trigger_ids::NOTE_ON_NOTE => {
                log::debug!("[MIDI_OSC] Note ON accumulator: node={} got note={}", node_id, value);
                entry.2 = Some(value as u8);
            }
            trigger_ids::NOTE_ON_VELOCITY => {
                log::debug!("[MIDI_OSC] Note ON accumulator: node={} got velocity={}", node_id, value);
                entry.3 = Some(value as u8);
            }
            _ => return None,
        }

        // Check if all values are present
        log::debug!(
            "[MIDI_OSC] Note ON accumulator state for node {}: device={:?} ch={:?} note={:?} vel={:?}",
            node_id, entry.0, entry.1, entry.2, entry.3
        );
        if let (Some(device), Some(ch), Some(note), Some(vel)) = *entry {
            log::debug!("[MIDI_OSC] Note ON complete for node {}: device={} ch={} note={} vel={}",
                node_id, device, ch, note, vel);
            self.note_on.remove(&node_id);
            Some((device, ch, note, vel))
        } else {
            None
        }
    }

    /// Process a note-off trigger and return Some if the message is complete.
    fn accumulate_note_off(
        &mut self,
        node_id: i32,
        trigger_id: i32,
        value: f32,
    ) -> Option<(u32, u8, u8)> {
        let entry = self.note_off.entry(node_id).or_default();

        match trigger_id {
            trigger_ids::NOTE_OFF_DEVICE_ID => entry.0 = Some(value as u32),
            trigger_ids::NOTE_OFF_CHANNEL => entry.1 = Some(value as u8),
            trigger_ids::NOTE_OFF_NOTE => entry.2 = Some(value as u8),
            _ => return None,
        }

        if let (Some(device), Some(ch), Some(note)) = *entry {
            self.note_off.remove(&node_id);
            Some((device, ch, note))
        } else {
            None
        }
    }

    /// Process a CC trigger and return Some if the message is complete.
    fn accumulate_cc(
        &mut self,
        node_id: i32,
        trigger_id: i32,
        value: f32,
    ) -> Option<(u32, u8, u8, u8)> {
        let entry = self.cc.entry(node_id).or_default();

        match trigger_id {
            trigger_ids::CC_DEVICE_ID => entry.0 = Some(value as u32),
            trigger_ids::CC_CHANNEL => entry.1 = Some(value as u8),
            trigger_ids::CC_NUM => entry.2 = Some(value as u8),
            trigger_ids::CC_VALUE => entry.3 = Some(value as u8),
            _ => return None,
        }

        if let (Some(device), Some(ch), Some(cc_num), Some(val)) = *entry {
            self.cc.remove(&node_id);
            Some((device, ch, cc_num, val))
        } else {
            None
        }
    }

    /// Process a pitch bend trigger and return Some if the message is complete.
    fn accumulate_pitch_bend(
        &mut self,
        node_id: i32,
        trigger_id: i32,
        value: f32,
    ) -> Option<(u32, u8, i32)> {
        let entry = self.pitch_bend.entry(node_id).or_default();

        match trigger_id {
            trigger_ids::PITCH_BEND_DEVICE_ID => entry.0 = Some(value as u32),
            trigger_ids::PITCH_BEND_CHANNEL => entry.1 = Some(value as u8),
            trigger_ids::PITCH_BEND_VALUE => entry.2 = Some(value as i32),
            _ => return None,
        }

        if let (Some(device), Some(ch), Some(val)) = *entry {
            self.pitch_bend.remove(&node_id);
            Some((device, ch, val))
        } else {
            None
        }
    }

    /// Clean up stale entries (for nodes that were freed before completing).
    fn cleanup_stale(&mut self) {
        // In practice, all triggers from a synth fire in the same control block,
        // so we shouldn't have many stale entries. But we can periodically clean
        // up entries that have been pending for too long if needed.
        // For now, rely on synth lifecycle to clean up.
    }
}

/// Handler for OSC replies from scsynth that trigger MIDI output.
///
/// This handler receives `/tr` messages from SuperCollider's SendTrig UGen
/// and converts them into MIDI messages sent to the appropriate device.
pub struct MidiOscHandler {
    /// MIDI output devices: device_id -> handle
    devices: Arc<RwLock<HashMap<u32, MidiOutputHandle>>>,
    /// Message accumulator for multi-trigger messages
    accumulator: MessageAccumulator,
    /// Statistics for debugging
    stats: HandlerStats,
}

#[derive(Debug, Default)]
pub struct HandlerStats {
    /// Total /tr messages received
    triggers_received: u64,
    /// Note-on messages sent
    notes_on_sent: u64,
    /// Note-off messages sent
    notes_off_sent: u64,
    /// CC messages sent
    cc_sent: u64,
    /// Clock ticks sent
    clock_sent: u64,
    /// Transport messages sent (start/stop/continue)
    transport_sent: u64,
    /// Errors (device not found, send failures)
    errors: u64,
}

impl MidiOscHandler {
    /// Create a new MIDI OSC handler.
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            accumulator: MessageAccumulator::new(),
            stats: HandlerStats::default(),
        }
    }

    /// Register a MIDI output device.
    pub fn register_device(&self, device_id: u32, handle: MidiOutputHandle) {
        self.devices.write().unwrap().insert(device_id, handle);
        log::debug!(
            "[MIDI_OSC] Registered device {} for OSC->MIDI handling",
            device_id
        );
    }

    /// Unregister a MIDI output device.
    pub fn unregister_device(&self, device_id: u32) {
        self.devices.write().unwrap().remove(&device_id);
    }

    /// Get a clone of the devices map for sharing.
    pub fn devices(&self) -> Arc<RwLock<HashMap<u32, MidiOutputHandle>>> {
        Arc::clone(&self.devices)
    }

    /// Handle an incoming OSC message.
    ///
    /// Returns true if this was a MIDI-related message that was processed.
    pub fn handle_osc(&mut self, addr: &str, args: &[OscType]) -> bool {
        if addr != "/tr" {
            return false;
        }

        self.stats.triggers_received += 1;

        // /tr format: [node_id: i32, trigger_id: i32, value: f32]
        if args.len() < 3 {
            log::warn!("[MIDI_OSC] Invalid /tr message: expected 3 args, got {}", args.len());
            return false;
        }

        let node_id = match &args[0] {
            OscType::Int(n) => *n,
            _ => {
                log::warn!("[MIDI_OSC] Invalid /tr node_id type");
                return false;
            }
        };

        let trigger_id = match &args[1] {
            OscType::Int(n) => *n,
            OscType::Float(f) => *f as i32,
            _ => {
                log::warn!("[MIDI_OSC] Invalid /tr trigger_id type");
                return false;
            }
        };

        let value = match &args[2] {
            OscType::Float(f) => *f,
            OscType::Int(n) => *n as f32,
            _ => {
                log::warn!("[MIDI_OSC] Invalid /tr value type");
                return false;
            }
        };

        // Debug log all received triggers
        log::trace!(
            "[MIDI_OSC] /tr received: node={} trigger_id={} value={}",
            node_id, trigger_id, value
        );

        // Determine message type and process
        match trigger_ids::message_type(trigger_id) {
            Some(MidiTriggerType::NoteOnPacked) => {
                // Decode packed value: (device << 21) | (channel << 14) | (note << 7) | velocity
                let packed = value as u32;
                let device_id = (packed >> 21) & 0x7FF; // 11 bits for device
                let channel = ((packed >> 14) & 0x7F) as u8; // 7 bits for channel
                let note = ((packed >> 7) & 0x7F) as u8; // 7 bits for note
                let velocity = (packed & 0x7F) as u8; // 7 bits for velocity
                log::debug!(
                    "[MIDI_OSC] Note ON packed: node={} packed={} -> dev={} ch={} note={} vel={}",
                    node_id, packed, device_id, channel, note, velocity
                );
                self.send_note_on(device_id, channel, note, velocity);
                self.stats.triggers_received += 1;
                true
            }
            Some(MidiTriggerType::NoteOffPacked) => {
                // Decode packed value: (device << 14) | (channel << 7) | note
                let packed = value as u32;
                let device_id = (packed >> 14) & 0x3FFF; // 14 bits for device
                let channel = ((packed >> 7) & 0x7F) as u8; // 7 bits for channel
                let note = (packed & 0x7F) as u8; // 7 bits for note
                log::debug!(
                    "[MIDI_OSC] Note OFF packed: node={} packed={} -> dev={} ch={} note={}",
                    node_id, packed, device_id, channel, note
                );
                self.send_note_off(device_id, channel, note);
                self.stats.triggers_received += 1;
                true
            }
            Some(MidiTriggerType::NoteOn) => {
                // Legacy multi-trigger (kept for backwards compatibility)
                log::debug!(
                    "[MIDI_OSC] Note ON trigger (legacy): node={} id={} value={}",
                    node_id, trigger_id, value
                );
                if let Some((device_id, channel, note, velocity)) =
                    self.accumulator.accumulate_note_on(node_id, trigger_id, value)
                {
                    self.send_note_on(device_id, channel, note, velocity);
                }
                true
            }
            Some(MidiTriggerType::NoteOff) => {
                // Legacy multi-trigger (kept for backwards compatibility)
                if let Some((device_id, channel, note)) =
                    self.accumulator.accumulate_note_off(node_id, trigger_id, value)
                {
                    self.send_note_off(device_id, channel, note);
                }
                true
            }
            Some(MidiTriggerType::CC) => {
                if let Some((device_id, channel, cc_num, cc_value)) =
                    self.accumulator.accumulate_cc(node_id, trigger_id, value)
                {
                    self.send_cc(device_id, channel, cc_num, cc_value);
                }
                true
            }
            Some(MidiTriggerType::PitchBend) => {
                if let Some((device_id, channel, bend_value)) =
                    self.accumulator.accumulate_pitch_bend(node_id, trigger_id, value)
                {
                    self.send_pitch_bend(device_id, channel, bend_value);
                }
                true
            }
            Some(MidiTriggerType::Clock) => {
                // Clock is a single-trigger message
                self.send_clock(value as u32);
                true
            }
            Some(MidiTriggerType::Start) => {
                self.send_start(value as u32);
                true
            }
            Some(MidiTriggerType::Stop) => {
                self.send_stop(value as u32);
                true
            }
            Some(MidiTriggerType::Continue) => {
                self.send_continue(value as u32);
                true
            }
            None => {
                // Not a MIDI trigger
                false
            }
        }
    }

    /// Send a note-on message to the specified device.
    fn send_note_on(&mut self, device_id: u32, channel: u8, note: u8, velocity: u8) {
        let devices = self.devices.read().unwrap();
        if let Some(handle) = devices.get(&device_id) {
            log::info!(
                "[MIDI_OSC] note_on: dev={} ch={} note={} vel={} (sample-accurate via SC)",
                device_id, channel + 1, note, velocity
            );
            if let Err(e) = handle.send(QueuedMidiEvent::note_on(channel, note, velocity)) {
                log::error!("[MIDI_OSC] Failed to send note-on: {}", e);
                self.stats.errors += 1;
            } else {
                self.stats.notes_on_sent += 1;
            }
        } else {
            log::warn!("[MIDI_OSC] Device {} not found for note-on", device_id);
            self.stats.errors += 1;
        }
    }

    /// Send a note-off message to the specified device.
    fn send_note_off(&mut self, device_id: u32, channel: u8, note: u8) {
        let devices = self.devices.read().unwrap();
        if let Some(handle) = devices.get(&device_id) {
            log::info!(
                "[MIDI_OSC] note_off: dev={} ch={} note={} (sample-accurate via SC)",
                device_id, channel + 1, note
            );
            if let Err(e) = handle.send(QueuedMidiEvent::note_off(channel, note)) {
                log::error!("[MIDI_OSC] Failed to send note-off: {}", e);
                self.stats.errors += 1;
            } else {
                self.stats.notes_off_sent += 1;
            }
        } else {
            log::warn!("[MIDI_OSC] Device {} not found for note-off", device_id);
            self.stats.errors += 1;
        }
    }

    /// Send a CC message to the specified device.
    fn send_cc(&mut self, device_id: u32, channel: u8, cc_num: u8, value: u8) {
        let devices = self.devices.read().unwrap();
        if let Some(handle) = devices.get(&device_id) {
            log::debug!(
                "[MIDI_OSC] CC: dev={} ch={} cc={} val={}",
                device_id, channel + 1, cc_num, value
            );
            if let Err(e) = handle.send(QueuedMidiEvent::control_change(channel, cc_num, value)) {
                log::error!("[MIDI_OSC] Failed to send CC: {}", e);
                self.stats.errors += 1;
            } else {
                self.stats.cc_sent += 1;
            }
        } else {
            log::warn!("[MIDI_OSC] Device {} not found for CC", device_id);
            self.stats.errors += 1;
        }
    }

    /// Send a pitch bend message to the specified device.
    fn send_pitch_bend(&mut self, device_id: u32, channel: u8, value: i32) {
        let devices = self.devices.read().unwrap();
        if let Some(handle) = devices.get(&device_id) {
            log::debug!(
                "[MIDI_OSC] Pitch Bend: dev={} ch={} val={}",
                device_id, channel + 1, value
            );
            // Convert from 0-16383 to -8192..+8191
            let centered = (value - 8192) as i16;
            if let Err(e) = handle.send(QueuedMidiEvent::pitch_bend(channel, centered)) {
                log::error!("[MIDI_OSC] Failed to send pitch bend: {}", e);
                self.stats.errors += 1;
            }
        } else {
            log::warn!("[MIDI_OSC] Device {} not found for pitch bend", device_id);
            self.stats.errors += 1;
        }
    }

    /// Send a clock tick to the specified device.
    fn send_clock(&mut self, device_id: u32) {
        let devices = self.devices.read().unwrap();
        if let Some(handle) = devices.get(&device_id) {
            if let Err(e) = handle.send(QueuedMidiEvent::clock()) {
                log::error!("[MIDI_OSC] Failed to send clock: {}", e);
                self.stats.errors += 1;
            } else {
                self.stats.clock_sent += 1;
            }
        } else {
            log::warn!("[MIDI_OSC] Device {} not found for clock", device_id);
            self.stats.errors += 1;
        }
    }

    /// Send a start message to the specified device.
    fn send_start(&mut self, device_id: u32) {
        let devices = self.devices.read().unwrap();
        if let Some(handle) = devices.get(&device_id) {
            log::debug!("[MIDI_OSC] Start: dev={}", device_id);
            if let Err(e) = handle.send(QueuedMidiEvent::start()) {
                log::error!("[MIDI_OSC] Failed to send start: {}", e);
                self.stats.errors += 1;
            } else {
                self.stats.transport_sent += 1;
            }
        } else {
            log::warn!("[MIDI_OSC] Device {} not found for start", device_id);
            self.stats.errors += 1;
        }
    }

    /// Send a stop message to the specified device.
    fn send_stop(&mut self, device_id: u32) {
        let devices = self.devices.read().unwrap();
        if let Some(handle) = devices.get(&device_id) {
            log::debug!("[MIDI_OSC] Stop: dev={}", device_id);
            if let Err(e) = handle.send(QueuedMidiEvent::stop()) {
                log::error!("[MIDI_OSC] Failed to send stop: {}", e);
                self.stats.errors += 1;
            } else {
                self.stats.transport_sent += 1;
            }
        } else {
            log::warn!("[MIDI_OSC] Device {} not found for stop", device_id);
            self.stats.errors += 1;
        }
    }

    /// Send a continue message to the specified device.
    fn send_continue(&mut self, device_id: u32) {
        let devices = self.devices.read().unwrap();
        if let Some(handle) = devices.get(&device_id) {
            log::debug!("[MIDI_OSC] Continue: dev={}", device_id);
            if let Err(e) = handle.send(QueuedMidiEvent::continue_msg()) {
                log::error!("[MIDI_OSC] Failed to send continue: {}", e);
                self.stats.errors += 1;
            } else {
                self.stats.transport_sent += 1;
            }
        } else {
            log::warn!("[MIDI_OSC] Device {} not found for continue", device_id);
            self.stats.errors += 1;
        }
    }

    /// Get handler statistics for debugging.
    pub fn stats(&self) -> &HandlerStats {
        &self.stats
    }

    /// Log current statistics.
    pub fn log_stats(&self) {
        log::info!(
            "[MIDI_OSC] Stats: triggers={} notes_on={} notes_off={} cc={} clock={} transport={} errors={}",
            self.stats.triggers_received,
            self.stats.notes_on_sent,
            self.stats.notes_off_sent,
            self.stats.cc_sent,
            self.stats.clock_sent,
            self.stats.transport_sent,
            self.stats.errors
        );
    }

    /// Periodically clean up stale message accumulators.
    pub fn cleanup(&mut self) {
        self.accumulator.cleanup_stale();
    }
}

impl Default for MidiOscHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_accumulator_note_on() {
        let mut acc = MessageAccumulator::new();
        let node_id = 1000;

        // Simulate triggers arriving (order doesn't matter)
        assert!(acc.accumulate_note_on(node_id, 103, 100.0).is_none()); // velocity
        assert!(acc.accumulate_note_on(node_id, 100, 1.0).is_none()); // device_id
        assert!(acc.accumulate_note_on(node_id, 102, 60.0).is_none()); // note

        // Last trigger completes the message
        let result = acc.accumulate_note_on(node_id, 101, 0.0); // channel
        assert!(result.is_some());

        let (device, ch, note, vel) = result.unwrap();
        assert_eq!(device, 1);
        assert_eq!(ch, 0);
        assert_eq!(note, 60);
        assert_eq!(vel, 100);

        // Entry should be removed
        assert!(acc.note_on.is_empty());
    }

    #[test]
    fn test_message_accumulator_note_off() {
        let mut acc = MessageAccumulator::new();
        let node_id = 2000;

        assert!(acc.accumulate_note_off(node_id, 110, 1.0).is_none()); // device_id
        assert!(acc.accumulate_note_off(node_id, 111, 2.0).is_none()); // channel

        let result = acc.accumulate_note_off(node_id, 112, 64.0); // note
        assert!(result.is_some());

        let (device, ch, note) = result.unwrap();
        assert_eq!(device, 1);
        assert_eq!(ch, 2);
        assert_eq!(note, 64);
    }

    #[test]
    fn test_separate_nodes() {
        let mut acc = MessageAccumulator::new();

        // Two different synths sending note-ons
        acc.accumulate_note_on(1000, 100, 1.0); // device_id for node 1000
        acc.accumulate_note_on(2000, 100, 2.0); // device_id for node 2000

        // They should be tracked separately
        assert_eq!(acc.note_on.len(), 2);
    }

    #[test]
    fn test_handler_creation() {
        let handler = MidiOscHandler::new();
        assert_eq!(handler.stats.triggers_received, 0);
    }
}
