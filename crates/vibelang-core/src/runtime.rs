//! Runtime - the main entry point for vibelang-core2.
//!
//! The [`Runtime`] struct is generic over a [`Backend`] and manages all
//! audio state and message processing.
//!
//! # Example
//!
//! ```ignore
//! use vibelang_core::{Runtime, Message};
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
use crate::compat::{channel, Instant, Receiver, ReceiverExt, RwLock, Sender, SenderExt};
#[cfg(not(target_arch = "wasm32"))]
use crate::compat::{timeout, Duration};
#[cfg(not(target_arch = "wasm32"))]
use crate::handlers::RecordingsHandler;
use crate::handlers::{
    merge_default_routes, EffectsHandler, FadesHandler, GroupsHandler, MelodiesHandler,
    ModulatorsHandler, PatternsHandler, RouteMap, RoutesHandler, SamplesHandler, SequencesHandler,
    SfzHandler, SynthDefsHandler, TransportHandler, VoicesHandler,
};
#[cfg(feature = "midi")]
use crate::message::MidiMessage;
#[cfg(not(target_arch = "wasm32"))]
use crate::message::RecordingMessage;
use crate::message::ReloadMessage;
use crate::message::{
    EffectMessage, FadeMessage, GroupMessage, MelodyMessage, Message, ModulatorMessage,
    PatternMessage, SampleMessage, SequenceMessage, SfzMessage, SyncMessage, SynthDefMessage,
    TransportMessage, VoiceMessage,
};
#[cfg(feature = "midi")]
use crate::midi::{QueuedMidiEvent, ScheduledMidiEvent};
use crate::reload;
#[cfg(feature = "midi")]
use crate::reload::MidiOutputMessage;
use crate::state::State;
#[cfg(feature = "midi")]
use crate::traits::Midi;
#[cfg(not(target_arch = "wasm32"))]
use crate::traits::Recordings;
use crate::traits::{
    Effects, Fades, Groups, Melodies, Modulators, Patterns, Samples, Sequences, Sfz, SynthDefs,
    Transport, Voices,
};
use crate::transport_snapshot::TransportSnapshot;
use crate::{Error, Result};
use std::sync::Arc;

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
    tx: Sender<Message>,

    /// Message receiver.
    rx: Receiver<Message>,

    /// Transport state snapshot for lock-free sharing with background threads.
    /// Used by MIDI clock thread and modulator polling thread.
    transport_snapshot: Arc<TransportSnapshot>,

    /// Tick counter for reducing frequency of some operations.
    #[cfg(feature = "midi")]
    tick_count: u32,

    /// Last applied per-voice routing map.
    ///
    /// Updated at the end of each [`Self::apply_reload`] so the next reload
    /// can produce a `RouteDiff` against this baseline. Mirrors the
    /// `ScriptState::routes` shape; populated only when the Rhai surface
    /// (Story 8) starts emitting routes.
    current_routes: RouteMap,

    /// Whether the MIDI clock thread has been started (for tick() users).
    #[cfg(feature = "midi")]
    clock_thread_started: bool,

    // =========================================================================
    // Feature Handlers
    // =========================================================================
    transport: TransportHandler<B>,
    groups: GroupsHandler<B>,
    voices: Arc<VoicesHandler<B>>,
    patterns: PatternsHandler<B>,
    melodies: MelodiesHandler<B>,
    sequences: SequencesHandler<B>,
    fades: FadesHandler<B>,
    effects: EffectsHandler<B>,
    modulators: ModulatorsHandler<B>,
    samples: SamplesHandler<B>,
    sfz: SfzHandler<B>,
    /// Per-voice output routing — turns voice port audio buses into mixer
    /// synths feeding their destination group's bus.
    ///
    /// `routes.finalize(diff)` is invoked from [`Self::apply_reload`] between
    /// the voice creation/update phase and `groups.finalize()` so that the
    /// mixer synths sit between voices and the group link synth in SC tree
    /// order (voices → routes → effects → link synth → main).
    routes: RoutesHandler<B>,
    #[cfg(not(target_arch = "wasm32"))]
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
        let (tx, rx) = channel(1024);
        let backend = Arc::new(backend);
        let state = Arc::new(RwLock::new(State::default()));
        let transport_snapshot = Arc::new(TransportSnapshot::new());

        // Create MIDI handler first so we can share output channels with voices
        #[cfg(feature = "midi")]
        let midi = MidiHandler::new(backend.clone(), state.clone(), tx.clone());

        // Create voices handler and connect MIDI outputs
        let mut voices_handler = VoicesHandler::new(backend.clone(), state.clone());
        #[cfg(feature = "midi")]
        voices_handler.set_midi_outputs(midi.output_channels());
        let voices = Arc::new(voices_handler);

        Self {
            backend: backend.clone(),
            state: state.clone(),
            tx,
            rx,
            transport_snapshot,
            transport: TransportHandler::new(backend.clone(), state.clone()),
            groups: GroupsHandler::new(backend.clone(), state.clone()),
            voices: voices.clone(),
            patterns: PatternsHandler::new(state.clone(), voices.clone()),
            melodies: MelodiesHandler::new(state.clone(), voices.clone()),
            sequences: SequencesHandler::new(backend.clone(), state.clone()),
            fades: FadesHandler::new(backend.clone(), state.clone()),
            effects: EffectsHandler::new(backend.clone(), state.clone()),
            modulators: ModulatorsHandler::new(backend.clone(), state.clone()),
            samples: SamplesHandler::new(backend.clone(), state.clone()),
            sfz: SfzHandler::new(backend.clone(), state.clone()),
            routes: RoutesHandler::new(backend.clone(), state.clone()),
            #[cfg(not(target_arch = "wasm32"))]
            recordings: RecordingsHandler::new(backend.clone(), state.clone()),
            synthdefs: SynthDefsHandler::new(backend.clone(), state.clone()),
            #[cfg(feature = "midi")]
            midi,
            #[cfg(feature = "midi")]
            tick_count: 0,
            #[cfg(feature = "midi")]
            clock_thread_started: false,
            current_routes: RouteMap::new(),
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
        self.tx
            .send_async(msg)
            .await
            .map_err(|_| Error::ChannelClosed)
    }

    /// Get the transport snapshot for sharing with background threads.
    ///
    /// The snapshot provides lock-free read access to transport state
    /// (beat position, tempo, playing status).
    pub fn transport_snapshot(&self) -> Arc<TransportSnapshot> {
        Arc::clone(&self.transport_snapshot)
    }

    /// Run the main loop until the channel is closed.
    ///
    /// This is the primary way to run the runtime on native platforms.
    /// The loop processes messages and ticks handlers at regular intervals.
    ///
    /// Note: This method is only available on native platforms. For WASM,
    /// use [`Runtime::tick()`] in a requestAnimationFrame loop.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run(&mut self) {
        let tick_interval = Duration::from_millis(2); // 500 Hz for tighter MIDI timing

        // Start MIDI clock thread (runs independently at 1kHz)
        #[cfg(feature = "midi")]
        {
            self.midi
                .start_clock_thread(Arc::clone(&self.transport_snapshot));
            self.clock_thread_started = true;
        }

        loop {
            // Process available messages
            while let Some(msg) = self.rx.try_recv_compat() {
                if let Err(e) = self.handle_message(msg).await {
                    tracing::warn!("Message handling error: {}", e);
                }
            }

            // Tick handlers
            self.tick_internal().await;

            // Wait for next message or timeout
            match timeout(tick_interval, self.rx.recv()).await {
                Ok(Some(msg)) => {
                    if let Err(e) = self.handle_message(msg).await {
                        tracing::warn!("Message handling error: {}", e);
                    }
                }
                Ok(None) => {
                    // Channel closed, exit
                    tracing::info!("Runtime channel closed, shutting down");

                    // Stop MIDI clock thread
                    #[cfg(feature = "midi")]
                    self.midi.stop_clock_thread();

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
        // Start MIDI clock thread on first tick (for users who use tick() instead of run())
        #[cfg(feature = "midi")]
        if !self.clock_thread_started {
            self.midi
                .start_clock_thread(Arc::clone(&self.transport_snapshot));
            self.clock_thread_started = true;
        }

        // Process all pending messages
        while let Some(msg) = self.rx.try_recv_compat() {
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

        // Get current beat for scheduling and update transport snapshot
        let current_beat = {
            let state = self.state.read().await;
            // Update transport snapshot for background threads (MIDI clock, modulators)
            self.transport_snapshot
                .update(state.current_beat.to_f64(), state.tempo, state.playing);
            state.current_beat
        };

        // Process sequences first - they may start fades and nested patterns/melodies
        self.sequences.tick(current_beat).await;

        // Tick fades before patterns/melodies so triggered notes get the current fade values
        self.fades.tick().await;

        // Tick effects to process pending frees (after grace period)
        self.effects.tick().await;

        // Tick schedulers (patterns and melodies now use updated fade values)
        self.patterns.tick(current_beat).await;
        self.melodies.tick(current_beat).await;

        // Tick recordings (start/stop based on beat position) - native only
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(e) = self.recordings.tick(current_beat).await {
            tracing::warn!("Recording tick error: {}", e);
        }

        // Tick MIDI (process incoming messages)
        #[cfg(feature = "midi")]
        self.midi.tick().await;

        // Note: MIDI clock output is now handled by the dedicated clock thread
        // started in run(). The clock thread reads transport state from
        // transport_snapshot which is updated above.

        // Tick MIDI voice modulator outputs (poll modulator values and send CC)
        // Run at reduced frequency (every 10 ticks = 50Hz) since CC doesn't need 500Hz
        #[cfg(feature = "midi")]
        {
            self.tick_count = self.tick_count.wrapping_add(1);
            if self.tick_count.is_multiple_of(10) {
                self.voices.tick_modulators().await;
            }
        }
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
                TransportMessage::Seek { beat } => {
                    self.transport_snapshot.signal_seek();
                    self.transport.seek(beat).await
                }
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
                GroupMessage::Create { id, name, parent } => {
                    self.groups.create(id, &name, parent).await
                }
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
                VoiceMessage::Create { id, config } => self.voices.create(id, *config).await,
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

            // Modulators
            Message::Modulator(modulator_msg) => match modulator_msg {
                ModulatorMessage::Create { id, config } => self.modulators.create(id, config).await,
                ModulatorMessage::Delete { id } => self.modulators.delete(id).await,
                ModulatorMessage::SetParam { id, param, value } => {
                    self.modulators.set_param(id, &param, value).await
                }
            },

            // Fades
            Message::Fade(fade_msg) => match fade_msg {
                FadeMessage::Start { config } => self.fades.fade(config).await,
                FadeMessage::Cancel { target, param } => self.fades.cancel(&target, &param).await,
            },

            // Reload - apply new script state
            Message::Reload(reload_msg) => match *reload_msg {
                ReloadMessage::Apply { state: new_state } => self.apply_reload(new_state).await,
            },

            // Sync - synchronize with backend and notify caller
            Message::Sync(sync_msg) => match sync_msg {
                SyncMessage::SyncAndNotify { notify } => {
                    tracing::info!("Processing sync request, syncing with backend...");
                    let result = self.backend.sync().await;
                    tracing::info!("Backend sync complete, notifying caller");
                    // Send notification regardless of sync result
                    let _ = notify.send(());
                    result.map_err(Error::backend)
                }
            },

            // MIDI - send to external devices
            #[cfg(feature = "midi")]
            Message::Midi(midi_msg) => match midi_msg {
                // Device management
                MidiMessage::OpenInput { device } => self.midi.open_input(device).await,
                MidiMessage::OpenOutput { device } => self.midi.open_output(device).await,
                MidiMessage::CloseDevice { device } => self.midi.close(device).await,

                // Note/CC output
                MidiMessage::NoteOn {
                    device,
                    channel,
                    note,
                    velocity,
                } => {
                    self.midi
                        .send_note_on(device, channel, note, velocity)
                        .await
                }
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
                MidiMessage::SendNoteOn {
                    device,
                    channel,
                    note,
                    velocity,
                } => {
                    self.midi
                        .send_note_on(device, channel, note, velocity)
                        .await
                }
                MidiMessage::SendNoteOff {
                    device,
                    channel,
                    note,
                } => self.midi.send_note_off(device, channel, note).await,
                MidiMessage::SendCC {
                    device,
                    channel,
                    cc,
                    value,
                } => self.midi.send_cc(device, channel, cc, value).await,

                // Recording
                MidiMessage::StartRecording { device } => self.midi.start_recording(device).await,
                MidiMessage::StartRecordingChannel { device, channel } => {
                    self.midi.start_recording_channel(device, channel).await
                }
                MidiMessage::StopRecording { device } => {
                    // Stop recording and discard the recording
                    // The HTTP API will need to use a different approach with shared state
                    let _ = self.midi.stop_recording(device).await;
                    Ok(())
                }

                // Clock output
                MidiMessage::EnableClockOutput { device } => {
                    self.midi.enable_clock_output(device).await
                }
                MidiMessage::DisableClockOutput { device } => {
                    self.midi.disable_clock_output(device).await
                }
                MidiMessage::SendStart { device } => self.midi.send_start(device).await,
                MidiMessage::SendStop { device } => self.midi.send_stop(device).await,
                MidiMessage::SendContinue { device } => self.midi.send_continue(device).await,

                // Route management
                MidiMessage::AddKeyboardRoute {
                    device,
                    voice,
                    channel,
                    note_min,
                    note_max,
                    transpose,
                } => {
                    use crate::midi::KeyboardRouteBuilder;
                    let mut route = KeyboardRouteBuilder::new(device);
                    route.target_voice = Some(voice);
                    if let Some(ch) = channel {
                        route.channel = Some(ch);
                    }
                    if let Some(min) = note_min {
                        route.note_min = min;
                    }
                    if let Some(max) = note_max {
                        route.note_max = max;
                    }
                    if let Some(tr) = transpose {
                        route.transpose = tr;
                    }
                    self.midi.add_keyboard_route(route).await;
                    Ok(())
                }
                MidiMessage::RemoveKeyboardRoute { index } => {
                    self.midi.remove_keyboard_route(index).await;
                    Ok(())
                }
                MidiMessage::ClearRoutes => {
                    self.midi.clear_routes().await;
                    Ok(())
                }
            },

            // SFZ - instrument loading
            Message::Sfz(sfz_msg) => match sfz_msg {
                SfzMessage::Load { id, path } => self.sfz.load(id, &path).await,
                SfzMessage::Unload { id } => self.sfz.unload(id).await,
            },

            // Recording - audio capture (native only)
            #[cfg(not(target_arch = "wasm32"))]
            Message::Recording(recording_msg) => match recording_msg {
                RecordingMessage::Start { id, config } => {
                    self.recordings.start(id, config).await?;
                    Ok(())
                }
                RecordingMessage::Stop { id } => self.recordings.stop(id).await,
                RecordingMessage::Cancel { id } => self.recordings.cancel(id).await,
                RecordingMessage::BufferAllocated { id } => {
                    self.recordings.buffer_allocated(id).await
                }
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

    /// Get the MIDI callback notification receiver.
    ///
    /// Returns a shared, mutex-guarded receiver that yields one
    /// [`MidiEventNotification`] per matched script-registered callback firing.
    /// Used by the CLI's `midi_dispatcher` task to deliver MIDI events into Rhai
    /// `FnPtr`s registered via `mpk.on_note(...)` etc.
    #[cfg(feature = "midi")]
    pub fn midi_callback_receiver(
        &self,
    ) -> Arc<std::sync::Mutex<tokio::sync::mpsc::Receiver<crate::handlers::MidiEventNotification>>>
    {
        self.midi.callback_receiver()
    }

    /// Load all built-in synthdefs from vibelang-dsp.
    ///
    /// This should be called during initialization to ensure essential
    /// synthdefs are available for sample playback, SFZ instruments,
    /// MIDI output, recording, and audio routing.
    pub async fn load_builtins(&self) -> Result<()> {
        self.synthdefs.load_builtins().await?;

        // Sync with backend to ensure all synthdefs are loaded before continuing
        tracing::debug!("Syncing with backend after loading builtins");
        let _ = self.backend.sync().await;

        Ok(())
    }

    /// Sync with the backend (non-blocking, minimal timeout).
    ///
    /// This method wraps backend.sync() with a very short timeout to avoid
    /// blocking the main tick loop. MIDI clock runs at 24 PPQN so we can't
    /// afford to block for more than a few milliseconds.
    ///
    /// Returns true if sync succeeded, false if it timed out or failed.
    /// On failure, the system will eventually converge - this is not fatal.
    #[cfg(not(target_arch = "wasm32"))]
    async fn sync_with_retry(&self, context: &str) -> bool {
        // Use minimal timeout (10ms) - MIDI clock must not be delayed
        // If scsynth doesn't respond in 10ms, continue anyway
        match timeout(Duration::from_millis(10), self.backend.sync()).await {
            Ok(Ok(())) => {
                tracing::trace!("Reload: {} sync succeeded", context);
                true
            }
            Ok(Err(e)) => {
                tracing::trace!("Reload: {} sync failed: {:?}, continuing", context, e);
                false
            }
            Err(_) => {
                // Timeout - don't retry, just continue
                tracing::trace!("Reload: {} sync timed out, continuing", context);
                false
            }
        }
    }

    /// Sync with the backend (WASM version - no timeout).
    #[cfg(target_arch = "wasm32")]
    async fn sync_with_retry(&self, context: &str) -> bool {
        match self.backend.sync().await {
            Ok(()) => {
                tracing::debug!("Reload: {} sync succeeded", context);
                true
            }
            Err(e) => {
                tracing::warn!("Reload: {} sync failed: {:?}", context, e);
                false
            }
        }
    }

    /// Calculate adaptive position epsilon based on tempo and pattern length.
    ///
    /// The epsilon prevents re-triggering steps that just fired when syncing
    /// pattern/melody positions during reload. It needs to be:
    /// - Large enough to skip past the tick interval (avoid re-triggering)
    /// - Small enough to not skip actual steps in the pattern
    ///
    /// Formula: epsilon = max(tick_window_beats, 0.001), clamped to 10% of length
    fn calculate_position_epsilon(
        &self,
        bpm: f64,
        length: crate::types::Beat,
    ) -> crate::types::Beat {
        // Conservative tick window of 20ms (accounts for tick jitter and scheduling delays)
        const TICK_WINDOW_MS: f64 = 20.0;

        // Calculate how many beats 20ms represents at current tempo
        // beats = (ms / 1000) * (bpm / 60) = ms * bpm / 60000
        let tick_window_beats = TICK_WINDOW_MS * bpm / 60000.0;

        // Ensure epsilon is at least 0.001 beats (minimum resolution)
        let epsilon = tick_window_beats.max(0.001);

        // Cap at 10% of pattern length to avoid skipping actual steps
        // (e.g., 16th notes at any tempo are 0.25 beats, so 10% of a 4-beat pattern
        // would be 0.4 beats, still well under 0.25)
        let max_epsilon = length.to_f64() * 0.10;
        let final_epsilon = epsilon.min(max_epsilon);

        crate::types::Beat::from_f64(final_epsilon)
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
    ///
    /// Applies state from a newly executed script to the runtime.
    async fn apply_reload(&mut self, new_state: reload::ScriptState) -> Result<()> {
        // Calculate diff
        let diff = {
            let current = self.state.read().await;
            reload::calculate_diff(&current, &new_state)
        };

        // If no changes, return early - patterns continue playing seamlessly
        // (Phase 6 only starts patterns that aren't already playing)
        if !diff.has_changes() {
            tracing::debug!("Reload: no changes detected, playback continues");
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
            tracing::debug!(
                "Reload: changing time signature to {}/{}",
                time_sig.numerator,
                time_sig.denominator
            );
            self.transport.set_time_signature(time_sig).await?;
        }

        // =========================================================================
        // Phase 1: Stop entities that will be deleted
        // =========================================================================
        // NOTE: We NO LONGER stop patterns/melodies that are being updated.
        // Instead, we queue content swaps in Phase 4 for seamless hot reload.

        // Cancel fades that will be deleted
        for id in &diff.fades.deleted {
            let config_opt = {
                let state = self.state.read().await;
                state.fade_configs.get(id).cloned()
            };
            if let Some(config) = config_opt {
                tracing::debug!("Reload: cancelling deleted fade {:?}", id);
                let _ = self.fades.cancel(&config.target, &config.param).await;
            }
        }

        // Cancel fades that will be updated (will be restarted with new config)
        for (id, _) in &diff.fades.updated {
            let config_opt = {
                let state = self.state.read().await;
                state.fade_configs.get(id).cloned()
            };
            if let Some(config) = config_opt {
                tracing::debug!("Reload: cancelling updated fade {:?}", id);
                let _ = self.fades.cancel(&config.target, &config.param).await;
            }
        }

        // Stop patterns that will be deleted (NOT updated - those continue playing)
        for id in &diff.patterns.deleted {
            let _ = self.patterns.stop(*id).await;
        }

        // Stop melodies that will be deleted (NOT updated - those continue playing)
        for id in &diff.melodies.deleted {
            let _ = self.melodies.stop(*id).await;
        }

        // Stop sequences that will be deleted or updated
        // (Sequences still use delete/create cycle for now)
        for id in &diff.sequences.deleted {
            let _ = self.sequences.stop(*id).await;
        }
        for id in diff.sequences.updated.keys() {
            let _ = self.sequences.stop(*id).await;
        }

        // =========================================================================
        // Phase 2: Delete entities (children before parents for groups)
        // =========================================================================

        // Delete modulators first (voices may depend on them)
        for id in &diff.modulators.deleted {
            tracing::debug!("Reload: deleting modulator {:?}", id);
            let _ = self.modulators.delete(*id).await;
        }

        // Delete effects (they depend on groups)
        for id in &diff.effects.deleted {
            tracing::debug!("Reload: deleting effect {:?}", id);
            let _ = self.effects.remove(*id).await;
        }

        // Delete fades from tracking state
        if !diff.fades.deleted.is_empty() {
            let mut state = self.state.write().await;
            for id in &diff.fades.deleted {
                tracing::debug!("Reload: deleting fade {:?}", id);
                state.fade_configs.remove(id);
            }
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

        // Free script-allocated buffers that disappeared from the script.
        // Updated buffers (frames/channels resize) are torn down here so
        // Phase 3 below can re-alloc them at the new size.
        for id in &diff.buffers.deleted {
            tracing::debug!("Reload: freeing script buffer {:?}", id);
            if let Err(e) = self.backend.free_buffer(*id).await {
                tracing::warn!("Reload: free_buffer({:?}) failed: {}", id, e);
            }
            self.state.write().await.buffers.remove(id);
        }
        for (id, _) in &diff.buffers.updated {
            tracing::debug!("Reload: freeing script buffer {:?} for resize", id);
            if let Err(e) = self.backend.free_buffer(*id).await {
                tracing::warn!("Reload: free_buffer({:?}) failed: {}", id, e);
            }
            self.state.write().await.buffers.remove(id);
        }

        // Delete SFZ instruments
        for id in &diff.sfz.deleted {
            tracing::debug!("Reload: deleting SFZ instrument {:?}", id);
            let _ = self.sfz.unload(*id).await;
            self.state.write().await.sfz_instruments.remove(id);
        }

        // Delete updated SFZ instruments (will be recreated in Phase 3)
        for (id, _) in &diff.sfz.updated {
            tracing::debug!("Reload: deleting SFZ instrument {:?} for update", id);
            let _ = self.sfz.unload(*id).await;
            self.state.write().await.sfz_instruments.remove(id);
        }

        // =========================================================================
        // Phase 2.5: Open MIDI devices (must be done before voices are created)
        // =========================================================================

        #[cfg(feature = "midi")]
        {
            // Open MIDI inputs
            for device_id in &new_state.midi_inputs {
                tracing::debug!("Reload: opening MIDI input {:?}", device_id);
                if let Err(e) = self.midi.open_input(*device_id).await {
                    tracing::error!("Reload: failed to open MIDI input {:?}: {}", device_id, e);
                }
            }

            // Open MIDI outputs
            for device_id in &new_state.midi_outputs {
                tracing::debug!("Reload: opening MIDI output {:?}", device_id);
                if let Err(e) = self.midi.open_output(*device_id).await {
                    tracing::error!("Reload: failed to open MIDI output {:?}: {}", device_id, e);
                }
            }

            // Update beats per bar for quantization from time signature
            self.midi.set_beats_per_bar(new_state.time_sig.numerator);

            // Process MIDI output messages
            if !new_state.midi_output_messages.is_empty() {
                // First pass: handle Start/Stop/Continue using direct connection.
                // Track per-device transport state to avoid re-sending on reload.
                for msg in &new_state.midi_output_messages {
                    match msg {
                        MidiOutputMessage::Start { device_id } => {
                            let transport_playing = self.state.read().await.playing;
                            if transport_playing {
                                tracing::trace!(
                                    "Reload: skipping MIDI Start to {:?} (transport already playing)",
                                    device_id
                                );
                            } else {
                                tracing::debug!("Reload: sending MIDI Start to {:?}", device_id);
                                if let Err(e) = self.midi.send_start(*device_id).await {
                                    tracing::warn!(
                                        "Reload: failed to send MIDI Start to {:?}: {}",
                                        device_id,
                                        e
                                    );
                                }
                            }
                        }
                        MidiOutputMessage::Stop { device_id } => {
                            let transport_playing = self.state.read().await.playing;
                            if !transport_playing {
                                tracing::trace!(
                                    "Reload: skipping MIDI Stop to {:?} (transport not playing)",
                                    device_id
                                );
                            } else {
                                tracing::debug!("Reload: sending MIDI Stop to {:?}", device_id);
                                if let Err(e) = self.midi.send_stop(*device_id).await {
                                    tracing::warn!(
                                        "Reload: failed to send MIDI Stop to {:?}: {}",
                                        device_id,
                                        e
                                    );
                                }
                            }
                        }
                        MidiOutputMessage::Continue { device_id } => {
                            tracing::debug!("Reload: sending MIDI Continue to {:?}", device_id);
                            if let Err(e) = self.midi.send_continue(*device_id).await {
                                tracing::warn!(
                                    "Reload: failed to send MIDI Continue to {:?}: {}",
                                    device_id,
                                    e
                                );
                            }
                        }
                        _ => {} // Other messages handled below
                    }
                }

                // Second pass: handle note/CC messages via output channels
                let output_channels = self.midi.output_channels();
                let Ok(channels) = output_channels.lock() else {
                    tracing::warn!("MIDI output channels mutex poisoned, skipping output");
                    return Ok(());
                };

                for msg in &new_state.midi_output_messages {
                    // Skip Start/Stop/Continue (handled above)
                    if matches!(
                        msg,
                        MidiOutputMessage::Start { .. }
                            | MidiOutputMessage::Stop { .. }
                            | MidiOutputMessage::Continue { .. }
                    ) {
                        continue;
                    }

                    let (device_id, event) = match msg {
                        MidiOutputMessage::Start { .. }
                        | MidiOutputMessage::Stop { .. }
                        | MidiOutputMessage::Continue { .. } => continue, // Already handled
                        MidiOutputMessage::NoteOn {
                            device_id,
                            channel,
                            note,
                            velocity,
                        } => (
                            *device_id,
                            QueuedMidiEvent::NoteOn {
                                channel: *channel,
                                note: *note,
                                velocity: *velocity,
                            },
                        ),
                        MidiOutputMessage::NoteOff {
                            device_id,
                            channel,
                            note,
                        } => (
                            *device_id,
                            QueuedMidiEvent::NoteOff {
                                channel: *channel,
                                note: *note,
                            },
                        ),
                        MidiOutputMessage::ControlChange {
                            device_id,
                            channel,
                            cc,
                            value,
                        } => (
                            *device_id,
                            QueuedMidiEvent::ControlChange {
                                channel: *channel,
                                cc: *cc,
                                value: *value,
                            },
                        ),
                        MidiOutputMessage::PitchBend {
                            device_id,
                            channel,
                            value,
                        } => (
                            *device_id,
                            QueuedMidiEvent::PitchBend {
                                channel: *channel,
                                value: *value,
                            },
                        ),
                        // Skip MIDI 2.0 messages for now (need separate handling)
                        MidiOutputMessage::ProgramChange { .. }
                        | MidiOutputMessage::Midi2NoteOn { .. }
                        | MidiOutputMessage::Midi2NoteOff { .. }
                        | MidiOutputMessage::Midi2ControlChange { .. }
                        | MidiOutputMessage::Midi2PitchBend { .. }
                        | MidiOutputMessage::Midi2PerNotePitchBend { .. }
                        | MidiOutputMessage::Midi2PerNoteController { .. }
                        | MidiOutputMessage::Midi2PolyPressure { .. } => {
                            tracing::debug!(
                                "Reload: skipping MIDI 2.0 message (not yet implemented)"
                            );
                            continue;
                        }
                    };

                    if let Some(sender) = channels.get(&device_id) {
                        let scheduled = ScheduledMidiEvent::immediate(event);
                        if let Err(e) = sender.try_send(scheduled) {
                            tracing::warn!(
                                "Reload: failed to send MIDI message to {:?}: {}",
                                device_id,
                                e
                            );
                        }
                    } else {
                        tracing::warn!("Reload: no output channel for MIDI device {:?}", device_id);
                    }
                }
            }
        }

        // =========================================================================
        // Phase 3: Create new entities (parents before children for groups)
        // =========================================================================

        // Load new samples first (other entities may depend on them)
        for (id, config) in &diff.samples.created {
            tracing::debug!("Reload: loading sample {:?}", id);
            let _ = self.samples.load(*id, config.clone()).await;
        }

        // Allocate new script buffers (and re-allocate updated ones at new
        // size — the prior generation was freed in Phase 2 above). Voices
        // wired with `set_param("bufnum", ...)` will reference these IDs.
        let buffers_to_alloc: Vec<_> = diff
            .buffers
            .created
            .iter()
            .chain(diff.buffers.updated.iter())
            .collect();
        for (id, config) in buffers_to_alloc {
            tracing::debug!(
                "Reload: allocating script buffer {:?} '{}' ({} frames × {} ch)",
                id,
                config.name,
                config.frames,
                config.channels
            );
            match self
                .backend
                .alloc_buffer(*id, config.frames, config.channels)
                .await
            {
                Ok(_info) => {
                    self.state
                        .write()
                        .await
                        .buffers
                        .insert(*id, config.clone());
                }
                Err(e) => {
                    tracing::error!(
                        "Reload: alloc_buffer({:?}, {}, {}) failed: {}",
                        id,
                        config.frames,
                        config.channels,
                        e
                    );
                }
            }
        }

        // Load new SFZ instruments (and re-load updated ones)
        let sfz_to_load: Vec<_> = diff
            .sfz
            .created
            .iter()
            .chain(diff.sfz.updated.iter())
            .collect();
        for (id, config) in sfz_to_load {
            tracing::debug!(
                "Reload: loading SFZ instrument {:?} from {:?}",
                id,
                config.path
            );
            if let Err(e) = self.sfz.load(*id, &config.path).await {
                tracing::error!("Reload: failed to load SFZ instrument {:?}: {}", id, e);
            }
        }

        // Create groups in correct order (parents first)
        let ordered_group_creations = reload::order_group_creations(&diff.groups.created);
        for id in ordered_group_creations {
            if let Some(config) = diff.groups.created.get(&id) {
                tracing::debug!("Reload: creating group {:?}", id);
                let _ = self.groups.create(id, &config.name, config.parent).await;
                // Apply initial params
                for (param, value) in &config.params {
                    let _ = self.groups.set_param(id, param, *value).await;
                }
                // Apply initial mute/solo state
                if config.muted {
                    let _ = self.groups.mute(id, true).await;
                }
                if config.soloed {
                    let _ = self.groups.solo(id, true).await;
                }
                // Apply output_bus routing override
                if config.output_bus.is_some() {
                    let mut state = self.state.write().await;
                    if let Some(group) = state.groups.get_mut(&id) {
                        group.output_bus = config.output_bus;
                    }
                }
            }
        }

        // Sync with backend to ensure groups are created before we create synths targeting them
        if !diff.groups.created.is_empty() {
            self.sync_with_retry("after group creation").await;
        }

        // Create new modulators (before voices so modulations can reference them)
        for (id, config) in &diff.modulators.created {
            tracing::debug!(
                "Reload: creating modulator {:?} with synthdef '{}'",
                id,
                config.synthdef
            );
            if let Err(e) = self.modulators.create(*id, config.clone()).await {
                tracing::error!("Reload: failed to create modulator {:?}: {}", id, e);
            }
        }

        // Create new voices
        for (id, config) in &diff.voices.created {
            tracing::debug!("Reload: creating voice {:?}", id);
            if let Err(e) = self.voices.create(*id, config.clone()).await {
                tracing::error!("Reload: failed to create voice {:?}: {}", id, e);
            }
        }

        // Create new patterns
        for (id, config) in &diff.patterns.created {
            tracing::debug!("Reload: creating pattern {:?}", id);
            if let Err(e) = self.patterns.create(*id, config.clone()).await {
                tracing::error!("Reload: failed to create pattern {:?}: {}", id, e);
            }
        }

        // Create new melodies
        for (id, config) in &diff.melodies.created {
            tracing::debug!("Reload: creating melody {:?}", id);
            if let Err(e) = self.melodies.create(*id, config.clone()).await {
                tracing::error!("Reload: failed to create melody {:?}: {}", id, e);
            }
        }

        // Create new sequences
        for (id, config) in &diff.sequences.created {
            tracing::debug!("Reload: creating sequence {:?}", id);
            if let Err(e) = self.sequences.create(*id, config.clone()).await {
                tracing::error!("Reload: failed to create sequence {:?}: {}", id, e);
            }
        }

        // NOTE: Effect creation is deferred to Phase 4.8 (after routes finalize)
        // so that effect synths sit in SC tree order *after* the route mixers
        // that sum voice ports onto the group's audio bus. See Phase 4.8 below.

        // =========================================================================
        // Phase 4: Update existing entities
        // =========================================================================

        // Update groups (params and mute/solo, parent changes not supported during reload)
        for (id, new_config) in &diff.groups.updated {
            // Apply all params from the new config
            for (param, value) in &new_config.params {
                tracing::debug!(
                    "Reload: updating group {:?} param {} to {}",
                    id,
                    param,
                    value
                );
                let _ = self.groups.set_param(*id, param, *value).await;
            }
            // Apply mute/solo state
            tracing::debug!(
                "Reload: updating group {:?} muted={} soloed={}",
                id,
                new_config.muted,
                new_config.soloed
            );
            let _ = self.groups.mute(*id, new_config.muted).await;
            let _ = self.groups.solo(*id, new_config.soloed).await;

            // Update output_bus routing if changed
            {
                let mut state = self.state.write().await;
                if let Some(group) = state.groups.get_mut(id) {
                    if group.output_bus != new_config.output_bus {
                        let old_output = group.output_bus;
                        group.output_bus = new_config.output_bus;

                        // Update link synth's outbus parameter if it exists
                        if let Some(link_node) = group.link_synth_node_id {
                            let new_outbus = if let Some(hw_bus) = new_config.output_bus {
                                hw_bus as f32
                            } else {
                                // Route back to parent or main
                                group
                                    .parent
                                    .and_then(|p| state.groups.get(&p))
                                    .map(|pg| pg.audio_bus.0 as f32)
                                    .unwrap_or(0.0)
                            };
                            drop(state);
                            let _ = self.backend.set_param(link_node, "outbus", new_outbus).await;
                            tracing::info!(
                                "Reload: updated group {:?} output_bus {:?} -> {:?}",
                                id,
                                old_output,
                                new_config.output_bus
                            );
                        }
                    }
                }
            }
        }

        // Update voices - only recreate if synthdef, group, or sfz_instrument changed
        // For param-only changes, use set_param to avoid audio gaps
        for (id, new_config) in &diff.voices.updated {
            // Get current voice state to compare
            let needs_recreate = {
                let state = self.state.read().await;
                if let Some(current_voice) = state.voices.get(id) {
                    // Recreate if synthdef, group, or sfz_instrument changed
                    current_voice.config.synthdef != new_config.synthdef
                        || current_voice.config.group != new_config.group
                        || current_voice.config.sfz_instrument != new_config.sfz_instrument
                } else {
                    // Voice not found - shouldn't happen, but recreate to be safe
                    true
                }
            };

            if needs_recreate {
                tracing::debug!(
                    "Reload: recreating voice {:?} (synthdef, group, or sfz changed)",
                    id
                );
                let _ = self.voices.graceful_delete(*id).await;
                let _ = self.voices.create(*id, new_config.clone()).await;
            } else {
                // Only params/config changed - update them without recreating the synth
                tracing::debug!(
                    "Reload: updating voice {:?} params only ({} params)",
                    id,
                    new_config.params.len()
                );
                for (param, value) in &new_config.params {
                    let _ = self.voices.set_param(*id, param, *value).await;
                }
                // Update the stored config for non-param fields (name, polyphony, etc.)
                let mut state = self.state.write().await;
                if let Some(voice) = state.voices.get_mut(id) {
                    voice.config.name = new_config.name.clone();
                    voice.config.polyphony = new_config.polyphony;
                    voice.config.round_robin_count = new_config.round_robin_count;
                    voice.config.choke_group = new_config.choke_group.clone();
                    voice.config.modulations = new_config.modulations.clone();
                    #[cfg(feature = "midi")]
                    {
                        voice.config.midi_output = new_config.midi_output;
                        voice.config.midi_channel = new_config.midi_channel;
                        voice.config.param_cc_map = new_config.param_cc_map.clone();
                    }
                }
            }
        }

        // Update patterns - queue content swap for seamless hot reload
        // Instead of delete/create cycle, queue new content to be applied at next bar boundary
        // This ensures no audio disruption during live reload
        for (id, config) in &diff.patterns.updated {
            tracing::debug!("Reload: queuing pattern {:?} content swap for next bar", id);
            let new_content = crate::traits::PatternContent::arc_from_config(config);
            let mut state = self.state.write().await;
            if let Some(pattern) = state.patterns.get_mut(id) {
                // Queue content swap with NextBar quantization (default)
                pattern.queue_content_swap(new_content, reload::ChangeQuant::default());
                tracing::debug!(
                    "Pattern {:?}: content swap queued (playing={})",
                    id,
                    pattern.playing
                );
            }
        }

        // Update melodies - queue content swap for seamless hot reload
        // Instead of delete/create cycle, queue new content to be applied at next bar boundary
        // This ensures no audio disruption during live reload
        for (id, config) in &diff.melodies.updated {
            tracing::debug!("Reload: queuing melody {:?} content swap for next bar", id);
            let new_content = crate::traits::MelodyContent::arc_from_config(config);
            let mut state = self.state.write().await;
            if let Some(melody) = state.melodies.get_mut(id) {
                // Queue content swap with NextBar quantization (default)
                melody.queue_content_swap(new_content, reload::ChangeQuant::default());
                tracing::debug!(
                    "Melody {:?}: content swap queued (playing={})",
                    id,
                    melody.playing
                );
            }
        }

        // Update sequences - update config in place, preserving playback state
        for (id, config) in &diff.sequences.updated {
            tracing::debug!("Reload: updating sequence {:?}", id);
            // Save current playback state
            let (was_playing, looping) = {
                let state = self.state.read().await;
                state
                    .sequences
                    .get(id)
                    .map(|s| (s.playing, s.looping))
                    .unwrap_or((false, false))
            };
            // Recreate with new config
            let _ = self.sequences.delete(*id).await;
            let _ = self.sequences.create(*id, config.clone()).await;
            // Restore playback state - sync position to current beat to avoid re-triggering clips
            if was_playing {
                let mut state = self.state.write().await;
                // Calculate adaptive epsilon based on tempo and sequence length
                let epsilon = self.calculate_position_epsilon(state.tempo, config.length);
                // For looping sequences, use modulo + epsilon; for non-looping, clamp to length
                let base_position = if looping {
                    state.current_beat % config.length
                } else {
                    state.current_beat.min(config.length)
                };
                let synced_position = base_position + epsilon;
                let synced_position = if looping && synced_position >= config.length {
                    synced_position - config.length
                } else {
                    synced_position
                };
                if let Some(sequence) = state.sequences.get_mut(id) {
                    sequence.playing = true;
                    sequence.position = synced_position;
                    sequence.looping = looping;
                }
            }
        }

        // NOTE: Effect updates are deferred to Phase 4.8 (after routes finalize),
        // alongside effect creation, so freshly-(re)spawned effect synths sit
        // *after* the route mixers in SC tree order. See Phase 4.8 below.

        // Update modulators - only recreate if synthdef or modulations changed;
        // for param-only changes use set_param to avoid resetting the LFO phase.
        for (id, new_config) in &diff.modulators.updated {
            let needs_recreate = {
                let state = self.state.read().await;
                if let Some(current) = state.modulators.get(id) {
                    current.config.synthdef != new_config.synthdef
                        || current.config.modulations != new_config.modulations
                } else {
                    true
                }
            };

            if needs_recreate {
                tracing::debug!(
                    "Reload: recreating modulator {:?} (synthdef or modulations changed, synthdef='{}')",
                    id,
                    new_config.synthdef
                );
                let _ = self.modulators.delete(*id).await;
                if let Err(e) = self.modulators.create(*id, new_config.clone()).await {
                    tracing::error!("Reload: failed to recreate modulator {:?}: {}", id, e);
                }
            } else {
                // Only params changed - update in place to preserve LFO phase
                tracing::debug!(
                    "Reload: updating modulator {:?} params only (synthdef='{}')",
                    id,
                    new_config.synthdef
                );
                for (param, value) in &new_config.params {
                    let _ = self.modulators.set_param(*id, param, *value).await;
                }
            }
        }

        // =========================================================================
        // Phase 4.7: Finalize routes (per-voice port → group mixer synths)
        // =========================================================================
        // Spawned between the voice creation/update phase and the group
        // link-synth phase so the SC tree order is voices → routes → effects →
        // link synth → main bus. The diff is computed against the
        // last-applied [`Self::current_routes`] snapshot.
        //
        // Story 5: the effective desired map is the union of count-based
        // defaults (installed in `state.default_routes` by VoicesHandler::create)
        // and the script-supplied user routes from `new_state.routes`, with
        // user entries winning on conflicts. We diff against — and persist —
        // this merged map so a later reload sees defaults as already-applied
        // and a removal of a user route correctly falls back to the default.
        let merged_routes = {
            let state = self.state.read().await;
            merge_default_routes(&new_state.routes, &state.default_routes)
        };
        let route_diff = RoutesHandler::<B>::diff(&self.current_routes, &merged_routes);
        if !route_diff.is_empty() {
            tracing::debug!(
                "Reload: finalizing routes (additions={}, removals={}, changes={})",
                route_diff.additions.len(),
                route_diff.removals.len(),
                route_diff.changes.len()
            );
            if let Err(e) = self.routes.finalize(&route_diff).await {
                tracing::error!("Reload: routes.finalize failed: {}", e);
            }
        }
        self.current_routes = merged_routes;

        // =========================================================================
        // Phase 4.8: Create / update effects (after route mixers)
        // =========================================================================
        // Effects must be inserted into the SC tree *after* the route mixers
        // emitted by Phase 4.7 so that an effect's `In.ar(group_audio_bus)`
        // sees the post-sum signal that routes have just deposited there.
        // On initial load (no link synth yet) we use `AddAction::Tail`, so
        // doing this strictly after Phase 4.7 (which also Tail-adds mixers
        // to the same group node) places effects after routes in tree order.
        // On later reloads where the link synth exists, `EffectsHandler::add`
        // already inserts `AddAction::Before` link, which is also after the
        // routes. Either way: voices → routes → effects → link → main.

        // Create new effects
        for (id, config) in &diff.effects.created {
            tracing::debug!("Reload: creating effect {:?}", id);
            if let Err(e) = self
                .effects
                .add(*id, config.group, &config.synthdef, &config.params)
                .await
            {
                tracing::error!("Reload: failed to create effect {:?}: {}", id, e);
            }
        }

        // Update effects - only recreate if synthdef or group changed
        // For param-only changes, use set_param to avoid audio gaps
        for (id, new_config) in &diff.effects.updated {
            // Get current effect state to compare
            let needs_recreate = {
                let state = self.state.read().await;
                if let Some(current_effect) = state.effects.get(id) {
                    // Recreate if synthdef or group changed
                    current_effect.synthdef != new_config.synthdef
                        || current_effect.group != new_config.group
                } else {
                    // Effect not found - shouldn't happen, but recreate to be safe
                    true
                }
            };

            if needs_recreate {
                tracing::debug!(
                    "Reload: recreating effect {:?} (synthdef or group changed, synthdef='{}', {} params)",
                    id,
                    new_config.synthdef,
                    new_config.params.len()
                );
                let _ = self.effects.remove(*id).await;
                if let Err(e) = self
                    .effects
                    .add(
                        *id,
                        new_config.group,
                        &new_config.synthdef,
                        &new_config.params,
                    )
                    .await
                {
                    tracing::error!("Reload: failed to recreate effect {:?}: {}", id, e);
                }
            } else {
                // Only params changed - update them without recreating the synth
                tracing::debug!(
                    "Reload: updating effect {:?} params only (synthdef='{}', {} params)",
                    id,
                    new_config.synthdef,
                    new_config.params.len()
                );
                for (param, value) in &new_config.params {
                    let _ = self.effects.set_param(*id, param, *value).await;
                }
            }
        }

        // =========================================================================
        // Phase 5: Finalize groups (create link synths)
        // =========================================================================

        // Finalize groups if any were created or updated — ensures link synths exist
        // and are properly configured. This handles group renames where the old group
        // is deleted and a new one is created.
        if !diff.groups.created.is_empty() || !diff.groups.updated.is_empty() {
            tracing::debug!("Reload: finalizing groups");
            let _ = self.groups.finalize().await;

            // Brief sync to let link synths be created (non-blocking, quick timeout)
            self.sync_with_retry("after finalize").await;
        }

        // =========================================================================
        // Phase 5.5: Process fades (stateful + legacy pending)
        //
        // Stateful fades participate in the diff system: unchanged fades are
        // not re-fired. Only new or updated fades are started.
        // =========================================================================

        // Start newly created fades
        for (id, config) in &diff.fades.created {
            if new_state.playing_fades.contains(id) {
                tracing::debug!(
                    "Reload: starting new fade {:?} on {:?}/{} from {:?} to {} over {:?}",
                    id,
                    config.target,
                    config.param,
                    config.from,
                    config.to,
                    config.duration
                );
                if let Err(e) = self.fades.fade(config.clone()).await {
                    tracing::error!("Reload: failed to start fade {:?}: {}", id, e);
                }
                // Track in runtime state for future diffing
                let mut state = self.state.write().await;
                state.fade_configs.insert(*id, config.clone());
            }
        }

        // Restart updated fades (already cancelled in Phase 1)
        for (id, config) in &diff.fades.updated {
            if new_state.playing_fades.contains(id) {
                tracing::debug!(
                    "Reload: restarting updated fade {:?} on {:?}/{} from {:?} to {} over {:?}",
                    id,
                    config.target,
                    config.param,
                    config.from,
                    config.to,
                    config.duration
                );
                if let Err(e) = self.fades.fade(config.clone()).await {
                    tracing::error!("Reload: failed to restart fade {:?}: {}", id, e);
                }
                // Update tracking state
                let mut state = self.state.write().await;
                state.fade_configs.insert(*id, config.clone());
            }
        }

        // Process legacy pending_fades/pending_fades_quantized for backward compatibility
        if !new_state.pending_fades.is_empty() || !new_state.pending_fades_quantized.is_empty() {
            tracing::warn!(
                "Reload: processing {} legacy pending fades (deprecated, use stateful fades)",
                new_state.pending_fades.len() + new_state.pending_fades_quantized.len()
            );
            for fade_config in new_state
                .pending_fades
                .iter()
                .chain(new_state.pending_fades_quantized.iter())
            {
                tracing::debug!(
                    "Reload: starting legacy fade on {:?}/{} from {:?} to {} over {:?}",
                    fade_config.target,
                    fade_config.param,
                    fade_config.from,
                    fade_config.to,
                    fade_config.duration
                );
                if let Err(e) = self.fades.fade(fade_config.clone()).await {
                    tracing::error!("Reload: failed to start legacy fade: {}", e);
                }
            }
        }

        // =========================================================================
        // Phase 6: Start patterns/melodies/sequences that should be playing
        //
        // Important: Only start entities that aren't already playing.
        // This ensures seamless live reload - unchanged patterns continue
        // from their current position without glitching.
        // =========================================================================

        // First, stop patterns/melodies/sequences that were playing but are no longer
        // in the playing_* sets. This handles the case where user changes start() to stop().
        //
        // IMPORTANT: Preserve playing state for UNCHANGED entities to avoid disruption
        // during hot reload. Only stop entities that:
        // 1. Were playing AND
        // 2. Are NOT in the new playing set AND
        // 3. Are NOT unchanged (i.e., they were updated or deleted)
        //
        // This ensures that if you just edit a melody's notes without touching .start(),
        // the melody keeps playing seamlessly.
        let patterns_to_stop: Vec<crate::types::PatternId> = {
            let state = self.state.read().await;
            state
                .patterns
                .iter()
                .filter(|(id, p)| {
                    p.playing
                        && !new_state.playing_patterns.contains(id)
                        // Only stop if NOT unchanged (was updated, deleted, or not in new state)
                        && !diff.patterns.unchanged.contains(id)
                })
                .map(|(id, _)| *id)
                .collect()
        };
        for id in patterns_to_stop {
            tracing::debug!(
                "Reload: stopping pattern {:?} (no longer in playing_patterns and was changed)",
                id
            );
            let _ = self.patterns.stop(id).await;
        }

        let melodies_to_stop: Vec<crate::types::MelodyId> = {
            let state = self.state.read().await;
            tracing::debug!(
                "Reload: checking {} runtime melodies against {} script playing_melodies, {} unchanged",
                state.melodies.len(),
                new_state.playing_melodies.len(),
                diff.melodies.unchanged.len()
            );
            for (id, m) in &state.melodies {
                tracing::debug!(
                    "  Runtime melody {:?}: playing={}, in_new_playing={}, unchanged={}",
                    id,
                    m.playing,
                    new_state.playing_melodies.contains(id),
                    diff.melodies.unchanged.contains(id)
                );
            }
            state
                .melodies
                .iter()
                .filter(|(id, m)| {
                    m.playing
                        && !new_state.playing_melodies.contains(id)
                        // Only stop if NOT unchanged (was updated, deleted, or not in new state)
                        && !diff.melodies.unchanged.contains(id)
                })
                .map(|(id, _)| *id)
                .collect()
        };
        tracing::debug!("Reload: melodies to stop: {:?}", melodies_to_stop);
        for id in melodies_to_stop {
            tracing::debug!(
                "Reload: stopping melody {:?} (no longer in playing_melodies and was changed)",
                id
            );
            let _ = self.melodies.stop(id).await;
        }

        let sequences_to_stop: Vec<crate::types::SequenceId> = {
            let state = self.state.read().await;
            state
                .sequences
                .iter()
                .filter(|(id, s)| {
                    s.playing
                        && !new_state.playing_sequences.contains(id)
                        // Only stop if NOT unchanged (was updated, deleted, or not in new state)
                        && !diff.sequences.unchanged.contains(id)
                })
                .map(|(id, _)| *id)
                .collect()
        };
        for id in sequences_to_stop {
            tracing::debug!(
                "Reload: stopping sequence {:?} (no longer in playing_sequences and was changed)",
                id
            );
            let _ = self.sequences.stop(id).await;
        }

        // Start patterns that should be playing (only if not already playing)
        // Also sync their position to current_beat to avoid triggering past steps
        for id in &new_state.playing_patterns {
            let (should_start, pattern_length) = {
                let state = self.state.read().await;
                let should_start = state.patterns.get(id).is_some_and(|p| !p.playing);
                let length = state
                    .patterns
                    .get(id)
                    .map(|p| p.content.length)
                    .unwrap_or(crate::types::Beat::from_f64(4.0));
                (should_start, length)
            };
            if should_start {
                tracing::debug!("Reload: starting pattern {:?}", id);
                let _ = self.patterns.start(*id).await;
                // Sync position to current beat + epsilon to avoid re-triggering past steps
                // BUT: don't add epsilon when starting at beat 0, as this causes wrap-around bugs
                let mut state = self.state.write().await;
                let base_position = state.current_beat % pattern_length;
                let synced_position = if base_position == crate::types::Beat::ZERO {
                    base_position
                } else {
                    // Use adaptive epsilon based on tempo
                    let epsilon = self.calculate_position_epsilon(state.tempo, pattern_length);
                    let pos = base_position + epsilon;
                    if pos >= pattern_length {
                        pos - pattern_length
                    } else {
                        pos
                    }
                };
                if let Some(pattern) = state.patterns.get_mut(id) {
                    pattern.loop_position = synced_position;
                }
            }
        }

        // Start melodies that should be playing (only if not already playing)
        // Also sync their position to current_beat to avoid triggering past notes
        tracing::debug!(
            "Reload: processing {} melodies to start from playing_melodies",
            new_state.playing_melodies.len()
        );
        for id in &new_state.playing_melodies {
            let (should_start, melody_length, melody_exists, notes_count) = {
                let state = self.state.read().await;
                let melody_exists = state.melodies.contains_key(id);
                let should_start = state.melodies.get(id).is_some_and(|m| !m.playing);
                let length = state
                    .melodies
                    .get(id)
                    .map(|m| m.content.length)
                    .unwrap_or(crate::types::Beat::from_f64(4.0));
                let notes_count = state
                    .melodies
                    .get(id)
                    .map(|m| m.content.notes.len())
                    .unwrap_or(0);
                (should_start, length, melody_exists, notes_count)
            };

            if !melody_exists {
                tracing::warn!(
                    "Reload: melody {:?} in playing_melodies but NOT in runtime state.melodies!",
                    id
                );
                continue;
            }

            tracing::debug!(
                "Reload: melody {:?} exists={}, should_start={}, notes_count={}",
                id,
                melody_exists,
                should_start,
                notes_count
            );

            if should_start {
                tracing::debug!("Reload: starting melody {:?}", id);
                let _ = self.melodies.start(*id).await;
                // Sync position to current beat + epsilon to avoid re-triggering past notes
                // BUT: don't add epsilon when starting at beat 0, as this causes wrap-around bugs
                let mut state = self.state.write().await;
                let base_position = state.current_beat % melody_length;
                let synced_position = if base_position == crate::types::Beat::ZERO {
                    // Starting fresh at beat 0 - don't add epsilon
                    base_position
                } else {
                    // Mid-song reload - use adaptive epsilon to avoid re-triggering
                    let epsilon = self.calculate_position_epsilon(state.tempo, melody_length);
                    let pos = base_position + epsilon;
                    if pos >= melody_length {
                        pos - melody_length
                    } else {
                        pos
                    }
                };
                if let Some(melody) = state.melodies.get_mut(id) {
                    melody.loop_position = synced_position;
                }
            }
        }

        // Start sequences that should be playing (only if not already playing)
        // Also sync their position to current_beat to avoid re-triggering past clips
        for id in &new_state.playing_sequences {
            let (should_start, sequence_length) = {
                let state = self.state.read().await;
                let should_start = state.sequences.get(id).is_some_and(|s| !s.playing);
                let length = state
                    .sequences
                    .get(id)
                    .map(|s| s.config.length)
                    .unwrap_or(crate::types::Beat::from_f64(16.0));
                (should_start, length)
            };
            if should_start {
                tracing::debug!("Reload: starting sequence {:?}", id);
                // Default to looping for sequences started via script
                let _ = self.sequences.start(*id, true).await;
                // Sync position to current beat + epsilon to avoid re-triggering past clips
                // BUT: don't add epsilon when starting at beat 0, as this causes wrap-around bugs
                let mut state = self.state.write().await;
                let base_position = state.current_beat % sequence_length;
                let synced_position = if base_position == crate::types::Beat::ZERO {
                    base_position
                } else {
                    // Use adaptive epsilon based on tempo
                    let epsilon = self.calculate_position_epsilon(state.tempo, sequence_length);
                    let pos = base_position + epsilon;
                    if pos >= sequence_length {
                        pos - sequence_length
                    } else {
                        pos
                    }
                };
                if let Some(sequence) = state.sequences.get_mut(id) {
                    sequence.position = synced_position;
                }
            }
        }

        // =========================================================================
        // Phase 6.5: Handle running voices (line-in, drones, etc.)
        // =========================================================================

        // First, stop voices that were running but are no longer in running_voices
        // This handles the case where .run() is removed from a voice
        let voices_to_stop: Vec<crate::types::VoiceId> = {
            let state = self.state.read().await;
            state
                .voices
                .iter()
                .filter(|(id, v)| {
                    // Voice has active nodes (was running) but is not in new running_voices
                    !v.active_nodes.is_empty() && !new_state.running_voices.contains(id)
                })
                .map(|(id, _)| *id)
                .collect()
        };

        for voice_id in voices_to_stop {
            tracing::debug!(
                "Reload: stopping voice {:?} (no longer in running_voices)",
                voice_id
            );
            let _ = self.voices.stop(voice_id).await;
        }

        // Then, trigger voices that should be running
        for voice_id in &new_state.running_voices {
            // Check if voice exists and doesn't already have active synths
            let should_trigger = {
                let state = self.state.read().await;
                state
                    .voices
                    .get(voice_id)
                    .is_some_and(|v| v.active_nodes.is_empty())
            };
            if should_trigger {
                tracing::debug!("Reload: triggering running voice {:?}", voice_id);
                // Trigger with gate=1.0 for continuous playback
                let params = crate::types::ParamMap::from([("gate".to_string(), 1.0f32)]);
                if let Err(e) = self.voices.trigger(*voice_id, &params).await {
                    tracing::error!(
                        "Reload: failed to trigger running voice {:?}: {}",
                        voice_id,
                        e
                    );
                }
            }
        }

        // =========================================================================
        // Phase 7: Apply MIDI routes from script state
        // =========================================================================

        #[cfg(feature = "midi")]
        {
            // Apply all MIDI route types. Each apply method clears its own
            // slice first, so calling with an empty vec correctly removes
            // all routes of that type (handles route removal on reload).
            self.midi
                .apply_basic_keyboard_routes(&new_state.midi_keyboard_routes)
                .await;
            self.midi
                .apply_advanced_keyboard_routes(&new_state.advanced_keyboard_routes)
                .await;
            self.midi
                .apply_advanced_note_routes(&new_state.advanced_note_routes)
                .await;
            self.midi.apply_cc_routes(&new_state.midi_cc_routes).await;
            self.midi
                .apply_advanced_cc_routes(&new_state.advanced_cc_routes)
                .await;
            self.midi
                .apply_midi2_keyboard_routes(&new_state.midi2_keyboard_routes)
                .await;
            self.midi
                .apply_midi2_per_note_routes(&new_state.midi2_per_note_routes)
                .await;
            self.midi
                .apply_midi2_cc_routes(&new_state.midi2_cc_routes)
                .await;

            // Reconcile looper instances against the new config list.
            self.midi.reconcile_loopers(&new_state.loopers).await;

            // Apply MIDI clock output requests
            for clock_req in &new_state.midi_clock_outputs {
                tracing::debug!(
                    "Reload: {} MIDI clock output for device {:?}",
                    if clock_req.enabled {
                        "enabling"
                    } else {
                        "disabling"
                    },
                    clock_req.device_id
                );
                if clock_req.enabled {
                    if let Err(e) = self.midi.enable_clock_output(clock_req.device_id).await {
                        tracing::error!(
                            "Reload: failed to enable clock output for device {:?}: {}",
                            clock_req.device_id,
                            e
                        );
                    }
                } else if let Err(e) = self.midi.disable_clock_output(clock_req.device_id).await {
                    tracing::error!(
                        "Reload: failed to disable clock output for device {:?}: {}",
                        clock_req.device_id,
                        e
                    );
                }
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
    tx: Sender<Message>,
}

impl RuntimeHandle {
    /// Send a message to the runtime.
    ///
    /// Returns an error if the runtime has been dropped.
    pub async fn send(&self, msg: Message) -> Result<()> {
        self.tx
            .send_async(msg)
            .await
            .map_err(|_| Error::ChannelClosed)
    }

    /// Try to send a message without waiting.
    ///
    /// Returns an error if the channel is full or closed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn try_send(&self, msg: Message) -> Result<()> {
        self.tx.try_send(msg).map_err(|_| Error::ChannelClosed)
    }

    /// Try to send a message without waiting.
    ///
    /// Returns an error if the channel is full or closed.
    #[cfg(target_arch = "wasm32")]
    pub fn try_send(&self, msg: Message) -> Result<()> {
        use futures::Sink;
        let mut tx = self.tx.clone();
        // In WASM, try_send is not directly available, but we can use start_send
        std::pin::Pin::new(&mut tx)
            .start_send(msg)
            .map_err(|_| Error::ChannelClosed)
    }

    /// Send a message, blocking the current thread until it's queued.
    ///
    /// This is useful when calling from synchronous code (like Rhai callbacks)
    /// where async is not available.
    ///
    /// Returns an error if the channel is closed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn blocking_send(&self, msg: Message) -> Result<()> {
        self.tx.blocking_send(msg).map_err(|_| Error::ChannelClosed)
    }

    /// Send a sync message and wait for the backend to complete all pending operations.
    ///
    /// This is a barrier that ensures:
    /// 1. All previously sent messages have been processed by the runtime
    /// 2. The backend has synced with scsynth (all d_recv, s_new, etc. completed)
    ///
    /// Use this after queueing synthdefs to ensure they're loaded before creating synths.
    pub async fn sync_and_wait(&self) -> Result<()> {
        let (tx, rx) = crate::compat::oneshot();
        self.send(Message::Sync(SyncMessage::SyncAndNotify { notify: tx }))
            .await?;
        rx.await.map_err(|_| Error::ChannelClosed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{AddAction, BufferInfo};
    use crate::types::{BufferId, NodeId, ParamMap};
    use std::collections::HashMap;
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

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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

        async fn map_param_to_bus(
            &self,
            _node: NodeId,
            _param: &str,
            _bus: u32,
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
            .send(Message::Transport(TransportMessage::SetTempo {
                bpm: 140.0,
            }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_runtime_tick() {
        let mut runtime = Runtime::new(MockBackend);

        // Send a message
        runtime
            .send(Message::Transport(TransportMessage::SetTempo {
                bpm: 140.0,
            }))
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
            .send(
                GroupMessage::Create {
                    id: GroupId::new(1),
                    name: "test_group".to_string(),
                    parent: None,
                }
                .into(),
            )
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
            .send(
                GroupMessage::Delete {
                    id: GroupId::new(1),
                }
                .into(),
            )
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
        use crate::message::{GroupMessage, SynthDefMessage, VoiceMessage};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};

        let mut runtime = Runtime::new(MockBackend);

        // First register the synthdef
        runtime
            .send(
                SynthDefMessage::Load {
                    name: "simple_sine".to_string(),
                    data: vec![],
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Create a group
        runtime
            .send(
                GroupMessage::Create {
                    id: GroupId::new(1),
                    name: "test_group".to_string(),
                    parent: None,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Create a voice
        let config = VoiceConfig::new("test_voice", "simple_sine", GroupId::new(1));
        runtime
            .send(
                VoiceMessage::Create {
                    id: VoiceId::new(1),
                    config: Box::new(config),
                }
                .into(),
            )
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
            .send(
                VoiceMessage::Trigger {
                    id: VoiceId::new(1),
                    params: ParamMap::new(),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Delete voice
        runtime
            .send(
                VoiceMessage::Delete {
                    id: VoiceId::new(1),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;
    }

    #[tokio::test]
    async fn test_transport_start_stop() {
        let mut runtime = Runtime::new(MockBackend);

        // Start transport
        runtime.send(TransportMessage::Start.into()).await.unwrap();
        runtime.tick().await;

        assert!(runtime.transport.is_playing().await);

        // Stop transport
        runtime.send(TransportMessage::Stop.into()).await.unwrap();
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
            .send(
                GroupMessage::Create {
                    id: GroupId::new(1),
                    name: "test_group".to_string(),
                    parent: None,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Create child group
        runtime
            .send(
                GroupMessage::Create {
                    id: GroupId::new(2),
                    name: "child_group".to_string(),
                    parent: Some(GroupId::new(1)),
                }
                .into(),
            )
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
        use crate::message::{GroupMessage, PatternMessage, SynthDefMessage, VoiceMessage};
        use crate::traits::{PatternConfig, VoiceConfig};
        use crate::types::{Beat, GroupId, PatternId, VoiceId};

        let mut runtime = Runtime::new(MockBackend);

        // First register the synthdef
        runtime
            .send(
                SynthDefMessage::Load {
                    name: "test".to_string(),
                    data: vec![],
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Setup: create group and voice
        runtime
            .send(
                GroupMessage::Create {
                    id: GroupId::new(1),
                    name: "test_group".to_string(),
                    parent: None,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime
            .send(
                VoiceMessage::Create {
                    id: VoiceId::new(1),
                    config: Box::new(VoiceConfig::new("test_voice", "test", GroupId::new(1))),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Create pattern
        let config = PatternConfig::new("test_pattern", VoiceId::new(1), Beat::from_f64(4.0));
        runtime
            .send(
                PatternMessage::Create {
                    id: PatternId::new(1),
                    config,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Verify pattern exists
        {
            let state = runtime.state.read().await;
            assert!(state.patterns.contains_key(&PatternId::new(1)));
        }

        // Start pattern
        runtime
            .send(
                PatternMessage::Start {
                    id: PatternId::new(1),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Stop pattern
        runtime
            .send(
                PatternMessage::Stop {
                    id: PatternId::new(1),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Delete pattern
        runtime
            .send(
                PatternMessage::Delete {
                    id: PatternId::new(1),
                }
                .into(),
            )
            .await
            .unwrap();
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
        handle1
            .send(TransportMessage::SetTempo { bpm: 120.0 }.into())
            .await
            .unwrap();
        handle2
            .send(TransportMessage::SetTempo { bpm: 130.0 }.into())
            .await
            .unwrap();
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
            .send(
                GroupMessage::Create {
                    id: GroupId::new(1),
                    name: "test_group".to_string(),
                    parent: None,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime
            .send(
                GroupMessage::Create {
                    id: GroupId::new(2),
                    name: "test_group2".to_string(),
                    parent: None,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Solo group 1
        runtime
            .send(
                GroupMessage::Solo {
                    id: GroupId::new(1),
                    solo: true,
                }
                .into(),
            )
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
            .send(
                GroupMessage::Solo {
                    id: GroupId::new(1),
                    solo: false,
                }
                .into(),
            )
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
            .send(
                GroupMessage::Create {
                    id: GroupId::new(1),
                    name: "test_group".to_string(),
                    parent: None,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Mute the group
        runtime
            .send(
                GroupMessage::Mute {
                    id: GroupId::new(1),
                    muted: true,
                }
                .into(),
            )
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
            .send(
                GroupMessage::Mute {
                    id: GroupId::new(1),
                    muted: false,
                }
                .into(),
            )
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
            .send(
                GroupMessage::Create {
                    id: GroupId::new(1),
                    name: "test_group".to_string(),
                    parent: None,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Now reload with updated params
        let mut new_state = ScriptState::new();
        let mut params = ParamMap::new();
        params.insert("amp".to_string(), 0.5);
        new_state.add_group(
            GroupId::new(1),
            GroupConfig {
                name: "test".to_string(),
                parent: None,
                params,
                effects: Vec::new(),
                muted: false,
                soloed: false,
                output_bus: None,
            },
        );

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
            .send(
                GroupMessage::Create {
                    id: GroupId::new(1),
                    name: "test_group".to_string(),
                    parent: None,
                }
                .into(),
            )
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

    // =========================================================================
    // Multi-output Story 7: post-mix invariant tests
    //
    // These tests pin the SC tree spawn order during apply_reload:
    //   voices' synth nodes (Head)
    //     → routes' mixer synths (RoutesHandler::finalize, Phase 4.7)
    //     → group's effect chain (EffectsHandler::add, Phase 4.8)
    //     → group bus (link synth created in Phase 5)
    //     → main bus
    //
    // The runtime must invoke `effects.add` strictly *after* `routes.finalize`
    // so that an effect's `In.ar(group_audio_bus)` reads the post-sum signal
    // that the route mixers have just deposited on the bus.
    // =========================================================================

    /// Recording backend used by the Story 7 finalize-ordering tests.
    ///
    /// Captures every `create_synth` call in invocation order along with the
    /// synthdef name, target node, add action, and `in_bus`/`out_bus`/
    /// `__fx_bus_in` params (the routing-relevant ones). Tests assert against
    /// this transcript to prove the post-mix invariant.
    struct RecordingBackend {
        create_synth_log: std::sync::Mutex<Vec<RecordedSynth>>,
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code)]
    struct RecordedSynth {
        def: String,
        node: NodeId,
        target: NodeId,
        action: AddAction,
        in_bus: Option<f32>,
        out_bus: Option<f32>,
        fx_bus_in: Option<f32>,
        fx_bus_out: Option<f32>,
    }

    impl RecordingBackend {
        fn new() -> Self {
            Self {
                create_synth_log: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn synths(&self) -> Vec<RecordedSynth> {
            self.create_synth_log.lock().unwrap().clone()
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl Backend for RecordingBackend {
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
            def: &str,
            node: NodeId,
            target: NodeId,
            action: AddAction,
            params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            self.create_synth_log.lock().unwrap().push(RecordedSynth {
                def: def.to_string(),
                node,
                target,
                action,
                in_bus: params.get("in_bus").copied(),
                out_bus: params.get("out_bus").copied(),
                fx_bus_in: params.get("__fx_bus_in").copied(),
                fx_bus_out: params.get("__fx_bus_out").copied(),
            });
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

        async fn map_param_to_bus(
            &self,
            _node: NodeId,
            _param: &str,
            _bus: u32,
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

        fn current_time(&self) -> Instant {
            Instant::now()
        }
    }

    /// Pre-register a synthdef name with explicit OutputPort descriptors so
    /// that the voice handler allocates one audio bus per port.
    async fn register_voice_synthdef(
        runtime: &Runtime<RecordingBackend>,
        name: &str,
        ports: Vec<vibelang_dsp::OutputPort>,
    ) {
        let mut state = runtime.state.write().await;
        state.synthdefs.insert(name.to_string());
        state.synthdef_outputs.insert(name.to_string(), ports);
    }

    /// Pre-register an effect synthdef name (no port descriptors needed).
    async fn register_effect_synthdef(runtime: &Runtime<RecordingBackend>, name: &str) {
        runtime.state.write().await.synthdefs.insert(name.to_string());
    }

    #[tokio::test]
    async fn finalize_ordering_two_default_routed_ports_hit_group_reverb_post_sum() {
        // A voice declares two stereo ports `a` and `b`. Both ports are
        // explicitly routed to the same group `main`, which has a reverb
        // effect. The post-mix invariant says that *both* port signals must
        // pass through the reverb — i.e. the reverb's `__fx_bus_in` equals
        // the group's audio bus, and the route mixers for both ports write
        // to that same bus, *before* the reverb runs.
        use crate::handlers::RouteDest;
        use crate::message::ReloadMessage;
        use crate::reload::{EffectConfig, GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{EffectId, GroupId, ParamMap, VoiceId};

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);

        let synthdef_name = "two_port_synth";
        register_voice_synthdef(
            &runtime,
            synthdef_name,
            vec![
                vibelang_dsp::OutputPort {
                    name: "a".to_string(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                vibelang_dsp::OutputPort {
                    name: "b".to_string(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                },
            ],
        )
        .await;
        register_effect_synthdef(&runtime, "reverb").await;

        // Build the script state: one group with reverb, one voice with both
        // ports default-routed (we set the routes explicitly to simulate the
        // default-route insertion that Story 6c performs at voice create).
        let group_id = GroupId::new(1);
        let voice_id = VoiceId::new(1);
        let effect_id = EffectId::new(1);

        let mut new_state = ScriptState::new();
        new_state.add_group(group_id, GroupConfig::default());
        new_state.add_voice(
            voice_id,
            VoiceConfig::new("v", synthdef_name, group_id),
        );
        new_state.add_effect(
            effect_id,
            EffectConfig {
                group: group_id,
                synthdef: "reverb".to_string(),
                params: ParamMap::new(),
            },
        );
        new_state.set_route(voice_id, "a", RouteDest::Group(group_id));
        new_state.set_route(voice_id, "b", RouteDest::Group(group_id));

        runtime
            .send(ReloadMessage::Apply { state: new_state }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Resolve the group's audio bus to compare against in/out_bus params.
        let group_audio_bus = runtime
            .state
            .read()
            .await
            .groups
            .get(&group_id)
            .expect("group exists after reload")
            .audio_bus
            .0 as f32;

        // Resolve each port's bus on the voice — both must be distinct from
        // the group bus, and both mixer synths must write to the group bus.
        let port_buses: HashMap<String, f32> = runtime
            .state
            .read()
            .await
            .voices
            .get(&voice_id)
            .expect("voice exists after reload")
            .output_buses
            .iter()
            .map(|(name, bus)| (name.clone(), bus.0 as f32))
            .collect();
        assert_eq!(port_buses.len(), 2, "voice has two declared ports");

        let log = runtime.backend.synths();

        // Find indices of the route mixers and the reverb effect synth.
        let mixer_indices: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(_, r)| r.def == "port_to_group_link_2")
            .map(|(i, _)| i)
            .collect();
        let reverb_idx = log
            .iter()
            .position(|r| r.def == "reverb")
            .expect("reverb effect was created");

        assert_eq!(
            mixer_indices.len(),
            2,
            "two route mixers spawned (one per port), got log: {:?}",
            log.iter().map(|r| r.def.clone()).collect::<Vec<_>>()
        );

        // Post-mix invariant — both mixers run before the reverb, so the
        // reverb sees the post-sum signal. This is what the runtime's
        // Phase 4.7 → Phase 4.8 ordering guarantees.
        for &mi in &mixer_indices {
            assert!(
                mi < reverb_idx,
                "route mixer at log[{}] must precede reverb at log[{}] — got: {:?}",
                mi,
                reverb_idx,
                log.iter().map(|r| r.def.clone()).collect::<Vec<_>>()
            );
        }

        // Both mixers write to the group's audio bus (the reverb's input).
        for &mi in &mixer_indices {
            let m = &log[mi];
            assert_eq!(
                m.out_bus.unwrap(),
                group_audio_bus,
                "mixer at log[{}] must write to group audio bus",
                mi
            );
            // And reads from one of the voice's port buses.
            let in_bus = m.in_bus.expect("mixer has in_bus param");
            assert!(
                port_buses.values().any(|&b| (b - in_bus).abs() < f32::EPSILON),
                "mixer in_bus {} did not match any voice port bus {:?}",
                in_bus,
                port_buses,
            );
        }

        // Reverb reads from and writes to the same group audio bus — i.e. it
        // processes the summed signal that the mixers just produced.
        let reverb = &log[reverb_idx];
        assert_eq!(
            reverb.fx_bus_in.unwrap(),
            group_audio_bus,
            "reverb's __fx_bus_in must equal group's audio bus"
        );
        assert_eq!(
            reverb.fx_bus_out.unwrap(),
            group_audio_bus,
            "reverb's __fx_bus_out must equal group's audio bus"
        );

        // The link synth runs last (Phase 5) and reads from the group bus.
        let link_idx = log
            .iter()
            .position(|r| r.def == "system_link_audio")
            .expect("link synth was created in Phase 5");
        assert!(
            reverb_idx < link_idx,
            "reverb must precede link synth — got: {:?}",
            log.iter().map(|r| r.def.clone()).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn finalize_ordering_explicit_even_route_through_fx_group_only() {
        // Voice has four mono ports: sine, sub, even, odd.
        //   - sine, sub default-routed to "main"          (bypass reverb)
        //   - even explicitly routed to "fx_evens"        (through reverb)
        //   - odd is muted (no mixer)
        // Group "fx_evens" has reverb, "main" has none. The reverb must read
        // from fx_evens.audio_bus, *not* main.audio_bus, so sine/sub bypass
        // the reverb at the bus level.
        use crate::handlers::RouteDest;
        use crate::message::ReloadMessage;
        use crate::reload::{EffectConfig, GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{EffectId, GroupId, ParamMap, VoiceId};

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);

        let synthdef_name = "spectraphon_lite";
        register_voice_synthdef(
            &runtime,
            synthdef_name,
            vec![
                vibelang_dsp::OutputPort {
                    name: "sine".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                vibelang_dsp::OutputPort {
                    name: "sub".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                vibelang_dsp::OutputPort {
                    name: "even".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                vibelang_dsp::OutputPort {
                    name: "odd".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
            ],
        )
        .await;
        register_effect_synthdef(&runtime, "reverb").await;

        let main_id = GroupId::new(1);
        let fx_evens_id = GroupId::new(2);
        let voice_id = VoiceId::new(1);
        let effect_id = EffectId::new(1);

        let mut new_state = ScriptState::new();
        new_state.add_group(main_id, GroupConfig::default());
        new_state.add_group(fx_evens_id, GroupConfig::default());
        new_state.add_voice(
            voice_id,
            VoiceConfig::new("v", synthdef_name, main_id),
        );
        new_state.add_effect(
            effect_id,
            EffectConfig {
                group: fx_evens_id,
                synthdef: "reverb".to_string(),
                params: ParamMap::new(),
            },
        );
        // sine + sub default-route to main; even routes to fx_evens; odd muted.
        new_state.set_route(voice_id, "sine", RouteDest::Group(main_id));
        new_state.set_route(voice_id, "sub", RouteDest::Group(main_id));
        new_state.set_route(voice_id, "even", RouteDest::Group(fx_evens_id));
        new_state.set_route(voice_id, "odd", RouteDest::Muted);

        runtime
            .send(ReloadMessage::Apply { state: new_state }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let (main_bus, fx_bus) = {
            let s = runtime.state.read().await;
            (
                s.groups.get(&main_id).unwrap().audio_bus.0 as f32,
                s.groups.get(&fx_evens_id).unwrap().audio_bus.0 as f32,
            )
        };
        assert_ne!(main_bus, fx_bus, "groups must have distinct audio buses");

        let port_buses: HashMap<String, f32> = runtime
            .state
            .read()
            .await
            .voices
            .get(&voice_id)
            .unwrap()
            .output_buses
            .iter()
            .map(|(n, b)| (n.clone(), b.0 as f32))
            .collect();

        let log = runtime.backend.synths();

        // Three mono mixers — one per non-muted route. The muted "odd" port
        // produces no mixer.
        let mixers: Vec<&RecordedSynth> = log
            .iter()
            .filter(|r| r.def == "port_to_group_link_1")
            .collect();
        assert_eq!(
            mixers.len(),
            3,
            "three mono mixers expected (sine, sub, even); odd is muted. log: {:?}",
            log.iter().map(|r| r.def.clone()).collect::<Vec<_>>()
        );

        // Look up which port each mixer corresponds to via the in_bus param.
        let bus_to_port: HashMap<u32, String> = port_buses
            .iter()
            .map(|(n, b)| (*b as u32, n.clone()))
            .collect();
        let mut mixer_dest_for_port: HashMap<String, f32> = HashMap::new();
        for m in &mixers {
            let in_bus = m.in_bus.unwrap() as u32;
            let port = bus_to_port
                .get(&in_bus)
                .unwrap_or_else(|| panic!("mixer in_bus {} does not match any port", in_bus));
            mixer_dest_for_port.insert(port.clone(), m.out_bus.unwrap());
        }

        // sine + sub mix into main; even mixes into fx_evens; odd has no mixer.
        assert_eq!(mixer_dest_for_port.get("sine").copied(), Some(main_bus));
        assert_eq!(mixer_dest_for_port.get("sub").copied(), Some(main_bus));
        assert_eq!(mixer_dest_for_port.get("even").copied(), Some(fx_bus));
        assert!(
            !mixer_dest_for_port.contains_key("odd"),
            "muted port must not get a mixer"
        );

        // The reverb reads from fx_evens.audio_bus — i.e. only the even
        // signal flows through it. sine/sub bypass at the bus level since
        // they were summed onto a different group bus.
        let reverb = log
            .iter()
            .find(|r| r.def == "reverb")
            .expect("reverb effect was created");
        assert_eq!(
            reverb.fx_bus_in.unwrap(),
            fx_bus,
            "reverb processes fx_evens bus, not main bus — sine/sub bypass"
        );
        assert_eq!(reverb.fx_bus_out.unwrap(), fx_bus);

        // Ordering invariant — the reverb spawns after every mixer that feeds
        // a bus the reverb might read; specifically, the even-port mixer must
        // precede the reverb so the post-sum signal is on fx_bus when the
        // reverb runs.
        let reverb_idx = log.iter().position(|r| r.def == "reverb").unwrap();
        for (i, m) in log.iter().enumerate() {
            if m.def == "port_to_group_link_1" {
                assert!(
                    i < reverb_idx,
                    "mixer at log[{}] must precede reverb at log[{}]",
                    i,
                    reverb_idx
                );
            }
        }
    }
}
