//! MIDI recording functionality.
//!
//! This module handles recording of MIDI input events for later playback
//! or conversion to patterns/melodies.

use super::types::MidiMessage;
use crate::compat::RwLock;
use crate::midi::MidiRecording;
use crate::state::State;
use crate::types::ids::MidiDeviceId;
use std::collections::HashMap;
use std::sync::Arc;

/// Manager for MIDI recording state.
pub struct MidiRecordingManager {
    /// Active recordings by device ID.
    recordings: Arc<RwLock<HashMap<MidiDeviceId, MidiRecording>>>,
}

impl Default for MidiRecordingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiRecordingManager {
    /// Create a new recording manager.
    pub fn new() -> Self {
        Self {
            recordings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get a reference to the recordings map for direct access.
    pub fn recordings(&self) -> Arc<RwLock<HashMap<MidiDeviceId, MidiRecording>>> {
        Arc::clone(&self.recordings)
    }

    /// Record a MIDI message if recording is active for this device.
    pub async fn record_message(
        &self,
        device_id: MidiDeviceId,
        msg: &MidiMessage,
        state: &Arc<RwLock<State>>,
    ) {
        let current_beat = {
            let state = state.read().await;
            state.current_beat
        };
        self.record_message_at_beat(device_id, msg, current_beat)
            .await;
    }

    pub async fn record_message_at_beat(
        &self,
        device_id: MidiDeviceId,
        msg: &MidiMessage,
        current_beat: crate::types::Beat,
    ) {
        let mut recordings = self.recordings.write().await;
        if let Some(recording) = recordings.get_mut(&device_id) {
            if recording.is_recording {
                match msg {
                    MidiMessage::NoteOn {
                        channel,
                        note,
                        velocity,
                    } => {
                        recording.record_note_on(*note, *velocity, *channel, current_beat);
                        tracing::debug!(
                            "Recorded note_on: device={}, note={}, velocity={}, beat={}",
                            device_id.0,
                            note,
                            velocity,
                            current_beat.to_f64()
                        );
                    }
                    MidiMessage::NoteOff { channel, note } => {
                        recording.record_note_off(*note, *channel, current_beat);
                        tracing::debug!(
                            "Recorded note_off: device={}, note={}, beat={}",
                            device_id.0,
                            note,
                            current_beat.to_f64()
                        );
                    }
                    MidiMessage::ControlChange { channel, cc, value } => {
                        recording.record_cc(*cc, *value, *channel, current_beat);
                        tracing::trace!(
                            "Recorded CC: device={}, cc={}, value={}, beat={}",
                            device_id.0,
                            cc,
                            value,
                            current_beat.to_f64()
                        );
                    }
                    _ => {} // Don't record other message types
                }
            }
        }
    }
}
