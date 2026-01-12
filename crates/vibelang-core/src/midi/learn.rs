//! MIDI Learn functionality.
//!
//! MIDI Learn allows users to map MIDI controllers to parameters by:
//! 1. Activating learn mode for a target parameter
//! 2. Moving a MIDI controller
//! 3. The system automatically creates the mapping

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::mpsc;

use super::devices::MidiInputId;
use super::events::{Channel, MidiMessage, TimestampedMidiEvent};
use crate::types::{EffectId, GroupId, VoiceId};

// ============================================================================
// Learn Target
// ============================================================================

/// Target for a MIDI Learn mapping.
#[derive(Clone, Debug, PartialEq)]
pub enum LearnTarget {
    /// Map to a voice parameter.
    Voice { voice_id: VoiceId, param: String },
    /// Map to an effect parameter.
    Effect { effect_id: EffectId, param: String },
    /// Map to a group parameter (e.g., amp, pan).
    Group { group_id: GroupId, param: String },
    /// Map to a global parameter.
    Global { param: String },
}

impl LearnTarget {
    /// Create a voice target.
    pub fn voice(voice_id: VoiceId, param: impl Into<String>) -> Self {
        Self::Voice {
            voice_id,
            param: param.into(),
        }
    }

    /// Create an effect target.
    pub fn effect(effect_id: EffectId, param: impl Into<String>) -> Self {
        Self::Effect {
            effect_id,
            param: param.into(),
        }
    }

    /// Create a group target.
    pub fn group(group_id: GroupId, param: impl Into<String>) -> Self {
        Self::Group {
            group_id,
            param: param.into(),
        }
    }

    /// Create a global target.
    pub fn global(param: impl Into<String>) -> Self {
        Self::Global {
            param: param.into(),
        }
    }
}

// ============================================================================
// Learned Mapping
// ============================================================================

/// A learned MIDI-to-parameter mapping.
#[derive(Clone, Debug)]
pub struct LearnedMapping {
    /// The MIDI input device.
    pub device_id: MidiInputId,
    /// MIDI channel (0-15).
    pub channel: Channel,
    /// CC number (for CC mappings).
    pub controller: u8,
    /// The target parameter.
    pub target: LearnTarget,
    /// Minimum parameter value (when CC = 0).
    pub min_value: f32,
    /// Maximum parameter value (when CC = 127).
    pub max_value: f32,
}

impl LearnedMapping {
    /// Create a new mapping with default range (0.0 to 1.0).
    pub fn new(
        device_id: MidiInputId,
        channel: Channel,
        controller: u8,
        target: LearnTarget,
    ) -> Self {
        Self {
            device_id,
            channel,
            controller,
            target,
            min_value: 0.0,
            max_value: 1.0,
        }
    }

    /// Set the value range.
    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        self.min_value = min;
        self.max_value = max;
        self
    }

    /// Calculate the parameter value for a CC value.
    pub fn calculate_value(&self, cc_value: u8) -> f32 {
        let normalized = cc_value as f32 / 127.0;
        self.min_value + normalized * (self.max_value - self.min_value)
    }
}

// ============================================================================
// MIDI Learn State
// ============================================================================

/// State for the MIDI Learn system.
pub struct MidiLearn {
    /// Whether learn mode is active.
    active: AtomicBool,
    /// Current learn target.
    target: RwLock<Option<LearnTarget>>,
    /// Unique ID for the current learn session.
    session_id: AtomicU64,
    /// Channel for sending learned mappings.
    learned_tx: mpsc::Sender<LearnedMapping>,
    /// Receiver for learned mappings (to be consumed by the application).
    learned_rx: RwLock<Option<mpsc::Receiver<LearnedMapping>>>,
}

impl MidiLearn {
    /// Create a new MIDI Learn system.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(16);
        Self {
            active: AtomicBool::new(false),
            target: RwLock::new(None),
            session_id: AtomicU64::new(0),
            learned_tx: tx,
            learned_rx: RwLock::new(Some(rx)),
        }
    }

    /// Start learning for a target.
    ///
    /// Returns a session ID that can be used to verify the mapping.
    pub fn start(&self, target: LearnTarget) -> u64 {
        let session = self.session_id.fetch_add(1, Ordering::SeqCst) + 1;
        *self.target.write() = Some(target.clone());
        self.active.store(true, Ordering::SeqCst);
        tracing::info!("[MIDI_LEARN] Started session {} for {:?}", session, target);
        session
    }

    /// Cancel the current learn session.
    pub fn cancel(&self) {
        if self.active.swap(false, Ordering::SeqCst) {
            *self.target.write() = None;
            tracing::info!("[MIDI_LEARN] Cancelled");
        }
    }

    /// Check if learn mode is active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Get the current learn target.
    pub fn current_target(&self) -> Option<LearnTarget> {
        self.target.read().clone()
    }

    /// Get the current session ID.
    pub fn session_id(&self) -> u64 {
        self.session_id.load(Ordering::SeqCst)
    }

    /// Take the receiver for learned mappings.
    ///
    /// This can only be called once. Returns `None` if already taken.
    pub fn take_receiver(&self) -> Option<mpsc::Receiver<LearnedMapping>> {
        self.learned_rx.write().take()
    }

    /// Process a MIDI event for learning.
    ///
    /// If learn mode is active and this is a CC message, creates a mapping
    /// and sends it to the learned channel.
    ///
    /// Returns `true` if the event was consumed for learning.
    pub fn process_event(&self, event: &TimestampedMidiEvent) -> bool {
        if !self.is_active() {
            return false;
        }

        // Only learn from CC messages
        let (channel, controller) = match &event.message {
            MidiMessage::ControlChange {
                channel,
                controller,
                ..
            } => (*channel, *controller),
            _ => return false,
        };

        // Get the target and create mapping
        let target = match self.target.read().clone() {
            Some(t) => t,
            None => return false,
        };

        let mapping = LearnedMapping::new(
            MidiInputId::new(event.device_id.0),
            channel,
            controller,
            target.clone(),
        );

        // Send the mapping
        if let Err(e) = self.learned_tx.try_send(mapping.clone()) {
            tracing::warn!("[MIDI_LEARN] Failed to send mapping: {}", e);
            return false;
        }

        tracing::info!(
            "[MIDI_LEARN] Learned: device={}, ch={}, cc={} -> {:?}",
            event.device_id.0,
            channel.display(),
            controller,
            target
        );

        // Deactivate learn mode
        self.active.store(false, Ordering::SeqCst);
        *self.target.write() = None;

        true
    }

    /// Wait for a mapping to be learned (async).
    ///
    /// Returns `None` if cancelled or timed out.
    pub async fn wait_for_mapping(&self, timeout: std::time::Duration) -> Option<LearnedMapping> {
        // This is a simple implementation that polls.
        // In production, you'd use the receiver directly.
        let start = std::time::Instant::now();

        while start.elapsed() < timeout && self.is_active() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        // Return None since we consumed via process_event
        None
    }
}

impl Default for MidiLearn {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Learn Manager
// ============================================================================

/// Manager for storing and applying learned mappings.
#[derive(Default)]
pub struct LearnManager {
    /// Stored mappings.
    mappings: RwLock<Vec<LearnedMapping>>,
    /// Learn state.
    learn: Arc<MidiLearn>,
}

impl LearnManager {
    /// Create a new learn manager.
    pub fn new() -> Self {
        Self {
            mappings: RwLock::new(Vec::new()),
            learn: Arc::new(MidiLearn::new()),
        }
    }

    /// Get the learn state.
    pub fn learn(&self) -> Arc<MidiLearn> {
        Arc::clone(&self.learn)
    }

    /// Add a mapping.
    pub fn add_mapping(&self, mapping: LearnedMapping) {
        self.mappings.write().push(mapping);
    }

    /// Remove mappings for a target.
    pub fn remove_mappings(&self, target: &LearnTarget) {
        self.mappings.write().retain(|m| m.target != *target);
    }

    /// Get all mappings.
    pub fn mappings(&self) -> Vec<LearnedMapping> {
        self.mappings.read().clone()
    }

    /// Clear all mappings.
    pub fn clear(&self) {
        self.mappings.write().clear();
    }

    /// Find mappings that match a MIDI event.
    pub fn find_mappings(
        &self,
        device_id: MidiInputId,
        channel: Channel,
        controller: u8,
    ) -> Vec<LearnedMapping> {
        self.mappings
            .read()
            .iter()
            .filter(|m| {
                m.device_id == device_id && m.channel == channel && m.controller == controller
            })
            .cloned()
            .collect()
    }

    /// Process a MIDI event, returning parameter updates.
    pub fn process_event(&self, event: &TimestampedMidiEvent) -> Vec<(LearnTarget, f32)> {
        // First check if we're learning
        if self.learn.process_event(event) {
            return Vec::new();
        }

        // Then check for existing mappings
        let (channel, controller, value) = match &event.message {
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => (*channel, *controller, *value),
            _ => return Vec::new(),
        };

        let device_id = MidiInputId::new(event.device_id.0);
        let mappings = self.find_mappings(device_id, channel, controller);

        mappings
            .into_iter()
            .map(|m| {
                let param_value = m.calculate_value(value);
                (m.target, param_value)
            })
            .collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ids::MidiDeviceId;
    use std::time::Instant;

    fn make_cc_event(device: u32, channel: u8, cc: u8, value: u8) -> TimestampedMidiEvent {
        TimestampedMidiEvent {
            timestamp_us: 0,
            received_at: Instant::now(),
            device_id: MidiDeviceId::new(device),
            message: MidiMessage::ControlChange {
                channel: Channel::from(channel),
                controller: cc,
                value,
            },
        }
    }

    #[test]
    fn test_learn_target() {
        let target = LearnTarget::voice(VoiceId::new(1), "cutoff");
        match target {
            LearnTarget::Voice { voice_id, param } => {
                assert_eq!(voice_id.0, 1);
                assert_eq!(param, "cutoff");
            }
            _ => panic!("Expected Voice target"),
        }
    }

    #[test]
    fn test_learned_mapping_value() {
        let mapping = LearnedMapping::new(
            MidiInputId::new(0),
            Channel::new(0),
            74,
            LearnTarget::global("filter"),
        )
        .with_range(100.0, 10000.0);

        assert_eq!(mapping.calculate_value(0), 100.0);
        assert!((mapping.calculate_value(127) - 10000.0).abs() < 0.01);
        assert!((mapping.calculate_value(64) - 5050.0).abs() < 100.0);
    }

    #[test]
    fn test_midi_learn_session() {
        let learn = MidiLearn::new();

        assert!(!learn.is_active());

        let session = learn.start(LearnTarget::global("volume"));
        assert!(learn.is_active());
        assert!(learn.current_target().is_some());
        assert_eq!(learn.session_id(), session);

        learn.cancel();
        assert!(!learn.is_active());
        assert!(learn.current_target().is_none());
    }

    #[test]
    fn test_midi_learn_process() {
        let learn = MidiLearn::new();
        let _rx = learn.take_receiver(); // Take to avoid blocking

        learn.start(LearnTarget::global("test"));

        let event = make_cc_event(0, 0, 74, 64);
        let consumed = learn.process_event(&event);

        assert!(consumed);
        assert!(!learn.is_active()); // Should deactivate after learning
    }

    #[test]
    fn test_learn_manager() {
        let manager = LearnManager::new();

        let mapping = LearnedMapping::new(
            MidiInputId::new(0),
            Channel::new(0),
            74,
            LearnTarget::global("filter"),
        );

        manager.add_mapping(mapping);
        assert_eq!(manager.mappings().len(), 1);

        let found = manager.find_mappings(MidiInputId::new(0), Channel::new(0), 74);
        assert_eq!(found.len(), 1);

        manager.remove_mappings(&LearnTarget::global("filter"));
        assert_eq!(manager.mappings().len(), 0);
    }

    #[test]
    fn test_learn_manager_process() {
        let manager = LearnManager::new();

        manager.add_mapping(
            LearnedMapping::new(
                MidiInputId::new(0),
                Channel::new(0),
                74,
                LearnTarget::global("filter"),
            )
            .with_range(0.0, 1.0),
        );

        let event = make_cc_event(0, 0, 74, 127);
        let updates = manager.process_event(&event);

        assert_eq!(updates.len(), 1);
        let (target, value) = &updates[0];
        assert!(matches!(target, LearnTarget::Global { .. }));
        assert!((value - 1.0).abs() < 0.01);
    }
}
