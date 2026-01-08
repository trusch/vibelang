//! Recordings handler implementation.
//!
//! This handler manages audio recording sessions, capturing audio from group
//! buses into buffers with optional file output.
//!
//! # Recording Workflow
//!
//! 1. **Start**: Allocate buffer, calculate start/end beats
//! 2. **Pending**: Wait for start beat (quantized start)
//! 3. **Recording**: Create recording synth to capture audio
//! 4. **Complete**: Stop synth, optionally save to file, make buffer available as sample
//!
//! # Example
//!
//! ```ignore
//! use vibelang_core2::{RecordingId, RecordingConfig};
//!
//! // Start a 4-bar recording from the drums group
//! let id = RecordingId::new(1);
//! let config = RecordingConfig::new(drums_group_id)
//!     .with_bars(4.0)
//!     .with_metronome(true)
//!     .to_file("recordings/drums.wav");
//! handler.start(id, config).await?;
//!
//! // Later, check status
//! let status = handler.status(id).await?;
//! ```

use crate::backend::{AddAction, Backend};
use crate::state::State;
use crate::traits::{RecordingConfig, RecordingInfo, RecordingStatus, Recordings};
use crate::types::{Beat, BufferId, RecordingId};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handler for audio recording sessions.
///
/// Records audio from group buses into buffers using the `system_record_mono`
/// or `system_record_stereo` synthdefs.
pub struct RecordingsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

impl<B: Backend> RecordingsHandler<B> {
    /// Create a new recordings handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }

    /// Called by the runtime's tick function to check for recordings that
    /// should start or stop based on the current beat.
    pub async fn tick(&self, current_beat: Beat) -> Result<()> {
        // Collect recordings that need to start
        let to_start: Vec<RecordingId> = {
            let state = self.state.read().await;
            state
                .recordings
                .iter()
                .filter(|(_, info)| {
                    info.status == RecordingStatus::Pending
                        && info.buffer_ready
                        && current_beat >= info.start_beat
                })
                .map(|(id, _)| *id)
                .collect()
        };

        // Start each recording
        for id in to_start {
            self.start_recording_synth(id).await?;
        }

        // Collect recordings that should complete
        let to_complete: Vec<RecordingId> = {
            let state = self.state.read().await;
            state
                .recordings
                .iter()
                .filter(|(_, info)| {
                    info.status == RecordingStatus::Recording && current_beat >= info.end_beat
                })
                .map(|(id, _)| *id)
                .collect()
        };

        // Complete each recording
        for id in to_complete {
            self.complete_recording(id).await?;
        }

        Ok(())
    }

    /// Mark a recording buffer as allocated (called when backend confirms allocation).
    pub async fn buffer_allocated(&self, id: RecordingId) -> Result<()> {
        let mut state = self.state.write().await;
        if let Some(info) = state.recordings.get_mut(&id) {
            info.buffer_ready = true;
            tracing::debug!(
                "Recording {} buffer allocated (buffer_id={})",
                id.0,
                info.buffer_id.0
            );
        }
        Ok(())
    }

    /// Internal: Create the recording synth to capture audio.
    async fn start_recording_synth(&self, id: RecordingId) -> Result<()> {
        let (buffer_id, audio_bus, num_channels, group_node_id) = {
            let state = self.state.read().await;
            let info = state
                .recordings
                .get(&id)
                .ok_or(Error::RecordingNotFound(id))?;

            // Get the group's node ID for placement
            let group_state = state.groups.get(&info.group).ok_or(Error::GroupNotFound(info.group))?;

            (
                info.buffer_id,
                info.audio_bus,
                info.num_channels,
                group_state.node_id,
            )
        };

        // Allocate node ID for the recording synth
        let node_id = {
            let mut state = self.state.write().await;
            state.alloc_node_id()
        };

        // Choose synthdef based on channel count
        let synthdef = if num_channels == 1 {
            "system_record_mono"
        } else {
            "system_record_stereo"
        };

        // Create params for the recording synth
        let mut params = crate::types::ParamMap::new();
        params.insert("bufnum".to_string(), buffer_id.raw() as f32);
        params.insert("bus".to_string(), audio_bus.raw() as f32);

        // Create the recording synth at the tail of the group
        self.backend
            .create_synth(synthdef, node_id, group_node_id, AddAction::Tail, &params)
            .await
            .map_err(Error::backend)?;

        // Update state
        {
            let mut state = self.state.write().await;
            if let Some(info) = state.recordings.get_mut(&id) {
                info.node_id = Some(node_id);
                info.status = RecordingStatus::Recording;
            }
        }

        tracing::info!(
            "Recording {} started (node_id={}, buffer_id={}, bus={})",
            id.0,
            node_id.0,
            buffer_id.0,
            audio_bus.0
        );

        Ok(())
    }

    /// Internal: Complete a recording, stop synth, save file if needed.
    async fn complete_recording(&self, id: RecordingId) -> Result<()> {
        let (node_id, buffer_id, file_path, num_channels, num_frames, sample_rate) = {
            let state = self.state.read().await;
            let info = state
                .recordings
                .get(&id)
                .ok_or(Error::RecordingNotFound(id))?;

            (
                info.node_id,
                info.buffer_id,
                info.file_path.clone(),
                info.num_channels,
                info.num_frames,
                info.sample_rate,
            )
        };

        // Stop the recording synth if it exists
        if let Some(node_id) = node_id {
            self.backend.free_node(node_id).await.map_err(Error::backend)?;
        }

        // Save to file if path was specified
        if let Some(path) = &file_path {
            self.backend
                .write_buffer(buffer_id, path)
                .await
                .map_err(Error::backend)?;
            tracing::info!("Recording {} saved to {:?}", id.0, path);
        }

        // Update state
        {
            let mut state = self.state.write().await;
            if let Some(info) = state.recordings.get_mut(&id) {
                info.status = RecordingStatus::Completed;
                info.node_id = None;

                // Create a sample entry so the recording can be used for playback
                let sample_id = crate::types::SampleId::new(buffer_id.raw());
                info.sample_id = Some(sample_id);

                let sample_info = crate::traits::SampleInfo {
                    id: sample_id,
                    buffer_id,
                    path: file_path.clone().unwrap_or_default(),
                    duration_secs: num_frames as f64 / sample_rate as f64,
                    sample_rate: sample_rate as f64,
                    channels: num_channels as u16,
                    detected_bpm: None,
                };
                state.samples.insert(sample_id, sample_info);
            }
        }

        tracing::info!("Recording {} completed", id.0);

        Ok(())
    }
}

#[async_trait]
impl<B: Backend> Recordings for RecordingsHandler<B> {
    async fn start(&self, id: RecordingId, config: RecordingConfig) -> Result<BufferId> {
        let (buffer_id, info) = {
            let mut state = self.state.write().await;

            // Check if recording already exists
            if state.recordings.contains_key(&id) {
                return Err(Error::RecordingAlreadyExists(id));
            }

            // Allocate buffer ID
            let buffer_id = state.alloc_buffer_id();

            // Get audio bus from the group
            let group_state = state.groups.get(&config.group).ok_or(Error::GroupNotFound(config.group))?;
            let audio_bus = group_state.audio_bus;

            // Calculate start beat (quantized if not specified)
            let start_beat = config.start_beat.unwrap_or_else(|| {
                // Default: start at the next bar
                let current = state.current_beat;
                let beats_per_bar = state.time_sig.beats_per_bar() as f64;
                let current_bar = current.to_f64() / beats_per_bar;
                Beat::from_f64((current_bar.floor() + 1.0) * beats_per_bar)
            });

            // Calculate end beat
            let length_beats = if let Some(beats) = config.length_beats {
                beats
            } else if let Some(secs) = config.length_seconds {
                // Convert seconds to beats
                secs * state.tempo / 60.0
            } else {
                16.0 // Default: 4 bars
            };
            let end_beat = Beat::from_f64(start_beat.to_f64() + length_beats);

            // Calculate buffer size
            let sample_rate = self.backend.sample_rate() as f32;
            let duration_secs = if let Some(secs) = config.length_seconds {
                secs
            } else {
                length_beats * 60.0 / state.tempo
            };
            let num_frames = (duration_secs * sample_rate as f64).ceil() as u32;

            // Create recording info
            let info = RecordingInfo {
                id,
                group: config.group,
                buffer_id,
                node_id: None,
                audio_bus,
                length_beats: config.length_beats,
                length_seconds: config.length_seconds,
                start_beat,
                end_beat,
                count_in_beats: config.count_in_beats,
                metronome: config.metronome,
                file_path: config.file_path,
                num_channels: config.num_channels,
                num_frames,
                sample_rate,
                status: RecordingStatus::Pending,
                buffer_ready: false,
                sample_id: None,
            };

            // Store in state
            state.recordings.insert(id, info.clone());

            (buffer_id, info)
        };

        // Allocate buffer via backend (outside of state lock)
        self.backend
            .alloc_buffer(buffer_id, info.num_frames, info.num_channels as u16)
            .await
            .map_err(Error::backend)?;

        // Mark buffer as allocated
        {
            let mut state = self.state.write().await;
            if let Some(rec) = state.recordings.get_mut(&id) {
                rec.buffer_ready = true;
            }
        }

        tracing::info!(
            "Recording {} created: buffer_id={}, frames={}, start_beat={:.2}, end_beat={:.2}",
            id.0,
            buffer_id.0,
            info.num_frames,
            info.start_beat.to_f64(),
            info.end_beat.to_f64()
        );

        Ok(buffer_id)
    }

    async fn stop(&self, id: RecordingId) -> Result<()> {
        // Get recording info
        let status = {
            let state = self.state.read().await;
            state
                .recordings
                .get(&id)
                .map(|info| info.status)
                .ok_or(Error::RecordingNotFound(id))?
        };

        match status {
            RecordingStatus::Recording => {
                // Complete the recording early
                self.complete_recording(id).await
            }
            RecordingStatus::Pending => {
                // Cancel pending recording
                self.cancel(id).await
            }
            _ => {
                // Already completed or cancelled
                Ok(())
            }
        }
    }

    async fn cancel(&self, id: RecordingId) -> Result<()> {
        let (node_id, buffer_id) = {
            let mut state = self.state.write().await;
            let info = state
                .recordings
                .remove(&id)
                .ok_or(Error::RecordingNotFound(id))?;
            (info.node_id, info.buffer_id)
        };

        // Free the recording synth if running
        if let Some(node_id) = node_id {
            self.backend.free_node(node_id).await.map_err(Error::backend)?;
        }

        // Free the buffer
        self.backend.free_buffer(buffer_id).await.map_err(Error::backend)?;

        tracing::info!("Recording {} cancelled", id.0);

        Ok(())
    }

    async fn status(&self, id: RecordingId) -> Result<RecordingStatus> {
        let state = self.state.read().await;
        state
            .recordings
            .get(&id)
            .map(|info| info.status)
            .ok_or(Error::RecordingNotFound(id))
    }

    async fn info(&self, id: RecordingId) -> Result<RecordingInfo> {
        let state = self.state.read().await;
        state
            .recordings
            .get(&id)
            .cloned()
            .ok_or(Error::RecordingNotFound(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GroupId;

    #[test]
    fn test_recording_config_defaults() {
        let config = RecordingConfig::new(GroupId::new(1));
        assert_eq!(config.group, GroupId::new(1));
        assert_eq!(config.length_beats, Some(16.0));
        assert_eq!(config.num_channels, 2);
    }

    #[test]
    fn test_recording_status_transitions() {
        // Recording status should follow: Pending -> Recording -> Completed
        let status = RecordingStatus::Pending;
        assert_eq!(status, RecordingStatus::Pending);

        let status = RecordingStatus::Recording;
        assert_eq!(status, RecordingStatus::Recording);

        let status = RecordingStatus::Completed;
        assert_eq!(status, RecordingStatus::Completed);
    }
}
