//! Runtime Handle
//!
//! The `RuntimeHandle` is the main interface for interacting with VibeLang
//! from the API layer. It provides thread-safe access to state and message sending.

use crate::midi::MidiMessage;
use crate::scsynth::Scsynth;
use crate::state::{ScriptState, StateManager, StateMessage};
use anyhow::Result;
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Handle to the running VibeLang runtime.
///
/// This is the main interface for interacting with VibeLang from the API layer.
/// It provides thread-safe access to state and message sending.
#[derive(Clone)]
pub struct RuntimeHandle {
    /// Sender for state messages.
    pub(crate) message_tx: Sender<StateMessage>,
    /// Shared state manager for read access.
    pub(crate) state_manager: StateManager,
    /// Reference to the SuperCollider client.
    pub(crate) scsynth: Scsynth,
    /// Flag to signal shutdown.
    pub(crate) shutdown: Arc<AtomicBool>,
    /// Receiver for sequence completion notifications.
    pub(crate) completion_rx: Option<crossbeam_channel::Receiver<String>>,
    /// Sender for MIDI messages to the runtime thread.
    pub(crate) midi_tx: Sender<MidiMessage>,
}

impl RuntimeHandle {
    /// Send a message to the runtime thread.
    pub fn send(&self, msg: StateMessage) -> Result<()> {
        self.message_tx
            .send(msg)
            .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))
    }

    /// Get the state manager for read access.
    pub fn state(&self) -> &StateManager {
        &self.state_manager
    }

    /// Get the SuperCollider client.
    pub fn scsynth(&self) -> &Scsynth {
        &self.scsynth
    }

    /// Read the current state with a closure.
    pub fn with_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&ScriptState) -> R,
    {
        self.state_manager.with_state_read(f)
    }

    /// Write to the state with a closure.
    pub fn with_state_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ScriptState) -> R,
    {
        self.state_manager.with_state_write(f)
    }

    /// Get a clone of the message sender.
    pub fn message_sender(&self) -> Sender<StateMessage> {
        self.message_tx.clone()
    }

    /// Signal the runtime to shut down.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Check if shutdown has been requested.
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    /// Wait for a sequence to complete (for play_once sequences).
    /// Returns true if the sequence completed, false if timeout or no channel.
    pub fn wait_for_sequence(&self, name: &str, timeout: Option<Duration>) -> bool {
        let rx = match &self.completion_rx {
            Some(rx) => rx,
            None => return false,
        };

        let deadline = timeout.map(|t| Instant::now() + t);

        loop {
            let remaining = match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        return false;
                    }
                    Some(d - now)
                }
                None => None,
            };

            let result = match remaining {
                Some(t) => rx.recv_timeout(t),
                None => rx
                    .recv()
                    .map_err(|_| crossbeam_channel::RecvTimeoutError::Disconnected),
            };

            match result {
                Ok(completed_name) if completed_name == name => return true,
                Ok(_) => continue, // Different sequence completed, keep waiting
                Err(_) => return false, // Timeout or channel closed
            }
        }
    }

    /// Try to receive a sequence completion notification without blocking.
    /// Returns Some(sequence_name) if a sequence completed, None otherwise.
    pub fn try_recv_completion(&self) -> Option<String> {
        self.completion_rx.as_ref()?.try_recv().ok()
    }

    /// Check if a specific sequence has completed (non-blocking).
    pub fn is_sequence_completed(&self, name: &str) -> bool {
        self.with_state(|state| {
            // If it's in active_sequences and marked completed, it's done
            if let Some(active) = state.active_sequences.get(name) {
                return active.completed;
            }
            // If it's not in active_sequences but the definition has play_once,
            // it was started and has now finished
            if let Some(def) = state.sequences.get(name) {
                if def.play_once {
                    // If play_once and not active, it either never started or completed
                    // We can't distinguish without more tracking, so just return false
                    return false;
                }
            }
            false
        })
    }

    /// Get the MIDI message sender for forwarding MIDI events.
    /// This is used by the API layer when opening MIDI devices.
    pub fn midi_sender(&self) -> Sender<MidiMessage> {
        self.midi_tx.clone()
    }

    /// Create a RuntimeHandle for validation mode (no runtime thread).
    ///
    /// This is used by the validation engine to execute scripts without
    /// starting a full runtime. The handle can receive messages but
    /// they won't be processed by a runtime thread.
    ///
    /// # Arguments
    /// * `message_tx` - Sender for state messages (will be received by caller)
    /// * `state_manager` - State manager for read/write access
    /// * `scsynth` - SuperCollider client (use `Scsynth::noop()` for validation)
    /// * `midi_tx` - Sender for MIDI messages (can be unused)
    pub fn new_validation(
        message_tx: Sender<StateMessage>,
        state_manager: StateManager,
        scsynth: Scsynth,
        midi_tx: Sender<MidiMessage>,
    ) -> Self {
        Self {
            message_tx,
            state_manager,
            scsynth,
            shutdown: Arc::new(AtomicBool::new(false)),
            completion_rx: None,
            midi_tx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateMessage;
    use std::time::Duration;

    /// Create a test handle for unit testing.
    fn create_test_handle() -> (
        RuntimeHandle,
        crossbeam_channel::Receiver<StateMessage>,
        crossbeam_channel::Receiver<MidiMessage>,
    ) {
        let (msg_tx, msg_rx) = crossbeam_channel::unbounded();
        let (midi_tx, midi_rx) = crossbeam_channel::unbounded();
        let state_manager = StateManager::new();
        let scsynth = Scsynth::noop();

        let handle = RuntimeHandle::new_validation(msg_tx, state_manager, scsynth, midi_tx);

        (handle, msg_rx, midi_rx)
    }

    #[test]
    fn test_handle_send_message() {
        let (handle, rx, _midi_rx) = create_test_handle();

        let result = handle.send(StateMessage::SetBpm { bpm: 130.0 });
        assert!(result.is_ok());

        let msg = rx.recv_timeout(Duration::from_millis(100)).unwrap();
        assert!(matches!(msg, StateMessage::SetBpm { bpm } if bpm == 130.0));
    }

    #[test]
    fn test_handle_shutdown_signal() {
        let (handle, _rx, _midi_rx) = create_test_handle();

        assert!(!handle.is_shutdown_requested());
        handle.shutdown();
        assert!(handle.is_shutdown_requested());
    }

    #[test]
    fn test_handle_state_access() {
        let (handle, _rx, _midi_rx) = create_test_handle();

        // Read initial state
        let tempo = handle.with_state(|state| state.tempo);
        assert_eq!(tempo, 120.0);

        // Write to state
        handle.with_state_mut(|state| {
            state.tempo = 140.0;
        });

        // Verify change
        let new_tempo = handle.with_state(|state| state.tempo);
        assert_eq!(new_tempo, 140.0);
    }

    #[test]
    fn test_handle_clone() {
        let (handle, rx, _midi_rx) = create_test_handle();

        let handle2 = handle.clone();

        // Both handles should send to the same channel
        handle.send(StateMessage::StartScheduler).unwrap();
        handle2.send(StateMessage::StopScheduler).unwrap();

        assert_eq!(rx.len(), 2);
    }

    #[test]
    fn test_handle_sequence_completion_no_channel() {
        let (handle, _rx, _midi_rx) = create_test_handle();

        // Without completion channel, should return false
        assert!(!handle.wait_for_sequence("test_seq", Some(Duration::from_millis(10))));
        assert!(!handle.is_sequence_completed("test_seq"));
    }

    #[test]
    fn test_handle_midi_sender() {
        let (handle, _rx, midi_rx) = create_test_handle();

        let midi_sender = handle.midi_sender();

        // Should be able to send MIDI messages
        let result = midi_sender.send(MidiMessage::Start { timestamp: 0 });
        assert!(result.is_ok());

        // Verify the message was received
        let msg = midi_rx.recv_timeout(Duration::from_millis(100)).unwrap();
        assert!(matches!(msg, MidiMessage::Start { .. }));
    }

    #[test]
    fn test_handle_message_sender_clone() {
        let (handle, rx, _midi_rx) = create_test_handle();

        let sender = handle.message_sender();
        sender.send(StateMessage::FinalizeGroups).unwrap();

        assert_eq!(rx.len(), 1);
    }

    #[test]
    fn test_handle_try_recv_completion_empty() {
        let (handle, _rx, _midi_rx) = create_test_handle();

        // No completion channel set up, should return None
        assert!(handle.try_recv_completion().is_none());
    }
}
