//! MIDI callback management.
//!
//! This module handles registration and invocation of callbacks
//! for incoming MIDI events.

use super::types::{MidiEventNotification, MidiMessage};
use crate::compat::RwLock;
use crate::midi::{CallbackData, CallbackType, MidiCallbacks};
use crate::types::ids::MidiDeviceId;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Manager for MIDI callbacks.
pub struct MidiCallbackManager {
    /// Callback storage.
    callbacks: Arc<RwLock<MidiCallbacks>>,

    /// Channel for sending callback notifications to the Rhai layer.
    callback_tx: mpsc::Sender<MidiEventNotification>,

    /// Receiver for callback notifications (to be polled by the Rhai layer).
    callback_rx: Arc<std::sync::Mutex<mpsc::Receiver<MidiEventNotification>>>,
}

impl MidiCallbackManager {
    /// Create a new callback manager.
    pub fn new() -> Self {
        let (callback_tx, callback_rx) = mpsc::channel(1024);
        Self {
            callbacks: Arc::new(RwLock::new(MidiCallbacks::new())),
            callback_tx,
            callback_rx: Arc::new(std::sync::Mutex::new(callback_rx)),
        }
    }

    /// Register a callback for MIDI events.
    ///
    /// Returns a unique callback ID that can be used to unregister the callback.
    pub async fn register_callback(
        &self,
        device_id: MidiDeviceId,
        callback_type: CallbackType,
        channel: Option<u8>,
        callback_data: CallbackData,
    ) -> u64 {
        let mut callbacks = self.callbacks.write().await;
        let id = callbacks.register(device_id, callback_type, channel, callback_data);
        tracing::info!(
            "Registered MIDI callback {} for device {} ({:?})",
            id,
            device_id.0,
            callback_type
        );
        id
    }

    /// Unregister a callback by ID.
    ///
    /// Returns true if the callback was found and removed.
    pub async fn unregister_callback(&self, id: u64) -> bool {
        let mut callbacks = self.callbacks.write().await;
        let removed = callbacks.unregister(id);
        if removed {
            tracing::info!("Unregistered MIDI callback {}", id);
        }
        removed
    }

    /// Clear all callbacks.
    pub async fn clear_callbacks(&self) {
        let mut callbacks = self.callbacks.write().await;
        callbacks.clear();
        tracing::info!("Cleared all MIDI callbacks");
    }

    /// Clear all callbacks for a specific device.
    pub async fn clear_device_callbacks(&self, device_id: MidiDeviceId) {
        let mut callbacks = self.callbacks.write().await;
        callbacks.clear_device(device_id);
        tracing::info!("Cleared MIDI callbacks for device {}", device_id.0);
    }

    /// Get a receiver for callback notifications.
    ///
    /// This can be used by the Rhai layer to poll for incoming MIDI events.
    pub fn callback_receiver(
        &self,
    ) -> Arc<std::sync::Mutex<mpsc::Receiver<MidiEventNotification>>> {
        Arc::clone(&self.callback_rx)
    }

    /// Try to receive callback notifications without blocking.
    ///
    /// Returns all pending notifications.
    pub fn poll_callbacks(&self) -> Vec<MidiEventNotification> {
        let Ok(mut rx) = self.callback_rx.lock() else {
            tracing::warn!("MIDI callback rx mutex poisoned, skipping poll");
            return Vec::new();
        };
        let mut notifications = Vec::new();
        while let Ok(notification) = rx.try_recv() {
            notifications.push(notification);
        }
        notifications
    }

    /// Invoke callbacks for a MIDI message.
    pub async fn invoke_callbacks(&self, device_id: MidiDeviceId, msg: &MidiMessage) {
        let callbacks = self.callbacks.read().await;

        // Determine the callback type and channel based on the message
        let (callback_type, channel) = match msg {
            MidiMessage::NoteOn { channel, .. } | MidiMessage::NoteOff { channel, .. } => {
                (CallbackType::KeyboardNote, Some(*channel))
            }
            MidiMessage::ControlChange { channel, cc, .. } => {
                (CallbackType::ControlChange(*cc), Some(*channel))
            }
            MidiMessage::PitchBend { channel, .. } => (CallbackType::PitchBend, Some(*channel)),
            MidiMessage::Clock
            | MidiMessage::Start
            | MidiMessage::Stop
            | MidiMessage::Continue => (CallbackType::ClockSync, None),
        };

        // Get matching callbacks - channel is optional now
        let matching = if let Some(ch) = channel {
            callbacks.get_matching(device_id, callback_type, ch)
        } else {
            // For clock messages, get all ClockSync callbacks for this device
            callbacks.get_matching_no_channel(device_id, callback_type)
        };

        // Send notifications for each matching callback
        for callback in matching {
            let notification = MidiEventNotification {
                callback_id: callback.id,
                device_id,
                message: msg.clone(),
            };

            // Try to send without blocking
            if let Err(e) = self.callback_tx.try_send(notification) {
                tracing::warn!("Failed to send MIDI callback notification: {}", e);
            }
        }
    }
}

impl Default for MidiCallbackManager {
    fn default() -> Self {
        Self::new()
    }
}
