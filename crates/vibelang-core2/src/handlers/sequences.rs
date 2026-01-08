//! Sequences handler implementation.

use crate::backend::Backend;
use crate::state::{ActiveFade, SequenceState, State};
use crate::traits::{Clip, FadeConfig, SequenceConfig, Sequences};
use crate::types::{Beat, MelodyId, PatternId, SequenceId};
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// A unique identifier for a clip within a sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ClipKey {
    sequence_id: SequenceId,
    clip_index: usize,
}

/// Actions to perform after releasing the state lock.
enum ClipAction {
    StartPattern(PatternId),
    StopPattern(PatternId),
    StartMelody(MelodyId),
    StopMelody(MelodyId),
    StartFade(FadeConfig),
    StartSequence(SequenceId),
}

/// Handler for sequence operations.
pub struct SequencesHandler<B: Backend> {
    #[allow(dead_code)]
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Tracks which clips are currently active: (sequence_id, clip_index)
    active_clips: Arc<RwLock<HashSet<ClipKey>>>,
    /// Tracks which fades have been triggered: (sequence_id, clip_index)
    triggered_fades: Arc<RwLock<HashSet<ClipKey>>>,
}

impl<B: Backend> SequencesHandler<B> {
    /// Create a new sequences handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self {
            backend,
            state,
            active_clips: Arc::new(RwLock::new(HashSet::new())),
            triggered_fades: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Process sequences for the current beat.
    ///
    /// Called by the runtime's tick loop to activate/deactivate clips.
    pub async fn tick(&self, current_beat: Beat) {
        let actions = self.collect_actions(current_beat).await;

        // Execute actions (lock released)
        for action in actions {
            match action {
                ClipAction::StartPattern(id) => {
                    let mut state = self.state.write().await;
                    if let Some(pattern) = state.patterns.get_mut(&id) {
                        pattern.playing = true;
                        pattern.loop_position = Beat::ZERO;
                    }
                }
                ClipAction::StopPattern(id) => {
                    let mut state = self.state.write().await;
                    if let Some(pattern) = state.patterns.get_mut(&id) {
                        pattern.playing = false;
                    }
                }
                ClipAction::StartMelody(id) => {
                    let mut state = self.state.write().await;
                    if let Some(melody) = state.melodies.get_mut(&id) {
                        melody.playing = true;
                        melody.loop_position = Beat::ZERO;
                    }
                }
                ClipAction::StopMelody(id) => {
                    let mut state = self.state.write().await;
                    if let Some(melody) = state.melodies.get_mut(&id) {
                        melody.playing = false;
                    }
                }
                ClipAction::StartFade(config) => {
                    let mut state = self.state.write().await;
                    // Get current value for from
                    let start_value = config.from.unwrap_or(0.0);
                    state.active_fades.push(ActiveFade {
                        config,
                        start_time: Instant::now(),
                        start_value,
                    });
                }
                ClipAction::StartSequence(id) => {
                    let mut state = self.state.write().await;
                    if let Some(sequence) = state.sequences.get_mut(&id) {
                        sequence.playing = true;
                        sequence.paused = false;
                        sequence.position = Beat::ZERO;
                    }
                }
            }
        }
    }

    /// Collect actions to perform based on current beat.
    async fn collect_actions(&self, current_beat: Beat) -> Vec<ClipAction> {
        let mut actions = Vec::new();
        let mut active_clips = self.active_clips.write().await;
        let mut triggered_fades = self.triggered_fades.write().await;

        let state = self.state.read().await;

        // Collect all sequence IDs that are playing
        let playing_sequences: Vec<(SequenceId, Beat, Beat, bool, Vec<Clip>)> = state
            .sequences
            .iter()
            .filter(|(_, s)| s.playing && !s.paused)
            .map(|(id, s)| {
                (
                    *id,
                    s.position,
                    s.config.length,
                    s.looping,
                    s.config.clips.clone(),
                )
            })
            .collect();

        drop(state);

        for (seq_id, last_pos, length, looping, clips) in playing_sequences {
            // Calculate new position
            let new_pos = if length > Beat::ZERO {
                if looping {
                    current_beat % length
                } else if current_beat < length {
                    current_beat
                } else {
                    length
                }
            } else {
                Beat::ZERO
            };

            // Check for wrap-around (loop restart)
            let wrapped = looping && new_pos < last_pos;

            // Process each clip
            for (clip_index, clip) in clips.iter().enumerate() {
                let clip_key = ClipKey {
                    sequence_id: seq_id,
                    clip_index,
                };

                match clip {
                    Clip::Pattern { id, start, end } => {
                        let is_active = active_clips.contains(&clip_key);
                        let should_be_active = new_pos >= *start && new_pos < *end;

                        // Handle wrap-around: stop all clips that were active
                        if wrapped && is_active {
                            actions.push(ClipAction::StopPattern(*id));
                            active_clips.remove(&clip_key);
                        }

                        // Start clip if entering range
                        if should_be_active && !active_clips.contains(&clip_key) {
                            // Check if we just entered the range
                            let just_entered = if wrapped {
                                new_pos >= *start
                            } else {
                                last_pos < *start && new_pos >= *start
                            };
                            if just_entered || (wrapped && should_be_active) {
                                actions.push(ClipAction::StartPattern(*id));
                                active_clips.insert(clip_key);
                            }
                        }

                        // Stop clip if exiting range
                        if !should_be_active && is_active {
                            actions.push(ClipAction::StopPattern(*id));
                            active_clips.remove(&clip_key);
                        }
                    }

                    Clip::Melody { id, start, end } => {
                        let is_active = active_clips.contains(&clip_key);
                        let should_be_active = new_pos >= *start && new_pos < *end;

                        // Handle wrap-around
                        if wrapped && is_active {
                            actions.push(ClipAction::StopMelody(*id));
                            active_clips.remove(&clip_key);
                        }

                        // Start clip if entering range
                        if should_be_active && !active_clips.contains(&clip_key) {
                            let just_entered = if wrapped {
                                new_pos >= *start
                            } else {
                                last_pos < *start && new_pos >= *start
                            };
                            if just_entered || (wrapped && should_be_active) {
                                actions.push(ClipAction::StartMelody(*id));
                                active_clips.insert(clip_key);
                            }
                        }

                        // Stop clip if exiting range
                        if !should_be_active && is_active {
                            actions.push(ClipAction::StopMelody(*id));
                            active_clips.remove(&clip_key);
                        }
                    }

                    Clip::Fade { config, start } => {
                        // Fades are one-shot triggers
                        let should_trigger = if wrapped {
                            // After wrap, trigger if we're past the start
                            new_pos >= *start && !triggered_fades.contains(&clip_key)
                        } else {
                            // Normal case: trigger when crossing start
                            last_pos < *start && new_pos >= *start
                        };

                        if wrapped {
                            // Reset triggered state on loop
                            triggered_fades.remove(&clip_key);
                        }

                        if should_trigger {
                            actions.push(ClipAction::StartFade(config.clone()));
                            triggered_fades.insert(clip_key);
                        }
                    }

                    Clip::Sequence { id, start } => {
                        // Nested sequences are one-shot triggers
                        let should_trigger = if wrapped {
                            new_pos >= *start && !triggered_fades.contains(&clip_key)
                        } else {
                            last_pos < *start && new_pos >= *start
                        };

                        if wrapped {
                            triggered_fades.remove(&clip_key);
                        }

                        if should_trigger {
                            actions.push(ClipAction::StartSequence(*id));
                            triggered_fades.insert(clip_key);
                        }
                    }
                }
            }

            // Update sequence position
            let mut state = self.state.write().await;
            if let Some(sequence) = state.sequences.get_mut(&seq_id) {
                sequence.position = new_pos;

                // Handle non-looping sequence completion
                if !looping && new_pos >= length {
                    sequence.playing = false;
                    // Clear all active clips for this sequence
                    active_clips.retain(|k| k.sequence_id != seq_id);
                    triggered_fades.retain(|k| k.sequence_id != seq_id);
                }
            }
        }

        actions
    }
}

#[async_trait]
impl<B: Backend> Sequences for SequencesHandler<B> {
    async fn create(&self, id: SequenceId, config: SequenceConfig) -> Result<()> {
        let mut state = self.state.write().await;

        if state.sequences.contains_key(&id) {
            return Err(Error::SequenceExists(id));
        }

        state.sequences.insert(
            id,
            SequenceState {
                id,
                config,
                playing: false,
                paused: false,
                looping: false,
                position: Beat::ZERO,
            },
        );

        Ok(())
    }

    async fn delete(&self, id: SequenceId) -> Result<()> {
        let mut state = self.state.write().await;

        state
            .sequences
            .remove(&id)
            .ok_or(Error::SequenceNotFound(id))?;

        Ok(())
    }

    async fn start(&self, id: SequenceId, looping: bool) -> Result<()> {
        let mut state = self.state.write().await;

        let sequence = state
            .sequences
            .get_mut(&id)
            .ok_or(Error::SequenceNotFound(id))?;

        sequence.playing = true;
        sequence.paused = false;
        sequence.looping = looping;
        sequence.position = Beat::ZERO;

        Ok(())
    }

    async fn stop(&self, id: SequenceId) -> Result<()> {
        let mut state = self.state.write().await;

        let sequence = state
            .sequences
            .get_mut(&id)
            .ok_or(Error::SequenceNotFound(id))?;

        sequence.playing = false;
        sequence.paused = false;

        Ok(())
    }

    async fn pause(&self, id: SequenceId) -> Result<()> {
        let mut state = self.state.write().await;

        let sequence = state
            .sequences
            .get_mut(&id)
            .ok_or(Error::SequenceNotFound(id))?;

        if sequence.playing {
            sequence.paused = true;
        }

        Ok(())
    }

    async fn resume(&self, id: SequenceId) -> Result<()> {
        let mut state = self.state.write().await;

        let sequence = state
            .sequences
            .get_mut(&id)
            .ok_or(Error::SequenceNotFound(id))?;

        if sequence.paused {
            sequence.paused = false;
        }

        Ok(())
    }
}
