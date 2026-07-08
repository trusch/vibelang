//! Sequences handler implementation.

use crate::backend::Backend;
use crate::compat::Instant;
use crate::compat::RwLock;
use crate::state::{ActiveFade, SequenceState, State};
use crate::traits::{Clip, FadeConfig, SequenceConfig, Sequences};
use crate::types::{Beat, MelodyId, PatternId, SequenceId};
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;

/// A unique identifier for a clip within a sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ClipKey {
    sequence_id: SequenceId,
    clip_index: usize,
}

/// Info about a playing sequence for tick processing.
struct PlayingSequence {
    id: SequenceId,
    position: Beat,
    length: Beat,
    looping: bool,
    start_beat: Option<Beat>,
    clips: Vec<Clip>,
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
                        tracing::debug!("Sequence starting melody {:?}", id);
                        melody.playing = true;
                        melody.loop_position = Beat::ZERO;
                    } else {
                        tracing::warn!(
                            "Sequence tried to start melody {:?} but it doesn't exist",
                            id
                        );
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

                    // Get current value from state (for logging)
                    use crate::traits::FadeTarget;
                    let current_state_value = match &config.target {
                        FadeTarget::Group(id) => state
                            .groups
                            .get(id)
                            .and_then(|g| g.params.get(&config.param).copied()),
                        FadeTarget::Voice(id) => state
                            .voices
                            .get(id)
                            .and_then(|v| v.config.params.get(&config.param).copied()),
                        FadeTarget::Effect(id) => state
                            .effects
                            .get(id)
                            .and_then(|e| e.params.get(&config.param).copied()),
                        FadeTarget::Pattern(id) => state
                            .patterns
                            .get(id)
                            .and_then(|p| p.content.steps.first())
                            .and_then(|s| s.params.get(&config.param).copied()),
                        FadeTarget::Melody(_) => None,
                    };

                    // Get starting value - use explicit `from` or look up current value from state
                    let start_value = config
                        .from
                        .unwrap_or_else(|| current_state_value.unwrap_or(0.0));

                    tracing::debug!(
                        "Starting fade on {:?}/{}: from={:?} (state={:?}), using start={}, to={}",
                        config.target,
                        config.param,
                        config.from,
                        current_state_value,
                        start_value,
                        config.to
                    );

                    // Immediately apply the starting value to state
                    // (The fades tick will send it to the backend)
                    match &config.target {
                        FadeTarget::Voice(id) => {
                            if let Some(voice) = state.voices.get_mut(id) {
                                voice
                                    .config
                                    .params
                                    .insert(config.param.clone(), start_value);
                            }
                        }
                        FadeTarget::Group(id) => {
                            if let Some(group) = state.groups.get_mut(id) {
                                group.params.insert(config.param.clone(), start_value);
                            }
                        }
                        FadeTarget::Effect(id) => {
                            if let Some(effect) = state.effects.get_mut(id) {
                                effect.params.insert(config.param.clone(), start_value);
                            }
                        }
                        FadeTarget::Pattern(id) => {
                            // One-shot at fade start (not per-tick): bake the
                            // start value into content once. The 500 Hz fade
                            // ticks then ride on the cheap `fade_overlay` path
                            // (see FadesHandler::tick / PatternState).
                            if let Some(pattern) = state.patterns.get_mut(id) {
                                pattern.write_param_to_all_steps(&config.param, start_value);
                            }
                        }
                        FadeTarget::Melody(_) => {}
                    }

                    // Cancel any existing fade on the same target/param
                    state.active_fades.retain(|f| {
                        f.config.target != config.target || f.config.param != config.param
                    });

                    state.active_fades.push(ActiveFade {
                        config,
                        start_time: Instant::now(),
                        start_value,
                    });
                }
                ClipAction::StartSequence(id) => {
                    let mut state = self.state.write().await;
                    let current_beat = state.current_beat;
                    if let Some(sequence) = state.sequences.get_mut(&id) {
                        sequence.playing = true;
                        sequence.paused = false;
                        sequence.position = Beat::ZERO;
                        sequence.start_beat = Some(current_beat);
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
        // Include start_beat to calculate elapsed time properly
        let playing_sequences: Vec<PlayingSequence> = state
            .sequences
            .iter()
            .filter(|(_, s)| s.playing && !s.paused)
            .map(|(id, s)| PlayingSequence {
                id: *id,
                position: s.position,
                length: s.config.length,
                looping: s.looping,
                start_beat: s.start_beat,
                clips: s.config.clips.clone(),
            })
            .collect();

        drop(state);

        if !playing_sequences.is_empty() {
            tracing::debug!(
                "Processing {} playing sequences at beat {}",
                playing_sequences.len(),
                current_beat.to_f64()
            );
        }

        for seq in playing_sequences {
            let (seq_id, last_pos, length, looping, start_beat, clips) = (
                seq.id,
                seq.position,
                seq.length,
                seq.looping,
                seq.start_beat,
                seq.clips,
            );
            // Calculate elapsed beats since sequence started
            // If start_beat is None, use current_beat directly (legacy behavior for top-level sequences)
            let elapsed = start_beat.map_or(current_beat, |sb| current_beat - sb);

            // Calculate new position based on elapsed time
            let new_pos = if length > Beat::ZERO {
                if looping {
                    elapsed % length
                } else if elapsed < length {
                    elapsed
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

                        // Handle wrap-around: only stop if pattern won't continue playing
                        // If pattern is active and will remain active, let it continue
                        // (patterns have their own internal looping, no need to restart)
                        if wrapped && is_active && !should_be_active {
                            actions.push(ClipAction::StopPattern(*id));
                            active_clips.remove(&clip_key);
                        }

                        // Start clip if entering range (and not already active)
                        if should_be_active && !active_clips.contains(&clip_key) {
                            // Check if we just entered the range or if this is the first tick
                            // (last_pos <= *start handles clips at beat 0 when sequence just started)
                            let just_entered = if wrapped {
                                new_pos >= *start
                            } else {
                                last_pos <= *start && new_pos >= *start
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

                        // Handle wrap-around: only stop if melody won't continue playing
                        // If melody is active and will remain active, let it continue
                        // (melodies have their own internal looping, no need to restart)
                        if wrapped && is_active && !should_be_active {
                            actions.push(ClipAction::StopMelody(*id));
                            active_clips.remove(&clip_key);
                        }

                        // Start clip if entering range (and not already active)
                        if should_be_active && !active_clips.contains(&clip_key) {
                            // (last_pos <= *start handles clips at beat 0 when sequence just started)
                            let just_entered = if wrapped {
                                new_pos >= *start
                            } else {
                                last_pos <= *start && new_pos >= *start
                            };
                            tracing::debug!(
                                "Melody clip {:?}: last_pos={}, new_pos={}, start={}, end={}, should_be_active={}, just_entered={}",
                                id, last_pos.to_f64(), new_pos.to_f64(), start.to_f64(), end.to_f64(), should_be_active, just_entered
                            );
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
                        // Reset triggered state on loop BEFORE checking
                        if wrapped {
                            triggered_fades.remove(&clip_key);
                        }

                        // Fades are one-shot triggers
                        let already_triggered = triggered_fades.contains(&clip_key);

                        // Only trigger when crossing the start beat AND not already triggered
                        let should_trigger = if wrapped {
                            // After wrap, trigger if we're past the start
                            new_pos >= *start && !already_triggered
                        } else {
                            // Normal case: trigger when crossing start (and not already triggered)
                            last_pos <= *start && new_pos >= *start && !already_triggered
                        };

                        if should_trigger {
                            tracing::debug!(
                                "Triggering fade on {:?}: param={}, from={:?}, to={}, duration={:?} beats, at beat {}",
                                config.target, config.param, config.from, config.to,
                                config.duration.to_beats(), new_pos.to_f64()
                            );
                            actions.push(ClipAction::StartFade(config.clone()));
                            triggered_fades.insert(clip_key);
                        }
                    }

                    Clip::Sequence { id, start } => {
                        // Reset triggered state on loop BEFORE checking
                        if wrapped {
                            triggered_fades.remove(&clip_key);
                        }

                        // Nested sequences are one-shot triggers
                        let already_triggered = triggered_fades.contains(&clip_key);
                        let should_trigger = if wrapped {
                            new_pos >= *start && !already_triggered
                        } else {
                            // Prevent double-trigger on first tick
                            last_pos <= *start && new_pos >= *start && !already_triggered
                        };

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

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
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
                start_beat: None,
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

        let current_beat = state.current_beat;
        let sequence = state
            .sequences
            .get_mut(&id)
            .ok_or(Error::SequenceNotFound(id))?;

        sequence.playing = true;
        sequence.paused = false;
        sequence.looping = looping;
        sequence.position = Beat::ZERO;
        sequence.start_beat = Some(current_beat);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{AddAction, BufferInfo};
    use crate::state::PatternState;
    use crate::traits::PatternConfig;
    use crate::types::{BufferId, NodeId, ParamMap};
    use std::path::Path;

    // =========================================================================
    // Mock Backend for Testing
    // =========================================================================

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock error")
        }
    }

    impl std::error::Error for MockError {}

    struct MockBackend;

    #[async_trait::async_trait]
    impl Backend for MockBackend {
        type Error = MockError;

        async fn load_synthdef(
            &self,
            _name: &str,
            _data: &[u8],
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn create_synth(
            &self,
            _def: &str,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
            _params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn create_group(
            &self,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_node(&self, _node: NodeId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn run_node(
            &self,
            _node: NodeId,
            _running: bool,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn set_param(
            &self,
            _node: NodeId,
            _param: &str,
            _value: f32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn load_buffer(
            &self,
            _id: BufferId,
            _path: &Path,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames: 44100,
                channels: 2,
                sample_rate: 44100.0,
            })
        }

        async fn alloc_buffer(
            &self,
            _id: BufferId,
            frames: u32,
            channels: u16,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames,
                channels,
                sample_rate: 44100.0,
            })
        }

        async fn write_buffer(
            &self,
            _id: BufferId,
            _path: &Path,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_buffer(&self, _id: BufferId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn map_param_to_bus(
            &self,
            _node: NodeId,
            _param: &str,
            _bus: u32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        fn current_time(&self) -> Instant {
            Instant::now()
        }
    }

    // =========================================================================
    // Helper Functions
    // =========================================================================

    fn create_handler() -> (SequencesHandler<MockBackend>, Arc<RwLock<State>>) {
        let backend = Arc::new(MockBackend);
        let state = Arc::new(RwLock::new(State::default()));
        let handler = SequencesHandler::new(backend, state.clone());
        (handler, state)
    }

    // =========================================================================
    // Sequence Creation Tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_sequence() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);

        let result = handler.create(seq_id, config).await;
        assert!(result.is_ok(), "Sequence creation should succeed");

        let state_read = state.read().await;
        assert!(state_read.sequences.contains_key(&seq_id));
    }

    #[tokio::test]
    async fn test_create_sequence_duplicate_fails() {
        let (handler, _state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);

        handler.create(seq_id, config.clone()).await.unwrap();

        let result = handler.create(seq_id, config).await;
        assert!(result.is_err(), "Duplicate sequence creation should fail");
    }

    #[tokio::test]
    async fn test_create_sequence_with_clips() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0)
            .with_clip(Clip::Pattern {
                id: PatternId::new(1),
                start: Beat::from_f64(0.0),
                end: Beat::from_f64(8.0),
            })
            .with_clip(Clip::Pattern {
                id: PatternId::new(2),
                start: Beat::from_f64(8.0),
                end: Beat::from_f64(16.0),
            });

        let result = handler.create(seq_id, config).await;
        assert!(result.is_ok());

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert_eq!(seq.config.clips.len(), 2);
    }

    // =========================================================================
    // Sequence Deletion Tests
    // =========================================================================

    #[tokio::test]
    async fn test_delete_sequence() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        let result = handler.delete(seq_id).await;
        assert!(result.is_ok(), "Sequence deletion should succeed");

        let state_read = state.read().await;
        assert!(!state_read.sequences.contains_key(&seq_id));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_sequence_fails() {
        let (handler, _state) = create_handler();

        let result = handler.delete(SequenceId::new(999)).await;
        assert!(
            result.is_err(),
            "Deleting non-existent sequence should fail"
        );
    }

    // =========================================================================
    // Sequence Start/Stop Tests
    // =========================================================================

    #[tokio::test]
    async fn test_start_sequence() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        let result = handler.start(seq_id, false).await;
        assert!(result.is_ok(), "Starting sequence should succeed");

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert!(seq.playing);
        assert!(!seq.paused);
        assert!(!seq.looping);
    }

    #[tokio::test]
    async fn test_start_sequence_with_looping() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        let result = handler.start(seq_id, true).await;
        assert!(result.is_ok());

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert!(seq.playing);
        assert!(seq.looping);
    }

    #[tokio::test]
    async fn test_start_nonexistent_sequence_fails() {
        let (handler, _state) = create_handler();

        let result = handler.start(SequenceId::new(999), false).await;
        assert!(
            result.is_err(),
            "Starting non-existent sequence should fail"
        );
    }

    #[tokio::test]
    async fn test_stop_sequence() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        handler.start(seq_id, false).await.unwrap();
        let result = handler.stop(seq_id).await;
        assert!(result.is_ok(), "Stopping sequence should succeed");

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert!(!seq.playing);
    }

    #[tokio::test]
    async fn test_stop_nonexistent_sequence_fails() {
        let (handler, _state) = create_handler();

        let result = handler.stop(SequenceId::new(999)).await;
        assert!(
            result.is_err(),
            "Stopping non-existent sequence should fail"
        );
    }

    // =========================================================================
    // Sequence Pause/Resume Tests
    // =========================================================================

    #[tokio::test]
    async fn test_pause_sequence() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        handler.start(seq_id, false).await.unwrap();
        let result = handler.pause(seq_id).await;
        assert!(result.is_ok(), "Pausing sequence should succeed");

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert!(seq.playing);
        assert!(seq.paused);
    }

    #[tokio::test]
    async fn test_pause_stopped_sequence_no_op() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        // Pause without starting
        let result = handler.pause(seq_id).await;
        assert!(result.is_ok());

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert!(
            !seq.paused,
            "Pausing stopped sequence should not set paused"
        );
    }

    #[tokio::test]
    async fn test_resume_sequence() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        handler.start(seq_id, false).await.unwrap();
        handler.pause(seq_id).await.unwrap();
        let result = handler.resume(seq_id).await;
        assert!(result.is_ok(), "Resuming sequence should succeed");

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert!(seq.playing);
        assert!(!seq.paused);
    }

    #[tokio::test]
    async fn test_resume_nonpaused_sequence_no_op() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        handler.start(seq_id, false).await.unwrap();
        // Resume without pausing first
        let result = handler.resume(seq_id).await;
        assert!(result.is_ok());

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert!(seq.playing);
        assert!(!seq.paused);
    }

    #[tokio::test]
    async fn test_pause_nonexistent_sequence_fails() {
        let (handler, _state) = create_handler();

        let result = handler.pause(SequenceId::new(999)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resume_nonexistent_sequence_fails() {
        let (handler, _state) = create_handler();

        let result = handler.resume(SequenceId::new(999)).await;
        assert!(result.is_err());
    }

    // =========================================================================
    // Tick Tests - Pattern Clips
    // =========================================================================

    #[tokio::test]
    async fn test_tick_starts_pattern_clip() {
        let (handler, state) = create_handler();

        // Create a pattern in state
        let pattern_id = PatternId::new(1);
        {
            let mut state_write = state.write().await;
            state_write.patterns.insert(
                pattern_id,
                PatternState::new(
                    pattern_id,
                    PatternConfig::with_length("test_pattern", crate::types::VoiceId::new(1), 4.0),
                ),
            );
        }

        // Create sequence with pattern clip
        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 8.0).with_clip(Clip::Pattern {
            id: pattern_id,
            start: Beat::from_f64(0.0),
            end: Beat::from_f64(4.0),
        });
        handler.create(seq_id, config).await.unwrap();
        handler.start(seq_id, false).await.unwrap();

        // Tick at beat 0
        handler.tick(Beat::ZERO).await;

        let state_read = state.read().await;
        let pattern = state_read.patterns.get(&pattern_id).unwrap();
        assert!(pattern.playing, "Pattern should be playing after tick");
    }

    #[tokio::test]
    async fn test_tick_stops_pattern_clip_at_end() {
        let (handler, state) = create_handler();

        // Create a pattern in state
        let pattern_id = PatternId::new(1);
        {
            let mut state_write = state.write().await;
            state_write.patterns.insert(
                pattern_id,
                PatternState::new(
                    pattern_id,
                    PatternConfig::with_length("test_pattern", crate::types::VoiceId::new(1), 4.0),
                ),
            );
        }

        // Create sequence with pattern clip 0-4 beats
        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 8.0).with_clip(Clip::Pattern {
            id: pattern_id,
            start: Beat::from_f64(0.0),
            end: Beat::from_f64(4.0),
        });
        handler.create(seq_id, config).await.unwrap();
        handler.start(seq_id, false).await.unwrap();

        // Tick at beat 0 to start pattern
        handler.tick(Beat::ZERO).await;

        // Update state to simulate time passing
        {
            let mut state_write = state.write().await;
            state_write.current_beat = Beat::from_f64(4.5);
        }

        // Tick at beat 4.5 (past clip end)
        handler.tick(Beat::from_f64(4.5)).await;

        let state_read = state.read().await;
        let pattern = state_read.patterns.get(&pattern_id).unwrap();
        assert!(
            !pattern.playing,
            "Pattern should be stopped after clip ends"
        );
    }

    // =========================================================================
    // Sequence State Tests
    // =========================================================================

    #[tokio::test]
    async fn test_start_resets_position() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        handler.start(seq_id, false).await.unwrap();

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert_eq!(seq.position, Beat::ZERO, "Start should reset position to 0");
    }

    #[tokio::test]
    async fn test_start_sets_start_beat() {
        let (handler, state) = create_handler();

        // Set current beat first
        {
            let mut state_write = state.write().await;
            state_write.current_beat = Beat::from_f64(10.0);
        }

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();
        handler.start(seq_id, false).await.unwrap();

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert_eq!(
            seq.start_beat,
            Some(Beat::from_f64(10.0)),
            "Start should set start_beat"
        );
    }

    // =========================================================================
    // Multiple Operations Tests
    // =========================================================================

    #[tokio::test]
    async fn test_start_stop_start() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        handler.start(seq_id, true).await.unwrap();
        handler.stop(seq_id).await.unwrap();
        handler.start(seq_id, false).await.unwrap();

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert!(seq.playing);
        // Looping should be reset on new start
        assert!(!seq.looping);
    }

    #[tokio::test]
    async fn test_pause_resume_preserves_state() {
        let (handler, state) = create_handler();

        let seq_id = SequenceId::new(1);
        let config = SequenceConfig::with_length("test_seq", 16.0);
        handler.create(seq_id, config).await.unwrap();

        handler.start(seq_id, true).await.unwrap();

        // Manually set position to simulate playback
        {
            let mut state_write = state.write().await;
            if let Some(seq) = state_write.sequences.get_mut(&seq_id) {
                seq.position = Beat::from_f64(5.0);
            }
        }

        handler.pause(seq_id).await.unwrap();
        handler.resume(seq_id).await.unwrap();

        let state_read = state.read().await;
        let seq = state_read.sequences.get(&seq_id).unwrap();
        assert!(seq.playing);
        assert!(seq.looping);
        // Position should be preserved through pause/resume
        assert_eq!(seq.position.to_f64(), 5.0);
    }
}
