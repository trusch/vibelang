//! Runtime - the main entry point for vibelang-core2.
//!
//! The [`Runtime`] struct is generic over a [`Backend`] and manages all
//! audio state and message processing.
//!
//! # Example
//!
//! ```ignore
//! use vibelang_core2::{Runtime, Message};
//!
//! // Create runtime with a backend
//! let backend = ScsynthBackend::new().await?;
//! let mut runtime = Runtime::new(backend);
//!
//! // Get a handle for sending messages
//! let handle = runtime.handle();
//!
//! // Spawn the runtime loop
//! tokio::spawn(async move {
//!     runtime.run().await;
//! });
//!
//! // Send messages via the handle
//! handle.send(Message::SetTempo { bpm: 128.0 }).await;
//! handle.send(Message::Start).await;
//! ```

use crate::backend::Backend;
use crate::handlers::{
    EffectsHandler, FadesHandler, GroupsHandler, MelodiesHandler, PatternsHandler, RecordingsHandler,
    SamplesHandler, SequencesHandler, SfzHandler, SynthDefsHandler, TransportHandler, VoicesHandler,
};
use crate::message::{
    EffectMessage, FadeMessage, GroupMessage, MelodyMessage, Message, PatternMessage, RecordingMessage,
    ReloadMessage, SampleMessage, SequenceMessage, SfzMessage, SynthDefMessage, TransportMessage, VoiceMessage,
};
#[cfg(feature = "midi")]
use crate::message::MidiMessage;
use crate::reload;
use crate::state::State;
use crate::traits::{
    Effects, Fades, Groups, Melodies, Patterns, Recordings, Samples, Sequences, Sfz, SynthDefs, Transport, Voices,
};
#[cfg(feature = "midi")]
use crate::traits::Midi;
use crate::{Error, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

#[cfg(feature = "midi")]
use crate::handlers::MidiHandler;

/// The VibeLang runtime, generic over a synthesis backend.
///
/// The runtime owns the message channel, state, and all feature handlers.
/// It processes messages from the channel and delegates to the appropriate
/// handler.
///
/// # Usage Patterns
///
/// ## Blocking Loop (native)
///
/// ```ignore
/// let mut runtime = Runtime::new(backend);
/// runtime.run().await; // Blocks until shutdown
/// ```
///
/// ## Non-blocking Tick (WASM/game loop)
///
/// ```ignore
/// let mut runtime = Runtime::new(backend);
/// loop {
///     runtime.tick().await;
///     // ... other game loop work
/// }
/// ```
pub struct Runtime<B: Backend> {
    /// The synthesis backend.
    backend: Arc<B>,

    /// Shared state.
    state: Arc<RwLock<State>>,

    /// Message sender (cloned for handles).
    tx: mpsc::Sender<Message>,

    /// Message receiver.
    rx: mpsc::Receiver<Message>,

    // =========================================================================
    // Feature Handlers
    // =========================================================================
    transport: TransportHandler<B>,
    groups: GroupsHandler<B>,
    voices: VoicesHandler<B>,
    patterns: PatternsHandler<B>,
    melodies: MelodiesHandler<B>,
    sequences: SequencesHandler<B>,
    fades: FadesHandler<B>,
    effects: EffectsHandler<B>,
    samples: SamplesHandler<B>,
    sfz: SfzHandler<B>,
    recordings: RecordingsHandler<B>,
    synthdefs: SynthDefsHandler<B>,

    #[cfg(feature = "midi")]
    midi: MidiHandler<B>,
}

impl<B: Backend> Runtime<B> {
    /// Create a new runtime with the given backend.
    ///
    /// The runtime creates an internal message channel with a buffer of 1024
    /// messages. Use [`Runtime::handle()`] to get a cloneable sender.
    pub fn new(backend: B) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        let backend = Arc::new(backend);
        let state = Arc::new(RwLock::new(State::default()));

        Self {
            backend: backend.clone(),
            state: state.clone(),
            tx,
            rx,
            transport: TransportHandler::new(backend.clone(), state.clone()),
            groups: GroupsHandler::new(backend.clone(), state.clone()),
            voices: VoicesHandler::new(backend.clone(), state.clone()),
            patterns: PatternsHandler::new(backend.clone(), state.clone()),
            melodies: MelodiesHandler::new(backend.clone(), state.clone()),
            sequences: SequencesHandler::new(backend.clone(), state.clone()),
            fades: FadesHandler::new(backend.clone(), state.clone()),
            effects: EffectsHandler::new(backend.clone(), state.clone()),
            samples: SamplesHandler::new(backend.clone(), state.clone()),
            sfz: SfzHandler::new(backend.clone(), state.clone()),
            recordings: RecordingsHandler::new(backend.clone(), state.clone()),
            synthdefs: SynthDefsHandler::new(backend.clone(), state.clone()),
            #[cfg(feature = "midi")]
            midi: MidiHandler::new(backend.clone(), state.clone()),
        }
    }

    /// Get a cloneable handle for sending messages.
    ///
    /// Handles can be sent across threads and cloned freely.
    pub fn handle(&self) -> RuntimeHandle {
        RuntimeHandle {
            tx: self.tx.clone(),
        }
    }

    /// Send a message directly (convenience method).
    ///
    /// Equivalent to `runtime.handle().send(msg).await`.
    pub async fn send(&self, msg: Message) -> Result<()> {
        self.tx.send(msg).await.map_err(|_| Error::ChannelClosed)
    }

    /// Run the main loop until the channel is closed.
    ///
    /// This is the primary way to run the runtime on native platforms.
    /// The loop processes messages and ticks handlers at regular intervals.
    pub async fn run(&mut self) {
        let tick_interval = Duration::from_millis(10); // 100 Hz

        loop {
            // Process available messages
            while let Ok(msg) = self.rx.try_recv() {
                if let Err(e) = self.handle_message(msg).await {
                    tracing::warn!("Message handling error: {}", e);
                }
            }

            // Tick handlers
            self.tick_internal().await;

            // Wait for next message or timeout
            match tokio::time::timeout(tick_interval, self.rx.recv()).await {
                Ok(Some(msg)) => {
                    if let Err(e) = self.handle_message(msg).await {
                        tracing::warn!("Message handling error: {}", e);
                    }
                }
                Ok(None) => {
                    // Channel closed, exit
                    tracing::info!("Runtime channel closed, shutting down");
                    break;
                }
                Err(_) => {
                    // Timeout, continue loop
                }
            }
        }
    }

    /// Process pending messages (non-blocking).
    ///
    /// Use this in WASM or game loops where you control the main loop.
    pub async fn tick(&mut self) {
        // Process all pending messages
        while let Ok(msg) = self.rx.try_recv() {
            if let Err(e) = self.handle_message(msg).await {
                tracing::warn!("Message handling error: {}", e);
            }
        }

        // Tick handlers
        self.tick_internal().await;
    }

    /// Internal tick for handlers.
    async fn tick_internal(&mut self) {
        let now = Instant::now();

        // Tick transport (updates current beat from clock)
        self.transport.tick(now).await;

        // Get current beat for scheduling
        let current_beat = {
            let state = self.state.read().await;
            state.current_beat
        };

        // Tick schedulers
        self.patterns.tick(current_beat).await;
        self.melodies.tick(current_beat).await;
        self.sequences.tick(current_beat).await;

        // Tick recordings (start/stop based on beat position)
        if let Err(e) = self.recordings.tick(current_beat).await {
            tracing::warn!("Recording tick error: {}", e);
        }

        // Tick fades
        self.fades.tick().await;

        // Tick MIDI (process incoming messages)
        #[cfg(feature = "midi")]
        self.midi.tick().await;
    }

    /// Handle a single message.
    async fn handle_message(&mut self, msg: Message) -> Result<()> {
        tracing::trace!("Handling message: {}", msg.type_name());

        match msg {
            // Transport
            Message::Transport(transport_msg) => match transport_msg {
                TransportMessage::SetTempo { bpm } => self.transport.set_tempo(bpm).await,
                TransportMessage::SetTimeSignature { time_sig } => {
                    self.transport.set_time_signature(time_sig).await
                }
                TransportMessage::Seek { beat } => self.transport.seek(beat).await,
                TransportMessage::Start => self.transport.start().await,
                TransportMessage::Stop => self.transport.stop().await,
            },

            // SynthDefs
            Message::SynthDef(synthdef_msg) => match synthdef_msg {
                SynthDefMessage::Load { name, data } => self.synthdefs.load(&name, &data).await,
            },

            // Samples
            Message::Sample(sample_msg) => match sample_msg {
                SampleMessage::Load { id, config } => {
                    self.samples.load(id, config).await?;
                    Ok(())
                }
                SampleMessage::Free { id } => self.samples.unload(id).await,
            },

            // Groups
            Message::Group(group_msg) => match group_msg {
                GroupMessage::Create { id, parent } => self.groups.create(id, parent).await,
                GroupMessage::Delete { id } => self.groups.delete(id).await,
                GroupMessage::SetParam { id, param, value } => {
                    self.groups.set_param(id, &param, value).await
                }
                GroupMessage::Mute { id, muted } => self.groups.mute(id, muted).await,
                GroupMessage::Solo { id, solo } => self.groups.solo(id, solo).await,
                GroupMessage::Finalize => self.groups.finalize().await,
            },

            // Voices
            Message::Voice(voice_msg) => match voice_msg {
                VoiceMessage::Create { id, config } => self.voices.create(id, config).await,
                VoiceMessage::Delete { id } => self.voices.delete(id).await,
                VoiceMessage::Trigger { id, params } => self.voices.trigger(id, &params).await,
                VoiceMessage::Stop { id } => self.voices.stop(id).await,
                VoiceMessage::NoteOn {
                    voice,
                    note,
                    velocity,
                } => self.voices.note_on(voice, note, velocity).await,
                VoiceMessage::NoteOff { voice, note } => self.voices.note_off(voice, note).await,
                VoiceMessage::Mute { id, muted } => self.voices.mute(id, muted).await,
                VoiceMessage::SetParam { id, param, value } => {
                    self.voices.set_param(id, &param, value).await
                }
            },

            // Patterns
            Message::Pattern(pattern_msg) => match pattern_msg {
                PatternMessage::Create { id, config } => self.patterns.create(id, config).await,
                PatternMessage::Delete { id } => self.patterns.delete(id).await,
                PatternMessage::Start { id } => self.patterns.start(id).await,
                PatternMessage::Stop { id } => self.patterns.stop(id).await,
                PatternMessage::SetParam { id, param, value } => {
                    self.patterns.set_param(id, &param, value).await
                }
            },

            // Melodies
            Message::Melody(melody_msg) => match melody_msg {
                MelodyMessage::Create { id, config } => self.melodies.create(id, config).await,
                MelodyMessage::Delete { id } => self.melodies.delete(id).await,
                MelodyMessage::Start { id } => self.melodies.start(id).await,
                MelodyMessage::Stop { id } => self.melodies.stop(id).await,
            },

            // Sequences
            Message::Sequence(sequence_msg) => match sequence_msg {
                SequenceMessage::Create { id, config } => self.sequences.create(id, config).await,
                SequenceMessage::Delete { id } => self.sequences.delete(id).await,
                SequenceMessage::Start { id, looping } => self.sequences.start(id, looping).await,
                SequenceMessage::Stop { id } => self.sequences.stop(id).await,
                SequenceMessage::Pause { id } => self.sequences.pause(id).await,
                SequenceMessage::Resume { id } => self.sequences.resume(id).await,
            },

            // Effects
            Message::Effect(effect_msg) => match effect_msg {
                EffectMessage::Add {
                    id,
                    group,
                    synthdef,
                    params,
                } => self.effects.add(id, group, &synthdef, &params).await,
                EffectMessage::Remove { id } => self.effects.remove(id).await,
                EffectMessage::SetParam { id, param, value } => {
                    self.effects.set_param(id, &param, value).await
                }
            },

            // Fades
            Message::Fade(fade_msg) => match fade_msg {
                FadeMessage::Start { config } => self.fades.fade(config).await,
                FadeMessage::Cancel { target, param } => self.fades.cancel(&target, &param).await,
            },

            // Reload - live hot-reloading
            Message::Reload(reload_msg) => match *reload_msg {
                ReloadMessage::Apply { state: new_state } => {
                    self.apply_reload(new_state).await
                }
            },

            // MIDI - send to external devices
            #[cfg(feature = "midi")]
            Message::Midi(midi_msg) => match midi_msg {
                MidiMessage::NoteOn {
                    device,
                    channel,
                    note,
                    velocity,
                } => self.midi.send_note_on(device, channel, note, velocity).await,
                MidiMessage::NoteOff {
                    device,
                    channel,
                    note,
                } => self.midi.send_note_off(device, channel, note).await,
                MidiMessage::Cc {
                    device,
                    channel,
                    cc,
                    value,
                } => self.midi.send_cc(device, channel, cc, value).await,
            },

            // SFZ - instrument loading
            Message::Sfz(sfz_msg) => match sfz_msg {
                SfzMessage::Load { id, path } => self.sfz.load(id, &path).await,
                SfzMessage::Unload { id } => self.sfz.unload(id).await,
            },

            // Recording - audio capture
            Message::Recording(recording_msg) => match recording_msg {
                RecordingMessage::Start { id, config } => {
                    self.recordings.start(id, config).await?;
                    Ok(())
                }
                RecordingMessage::Stop { id } => self.recordings.stop(id).await,
                RecordingMessage::Cancel { id } => self.recordings.cancel(id).await,
                RecordingMessage::BufferAllocated { id } => self.recordings.buffer_allocated(id).await,
                RecordingMessage::Completed { id } => {
                    // This is handled internally by tick(), but can also be triggered externally
                    tracing::debug!("Recording {} completed via message", id.0);
                    Ok(())
                }
            },
        }
    }

    /// Get read-only access to the state.
    pub fn state(&self) -> &Arc<RwLock<State>> {
        &self.state
    }

    /// Get a reference to the backend.
    pub fn backend(&self) -> &Arc<B> {
        &self.backend
    }

    /// Load all built-in synthdefs from vibelang-dsp.
    ///
    /// This should be called during initialization to ensure essential
    /// synthdefs are available for sample playback, SFZ instruments,
    /// MIDI output, recording, and audio routing.
    pub async fn load_builtins(&self) -> Result<()> {
        self.synthdefs.load_builtins().await
    }

    /// Apply a live reload from a new script state.
    ///
    /// This calculates the minimal diff between current and new state,
    /// then applies changes in the correct order:
    /// 1. Stop patterns/melodies/sequences
    /// 2. Delete entities (children before parents for groups)
    /// 3. Update tempo/time signature
    /// 4. Create entities (parents before children for groups)
    /// 5. Restart patterns/melodies/sequences
    async fn apply_reload(&mut self, new_state: reload::ScriptState) -> Result<()> {
        // Calculate diff
        let diff = {
            let current = self.state.read().await;
            reload::calculate_diff(&current, &new_state)
        };

        if !diff.has_changes() {
            tracing::debug!("Reload: no changes detected");
            return Ok(());
        }

        tracing::info!("Reload: applying changes");

        // Apply tempo change if needed
        if let Some(bpm) = diff.tempo_changed {
            tracing::debug!("Reload: changing tempo to {}", bpm);
            self.transport.set_tempo(bpm).await?;
        }

        // Apply time signature change if needed
        if let Some(time_sig) = diff.time_sig_changed {
            tracing::debug!("Reload: changing time signature to {}/{}", time_sig.numerator, time_sig.denominator);
            self.transport.set_time_signature(time_sig).await?;
        }

        // =========================================================================
        // Phase 1: Stop all patterns/melodies/sequences that will be modified
        // =========================================================================

        // Stop patterns that will be deleted or updated
        for id in &diff.patterns.deleted {
            let _ = self.patterns.stop(*id).await;
        }
        for id in diff.patterns.updated.keys() {
            let _ = self.patterns.stop(*id).await;
        }

        // Stop melodies that will be deleted or updated
        for id in &diff.melodies.deleted {
            let _ = self.melodies.stop(*id).await;
        }
        for id in diff.melodies.updated.keys() {
            let _ = self.melodies.stop(*id).await;
        }

        // Stop sequences that will be deleted or updated
        for id in &diff.sequences.deleted {
            let _ = self.sequences.stop(*id).await;
        }
        for id in diff.sequences.updated.keys() {
            let _ = self.sequences.stop(*id).await;
        }

        // =========================================================================
        // Phase 2: Delete entities (children before parents for groups)
        // =========================================================================

        // Delete effects first (they depend on groups)
        for id in &diff.effects.deleted {
            tracing::debug!("Reload: deleting effect {:?}", id);
            let _ = self.effects.remove(*id).await;
        }

        // Delete patterns
        for id in &diff.patterns.deleted {
            tracing::debug!("Reload: deleting pattern {:?}", id);
            let _ = self.patterns.delete(*id).await;
        }

        // Delete melodies
        for id in &diff.melodies.deleted {
            tracing::debug!("Reload: deleting melody {:?}", id);
            let _ = self.melodies.delete(*id).await;
        }

        // Delete sequences
        for id in &diff.sequences.deleted {
            tracing::debug!("Reload: deleting sequence {:?}", id);
            let _ = self.sequences.delete(*id).await;
        }

        // Delete voices (before groups they belong to)
        for id in &diff.voices.deleted {
            tracing::debug!("Reload: deleting voice {:?}", id);
            let _ = self.voices.delete(*id).await;
        }

        // Delete groups in correct order (children first)
        let ordered_group_deletions = {
            let state = self.state.read().await;
            reload::order_group_deletions(&state.groups, &diff.groups.deleted)
        };
        for id in ordered_group_deletions {
            tracing::debug!("Reload: deleting group {:?}", id);
            let _ = self.groups.delete(id).await;
        }

        // Delete samples
        for id in &diff.samples.deleted {
            tracing::debug!("Reload: deleting sample {:?}", id);
            let _ = self.samples.unload(*id).await;
        }

        // =========================================================================
        // Phase 3: Create new entities (parents before children for groups)
        // =========================================================================

        // Load new samples first (other entities may depend on them)
        for (id, config) in &diff.samples.created {
            tracing::debug!("Reload: loading sample {:?}", id);
            let _ = self.samples.load(*id, config.clone()).await;
        }

        // Create groups in correct order (parents first)
        let ordered_group_creations = reload::order_group_creations(&diff.groups.created);
        for id in ordered_group_creations {
            if let Some(config) = diff.groups.created.get(&id) {
                tracing::debug!("Reload: creating group {:?}", id);
                let _ = self.groups.create(id, config.parent).await;
                // Apply initial params
                for (param, value) in &config.params {
                    let _ = self.groups.set_param(id, param, *value).await;
                }
            }
        }

        // Create new voices
        for (id, config) in &diff.voices.created {
            tracing::debug!("Reload: creating voice {:?}", id);
            let _ = self.voices.create(*id, config.clone()).await;
        }

        // Create new patterns
        for (id, config) in &diff.patterns.created {
            tracing::debug!("Reload: creating pattern {:?}", id);
            let _ = self.patterns.create(*id, config.clone()).await;
        }

        // Create new melodies
        for (id, config) in &diff.melodies.created {
            tracing::debug!("Reload: creating melody {:?}", id);
            let _ = self.melodies.create(*id, config.clone()).await;
        }

        // Create new sequences
        for (id, config) in &diff.sequences.created {
            tracing::debug!("Reload: creating sequence {:?}", id);
            let _ = self.sequences.create(*id, config.clone()).await;
        }

        // Create new effects
        for (id, config) in &diff.effects.created {
            tracing::debug!("Reload: creating effect {:?}", id);
            let _ = self.effects.add(*id, config.group, &config.synthdef, &config.params).await;
        }

        // =========================================================================
        // Phase 4: Update existing entities
        // =========================================================================

        // Update groups (params only, parent changes not supported during reload)
        for (id, new_config) in &diff.groups.updated {
            // Apply all params from the new config
            for (param, value) in &new_config.params {
                tracing::debug!("Reload: updating group {:?} param {} to {}", id, param, value);
                let _ = self.groups.set_param(*id, param, *value).await;
            }
        }

        // Update voices - recreate with new config
        for (id, config) in &diff.voices.updated {
            tracing::debug!("Reload: updating voice {:?}", id);
            // Delete and recreate
            let _ = self.voices.delete(*id).await;
            let _ = self.voices.create(*id, config.clone()).await;
        }

        // Update patterns - recreate with new config
        for (id, config) in &diff.patterns.updated {
            tracing::debug!("Reload: updating pattern {:?}", id);
            let _ = self.patterns.delete(*id).await;
            let _ = self.patterns.create(*id, config.clone()).await;
        }

        // Update melodies - recreate with new config
        for (id, config) in &diff.melodies.updated {
            tracing::debug!("Reload: updating melody {:?}", id);
            let _ = self.melodies.delete(*id).await;
            let _ = self.melodies.create(*id, config.clone()).await;
        }

        // Update sequences - recreate with new config
        for (id, config) in &diff.sequences.updated {
            tracing::debug!("Reload: updating sequence {:?}", id);
            let _ = self.sequences.delete(*id).await;
            let _ = self.sequences.create(*id, config.clone()).await;
        }

        // Update effects - apply all params from new config
        for (id, new_config) in &diff.effects.updated {
            for (param, value) in &new_config.params {
                tracing::debug!("Reload: updating effect {:?} param {} to {}", id, param, value);
                let _ = self.effects.set_param(*id, param, *value).await;
            }
        }

        // =========================================================================
        // Phase 5: Finalize groups (create link synths)
        // =========================================================================

        // Only finalize if we created new groups
        if !diff.groups.created.is_empty() {
            tracing::debug!("Reload: finalizing groups");
            let _ = self.groups.finalize().await;
        }

        // =========================================================================
        // Phase 6: Apply MIDI routes from script state
        // =========================================================================

        #[cfg(feature = "midi")]
        {
            if !new_state.midi_cc_routes.is_empty() {
                tracing::debug!(
                    "Reload: applying {} MIDI CC routes",
                    new_state.midi_cc_routes.len()
                );
                self.midi.apply_cc_routes(&new_state.midi_cc_routes).await;
            }
        }

        tracing::info!("Reload: complete");
        Ok(())
    }
}

/// A cloneable handle for sending messages to the runtime.
///
/// Handles are cheap to clone and can be shared across threads.
#[derive(Clone)]
pub struct RuntimeHandle {
    tx: mpsc::Sender<Message>,
}

impl RuntimeHandle {
    /// Send a message to the runtime.
    ///
    /// Returns an error if the runtime has been dropped.
    pub async fn send(&self, msg: Message) -> Result<()> {
        self.tx.send(msg).await.map_err(|_| Error::ChannelClosed)
    }

    /// Try to send a message without waiting.
    ///
    /// Returns an error if the channel is full or closed.
    pub fn try_send(&self, msg: Message) -> Result<()> {
        self.tx.try_send(msg).map_err(|_| Error::ChannelClosed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{AddAction, BufferInfo};
    use crate::types::{BufferId, NodeId, ParamMap};
    use std::path::Path;

    /// A mock backend error.
    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock error")
        }
    }

    impl std::error::Error for MockError {}

    /// A mock backend for testing.
    struct MockBackend;

    #[async_trait::async_trait]
    impl Backend for MockBackend {
        type Error = MockError;

        async fn load_synthdef(&self, _name: &str, _data: &[u8]) -> std::result::Result<(), Self::Error> {
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

        async fn run_node(&self, _node: NodeId, _running: bool) -> std::result::Result<(), Self::Error> {
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

        async fn write_buffer(&self, _id: BufferId, _path: &Path) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_buffer(&self, _id: BufferId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        fn current_time(&self) -> Instant {
            Instant::now()
        }
    }

    #[tokio::test]
    async fn test_runtime_creation() {
        let runtime = Runtime::new(MockBackend);
        assert!(!runtime.transport.is_playing().await);
    }

    #[tokio::test]
    async fn test_runtime_handle() {
        let runtime = Runtime::new(MockBackend);
        let handle = runtime.handle();

        // Can send messages
        handle
            .send(Message::Transport(TransportMessage::SetTempo { bpm: 140.0 }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_runtime_tick() {
        let mut runtime = Runtime::new(MockBackend);

        // Send a message
        runtime
            .send(Message::Transport(TransportMessage::SetTempo { bpm: 140.0 }))
            .await
            .unwrap();

        // Tick to process it
        runtime.tick().await;

        // Tempo should be updated
        let state = runtime.state.read().await;
        assert!((state.tempo - 140.0).abs() < 0.001);
    }

    // =========================================================================
    // Integration Tests
    // =========================================================================

    #[tokio::test]
    async fn test_group_creation_and_deletion() {
        use crate::message::GroupMessage;
        use crate::types::GroupId;

        let mut runtime = Runtime::new(MockBackend);

        // Create a group
        runtime
            .send(GroupMessage::Create { id: GroupId::new(1), parent: None }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify group exists
        {
            let state = runtime.state.read().await;
            assert!(state.groups.contains_key(&GroupId::new(1)));
        }

        // Delete the group
        runtime
            .send(GroupMessage::Delete { id: GroupId::new(1) }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify group is gone
        {
            let state = runtime.state.read().await;
            assert!(!state.groups.contains_key(&GroupId::new(1)));
        }
    }

    #[tokio::test]
    async fn test_voice_creation_and_triggering() {
        use crate::message::{GroupMessage, VoiceMessage};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};

        let mut runtime = Runtime::new(MockBackend);

        // First create a group
        runtime
            .send(GroupMessage::Create { id: GroupId::new(1), parent: None }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Create a voice
        let config = VoiceConfig {
            synthdef: "simple_sine".to_string(),
            group: GroupId::new(1),
            polyphony: 8,
            params: ParamMap::new(),
            muted: false,
            soloed: false,
            sfz_instrument: None,
            round_robin_count: 0,
            choke_group: None,
            #[cfg(feature = "midi")]
            midi_output: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
        };
        runtime
            .send(VoiceMessage::Create { id: VoiceId::new(1), config }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify voice exists
        {
            let state = runtime.state.read().await;
            assert!(state.voices.contains_key(&VoiceId::new(1)));
        }

        // Trigger the voice
        runtime
            .send(VoiceMessage::Trigger { id: VoiceId::new(1), params: ParamMap::new() }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Delete voice
        runtime
            .send(VoiceMessage::Delete { id: VoiceId::new(1) }.into())
            .await
            .unwrap();
        runtime.tick().await;
    }

    #[tokio::test]
    async fn test_transport_start_stop() {
        let mut runtime = Runtime::new(MockBackend);

        // Start transport
        runtime
            .send(TransportMessage::Start.into())
            .await
            .unwrap();
        runtime.tick().await;

        assert!(runtime.transport.is_playing().await);

        // Stop transport
        runtime
            .send(TransportMessage::Stop.into())
            .await
            .unwrap();
        runtime.tick().await;

        assert!(!runtime.transport.is_playing().await);
    }

    #[tokio::test]
    async fn test_tempo_change() {
        let mut runtime = Runtime::new(MockBackend);

        // Change tempo
        runtime
            .send(TransportMessage::SetTempo { bpm: 180.0 }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let tempo = runtime.transport.tempo().await;
        assert!((tempo - 180.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_nested_groups() {
        use crate::message::GroupMessage;
        use crate::types::GroupId;

        let mut runtime = Runtime::new(MockBackend);

        // Create parent group
        runtime
            .send(GroupMessage::Create { id: GroupId::new(1), parent: None }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Create child group
        runtime
            .send(GroupMessage::Create { id: GroupId::new(2), parent: Some(GroupId::new(1)) }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify both exist with correct parent relationship
        {
            let state = runtime.state.read().await;
            assert!(state.groups.contains_key(&GroupId::new(1)));
            assert!(state.groups.contains_key(&GroupId::new(2)));

            let child = state.groups.get(&GroupId::new(2)).unwrap();
            assert_eq!(child.parent, Some(GroupId::new(1)));
        }
    }

    #[tokio::test]
    async fn test_pattern_lifecycle() {
        use crate::message::{GroupMessage, PatternMessage, VoiceMessage};
        use crate::traits::{PatternConfig, VoiceConfig};
        use crate::types::{Beat, GroupId, PatternId, VoiceId};

        let mut runtime = Runtime::new(MockBackend);

        // Setup: create group and voice
        runtime.send(GroupMessage::Create { id: GroupId::new(1), parent: None }.into()).await.unwrap();
        runtime.send(VoiceMessage::Create {
            id: VoiceId::new(1),
            config: VoiceConfig {
                synthdef: "test".to_string(),
                group: GroupId::new(1),
                polyphony: 4,
                params: ParamMap::new(),
                muted: false,
                soloed: false,
                sfz_instrument: None,
                round_robin_count: 0,
                choke_group: None,
                #[cfg(feature = "midi")]
                midi_output: None,
                #[cfg(feature = "midi")]
                midi_channel: 0,
            }
        }.into()).await.unwrap();
        runtime.tick().await;

        // Create pattern
        let config = PatternConfig {
            voice: Some(VoiceId::new(1)),
            steps: vec![],
            length: Beat::from_f64(4.0),
            swing: 0.0,
        };
        runtime.send(PatternMessage::Create { id: PatternId::new(1), config }.into()).await.unwrap();
        runtime.tick().await;

        // Verify pattern exists
        {
            let state = runtime.state.read().await;
            assert!(state.patterns.contains_key(&PatternId::new(1)));
        }

        // Start pattern
        runtime.send(PatternMessage::Start { id: PatternId::new(1) }.into()).await.unwrap();
        runtime.tick().await;

        // Stop pattern
        runtime.send(PatternMessage::Stop { id: PatternId::new(1) }.into()).await.unwrap();
        runtime.tick().await;

        // Delete pattern
        runtime.send(PatternMessage::Delete { id: PatternId::new(1) }.into()).await.unwrap();
        runtime.tick().await;

        {
            let state = runtime.state.read().await;
            assert!(!state.patterns.contains_key(&PatternId::new(1)));
        }
    }

    #[tokio::test]
    async fn test_handle_clone() {
        let runtime = Runtime::new(MockBackend);
        let handle1 = runtime.handle();
        let handle2 = handle1.clone();

        // Both handles should work
        handle1.send(TransportMessage::SetTempo { bpm: 120.0 }.into()).await.unwrap();
        handle2.send(TransportMessage::SetTempo { bpm: 130.0 }.into()).await.unwrap();
    }

    #[tokio::test]
    async fn test_try_send() {
        let runtime = Runtime::new(MockBackend);
        let handle = runtime.handle();

        // try_send should work when channel has space
        handle.try_send(TransportMessage::Start.into()).unwrap();
    }

    #[tokio::test]
    async fn test_state_access() {
        let runtime = Runtime::new(MockBackend);

        // Should be able to read state
        let state = runtime.state().read().await;
        assert_eq!(state.tempo, 120.0); // Default tempo
    }

    #[tokio::test]
    async fn test_backend_access() {
        let runtime = Runtime::new(MockBackend);

        // Should be able to access backend
        let _backend = runtime.backend();
    }

    #[tokio::test]
    async fn test_group_solo() {
        use crate::message::GroupMessage;
        use crate::types::GroupId;

        let mut runtime = Runtime::new(MockBackend);

        // Create two groups
        runtime
            .send(GroupMessage::Create { id: GroupId::new(1), parent: None }.into())
            .await
            .unwrap();
        runtime
            .send(GroupMessage::Create { id: GroupId::new(2), parent: None }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Solo group 1
        runtime
            .send(GroupMessage::Solo { id: GroupId::new(1), solo: true }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify group 1 is soloed
        {
            let state = runtime.state.read().await;
            let group1 = state.groups.get(&GroupId::new(1)).unwrap();
            let group2 = state.groups.get(&GroupId::new(2)).unwrap();
            assert!(group1.soloed);
            assert!(!group2.soloed);
        }

        // Unsolo group 1
        runtime
            .send(GroupMessage::Solo { id: GroupId::new(1), solo: false }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify no groups are soloed
        {
            let state = runtime.state.read().await;
            let group1 = state.groups.get(&GroupId::new(1)).unwrap();
            assert!(!group1.soloed);
        }
    }

    #[tokio::test]
    async fn test_group_mute() {
        use crate::message::GroupMessage;
        use crate::types::GroupId;

        let mut runtime = Runtime::new(MockBackend);

        // Create a group
        runtime
            .send(GroupMessage::Create { id: GroupId::new(1), parent: None }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Mute the group
        runtime
            .send(GroupMessage::Mute { id: GroupId::new(1), muted: true }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify group is muted
        {
            let state = runtime.state.read().await;
            let group = state.groups.get(&GroupId::new(1)).unwrap();
            assert!(group.muted);
        }

        // Unmute the group
        runtime
            .send(GroupMessage::Mute { id: GroupId::new(1), muted: false }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify group is unmuted
        {
            let state = runtime.state.read().await;
            let group = state.groups.get(&GroupId::new(1)).unwrap();
            assert!(!group.muted);
        }
    }

    #[tokio::test]
    async fn test_reload_creates_groups() {
        use crate::message::ReloadMessage;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::types::GroupId;

        let mut runtime = Runtime::new(MockBackend);

        // Create a ScriptState with a new group
        let mut new_state = ScriptState::new().with_tempo(140.0);
        new_state.add_group(GroupId::new(1), GroupConfig::default());

        // Send reload message
        runtime
            .send(ReloadMessage::Apply { state: new_state }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify tempo changed
        let tempo = runtime.transport.tempo().await;
        assert!((tempo - 140.0).abs() < 0.001);

        // Verify group was created
        {
            let state = runtime.state.read().await;
            assert!(state.groups.contains_key(&GroupId::new(1)));
        }
    }

    #[tokio::test]
    async fn test_reload_updates_group_params() {
        use crate::message::{GroupMessage, ReloadMessage};
        use crate::reload::{GroupConfig, ScriptState};
        use crate::types::{GroupId, ParamMap};

        let mut runtime = Runtime::new(MockBackend);

        // First create a group normally
        runtime
            .send(GroupMessage::Create { id: GroupId::new(1), parent: None }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Now reload with updated params
        let mut new_state = ScriptState::new();
        let mut params = ParamMap::new();
        params.insert("amp".to_string(), 0.5);
        new_state.add_group(GroupId::new(1), GroupConfig {
            name: "test".to_string(),
            parent: None,
            params,
            effects: Vec::new(),
            muted: false,
            soloed: false,
        });

        runtime
            .send(ReloadMessage::Apply { state: new_state }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify group still exists and params were updated
        {
            let state = runtime.state.read().await;
            let group = state.groups.get(&GroupId::new(1)).unwrap();
            assert_eq!(group.params.get("amp"), Some(&0.5));
        }
    }

    #[tokio::test]
    async fn test_reload_deletes_groups() {
        use crate::message::{GroupMessage, ReloadMessage};
        use crate::reload::ScriptState;
        use crate::types::GroupId;

        let mut runtime = Runtime::new(MockBackend);

        // First create a group
        runtime
            .send(GroupMessage::Create { id: GroupId::new(1), parent: None }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify group exists
        {
            let state = runtime.state.read().await;
            assert!(state.groups.contains_key(&GroupId::new(1)));
        }

        // Reload with empty state (group should be deleted)
        let new_state = ScriptState::new();
        runtime
            .send(ReloadMessage::Apply { state: new_state }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify group was deleted
        {
            let state = runtime.state.read().await;
            assert!(!state.groups.contains_key(&GroupId::new(1)));
        }
    }

    #[tokio::test]
    async fn test_reload_no_changes() {
        use crate::message::ReloadMessage;
        use crate::reload::ScriptState;

        let mut runtime = Runtime::new(MockBackend);

        // Reload with default state (should be no-op)
        let new_state = ScriptState::new(); // Default tempo 120
        runtime
            .send(ReloadMessage::Apply { state: new_state }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Verify tempo is still 120 (default)
        let tempo = runtime.transport.tempo().await;
        assert!((tempo - 120.0).abs() < 0.001);
    }
}
