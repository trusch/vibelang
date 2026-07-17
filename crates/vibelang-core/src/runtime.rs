//! Runtime - the main entry point for vibelang-core.
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
    default_routes_for_voice, EffectsHandler, FadesHandler, GroupsHandler, InputRouteMap,
    InputRouteSrc, MelodiesHandler, ParamRoute, ParamRouteDiff, ParamRouteMap, PatternsHandler,
    RouteMap, RoutesHandler, SamplesHandler, SequencesHandler, SfzHandler, SynthDefsHandler,
    TransportHandler, VoicesHandler,
};
#[cfg(feature = "midi")]
use crate::message::MidiMessage;
#[cfg(not(target_arch = "wasm32"))]
use crate::message::RecordingMessage;
use crate::message::ReloadMessage;
use crate::message::{
    ContextualMessage, EffectMessage, FadeMessage, GroupMessage, MelodyMessage, Message,
    MessageClass, PatternMessage, SampleMessage, SequenceMessage, SfzMessage, SyncMessage,
    SynthDefMessage, TransportMessage, VoiceMessage,
};
#[cfg(feature = "midi")]
use crate::midi::{
    resolve_midi_output_endpoint, MidiOutputEndpoint, QueuedMidiEvent, ScheduledMidiEvent,
};
use crate::mutation::{
    Applied, Atomicity, CommitPhase, ComponentOutcome, ComponentState, Confirmation, Diagnostic,
    DiagnosticSeverity, EffectiveAt, EventQueryResult, FailurePhase, LedgerConfig, LiveState,
    MutationContext, MutationEventSink, MutationKind, MutationLedger, MutationReceipt,
    MutationReplySink, MutationSource, Partial, PlannedComponent, ReceiptState, Rejected,
    RequestMaterial, RollbackState, Submission, SubmissionResult, SupersessionPolicy,
    TerminalOutcome, Timestamp,
};
use crate::reload;
#[cfg(feature = "midi")]
use crate::reload::MidiOutputMessage;
use crate::state::{State, VoiceRole};
#[cfg(feature = "midi")]
use crate::traits::Midi;
#[cfg(not(target_arch = "wasm32"))]
use crate::traits::Recordings;
use crate::traits::{
    Effects, Fades, Groups, Melodies, Patterns, Samples, Sequences, Sfz, SynthDefs, Transport,
    Voices,
};
use crate::transport_snapshot::TransportSnapshot;
use crate::types::VoiceId;
use crate::{Error, Result};
use std::sync::Arc;
use std::time::SystemTime;
use vibelang_dsp::OutputPort;

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
    tx: Sender<ContextualMessage>,

    /// Message receiver.
    rx: Receiver<ContextualMessage>,

    /// Canonical receipt ledger shared with every runtime handle.
    mutation_ledger: MutationLedger,

    /// Explicit v1 best-effort continuation acknowledgement state.
    mutation_policy: Arc<parking_lot::Mutex<MutationPolicy>>,

    /// Native backend barrier currently running off-task. Messages admitted
    /// after it remain deferred until the barrier terminalizes or times out.
    async_mutation_in_flight: Arc<parking_lot::Mutex<Option<crate::mutation::AttemptId>>>,

    /// Transport state snapshot for lock-free sharing with background threads.
    /// Used by MIDI clock thread and modulator polling thread.
    transport_snapshot: Arc<TransportSnapshot>,

    /// Tick counter for reducing frequency of some operations.
    #[cfg(feature = "midi")]
    tick_count: u32,

    /// Whether the MIDI clock thread has been started (for tick() users).
    #[cfg(feature = "midi")]
    clock_thread_started: bool,

    /// Apply-time exact-name output mappings used by MIDI clock and transport.
    #[cfg(feature = "midi")]
    midi_output_endpoints: std::collections::HashMap<String, MidiOutputEndpoint>,

    /// True while a reload's expensive buffer loads (samples, SFZ) are
    /// being staged on a side task. While set, later contextual mutations
    /// wait in [`Self::deferred_mutations`] until the linked completion has
    /// terminalized the earlier revision.
    #[cfg(not(target_arch = "wasm32"))]
    reload_staging_in_flight: bool,

    /// Reload requests that arrived while a staging was in flight, in
    /// arrival order. Applied strictly in order once staging completes.
    #[cfg(not(target_arch = "wasm32"))]
    pending_reloads: std::collections::VecDeque<PendingReload>,

    /// Ordered work accepted behind an off-task reload or backend barrier.
    /// V1-compatible independent handlers may execute immediately, but their
    /// success receipt waits for lower revisions; reload and sync work waits
    /// in full. Both forms retain admission order here.
    #[cfg(not(target_arch = "wasm32"))]
    deferred_mutations: std::collections::VecDeque<DeferredMutation>,

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

    /// Legacy MIDI callback/hotplug ingress, promoted into contextual runtime
    /// messages at the start of each tick.
    #[cfg(feature = "midi")]
    midi_runtime_rx: Receiver<Message>,
}

#[derive(Debug, Default)]
struct MutationPolicy {
    acknowledged_fence: Option<crate::mutation::AttemptId>,
}

#[derive(Clone, Debug)]
struct PendingReload {
    state: reload::ScriptState,
    context: MutationContext,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
enum DeferredMutation {
    Execute(ContextualMessage),
    Complete(DeferredCompletion),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct DeferredCompletion {
    context: MutationContext,
    component_path: String,
    action: String,
}

#[derive(Clone, Debug)]
struct PendingVoicePortReconcile {
    voice_id: VoiceId,
    /// Old port set snapshotted BEFORE any reconcile runs. Voices sharing a
    /// synthdef must each reconcile against this snapshot — the first
    /// voice's reconcile overwrites `state.synthdef_outputs`, so a live
    /// lookup for later voices would compare new-vs-new and no-op.
    old_ports: Vec<OutputPort>,
    new_ports: Vec<OutputPort>,
    refreshed_ports: Vec<String>,
}

/// The buffer loads a reload needs before it can apply, in deterministic
/// (raw-ID-sorted) order. Computed on the runtime task from cheap state
/// reads; executed off-task by [`Runtime::spawn_reload_staging`].
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default)]
struct ReloadStagingPlan {
    /// New or content-changed samples to load (created entries, plus
    /// updated ones whose path or source-file mtime changed — those swap
    /// to a fresh buffer at apply time).
    samples: Vec<(crate::types::SampleId, crate::traits::SampleConfig)>,
    /// New or path-changed SFZ instruments to load.
    sfz: Vec<(crate::types::SfzId, std::path::PathBuf)>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ReloadStagingPlan {
    fn is_empty(&self) -> bool {
        self.samples.is_empty() && self.sfz.is_empty()
    }
}

const RELOAD_PHASE_COMPONENTS: [(&str, &str); 16] = [
    ("reload/transport", "apply_changes"),
    ("reload/stop_deleted", "stop"),
    ("reload/delete_entities", "delete"),
    ("reload/midi_devices", "open"),
    ("reload/create_entities", "create"),
    ("reload/update_entities", "update"),
    ("reload/output_routes", "finalize"),
    ("reload/input_routes", "finalize"),
    ("reload/effects", "apply"),
    ("reload/groups", "finalize"),
    ("reload/fades", "apply"),
    ("reload/patterns", "reconcile_playback"),
    ("reload/voices", "reconcile_running"),
    ("reload/param_routes", "finalize"),
    ("reload/midi_routes", "apply"),
    ("reload/staged_assets", "discard_leftovers"),
];

#[derive(Debug)]
struct ReloadPhaseFailure {
    code: &'static str,
    message: String,
}

impl ReloadPhaseFailure {
    fn new(code: &'static str, error: impl std::fmt::Display) -> Self {
        Self {
            code,
            message: error.to_string(),
        }
    }
}

fn record_reload_result<T, E: std::fmt::Display>(
    failures: &mut Vec<ReloadPhaseFailure>,
    code: &'static str,
    result: std::result::Result<T, E>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            failures.push(ReloadPhaseFailure::new(code, error));
            None
        }
    }
}

#[derive(Debug)]
struct ReloadPhaseOutcome {
    path: &'static str,
    action: &'static str,
    failures: Vec<ReloadPhaseFailure>,
    started: bool,
}

impl ReloadPhaseOutcome {
    fn pending(path: &'static str, action: &'static str) -> Self {
        Self {
            path,
            action,
            failures: Vec::new(),
            started: false,
        }
    }
}

#[derive(Debug)]
struct ReloadExecution {
    result: Result<()>,
    staging: Vec<reload::StagedAssetOutcome>,
    phases: Vec<ReloadPhaseOutcome>,
}

/// Default value a group param falls back to when removed from the script.
///
/// Group params only have a well-defined synthdef default for `amp`/`pan` —
/// the two params `GroupsHandler::set_param` routes to the group's link synth
/// (`system_link_audio` / `system_link_audio_mono`). The defaults are looked
/// up in the process-global synthdef registry when the system synthdefs are
/// registered there, otherwise the declared IR defaults are hardcoded
/// (`amp = 1.0` mirrors the `unwrap_or(1.0)` in `GroupsHandler::finalize`,
/// `pan = 0.0` mirrors `system_synthdefs::routing`). Every other group param
/// returns `None`: it is broadcast to the group node's children and has no
/// single default.
fn group_link_param_default(param: &str) -> Option<f32> {
    let fallback = match param {
        "amp" => 1.0,
        "pan" => 0.0,
        _ => return None,
    };
    Some(
        vibelang_dsp::get_synthdef_param_defaults("system_link_audio")
            .get(param)
            .copied()
            .unwrap_or(fallback),
    )
}

impl<B: Backend> Runtime<B> {
    /// Create a new runtime with the given backend.
    ///
    /// The runtime creates an internal message channel with a buffer of 1024
    /// messages. Use [`Runtime::handle()`] to get a cloneable sender.
    ///
    /// Uses [`State::default()`] for the initial state, which configures the
    /// audio bus allocator with a fixed 16-bus hardware reserve. Production
    /// code paths that know the actual scsynth `-i`/`-o` channel counts
    /// should use [`Runtime::new_with_audio_config`] instead so user bus
    /// allocation cannot collide with hardware input/output buses.
    pub fn new(backend: B) -> Self {
        Self::new_with_state(backend, State::default())
    }

    /// Create a new runtime configured for a known hardware I/O layout.
    ///
    /// Threads the scsynth `-i input_channels -o output_channels` settings
    /// through to [`State::with_audio_config`] so the audio bus allocator
    /// starts handing out user buses at exactly `output_channels +
    /// input_channels`, the first private bus index in scsynth's contiguous
    /// `[outputs | inputs | private]` layout. This is the constructor every
    /// CLI / driver path should use.
    pub fn new_with_audio_config(backend: B, output_channels: u32, input_channels: u32) -> Self {
        Self::new_with_state(
            backend,
            State::with_audio_config(output_channels, input_channels),
        )
    }

    fn new_with_state(backend: B, state: State) -> Self {
        Self::new_with_state_and_channel_capacity(backend, state, 1024)
    }

    #[cfg(test)]
    fn new_with_channel_capacity(backend: B, capacity: usize) -> Self {
        Self::new_with_state_and_channel_capacity(backend, State::default(), capacity)
    }

    fn new_with_state_and_channel_capacity(backend: B, state: State, capacity: usize) -> Self {
        let (tx, rx) = channel(capacity);
        let backend = Arc::new(backend);
        let state = Arc::new(RwLock::new(state));
        let transport_snapshot = Arc::new(TransportSnapshot::new());
        let mutation_ledger = MutationLedger::new(LedgerConfig::default())
            .expect("default mutation ledger configuration must initialize");
        let mutation_policy = Arc::new(parking_lot::Mutex::new(MutationPolicy::default()));
        let async_mutation_in_flight = Arc::new(parking_lot::Mutex::new(None));

        // Create MIDI handler first so we can share output channels with voices
        #[cfg(feature = "midi")]
        let (midi_runtime_tx, midi_runtime_rx) = channel(1024);
        #[cfg(feature = "midi")]
        let midi = MidiHandler::new(backend.clone(), state.clone(), midi_runtime_tx);

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
            mutation_ledger,
            mutation_policy,
            async_mutation_in_flight,
            transport_snapshot,
            transport: TransportHandler::new(backend.clone(), state.clone()),
            groups: GroupsHandler::new(backend.clone(), state.clone()),
            voices: voices.clone(),
            patterns: PatternsHandler::new(state.clone(), voices.clone()),
            melodies: MelodiesHandler::new(state.clone(), voices.clone()),
            sequences: SequencesHandler::new(backend.clone(), state.clone()),
            fades: FadesHandler::new(backend.clone(), state.clone()),
            effects: EffectsHandler::new(backend.clone(), state.clone()),
            samples: SamplesHandler::new(backend.clone(), state.clone()),
            sfz: SfzHandler::new(backend.clone(), state.clone()),
            routes: RoutesHandler::new(backend.clone(), state.clone()),
            #[cfg(not(target_arch = "wasm32"))]
            recordings: RecordingsHandler::new(backend.clone(), state.clone()),
            synthdefs: SynthDefsHandler::new(backend.clone(), state.clone()),
            #[cfg(feature = "midi")]
            midi,
            #[cfg(feature = "midi")]
            midi_runtime_rx,
            #[cfg(feature = "midi")]
            midi_output_endpoints: std::collections::HashMap::new(),
            #[cfg(feature = "midi")]
            tick_count: 0,
            #[cfg(feature = "midi")]
            clock_thread_started: false,
            #[cfg(not(target_arch = "wasm32"))]
            reload_staging_in_flight: false,
            #[cfg(not(target_arch = "wasm32"))]
            pending_reloads: std::collections::VecDeque::new(),
            #[cfg(not(target_arch = "wasm32"))]
            deferred_mutations: std::collections::VecDeque::new(),
        }
    }

    /// Get a cloneable handle for sending messages.
    ///
    /// Handles can be sent across threads and cloned freely.
    pub fn handle(&self) -> RuntimeHandle {
        RuntimeHandle {
            tx: self.tx.clone(),
            ledger: self.mutation_ledger.clone(),
            policy: self.mutation_policy.clone(),
            async_mutation_in_flight: self.async_mutation_in_flight.clone(),
        }
    }

    /// Send a message directly (convenience method).
    ///
    /// Equivalent to `runtime.handle().send(msg).await`.
    pub async fn send(&self, msg: Message) -> Result<()> {
        self.handle().send(msg).await
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
            // Watch for MIDI devices appearing/disappearing so inputs recover
            // from unplug/replug and power-on-after-start.
            #[cfg(not(target_arch = "wasm32"))]
            self.midi.start_input_hotplug_watcher();
        }

        loop {
            // Process available messages
            while let Some(msg) = self.try_next_contextual_message() {
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

                    // Stop MIDI clock thread + hot-plug watcher
                    #[cfg(feature = "midi")]
                    {
                        self.midi.stop_clock_thread();
                        #[cfg(not(target_arch = "wasm32"))]
                        self.midi.stop_input_hotplug_watcher();
                    }

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
            // Same first-tick guard starts the input hot-plug watcher, so the
            // CLI (which drives the runtime via tick(), not run()) gets device
            // recovery too.
            #[cfg(not(target_arch = "wasm32"))]
            self.midi.start_input_hotplug_watcher();
            self.clock_thread_started = true;
        }

        // Process all pending messages
        while let Some(msg) = self.try_next_contextual_message() {
            if let Err(e) = self.handle_message(msg).await {
                tracing::warn!("Message handling error: {}", e);
            }
        }

        // Tick handlers
        self.tick_internal().await;
    }

    /// Internal tick for handlers.
    async fn tick_internal(&mut self) {
        #[cfg(feature = "midi")]
        while let Some(message) = self.midi_runtime_rx.try_recv_compat() {
            if let Err(error) = self.dispatch_message(message).await {
                tracing::warn!("MIDI runtime maintenance failed: {}", error);
            }
        }

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

        // Tick voices/groups to process deferred teardown: fallback frees,
        // node-ID recycling, and bus reclaim for gate-released nodes.
        self.voices.tick().await;
        self.groups.tick().await;

        // Tick sample/SFZ buffer frees deferred past their grace period
        // (buffers displaced by in-place content reloads or unloads).
        self.samples.tick().await;
        self.sfz.tick().await;

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

        #[cfg(feature = "midi")]
        {
            self.tick_count = self.tick_count.wrapping_add(1);
        }
    }

    /// Handle one context-bearing message and publish its canonical outcome.
    async fn handle_message(&mut self, envelope: ContextualMessage) -> Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        if (self.reload_staging_in_flight || self.async_mutation_in_flight.lock().is_some())
            && !matches!(
                &envelope.message,
                Message::Reload(reload)
                    if matches!(reload.as_ref(), ReloadMessage::ApplyStaged { .. })
            )
        {
            return self.handle_during_async_boundary(envelope).await;
        }
        let ContextualMessage { context, message } = envelope;
        let receipt = self
            .mutation_ledger
            .receipt(context.attempt_id())
            .map_err(mutation_ledger_error)?;
        if matches!(receipt.state, ReceiptState::Evaluating { .. }) {
            return Err(Error::MutationLedger(
                "runtime received a mutation before queue admission completed".into(),
            ));
        }
        let context = if let Some(revision) = receipt.revision {
            context
                .with_revision(revision)
                .map_err(Error::MutationLedger)?
        } else {
            context
        };
        if receipt.state.is_terminal() {
            return Ok(());
        }
        if mutation_is_fenced(&self.mutation_ledger, &self.mutation_policy) {
            reject_contextual_admission(
                &self.mutation_ledger,
                &context,
                "runtime_fenced",
                "a partial or unknown mutation must be acknowledged before continuing",
                SystemTime::now(),
            )?;
            if let Message::Sync(SyncMessage::SyncAndNotify { notify }) = message {
                let _ = notify.send(Err("runtime_fenced".into()));
            }
            return Ok(());
        }

        match message {
            Message::Reload(reload) => self.handle_reload_message(*reload, context).await,
            Message::Sync(sync) => self.handle_sync_message(sync, context).await,
            message => {
                let component_path = message.component_path();
                let action = message.operation().to_lowercase();
                self.begin_contextual_work(&context, &component_path, &action)?;
                let result = self.dispatch_message(message).await;
                self.finish_contextual_work(
                    &context,
                    &component_path,
                    &action,
                    result.as_ref().err(),
                    Confirmation::RuntimeCommit,
                )?;
                result
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn handle_during_async_boundary(&mut self, envelope: ContextualMessage) -> Result<()> {
        if matches!(&envelope.message, Message::Reload(_) | Message::Sync(_)) {
            self.deferred_mutations
                .push_back(DeferredMutation::Execute(envelope));
            return Ok(());
        }

        let ContextualMessage { context, message } = envelope;
        let receipt = self
            .mutation_ledger
            .receipt(context.attempt_id())
            .map_err(mutation_ledger_error)?;
        if matches!(receipt.state, ReceiptState::Evaluating { .. }) {
            return Err(Error::MutationLedger(
                "runtime received a mutation before queue admission completed".into(),
            ));
        }
        let context = if let Some(revision) = receipt.revision {
            context
                .with_revision(revision)
                .map_err(Error::MutationLedger)?
        } else {
            context
        };
        if receipt.state.is_terminal() {
            return Ok(());
        }
        if mutation_is_fenced(&self.mutation_ledger, &self.mutation_policy) {
            reject_contextual_admission(
                &self.mutation_ledger,
                &context,
                "runtime_fenced",
                "a partial or unknown mutation must be acknowledged before continuing",
                SystemTime::now(),
            )?;
            return Ok(());
        }

        let component_path = message.component_path();
        let action = message.operation().to_lowercase();
        let result = self.dispatch_message(message).await;
        if let Err(error) = &result {
            finish_contextual_receipt(
                &self.mutation_ledger,
                &context,
                &component_path,
                &action,
                Some(WorkFailure {
                    code: error_code(error),
                    message: error.to_string(),
                    definitely_no_effect: error_is_pre_effect(error),
                    phase: FailurePhase::Reconcile,
                }),
                Confirmation::RuntimeCommit,
            )?;
        } else {
            self.deferred_mutations
                .push_back(DeferredMutation::Complete(DeferredCompletion {
                    context,
                    component_path,
                    action,
                }));
        }
        result
    }

    fn try_next_contextual_message(&mut self) -> Option<ContextualMessage> {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.reload_staging_in_flight && self.async_mutation_in_flight.lock().is_none() {
            loop {
                match self.deferred_mutations.pop_front() {
                    Some(DeferredMutation::Execute(message)) => return Some(message),
                    Some(DeferredMutation::Complete(completion)) => {
                        if let Err(error) = self.finish_deferred_completion(completion) {
                            tracing::error!(
                                "Failed to publish deferred v1 mutation completion: {}",
                                error
                            );
                        }
                    }
                    None => break,
                }
            }
        }
        self.rx.try_recv_compat()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_deferred_completion(&self, completion: DeferredCompletion) -> Result<()> {
        if mutation_is_fenced(&self.mutation_ledger, &self.mutation_policy) {
            finish_deferred_effect_after_fence(&self.mutation_ledger, &completion)?;
        } else {
            self.begin_contextual_work(
                &completion.context,
                &completion.component_path,
                &completion.action,
            )?;
            self.finish_contextual_work(
                &completion.context,
                &completion.component_path,
                &completion.action,
                None,
                Confirmation::RuntimeCommit,
            )?;
        }
        Ok(())
    }

    /// Dispatch a non-reload, non-sync message to its existing v1 handler.
    async fn dispatch_message(&mut self, msg: Message) -> Result<()> {
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
                // Gate-release teardown: sounding nodes tail out via their
                // release envelope instead of clicking off.
                VoiceMessage::Delete { id } => self.voices.graceful_delete(id).await,
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
                PatternMessage::Create { id, config, owner } => {
                    self.patterns.create_with_owner(id, config, owner).await
                }
                PatternMessage::Delete { id } => self.patterns.delete(id).await,
                PatternMessage::Start { id } => self.patterns.start(id).await,
                PatternMessage::StartOnGrid { id } => self.patterns.start_on_grid(id).await,
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

            // Reload - apply new script state. On native, expensive buffer
            // loads (samples, SFZ) are staged on a side task first so the
            // tick loop keeps running; the apply itself stays on this task.
            Message::Reload(_) => unreachable!("reload messages use handle_reload_message"),

            // Sync - synchronize with backend and notify caller
            Message::Sync(_) => unreachable!("sync messages use handle_sync_message"),

            // MIDI - send to external devices
            #[cfg(feature = "midi")]
            Message::Midi(midi_msg) => match midi_msg {
                // Device management
                MidiMessage::OpenInput { device } => self.midi.open_input(device).await,
                MidiMessage::OpenOutput { device } => self.midi.open_output(device).await,
                MidiMessage::CloseDevice { device } => self.midi.close(device).await,
                MidiMessage::ReconcileInputs { present } => {
                    self.midi.reconcile_pipewire_inputs(present).await;
                    Ok(())
                }

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
                    self.midi.stop_recording(device).await.map(|_| ())
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

    fn begin_contextual_work(
        &self,
        context: &MutationContext,
        component_path: &str,
        action: &str,
    ) -> Result<()> {
        let now = SystemTime::now();
        let planning = self
            .mutation_ledger
            .begin_planning(
                context.attempt_id(),
                vec![PlannedComponent {
                    path: component_path.into(),
                    action: action.into(),
                }],
                now,
            )
            .map_err(mutation_ledger_error)?;
        publish_mutation_transition(&self.mutation_ledger, context, &planning, now);
        let committing = self
            .mutation_ledger
            .transition(
                context.attempt_id(),
                ReceiptState::Committing {
                    phase: CommitPhase::Reconcile,
                },
                SystemTime::now(),
            )
            .map_err(mutation_ledger_error)?;
        publish_mutation_transition(
            &self.mutation_ledger,
            context,
            &committing,
            SystemTime::now(),
        );
        Ok(())
    }

    fn finish_contextual_work(
        &self,
        context: &MutationContext,
        component_path: &str,
        action: &str,
        error: Option<&Error>,
        confirmation: Confirmation,
    ) -> Result<MutationReceipt> {
        finish_contextual_receipt(
            &self.mutation_ledger,
            context,
            component_path,
            action,
            error.map(|error| WorkFailure {
                code: error_code(error),
                message: error.to_string(),
                definitely_no_effect: error_is_pre_effect(error),
                phase: FailurePhase::Reconcile,
            }),
            confirmation,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn begin_reload_work(&self, context: &MutationContext, plan: &ReloadStagingPlan) -> Result<()> {
        let mut planned =
            Vec::with_capacity(RELOAD_PHASE_COMPONENTS.len() + plan.samples.len() + plan.sfz.len());
        planned.extend(plan.samples.iter().map(|(id, _)| PlannedComponent {
            path: format!("reload/staging/sample/{}", id.raw()),
            action: "load".into(),
        }));
        planned.extend(plan.sfz.iter().map(|(id, _)| PlannedComponent {
            path: format!("reload/staging/sfz/{}", id.raw()),
            action: "load".into(),
        }));
        planned.extend(
            RELOAD_PHASE_COMPONENTS
                .iter()
                .map(|(path, action)| PlannedComponent {
                    path: (*path).into(),
                    action: (*action).into(),
                }),
        );
        self.begin_contextual_components(context, planned, !plan.is_empty())
    }

    #[cfg(target_arch = "wasm32")]
    fn begin_reload_work(&self, context: &MutationContext) -> Result<()> {
        let planned = RELOAD_PHASE_COMPONENTS
            .iter()
            .map(|(path, action)| PlannedComponent {
                path: (*path).into(),
                action: (*action).into(),
            })
            .collect();
        self.begin_contextual_components(context, planned, false)
    }

    fn begin_contextual_components(
        &self,
        context: &MutationContext,
        planned: Vec<PlannedComponent>,
        staging: bool,
    ) -> Result<()> {
        let now = SystemTime::now();
        let total = u32::try_from(planned.len().saturating_sub(RELOAD_PHASE_COMPONENTS.len()))
            .unwrap_or(u32::MAX);
        let planning = self
            .mutation_ledger
            .begin_planning(context.attempt_id(), planned, now)
            .map_err(mutation_ledger_error)?;
        publish_mutation_transition(&self.mutation_ledger, context, &planning, now);
        let state = if staging {
            ReceiptState::Staging {
                completed: 0,
                total,
            }
        } else {
            ReceiptState::Committing {
                phase: CommitPhase::Reconcile,
            }
        };
        let transitioned = self
            .mutation_ledger
            .transition(context.attempt_id(), state, SystemTime::now())
            .map_err(mutation_ledger_error)?;
        publish_mutation_transition(
            &self.mutation_ledger,
            context,
            &transitioned,
            SystemTime::now(),
        );
        Ok(())
    }

    fn enter_reload_commit(&self, context: &MutationContext) -> Result<()> {
        let committing = self
            .mutation_ledger
            .transition(
                context.attempt_id(),
                ReceiptState::Committing {
                    phase: CommitPhase::Reconcile,
                },
                SystemTime::now(),
            )
            .map_err(mutation_ledger_error)?;
        publish_mutation_transition(
            &self.mutation_ledger,
            context,
            &committing,
            SystemTime::now(),
        );
        Ok(())
    }

    fn finish_reload_work(
        &self,
        context: &MutationContext,
        execution: &ReloadExecution,
    ) -> Result<MutationReceipt> {
        finish_reload_receipt(&self.mutation_ledger, context, execution)
    }

    async fn handle_reload_message(
        &mut self,
        message: ReloadMessage,
        context: MutationContext,
    ) -> Result<()> {
        match message {
            ReloadMessage::Apply { state } => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.pending_reloads
                        .push_back(PendingReload { state, context });
                    self.advance_reload_queue().await;
                    Ok(())
                }
                #[cfg(target_arch = "wasm32")]
                {
                    self.begin_reload_work(&context)?;
                    let execution = self
                        .apply_reload_execution(state, reload::StagedReloadAssets::default())
                        .await;
                    self.finish_reload_work(&context, &execution)?;
                    execution.result
                }
            }
            ReloadMessage::ApplyStaged { state, assets } => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.reload_staging_in_flight = false;
                    self.enter_reload_commit(&context)?;
                    let execution = self.apply_reload_execution(state, assets).await;
                    self.finish_reload_work(&context, &execution)?;
                    self.advance_reload_queue().await;
                    execution.result
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let execution = self.apply_reload_execution(state, assets).await;
                    self.finish_reload_work(&context, &execution)?;
                    execution.result
                }
            }
        }
    }

    async fn handle_sync_message(
        &mut self,
        message: SyncMessage,
        context: MutationContext,
    ) -> Result<()> {
        const COMPONENT: &str = "sync/backend_barrier";
        const ACTION: &str = "sync_and_wait";
        self.begin_contextual_work(&context, COMPONENT, ACTION)?;
        let SyncMessage::SyncAndNotify { notify } = message;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let attempt_id = context.attempt_id();
            *self.async_mutation_in_flight.lock() = Some(attempt_id);
            let backend = self.backend.clone();
            let ledger = self.mutation_ledger.clone();
            let async_mutation_in_flight = self.async_mutation_in_flight.clone();
            tokio::spawn(async move {
                let result = backend.sync().await;
                let failure = result.as_ref().err().map(|error| WorkFailure {
                    code: "backend_sync_failed",
                    message: error.to_string(),
                    definitely_no_effect: false,
                    phase: FailurePhase::BackendBarrier,
                });
                let receipt_result = finish_contextual_receipt(
                    &ledger,
                    &context,
                    COMPONENT,
                    ACTION,
                    failure,
                    Confirmation::BackendBarrier {
                        backend: "runtime".into(),
                        token: attempt_id.to_string(),
                    },
                );
                let response = match result {
                    Ok(()) => receipt_result
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                };
                clear_async_mutation(&async_mutation_in_flight, attempt_id);
                let _ = notify.send(response);
            });
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let result = self.backend.sync().await;
            let failure = result.as_ref().err().map(|error| WorkFailure {
                code: "backend_sync_failed",
                message: error.to_string(),
                definitely_no_effect: false,
                phase: FailurePhase::BackendBarrier,
            });
            finish_contextual_receipt(
                &self.mutation_ledger,
                &context,
                COMPONENT,
                ACTION,
                failure,
                Confirmation::BackendBarrier {
                    backend: "runtime".into(),
                    token: context.attempt_id().to_string(),
                },
            )?;
            notify
                .send(result.map_err(|error| error.to_string()))
                .map_err(|_| Error::ChannelClosed)?;
            Ok(())
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
        self.backend.sync().await.map_err(Error::backend)?;

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

    async fn effective_input_routes(&self, new_state: &reload::ScriptState) -> InputRouteMap {
        let mut routes = InputRouteMap::new();
        {
            let state = self.state.read().await;
            for (voice_id, config) in &new_state.voices {
                for input in state.synthdef_inputs(&config.synthdef) {
                    if input.rate == vibelang_dsp::PortRate::Ar && matches!(input.channels, 1 | 2) {
                        let has_explicit_route = new_state
                            .input_routes
                            .contains_key(&(*voice_id, input.name.clone()));
                        let src = if has_explicit_route {
                            continue;
                        } else if input.name == "in" && input.channels == 2 {
                            InputRouteSrc::Group(config.group)
                        } else {
                            if input.name == "in" && input.channels == 1 {
                                tracing::warn!(
                                    "Named input 'in' on voice {:?} is mono; parent-group autofeed requires stereo, leaving input silent",
                                    voice_id
                                );
                            }
                            InputRouteSrc::Silent
                        };
                        routes.insert((*voice_id, input.name), vec![src]);
                    }
                }
            }
        }
        for (key, srcs) in &new_state.input_routes {
            routes.insert(key.clone(), srcs.clone());
        }
        routes
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
    fn voice_needs_structural_recreate(
        current: &crate::traits::VoiceConfig,
        new: &crate::traits::VoiceConfig,
    ) -> bool {
        current.synthdef != new.synthdef
            || current.group != new.group
            || current.sfz_instrument != new.sfz_instrument
    }

    #[cfg(test)]
    async fn apply_reload(&mut self, new_state: reload::ScriptState) -> Result<()> {
        self.apply_reload_execution(new_state, reload::StagedReloadAssets::default())
            .await
            .result
    }

    async fn structurally_recreated_voice_ids(
        &self,
        diff: &reload::ReloadDiff,
        new_state: &reload::ScriptState,
    ) -> Vec<VoiceId> {
        let state = self.state.read().await;
        let mut ids: Vec<_> = diff
            .voices
            .updated
            .iter()
            .filter_map(|(id, new_config)| {
                let current = state.voices.get(id)?;
                (Self::voice_needs_structural_recreate(&current.config, new_config)
                    || reload::synthdef_body_changed(&state, new_state, &new_config.synthdef))
                .then_some(*id)
            })
            .collect();
        ids.sort_by_key(|id| {
            (
                new_state
                    .voice_order
                    .iter()
                    .position(|ordered| ordered == id)
                    .unwrap_or(usize::MAX),
                id.raw(),
            )
        });
        ids
    }

    fn push_param_route_once(routes: &mut Vec<ParamRoute>, route: ParamRoute) {
        if !routes.contains(&route) {
            routes.push(route);
        }
    }

    fn force_param_route_refresh_for_voice(
        old: &ParamRouteMap,
        new: &ParamRouteMap,
        diff: &mut ParamRouteDiff,
        voice_id: VoiceId,
    ) {
        for ((source_voice, source_port), targets) in old {
            for (target, target_param) in targets {
                if *source_voice == voice_id
                    || *target == crate::handlers::ParamRouteTarget::Voice(voice_id)
                {
                    Self::push_param_route_once(
                        &mut diff.removals,
                        ParamRoute {
                            source_voice: *source_voice,
                            source_port: source_port.clone(),
                            target: *target,
                            target_param: target_param.clone(),
                        },
                    );
                }
            }
        }

        for ((source_voice, source_port), targets) in new {
            for (target, target_param) in targets {
                if *source_voice == voice_id
                    || *target == crate::handlers::ParamRouteTarget::Voice(voice_id)
                {
                    Self::push_param_route_once(
                        &mut diff.additions,
                        ParamRoute {
                            source_voice: *source_voice,
                            source_port: source_port.clone(),
                            target: *target,
                            target_param: target_param.clone(),
                        },
                    );
                }
            }
        }
    }

    fn force_param_route_refresh(
        old: &ParamRouteMap,
        new: &ParamRouteMap,
        diff: &mut ParamRouteDiff,
        voice_id: VoiceId,
        port_name: &str,
    ) {
        let key = (voice_id, port_name.to_string());
        if let Some(targets) = old.get(&key) {
            for (target, target_param) in targets {
                let route = ParamRoute {
                    source_voice: voice_id,
                    source_port: port_name.to_string(),
                    target: *target,
                    target_param: target_param.clone(),
                };
                if !diff.removals.contains(&route) {
                    diff.removals.push(route);
                }
            }
        }
        if let Some(targets) = new.get(&key) {
            for (target, target_param) in targets {
                let route = ParamRoute {
                    source_voice: voice_id,
                    source_port: port_name.to_string(),
                    target: *target,
                    target_param: target_param.clone(),
                };
                if !diff.additions.contains(&route) {
                    diff.additions.push(route);
                }
            }
        }
    }

    async fn pending_voice_port_reconciles(
        &self,
        new_state: &reload::ScriptState,
    ) -> Vec<PendingVoicePortReconcile> {
        let state = self.state.read().await;
        let mut pending = Vec::new();

        let mut voice_ids: Vec<_> = state.voices.keys().copied().collect();
        voice_ids.sort_by_key(|id| id.raw());

        for voice_id in voice_ids {
            let Some(voice) = state.voices.get(&voice_id) else {
                continue;
            };
            let synthdef = &voice.config.synthdef;
            if new_state
                .voices
                .get(&voice_id)
                .is_none_or(|config| config.synthdef != *synthdef)
            {
                continue;
            }
            let Some(new_ports) = vibelang_dsp::get_synthdef_outputs(synthdef) else {
                continue;
            };
            let old_ports = state.synthdef_outputs(synthdef);
            if old_ports == new_ports {
                continue;
            }

            let diff = reload::diff_port_set(&old_ports, &new_ports);
            let mut refreshed_ports = Vec::new();
            for port in &diff.removed {
                let new_rate = diff
                    .added
                    .iter()
                    .find(|added| added.name == port.name)
                    .map(|added| added.rate);
                if new_rate.is_some() {
                    refreshed_ports.push(port.name.clone());
                }
            }

            pending.push(PendingVoicePortReconcile {
                voice_id,
                old_ports,
                new_ports,
                refreshed_ports,
            });
        }

        pending
    }

    async fn apply_voice_port_reconciles(
        &mut self,
        pending: &[PendingVoicePortReconcile],
        effective_routes: &mut RouteMap,
    ) -> Vec<ReloadPhaseFailure> {
        if pending.is_empty() {
            return Vec::new();
        }

        let mut state = self.state.write().await;
        let mut failures = Vec::new();

        for reconcile in pending {
            let outcome = match reload::reconcile_voice_ports_from(
                &mut state,
                reconcile.voice_id,
                &reconcile.old_ports,
                &reconcile.new_ports,
                effective_routes,
            ) {
                Ok(outcome) => outcome,
                Err(e) => {
                    // Bus allocation failed (ID space exhausted) — skip this
                    // voice's port reconcile rather than aborting the reload.
                    tracing::error!(
                        "Reload: port reconcile failed for voice {:?}: {}",
                        reconcile.voice_id,
                        e
                    );
                    failures.push(ReloadPhaseFailure::new(
                        "reload_voice_port_reconcile_failed",
                        e,
                    ));
                    continue;
                }
            };
            let _ = outcome.diff.is_unchanged();

            if let Some(group) = state
                .voices
                .get(&reconcile.voice_id)
                .map(|voice| voice.config.group)
            {
                state
                    .default_routes
                    .retain(|(voice_id, _), _| *voice_id != reconcile.voice_id);
                for (port_name, dests) in default_routes_for_voice(group, &reconcile.new_ports) {
                    state
                        .default_routes
                        .insert((reconcile.voice_id, port_name), dests);
                }
            }
        }

        failures
    }

    async fn apply_voice_roles(&self, roles: &std::collections::HashMap<VoiceId, VoiceRole>) {
        if roles.is_empty() {
            return;
        }

        let mut state = self.state.write().await;
        for (voice_id, role) in roles {
            if let Some(voice) = state.voices.get_mut(voice_id) {
                voice.role = *role;
            }
        }
    }

    /// Processes queued reload requests in arrival order.
    ///
    /// For each queued script state: if its expensive buffer loads (new
    /// samples, new/changed SFZ instruments) are already satisfied, the
    /// reload applies immediately on this task; otherwise the loads are
    /// staged on a side task and the loop stops — the staged apply arrives
    /// later as [`ReloadMessage::ApplyStaged`], which re-enters this queue.
    /// Once the queue drains with no staging in flight, deferred contextual
    /// messages resume in their original admission order.
    #[cfg(not(target_arch = "wasm32"))]
    async fn advance_reload_queue(&mut self) {
        while !self.reload_staging_in_flight {
            let Some(next) = self.pending_reloads.pop_front() else {
                break;
            };
            let plan = self.reload_staging_plan(&next.state).await;
            if let Err(error) = self.begin_reload_work(&next.context, &plan) {
                tracing::error!("Reload planning receipt transition failed: {}", error);
                if let Err(terminal_error) = finish_contextual_receipt(
                    &self.mutation_ledger,
                    &next.context,
                    "reload/planning",
                    "plan",
                    Some(WorkFailure {
                        code: "reload_planning_failed",
                        message: error.to_string(),
                        definitely_no_effect: true,
                        phase: FailurePhase::Reconcile,
                    }),
                    Confirmation::RuntimeCommit,
                ) {
                    tracing::error!(
                        "Reload planning failure could not be terminalized: {}",
                        terminal_error
                    );
                }
                continue;
            }
            if plan.is_empty() {
                // Nothing expensive to load — apply directly (the path every
                // sample-free reload takes, preserving single-tick applies).
                let execution = self
                    .apply_reload_execution(next.state, reload::StagedReloadAssets::default())
                    .await;
                if let Err(error) = self.finish_reload_work(&next.context, &execution) {
                    tracing::error!("Reload receipt transition failed: {}", error);
                }
                if let Err(e) = execution.result {
                    tracing::warn!("Reload apply failed: {}", e);
                }
            } else {
                self.spawn_reload_staging(next.state, plan, next.context);
            }
        }
    }

    /// Computes which buffer-backed assets a reload would have to load.
    ///
    /// Mirrors the sample/SFZ portions of `reload::calculate_diff`:
    /// samples are loaded when created or when their buffer identity
    /// (path or source-file mtime) changed — the apply phases swap those
    /// to a fresh buffer ID; SFZ instruments when created or when their
    /// path changed (the apply tears down and recreates those).
    /// Entries are sorted by raw ID so staged buffer allocation stays
    /// deterministic — cold boot must produce identical buffer IDs on
    /// every run.
    #[cfg(not(target_arch = "wasm32"))]
    async fn reload_staging_plan(&self, new_state: &reload::ScriptState) -> ReloadStagingPlan {
        let state = self.state.read().await;
        let mut samples: Vec<_> = new_state
            .samples
            .iter()
            .filter(|(id, config)| {
                state.samples.get(id).is_none_or(|info| {
                    info.path != config.path || info.source_mtime != config.mtime
                })
            })
            .map(|(id, config)| (*id, config.clone()))
            .collect();
        samples.sort_by_key(|(id, _)| id.raw());
        let mut sfz: Vec<_> = new_state
            .sfz_instruments
            .iter()
            .filter(|(id, config)| {
                state
                    .sfz_instruments
                    .get(id)
                    .map(|current| current.path != config.path)
                    .unwrap_or(true)
            })
            .map(|(id, config)| (*id, config.path.clone()))
            .collect();
        sfz.sort_by_key(|(id, _)| id.raw());
        ReloadStagingPlan { samples, sfz }
    }

    /// Stages a reload's buffer loads on a side task.
    ///
    /// The task performs the file I/O and backend round-trips, then sends
    /// the script state back as [`ReloadMessage::ApplyStaged`] so the apply
    /// itself (cheap state mutation + OSC sends) still runs on the runtime
    /// task, atomically with respect to ticks.
    #[cfg(not(target_arch = "wasm32"))]
    fn spawn_reload_staging(
        &mut self,
        new_state: reload::ScriptState,
        plan: ReloadStagingPlan,
        context: MutationContext,
    ) {
        tracing::debug!(
            "Reload: staging {} sample(s) and {} SFZ instrument(s) off-task",
            plan.samples.len(),
            plan.sfz.len()
        );
        self.reload_staging_in_flight = true;
        let samples_handler = SamplesHandler::new(self.backend.clone(), self.state.clone());
        let sfz_handler = SfzHandler::new(self.backend.clone(), self.state.clone());
        let tx = self.tx.clone();
        let ledger = self.mutation_ledger.clone();
        tokio::spawn(async move {
            let mut assets = reload::StagedReloadAssets::default();

            // Samples load concurrently: each future allocates its buffer
            // ID on its first poll (in sorted dispatch order, so IDs stay
            // deterministic) and the backend round-trips overlap.
            let loads = plan.samples.into_iter().map(|(id, config)| {
                let handler = &samples_handler;
                async move {
                    let path = format!("reload/staging/sample/{}", id.raw());
                    match handler.stage_load(id, config).await {
                        Ok(info) => (
                            id,
                            Some(info),
                            reload::StagedAssetOutcome {
                                path,
                                action: "load".into(),
                                error: None,
                            },
                        ),
                        Err(e) => {
                            tracing::error!("Reload: staging sample {:?} failed: {}", id, e);
                            (
                                id,
                                None,
                                reload::StagedAssetOutcome {
                                    path,
                                    action: "load".into(),
                                    error: Some(e.to_string()),
                                },
                            )
                        }
                    }
                }
            });
            for (id, info, outcome) in futures::future::join_all(loads).await {
                if let Some(info) = info {
                    assets.samples.insert(id, info);
                }
                assets.outcomes.push(outcome);
            }

            // SFZ instruments load sequentially: each allocates many buffer
            // IDs internally, and keeping those allocations ordered keeps
            // cold boot deterministic.
            for (id, path) in plan.sfz {
                match sfz_handler.stage_load(id, &path).await {
                    Ok(instrument) => {
                        assets.sfz.insert(id, instrument);
                        assets.outcomes.push(reload::StagedAssetOutcome {
                            path: format!("reload/staging/sfz/{}", id.raw()),
                            action: "load".into(),
                            error: None,
                        });
                    }
                    Err(e) => {
                        tracing::error!("Reload: staging SFZ instrument {:?} failed: {}", id, e);
                        assets.outcomes.push(reload::StagedAssetOutcome {
                            path: format!("reload/staging/sfz/{}", id.raw()),
                            action: "load".into(),
                            error: Some(e.to_string()),
                        });
                    }
                }
            }

            let completed = u32::try_from(assets.outcomes.len()).unwrap_or(u32::MAX);
            if let Ok(staging) = ledger.transition(
                context.attempt_id(),
                ReceiptState::Staging {
                    completed,
                    total: completed,
                },
                SystemTime::now(),
            ) {
                publish_mutation_transition(&ledger, &context, &staging, SystemTime::now());
            }

            let msg = ContextualMessage::new(
                context,
                Message::Reload(Box::new(ReloadMessage::ApplyStaged {
                    state: new_state,
                    assets,
                })),
            );
            if let Err(error) = tx.send_async(msg).await {
                tracing::warn!("Reload: runtime channel closed before staged reload could apply");
                let ContextualMessage { context, message } = error.0;
                let Message::Reload(reload) = message else {
                    return;
                };
                let ReloadMessage::ApplyStaged { assets, .. } = *reload else {
                    return;
                };
                if let Err(error) = finish_lost_staged_reload(&ledger, &context, assets.outcomes) {
                    tracing::error!("Failed to record lost staged reload: {}", error);
                }
            }
        });
    }

    async fn apply_reload_execution(
        &mut self,
        new_state: reload::ScriptState,
        mut staged: reload::StagedReloadAssets,
    ) -> ReloadExecution {
        let staging = std::mem::take(&mut staged.outcomes);
        let mut execution = self.apply_reload_inner(new_state, &mut staged).await;
        let cleanup = self.discard_staged_leftovers(staged).await;
        let cleanup_phase = execution
            .phases
            .last_mut()
            .expect("reload phase table always contains staged asset cleanup");
        cleanup_phase.started = true;
        cleanup_phase.failures = cleanup;
        execution.staging = staging;
        execution
    }

    /// Frees buffers held by staged assets that were not consumed by the
    /// apply phases.
    async fn discard_staged_leftovers(
        &mut self,
        staged: reload::StagedReloadAssets,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
        if staged.is_empty() {
            return failures;
        }
        let mut buffer_ids: Vec<crate::types::BufferId> =
            staged.samples.values().map(|info| info.buffer_id).collect();
        for instrument in staged.sfz.values() {
            let unique: std::collections::HashSet<crate::types::BufferId> =
                instrument.regions.iter().map(|r| r.buffer_id).collect();
            buffer_ids.extend(unique);
        }
        tracing::debug!(
            "Reload: freeing {} staged buffer(s) the apply did not consume",
            buffer_ids.len()
        );
        {
            let mut state = self.state.write().await;
            for id in &buffer_ids {
                state.free_buffer_id(*id);
            }
        }
        for id in buffer_ids {
            if let Err(e) = self.backend.free_buffer(id).await {
                tracing::warn!("Reload: free_buffer({:?}) failed: {}", id, e);
                failures.push(ReloadPhaseFailure::new("staged_asset_cleanup_failed", e));
            }
        }
        failures
    }

    async fn apply_reload_inner(
        &mut self,
        new_state: reload::ScriptState,
        staged: &mut reload::StagedReloadAssets,
    ) -> ReloadExecution {
        let mut execution = ReloadExecution {
            result: Ok(()),
            staging: Vec::new(),
            phases: RELOAD_PHASE_COMPONENTS
                .iter()
                .map(|(path, action)| ReloadPhaseOutcome::pending(path, action))
                .collect(),
        };
        let (diff, input_routes, port_reconcile_failures) =
            self.build_reload_diff(&new_state).await;
        execution.phases[6].failures = port_reconcile_failures;

        // If no changes, return early - patterns continue playing seamlessly.
        if !diff.has_changes() {
            // Still refresh the script-config snapshots: a clean diff means
            // the live state already matches the script (possibly via the
            // live-map fallback for entities that predate snapshot tracking),
            // so recording the script maps here is a semantic no-op that
            // seeds removal tracking for subsequent reloads.
            self.snapshot_script_config(new_state).await;
            tracing::debug!("Reload: no changes detected, playback continues");
            for phase in &mut execution.phases[..RELOAD_PHASE_COMPONENTS.len() - 1] {
                phase.started = true;
            }
            return execution;
        }

        tracing::info!("Reload: applying changes");

        execution.phases[0].started = true;
        if let Err(error) = self.phase_apply_transport_changes(&diff).await {
            execution.phases[0]
                .failures
                .push(ReloadPhaseFailure::new("reload_transport_failed", &error));
            execution.result = Err(error);
            return execution;
        }
        execution.phases[1].started = true;
        execution.phases[1].failures = self.phase_stop_deleted_entities(&diff).await;
        execution.phases[2].started = true;
        execution.phases[2].failures = self.phase_delete_entities(&diff).await;
        execution.phases[3].started = true;
        let (midi_device_failures, continue_apply) = self.phase_open_midi_devices(&new_state).await;
        execution.phases[3].failures = midi_device_failures;
        if !continue_apply {
            return execution;
        }
        execution.phases[4].started = true;
        execution.phases[4].failures = self.phase_create_entities(&diff, &new_state, staged).await;
        execution.phases[5].started = true;
        execution.phases[5].failures = self.phase_update_entities(&diff, &new_state).await;
        execution.phases[6].started = true;
        if let Err(error) = self.phase_finalize_output_routes(&diff).await {
            execution.phases[6].failures.push(ReloadPhaseFailure::new(
                "reload_output_routes_failed",
                &error,
            ));
            execution.result = Err(error);
            return execution;
        }
        execution.phases[7].started = true;
        if let Err(error) = self.phase_finalize_input_routes(&input_routes).await {
            execution.phases[7].failures.push(ReloadPhaseFailure::new(
                "reload_input_routes_failed",
                &error,
            ));
            execution.result = Err(error);
            return execution;
        }
        execution.phases[8].started = true;
        execution.phases[8].failures = self.phase_apply_effects(&diff, &new_state).await;
        execution.phases[9].started = true;
        execution.phases[9].failures = self.phase_finalize_groups(&diff).await;
        execution.phases[10].started = true;
        execution.phases[10].failures = self.phase_apply_fades(&diff, &new_state).await;
        execution.phases[11].started = true;
        execution.phases[11].failures = self.phase_start_running_patterns(&diff, &new_state).await;
        execution.phases[12].started = true;
        execution.phases[12].failures = self.phase_trigger_running_voices(&new_state).await;
        execution.phases[13].started = true;
        if let Err(error) = self.phase_finalize_param_routes(&diff, &new_state).await {
            execution.phases[13].failures.push(ReloadPhaseFailure::new(
                "reload_param_routes_failed",
                &error,
            ));
            execution.result = Err(error);
            return execution;
        }
        execution.phases[14].started = true;
        execution.phases[14].failures = self.phase_apply_midi_routes(&new_state).await;

        // Snapshot the script-declared config for the next reload's diff.
        // Written only after the best-effort phase sequence completes. An
        // aborting route or MIDI-channel failure keeps the previous snapshot,
        // matching how `current_routes` advances only on route success.
        self.snapshot_script_config(new_state).await;

        tracing::info!("Reload: complete");
        execution
    }

    /// Records the script-declared config maps for the next reload's diff.
    ///
    /// `reload::calculate_diff` compares these snapshots — not the live
    /// `params` maps, which absorb HTTP/MIDI `set_param` tweaks and
    /// runtime-set values — against the incoming ScriptState, so the diff is
    /// script-vs-script: live tweaks never dirty the diff and removed script
    /// params are detectable. Rebuilding wholesale from `new_state` also
    /// drops entries for deleted entities.
    ///
    /// Consumes `new_state`: snapshotting is the caller's last use of it on
    /// both the no-change and success paths, so the param maps / effect lists /
    /// synthdef hashes are **moved** out of it rather than cloned. Every reload
    /// (including no-op saves) formerly paid O(total params) allocations here;
    /// moving makes the no-change path allocation-free apart from the two owned
    /// group maps that must be split field-by-field.
    async fn snapshot_script_config(&mut self, new_state: reload::ScriptState) {
        let reload::ScriptState {
            groups,
            voices,
            effects,
            synthdef_hashes,
            ..
        } = new_state;

        let mut script_group_params = std::collections::HashMap::with_capacity(groups.len());
        let mut script_group_effects = std::collections::HashMap::with_capacity(groups.len());
        for (id, config) in groups {
            script_group_params.insert(id, config.params);
            script_group_effects.insert(id, config.effects);
        }

        let mut state = self.state.write().await;
        state.script_group_params = script_group_params;
        state.script_group_effects = script_group_effects;
        state.script_voice_params = voices
            .into_iter()
            .map(|(id, config)| (id, config.params))
            .collect();
        state.script_effect_params = effects
            .into_iter()
            .map(|(id, config)| (id, config.params))
            .collect();
        state.script_synthdef_hashes = synthdef_hashes;
    }

    /// Builds the reload diff and derived route maps used by the apply phases.
    async fn build_reload_diff(
        &mut self,
        new_state: &reload::ScriptState,
    ) -> (reload::ReloadDiff, InputRouteMap, Vec<ReloadPhaseFailure>) {
        let pending_port_reconciles = self.pending_voice_port_reconciles(&new_state).await;
        // Calculate diff
        let mut diff = {
            let current = self.state.read().await;
            reload::calculate_diff(&current, &new_state, &current.current_routes)
        };
        let structurally_recreated_voices = self
            .structurally_recreated_voice_ids(&diff, &new_state)
            .await;
        let input_routes = self.effective_input_routes(&new_state).await;
        let (mut param_set_diff, mut param_bend_diff, mut param_trigger_diff) = {
            let state = self.state.read().await;
            (
                reload::diff_param_routes_with_shaping(
                    &state.param_routes_set,
                    &new_state.param_routes_set,
                    &state.param_route_set_shaping,
                    &new_state.param_route_set_shaping,
                ),
                reload::diff_param_routes_with_shaping(
                    &state.param_routes_bend,
                    &new_state.param_routes_bend,
                    &state.param_route_bend_shaping,
                    &new_state.param_route_bend_shaping,
                ),
                reload::diff_param_routes(
                    &state.param_routes_trigger,
                    &new_state.param_routes_trigger,
                ),
            )
        };
        {
            let state = self.state.read().await;
            for reconcile in &pending_port_reconciles {
                for port_name in &reconcile.refreshed_ports {
                    Self::force_param_route_refresh(
                        &state.param_routes_set,
                        &new_state.param_routes_set,
                        &mut param_set_diff,
                        reconcile.voice_id,
                        port_name,
                    );
                    Self::force_param_route_refresh(
                        &state.param_routes_bend,
                        &new_state.param_routes_bend,
                        &mut param_bend_diff,
                        reconcile.voice_id,
                        port_name,
                    );
                    Self::force_param_route_refresh(
                        &state.param_routes_trigger,
                        &new_state.param_routes_trigger,
                        &mut param_trigger_diff,
                        reconcile.voice_id,
                        port_name,
                    );
                }
            }
            for voice_id in &structurally_recreated_voices {
                Self::force_param_route_refresh_for_voice(
                    &state.param_routes_set,
                    &new_state.param_routes_set,
                    &mut param_set_diff,
                    *voice_id,
                );
                Self::force_param_route_refresh_for_voice(
                    &state.param_routes_bend,
                    &new_state.param_routes_bend,
                    &mut param_bend_diff,
                    *voice_id,
                );
                Self::force_param_route_refresh_for_voice(
                    &state.param_routes_trigger,
                    &new_state.param_routes_trigger,
                    &mut param_trigger_diff,
                    *voice_id,
                );
            }
        }
        diff.param_routes_set = param_set_diff;
        diff.param_routes_bend = param_bend_diff;
        diff.param_routes_trigger = param_trigger_diff;
        diff.voice_port_reconciles = pending_port_reconciles.len();
        let mut route_base = {
            let state = self.state.read().await;
            state.current_routes.clone()
        };
        for voice_id in &structurally_recreated_voices {
            route_base.retain(|(id, _), _| id != voice_id);
        }
        let port_reconcile_failures = self
            .apply_voice_port_reconciles(
                &pending_port_reconciles,
                &mut diff.effective_output_routes,
            )
            .await;
        diff.output_routes = reload::diff_routes(&route_base, &diff.effective_output_routes);
        diff.input_routes = {
            let state = self.state.read().await;
            crate::handlers::compute_input_route_diff(&state.input_routes, &input_routes)
        };
        #[cfg(feature = "midi")]
        {
            diff.midi_routes_changed = self.midi.script_routes_changed(new_state);
        }
        (diff, input_routes, port_reconcile_failures)
    }

    /// Applies tempo and time signature changes before entity reconciliation.
    async fn phase_apply_transport_changes(&mut self, diff: &reload::ReloadDiff) -> Result<()> {
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
        Ok(())
    }

    /// Stops or cancels runtime activity for entities that are about to be removed or restarted.
    async fn phase_stop_deleted_entities(
        &mut self,
        diff: &reload::ReloadDiff,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
        // NOTE: We NO LONGER stop patterns/melodies that are being updated.
        // Instead, we queue content swaps during entity updates for seamless hot reload.

        // Cancel fades that will be deleted
        for id in &diff.fades.deleted {
            let config_opt = {
                let state = self.state.read().await;
                state.fade_configs.get(id).cloned()
            };
            if let Some(config) = config_opt {
                tracing::debug!("Reload: cancelling deleted fade {:?}", id);
                record_reload_result(
                    &mut failures,
                    "reload_fade_cancel_failed",
                    self.fades.cancel(&config.target, &config.param).await,
                );
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
                record_reload_result(
                    &mut failures,
                    "reload_fade_cancel_failed",
                    self.fades.cancel(&config.target, &config.param).await,
                );
            }
        }

        // Stop patterns that will be deleted (NOT updated - those continue playing)
        for id in &diff.patterns.deleted {
            record_reload_result(
                &mut failures,
                "reload_pattern_stop_failed",
                self.patterns.stop(*id).await,
            );
        }

        // Stop melodies that will be deleted (NOT updated - those continue playing)
        for id in &diff.melodies.deleted {
            record_reload_result(
                &mut failures,
                "reload_melody_stop_failed",
                self.melodies.stop(*id).await,
            );
        }

        // Stop sequences that will be deleted or updated
        // (Sequences still use delete/create cycle for now)
        for id in &diff.sequences.deleted {
            record_reload_result(
                &mut failures,
                "reload_sequence_stop_failed",
                self.sequences.stop(*id).await,
            );
        }
        for id in diff.sequences.updated.keys() {
            record_reload_result(
                &mut failures,
                "reload_sequence_stop_failed",
                self.sequences.stop(*id).await,
            );
        }
        failures
    }

    /// Removes deleted runtime entities and frees script-owned resources.
    async fn phase_delete_entities(
        &mut self,
        diff: &reload::ReloadDiff,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
        // Delete effects (they depend on groups)
        for id in &diff.effects.deleted {
            tracing::debug!("Reload: deleting effect {:?}", id);
            record_reload_result(
                &mut failures,
                "reload_effect_delete_failed",
                self.effects.remove(*id).await,
            );
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
            record_reload_result(
                &mut failures,
                "reload_pattern_delete_failed",
                self.patterns.delete(*id).await,
            );
        }

        // Delete melodies
        for id in &diff.melodies.deleted {
            tracing::debug!("Reload: deleting melody {:?}", id);
            record_reload_result(
                &mut failures,
                "reload_melody_delete_failed",
                self.melodies.delete(*id).await,
            );
        }

        // Delete sequences
        for id in &diff.sequences.deleted {
            tracing::debug!("Reload: deleting sequence {:?}", id);
            record_reload_result(
                &mut failures,
                "reload_sequence_delete_failed",
                self.sequences.delete(*id).await,
            );
        }

        // Compute the group-teardown grace BEFORE deleting the voices:
        // group nodes must outlive the release tails of every voice node
        // that is gate-released in this reload pass (deleted voices below,
        // plus structurally-recreated voices in phase_update_entities whose
        // old nodes sit inside a group deleted here).
        let group_teardown_grace = {
            let state = self.state.read().await;
            let mut grace = crate::compat::Duration::ZERO;
            let deleted = diff.voices.deleted.iter();
            let recreated = diff.voices.updated.iter().filter_map(|(id, new_config)| {
                let current = state.voices.get(id)?;
                Self::voice_needs_structural_recreate(&current.config, new_config).then_some(id)
            });
            for id in deleted.chain(recreated) {
                if let Some(voice) = state.voices.get(id) {
                    let sounding = !voice.active_nodes.is_empty() || !voice.note_nodes.is_empty();
                    if sounding && crate::handlers::voice_is_gated(&voice.config) {
                        grace = grace.max(crate::handlers::voice_release_grace(&voice.config));
                    }
                }
            }
            grace
        };

        // Delete voices (before groups they belong to). Graceful: sounding
        // gated nodes get gate=0 and tail out via their release envelope;
        // node IDs, route mixers, and buses are reclaimed after the grace.
        for id in &diff.voices.deleted {
            tracing::debug!("Reload: deleting voice {:?}", id);
            record_reload_result(
                &mut failures,
                "reload_voice_delete_failed",
                self.voices.graceful_delete(*id).await,
            );
        }

        // Delete groups in correct order (children first). The backend free
        // is deferred by the release grace computed above so gate-released
        // child nodes are not truncated by the recursive group free.
        let ordered_group_deletions = {
            let state = self.state.read().await;
            reload::order_group_deletions(&state.groups, &diff.groups.deleted)
        };
        for id in ordered_group_deletions {
            tracing::debug!("Reload: deleting group {:?}", id);
            record_reload_result(
                &mut failures,
                "reload_group_delete_failed",
                self.groups
                    .delete_with_grace(id, group_teardown_grace)
                    .await,
            );
        }

        // Delete samples
        for id in &diff.samples.deleted {
            tracing::debug!("Reload: deleting sample {:?}", id);
            record_reload_result(
                &mut failures,
                "reload_sample_delete_failed",
                self.samples.unload(*id).await,
            );
        }

        // Free script-allocated buffers that disappeared from the script.
        // Updated buffers (frames/channels resize) are torn down here so
        // entity creation can re-alloc them at the new size.
        for id in &diff.buffers.deleted {
            tracing::debug!("Reload: freeing script buffer {:?}", id);
            if let Err(e) = self.backend.free_buffer(*id).await {
                tracing::warn!("Reload: free_buffer({:?}) failed: {}", id, e);
                failures.push(ReloadPhaseFailure::new("reload_buffer_delete_failed", e));
            }
            self.state.write().await.buffers.remove(id);
        }
        for (id, _) in &diff.buffers.updated {
            tracing::debug!("Reload: freeing script buffer {:?} for resize", id);
            if let Err(e) = self.backend.free_buffer(*id).await {
                tracing::warn!("Reload: free_buffer({:?}) failed: {}", id, e);
                failures.push(ReloadPhaseFailure::new("reload_buffer_resize_failed", e));
            }
            self.state.write().await.buffers.remove(id);
        }

        // Delete SFZ instruments
        for id in &diff.sfz.deleted {
            tracing::debug!("Reload: deleting SFZ instrument {:?}", id);
            record_reload_result(
                &mut failures,
                "reload_sfz_delete_failed",
                self.sfz.unload(*id).await,
            );
            self.state.write().await.sfz_instruments.remove(id);
        }

        // Delete updated SFZ instruments (will be recreated when entities are created)
        for (id, _) in &diff.sfz.updated {
            tracing::debug!("Reload: deleting SFZ instrument {:?} for update", id);
            record_reload_result(
                &mut failures,
                "reload_sfz_update_delete_failed",
                self.sfz.unload(*id).await,
            );
            self.state.write().await.sfz_instruments.remove(id);
        }
        failures
    }

    /// Opens MIDI devices and dispatches immediate MIDI output messages before voices are created.
    async fn phase_open_midi_devices(
        &mut self,
        new_state: &reload::ScriptState,
    ) -> (Vec<ReloadPhaseFailure>, bool) {
        #[cfg(feature = "midi")]
        let mut failures = Vec::new();
        #[cfg(not(feature = "midi"))]
        let failures = Vec::new();
        #[cfg(not(feature = "midi"))]
        let _ = new_state;
        #[cfg(feature = "midi")]
        {
            // Story 4: open MIDI devices in `MidiDeviceId::raw()` order. The
            // backing collections are `HashSet<MidiDeviceId>` so iteration is
            // randomised per process; without sorting the order in which we
            // grab ALSA/JACK MIDI ports — and any error reporting tied to the
            // first/second device — would flicker reload-to-reload.
            // Record the script's requested inputs so the hot-plug watcher
            // reopens exactly these (and stops reopening ones removed from the
            // script) as devices come and go.
            self.midi.set_requested_inputs(&new_state.midi_inputs);

            let mut midi_input_ids: Vec<_> = new_state.midi_inputs.iter().copied().collect();
            midi_input_ids.sort_by_key(|id| id.raw());
            for device_id in &midi_input_ids {
                tracing::debug!("Reload: opening MIDI input {:?}", device_id);
                if let Err(e) = self.midi.open_input(*device_id).await {
                    tracing::error!("Reload: failed to open MIDI input {:?}: {}", device_id, e);
                    failures.push(ReloadPhaseFailure::new("reload_midi_input_open_failed", e));
                }
            }

            // Open numeric MIDI outputs used by voices and direct note/CC
            // methods (sorted, see Story 4 note above).
            let mut midi_output_ids: Vec<_> = new_state.midi_outputs.iter().copied().collect();
            midi_output_ids.sort_by_key(|id| id.raw());
            for device_id in &midi_output_ids {
                tracing::debug!("Reload: opening MIDI output {:?}", device_id);
                if let Err(e) = self.midi.open_output(*device_id).await {
                    tracing::error!("Reload: failed to open MIDI output {:?}: {}", device_id, e);
                    failures.push(ReloadPhaseFailure::new("reload_midi_output_open_failed", e));
                }
            }

            // Clock and realtime transport retain exact output names. Resolve
            // them again at apply time so enumeration reorder between script
            // evaluation and reload cannot redirect a send.
            self.midi_output_endpoints.clear();
            let mut stable_endpoints = new_state
                .midi_output_endpoints
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            stable_endpoints.sort_by(|left, right| left.stable_name.cmp(&right.stable_name));
            for requested in stable_endpoints {
                match resolve_midi_output_endpoint(&requested.stable_name) {
                    Ok(resolved) => match self.midi.open_output(resolved.id).await {
                        Ok(()) => {
                            tracing::info!(
                                "MIDI readiness: output endpoint {:?} -> {} (ONLINE)",
                                requested.stable_name,
                                resolved
                            );
                            self.midi_output_endpoints
                                .insert(requested.stable_name, resolved);
                        }
                        Err(error) => {
                            tracing::error!(
                                "MIDI readiness: output endpoint {:?} -> output[{}] (FAILED: {}); no clock or transport message will be sent",
                                requested.stable_name,
                                resolved.id.raw(),
                                error
                            );
                            failures.push(ReloadPhaseFailure::new(
                                "reload_midi_endpoint_open_failed",
                                error,
                            ));
                        }
                    },
                    Err(error) => {
                        tracing::error!(
                            "MIDI readiness: output endpoint {:?} is UNRESOLVED: {}; no clock or transport message will be sent",
                            requested.stable_name,
                            error
                        );
                        failures.push(ReloadPhaseFailure::new(
                            "reload_midi_endpoint_unresolved",
                            error,
                        ));
                    }
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
                        MidiOutputMessage::Start { endpoint } => {
                            let Some(device_id) = self
                                .midi_output_endpoints
                                .get(&endpoint.stable_name)
                                .map(|resolved| resolved.id)
                            else {
                                tracing::error!(
                                    "Reload: MIDI Start output endpoint {:?} is not ONLINE; sending nothing",
                                    endpoint.stable_name
                                );
                                continue;
                            };
                            let transport_playing = self.state.read().await.playing;
                            if transport_playing {
                                tracing::trace!(
                                    "Reload: skipping MIDI Start to {:?} (transport already playing)",
                                    device_id
                                );
                            } else {
                                tracing::debug!("Reload: sending MIDI Start to {:?}", device_id);
                                if let Err(e) = self.midi.send_start(device_id).await {
                                    tracing::warn!(
                                        "Reload: failed to send MIDI Start to {:?}: {}",
                                        device_id,
                                        e
                                    );
                                    failures.push(ReloadPhaseFailure::new(
                                        "reload_midi_start_failed",
                                        e,
                                    ));
                                }
                            }
                        }
                        MidiOutputMessage::Stop { endpoint } => {
                            let Some(device_id) = self
                                .midi_output_endpoints
                                .get(&endpoint.stable_name)
                                .map(|resolved| resolved.id)
                            else {
                                tracing::error!(
                                    "Reload: MIDI Stop output endpoint {:?} is not ONLINE; sending nothing",
                                    endpoint.stable_name
                                );
                                continue;
                            };
                            let transport_playing = self.state.read().await.playing;
                            if !transport_playing {
                                tracing::trace!(
                                    "Reload: skipping MIDI Stop to {:?} (transport not playing)",
                                    device_id
                                );
                            } else {
                                tracing::debug!("Reload: sending MIDI Stop to {:?}", device_id);
                                if let Err(e) = self.midi.send_stop(device_id).await {
                                    tracing::warn!(
                                        "Reload: failed to send MIDI Stop to {:?}: {}",
                                        device_id,
                                        e
                                    );
                                    failures.push(ReloadPhaseFailure::new(
                                        "reload_midi_stop_failed",
                                        e,
                                    ));
                                }
                            }
                        }
                        MidiOutputMessage::Continue { endpoint } => {
                            let Some(device_id) = self
                                .midi_output_endpoints
                                .get(&endpoint.stable_name)
                                .map(|resolved| resolved.id)
                            else {
                                tracing::error!(
                                    "Reload: MIDI Continue output endpoint {:?} is not ONLINE; sending nothing",
                                    endpoint.stable_name
                                );
                                continue;
                            };
                            tracing::debug!("Reload: sending MIDI Continue to {:?}", device_id);
                            if let Err(e) = self.midi.send_continue(device_id).await {
                                tracing::warn!(
                                    "Reload: failed to send MIDI Continue to {:?}: {}",
                                    device_id,
                                    e
                                );
                                failures.push(ReloadPhaseFailure::new(
                                    "reload_midi_continue_failed",
                                    e,
                                ));
                            }
                        }
                        _ => {} // Other messages handled below
                    }
                }

                // Second pass: handle note/CC messages via output channels
                let output_channels = self.midi.output_channels();
                let Ok(channels) = output_channels.lock() else {
                    tracing::warn!("MIDI output channels mutex poisoned, skipping output");
                    failures.push(ReloadPhaseFailure::new(
                        "reload_midi_output_channels_unavailable",
                        "MIDI output channels mutex was poisoned",
                    ));
                    return (failures, false);
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
                            failures.push(ReloadPhaseFailure::new(
                                "reload_midi_output_queue_failed",
                                e,
                            ));
                        }
                    } else {
                        tracing::warn!("Reload: no output channel for MIDI device {:?}", device_id);
                        failures.push(ReloadPhaseFailure::new(
                            "reload_midi_output_channel_missing",
                            format!("no output channel for MIDI device {device_id:?}"),
                        ));
                    }
                }
            }
        }
        (failures, true)
    }

    /// Creates new samples, buffers, groups, voices, patterns, melodies, sequences, and SFZ instruments.
    async fn phase_create_entities(
        &mut self,
        diff: &reload::ReloadDiff,
        new_state: &reload::ScriptState,
        staged: &mut reload::StagedReloadAssets,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
        // Publish new samples first (other entities may depend on them),
        // and swap UPDATED samples (path or source mtime changed) to their
        // freshly loaded buffers. On native these were already loaded
        // off-task into fresh buffer IDs and arrive in `staged` —
        // committing is a plain state insert; for updated samples the
        // insert displaces the old mapping and `SamplesHandler::commit`
        // defers the old buffer's free past the grace period, so playing
        // notes never see their buffer freed or reallocated under them.
        // Anything the staging missed falls back to an inline load; those
        // parallelize (scsynth /b_allocRead + /b_query round-trips
        // overlap).
        //
        // Story 4: stable iteration via sorted IDs, so logging / error
        // reporting surfaces deterministic per-reboot output.
        if !diff.samples.created.is_empty() || !diff.samples.updated.is_empty() {
            let mut sample_ids: Vec<_> = diff
                .samples
                .created
                .keys()
                .chain(diff.samples.updated.keys())
                .copied()
                .collect();
            sample_ids.sort_by_key(|id| id.raw());
            let mut to_load = Vec::new();
            for id in sample_ids {
                let config = diff
                    .samples
                    .created
                    .get(&id)
                    .or_else(|| diff.samples.updated.get(&id))
                    .expect("just collected")
                    .clone();
                match staged.samples.remove(&id) {
                    Some(info) if info.path == config.path && info.source_mtime == config.mtime => {
                        tracing::debug!("Reload: committing pre-staged sample {:?}", id);
                        self.samples.commit(info).await;
                    }
                    Some(info) => {
                        // Staged against a different path or file version
                        // (state drifted between staging and apply) — leave
                        // it for the leftover cleanup and load inline
                        // instead.
                        staged.samples.insert(id, info);
                        to_load.push((id, config));
                    }
                    None => to_load.push((id, config)),
                }
            }
            if !to_load.is_empty() {
                let loads = to_load.into_iter().map(|(id, config)| {
                    tracing::debug!("Reload: loading sample {:?}", id);
                    self.samples.load(id, config)
                });
                for result in futures::future::join_all(loads).await {
                    record_reload_result(&mut failures, "reload_sample_create_failed", result);
                }
            }
        }

        // Allocate new script buffers (and re-allocate updated ones at new
        // size — the prior generation was already freed). Voices
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
                    self.state.write().await.buffers.insert(*id, config.clone());
                }
                Err(e) => {
                    tracing::error!(
                        "Reload: alloc_buffer({:?}, {}, {}) failed: {}",
                        id,
                        config.frames,
                        config.channels,
                        e
                    );
                    failures.push(ReloadPhaseFailure::new("reload_buffer_create_failed", e));
                }
            }
        }

        // Publish new SFZ instruments (and re-load updated ones). Pre-staged
        // instruments commit with a plain state insert; the rest load inline.
        let sfz_to_load: Vec<_> = diff
            .sfz
            .created
            .iter()
            .chain(diff.sfz.updated.iter())
            .collect();
        for (id, config) in sfz_to_load {
            match staged.sfz.remove(id) {
                Some(instrument) if instrument.path == config.path => {
                    tracing::debug!("Reload: committing pre-staged SFZ instrument {:?}", id);
                    self.sfz.commit(instrument).await;
                }
                other => {
                    if let Some(instrument) = other {
                        // Staged against a different path — leave it for the
                        // leftover cleanup and load inline instead.
                        staged.sfz.insert(*id, instrument);
                    }
                    tracing::debug!(
                        "Reload: loading SFZ instrument {:?} from {:?}",
                        id,
                        config.path
                    );
                    if let Err(e) = self.sfz.load(*id, &config.path).await {
                        tracing::error!("Reload: failed to load SFZ instrument {:?}: {}", id, e);
                        failures.push(ReloadPhaseFailure::new("reload_sfz_create_failed", e));
                    }
                }
            }
        }

        // Create groups in correct order (parents first)
        let ordered_group_creations = reload::order_group_creations(&diff.groups.created);
        for id in ordered_group_creations {
            if let Some(config) = diff.groups.created.get(&id) {
                tracing::debug!("Reload: creating group {:?}", id);
                if let Err(e) = self.groups.create(id, &config.name, config.parent).await {
                    tracing::error!(
                        "Reload: failed to create group {:?} '{}': {}",
                        id,
                        config.name,
                        e
                    );
                    failures.push(ReloadPhaseFailure::new("reload_group_create_failed", e));
                    continue;
                }
                // Apply initial params
                for (param, value) in &config.params {
                    if let Err(e) = self.groups.set_param(id, param, *value).await {
                        tracing::warn!(
                            "Reload: failed to set initial param '{}'={} on group {:?} '{}': {}",
                            param,
                            value,
                            id,
                            config.name,
                            e
                        );
                        failures.push(ReloadPhaseFailure::new("reload_group_param_failed", e));
                    }
                }
                // Apply initial mute/solo state
                if config.muted {
                    if let Err(e) = self.groups.mute(id, true).await {
                        tracing::warn!(
                            "Reload: failed to mute group {:?} '{}': {}",
                            id,
                            config.name,
                            e
                        );
                        failures.push(ReloadPhaseFailure::new("reload_group_mute_failed", e));
                    }
                }
                if config.soloed {
                    if let Err(e) = self.groups.solo(id, true).await {
                        tracing::warn!(
                            "Reload: failed to solo group {:?} '{}': {}",
                            id,
                            config.name,
                            e
                        );
                        failures.push(ReloadPhaseFailure::new("reload_group_solo_failed", e));
                    }
                }
                // Apply output_bus / output_channels routing override.
                // The two fields are coupled (both Some(_) or both None);
                // mirror them together so the link-synth dispatch in
                // `GroupsHandler::finalize` sees a consistent pair.
                if config.output_bus.is_some() {
                    let mut state = self.state.write().await;
                    if let Some(group) = state.groups.get_mut(&id) {
                        group.output_bus = config.output_bus;
                        group.output_channels = config.output_channels;
                    }
                }
            }
        }

        // Sync with backend to ensure groups are created before we create synths targeting them
        if !diff.groups.created.is_empty() {
            if !self.sync_with_retry("after group creation").await {
                failures.push(ReloadPhaseFailure::new(
                    "reload_group_sync_failed",
                    "backend sync failed or timed out after group creation",
                ));
            }
        }

        // Create new voices in merged script evaluation order. Repeated
        // group bodies contribute to one ScriptState, so this is the order the
        // Rhai layer recorded after all body/include merging.
        let mut created_voice_ids: Vec<_> = diff.voices.created.keys().copied().collect();
        created_voice_ids.sort_by_key(|id| {
            (
                new_state
                    .voice_order
                    .iter()
                    .position(|ordered| ordered == id)
                    .unwrap_or(usize::MAX),
                id.raw(),
            )
        });
        for id in created_voice_ids {
            let Some(config) = diff.voices.created.get(&id) else {
                continue;
            };
            tracing::debug!("Reload: creating voice {:?}", id);
            if let Err(e) = self.voices.create(id, config.clone()).await {
                tracing::error!("Reload: failed to create voice {:?}: {}", id, e);
                failures.push(ReloadPhaseFailure::new("reload_voice_create_failed", e));
            }
        }

        // Create new patterns
        for (id, config) in &diff.patterns.created {
            tracing::debug!("Reload: creating pattern {:?}", id);
            if let Err(e) = self.patterns.create(*id, config.clone()).await {
                tracing::error!("Reload: failed to create pattern {:?}: {}", id, e);
                failures.push(ReloadPhaseFailure::new("reload_pattern_create_failed", e));
            }
        }

        // Create new melodies
        for (id, config) in &diff.melodies.created {
            tracing::debug!("Reload: creating melody {:?}", id);
            if let Err(e) = self.melodies.create(*id, config.clone()).await {
                tracing::error!("Reload: failed to create melody {:?}: {}", id, e);
                failures.push(ReloadPhaseFailure::new("reload_melody_create_failed", e));
            }
        }

        // Create new sequences
        for (id, config) in &diff.sequences.created {
            tracing::debug!("Reload: creating sequence {:?}", id);
            if let Err(e) = self.sequences.create(*id, config.clone()).await {
                tracing::error!("Reload: failed to create sequence {:?}: {}", id, e);
                failures.push(ReloadPhaseFailure::new("reload_sequence_create_failed", e));
            }
        }

        // NOTE: Effect creation is deferred until after routes finalize
        // so that effect synths sit in SC tree order *after* the route mixers
        // that sum voice ports onto the group's audio bus.
        failures
    }

    /// Applies in-place updates and structural recreations for existing groups, voices, patterns, melodies, and sequences.
    async fn phase_update_entities(
        &mut self,
        diff: &reload::ReloadDiff,
        new_state: &reload::ScriptState,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
        // Update groups (params and mute/solo, parent changes not supported during reload).
        //
        // Story 4: sort by `GroupId::raw()` for deterministic apply order. The
        // body re-routes `output_bus`/`output_channels` and tears down link
        // synth nodes; doing that in `HashMap`-iteration order means the same
        // edit lands in different sequences on different processes, which can
        // surface as different node-id allocations on the SC server. Same fix
        // pattern as `order_group_creations`/`order_group_deletions`: hash-stable.
        let mut updated_group_ids: Vec<_> = diff.groups.updated.keys().copied().collect();
        updated_group_ids.sort_by_key(|id| id.raw());
        for id in updated_group_ids {
            let Some(new_config) = diff.groups.updated.get(&id) else {
                continue;
            };
            let id = &id;
            // Apply only the params that actually changed relative to the
            // script's previous declaration (the snapshot in
            // `state.script_group_params`). Re-applying the full map on every
            // reload would snap live-tweaked values back on unrelated saves;
            // ignoring removals would keep a deleted script param at its last
            // value forever, diverging from a cold boot.
            let (old_script_params, cur_muted, cur_soloed) = {
                let state = self.state.read().await;
                let tracked = state.script_group_params.get(id).cloned();
                let group = state.groups.get(id);
                (
                    // Fall back to the live params map (pre-snapshot
                    // behaviour) for groups created outside the reload path.
                    // Removals are only honoured for tracked groups: the live
                    // map may contain non-script keys we must not reset.
                    tracked,
                    group.map(|g| g.muted).unwrap_or(new_config.muted),
                    group.map(|g| g.soloed).unwrap_or(new_config.soloed),
                )
            };
            let track_removals = old_script_params.is_some();
            let old_params = match old_script_params {
                Some(params) => params,
                None => {
                    let state = self.state.read().await;
                    state
                        .groups
                        .get(id)
                        .map(|g| g.params.clone())
                        .unwrap_or_default()
                }
            };
            let param_diff = reload::ParamDiff::diff(&old_params, &new_config.params);
            for (param, value) in param_diff.added.iter().chain(param_diff.changed.iter()) {
                tracing::debug!(
                    "Reload: updating group {:?} param {} to {}",
                    id,
                    param,
                    value
                );
                if let Err(e) = self.groups.set_param(*id, param, *value).await {
                    tracing::warn!(
                        "Reload: failed to update param '{}'={} on group {:?} '{}': {}",
                        param,
                        value,
                        id,
                        new_config.name,
                        e
                    );
                    failures.push(ReloadPhaseFailure::new(
                        "reload_group_param_update_failed",
                        e,
                    ));
                }
            }
            if track_removals {
                for param in &param_diff.removed {
                    // A removed script param must land at the value a cold
                    // boot would produce. Only `amp`/`pan` have a well-defined
                    // default — they actuate the group's link synth
                    // (`system_link_audio*`). Any other group param is
                    // broadcast to the group node's children, which are
                    // heterogeneous synths with per-synthdef defaults, so no
                    // single reset value exists; warn loudly and skip.
                    match group_link_param_default(param) {
                        Some(default) => {
                            tracing::debug!(
                                "Reload: group {:?} param '{}' removed from script; resetting to default {}",
                                id,
                                param,
                                default
                            );
                            if let Err(e) = self.groups.set_param(*id, param, default).await {
                                tracing::warn!(
                                    "Reload: failed to reset removed param '{}' on group {:?} '{}': {}",
                                    param,
                                    id,
                                    new_config.name,
                                    e
                                );
                                failures.push(ReloadPhaseFailure::new(
                                    "reload_group_param_reset_failed",
                                    e,
                                ));
                            }
                        }
                        None => {
                            tracing::warn!(
                                "Reload: param '{}' removed from group '{}' but group params broadcast to child synths with no single default; running nodes keep the last value {:?} until recreated",
                                param,
                                new_config.name,
                                old_params.get(param)
                            );
                        }
                    }
                    // Drop the key from the live map either way so the stored
                    // state matches a cold boot (finalize's `amp` lookup falls
                    // back to 1.0) and the diff converges next reload.
                    let mut state = self.state.write().await;
                    if let Some(group) = state.groups.get_mut(id) {
                        group.params.remove(param);
                    }
                }
            }
            // Apply mute/solo only on actual change — re-asserting them on
            // every reload is redundant backend traffic and would fight solo
            // recomputation across unrelated groups.
            if cur_muted != new_config.muted {
                tracing::debug!("Reload: updating group {:?} muted={}", id, new_config.muted);
                if let Err(e) = self.groups.mute(*id, new_config.muted).await {
                    tracing::warn!(
                        "Reload: failed to set mute={} on group {:?} '{}': {}",
                        new_config.muted,
                        id,
                        new_config.name,
                        e
                    );
                    failures.push(ReloadPhaseFailure::new(
                        "reload_group_mute_update_failed",
                        e,
                    ));
                }
            }
            if cur_soloed != new_config.soloed {
                tracing::debug!(
                    "Reload: updating group {:?} soloed={}",
                    id,
                    new_config.soloed
                );
                if let Err(e) = self.groups.solo(*id, new_config.soloed).await {
                    tracing::warn!(
                        "Reload: failed to set solo={} on group {:?} '{}': {}",
                        new_config.soloed,
                        id,
                        new_config.name,
                        e
                    );
                    failures.push(ReloadPhaseFailure::new(
                        "reload_group_solo_update_failed",
                        e,
                    ));
                }
            }

            // Update output_bus / output_channels routing if either changed.
            //
            // `output_channels` selects between the stereo `system_link_audio`
            // and the mono `system_link_audio_mono` mixdown variants — a
            // mono↔stereo flip therefore can't be patched in place. To keep
            // the code path uniform across all four mono/stereo × bus-change
            // combinations, we always tear down the existing link synth and
            // clear `link_synth_node_id` when either field changes;
            // `groups.finalize()` then spawns the right variant for the new
            // `output_channels` and routes it to the resolved out_bus, reusing
            // the dispatch already covered by `handlers::groups` unit tests.
            let teardown_link_node: Option<crate::types::NodeId> = {
                let mut state = self.state.write().await;
                if let Some(group) = state.groups.get_mut(id) {
                    let bus_changed = group.output_bus != new_config.output_bus;
                    let channels_changed = group.output_channels != new_config.output_channels;
                    if bus_changed || channels_changed {
                        let old_bus = group.output_bus;
                        let old_channels = group.output_channels;
                        group.output_bus = new_config.output_bus;
                        group.output_channels = new_config.output_channels;
                        let teardown = group.link_synth_node_id.take();
                        if let Some(node) = teardown {
                            state.free_node_id(node);
                        }
                        tracing::info!(
                            "Reload: group {:?} routing changed (bus {:?} -> {:?}, channels {:?} -> {:?}); link will be respawned by finalize",
                            id,
                            old_bus,
                            new_config.output_bus,
                            old_channels,
                            new_config.output_channels
                        );
                        teardown
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(node) = teardown_link_node {
                record_reload_result(
                    &mut failures,
                    "reload_group_link_teardown_failed",
                    self.backend.free_node(node).await,
                );
            }
        }

        // Update voices - only recreate if synthdef, group, or sfz_instrument changed
        // For param-only changes, use set_param to avoid audio gaps.
        //
        // Story 4: iterate in script-order then `id.raw()` tiebreak. The recreate
        // branch below calls `voices.recreate` (detach/delete + create), and
        // `voices.create`
        // can `state.alloc_audio_bus(...)` for kr/ar output ports. `diff.voices.updated`
        // is a `HashMap`, so without sorting two reloads of the same script would
        // hand out kr-bus IDs in different orders — same class of bug as the root-group
        // scramble, just on the voice level. Use the same sort key as voice creation
        // so reload symmetry is preserved.
        let mut updated_voice_ids: Vec<_> = diff.voices.updated.keys().copied().collect();
        updated_voice_ids.sort_by_key(|id| {
            (
                new_state
                    .voice_order
                    .iter()
                    .position(|ordered| ordered == id)
                    .unwrap_or(usize::MAX),
                id.raw(),
            )
        });
        for id in updated_voice_ids {
            let Some(new_config) = diff.voices.updated.get(&id) else {
                continue;
            };
            let id = &id;
            // Get current voice state to compare
            let (needs_recreate, old_script_params) = {
                let state = self.state.read().await;
                if let Some(current_voice) = state.voices.get(id) {
                    // Recreate if synthdef, group, or sfz_instrument changed,
                    // or if the synthdef's BODY changed (same name, different
                    // compiled graph — detected via the content-hash snapshot).
                    //
                    // The old-param baseline for the in-place update is the
                    // script-config snapshot; the live `config.params` map is
                    // only a fallback for voices created outside the reload
                    // path (removals are skipped for those — the live map may
                    // hold runtime-only keys like `gate` that must never be
                    // "reset").
                    (
                        Self::voice_needs_structural_recreate(&current_voice.config, new_config)
                            || reload::synthdef_body_changed(
                                &state,
                                new_state,
                                &new_config.synthdef,
                            ),
                        state.script_voice_params.get(id).cloned(),
                    )
                } else {
                    // Voice not found - shouldn't happen, but recreate to be safe
                    (true, None)
                }
            };

            if needs_recreate {
                tracing::debug!(
                    "Reload: recreating voice {:?} (synthdef, group, or sfz changed)",
                    id
                );
                // Spawn-before-release: the new voice is materialized first,
                // then the old sounding nodes are gate-released so their
                // tail overlaps the new sound instead of leaving a gap.
                record_reload_result(
                    &mut failures,
                    "reload_voice_recreate_failed",
                    self.voices.recreate(*id, new_config.clone()).await,
                );
            } else {
                // Only params/config changed - update them without recreating the synth
                tracing::debug!(
                    "Reload: updating voice {:?} params only ({} params)",
                    id,
                    new_config.params.len()
                );
                let track_removals = old_script_params.is_some();
                let old_params = match old_script_params {
                    Some(params) => params,
                    None => {
                        let state = self.state.read().await;
                        state
                            .voices
                            .get(id)
                            .map(|v| v.config.params.clone())
                            .unwrap_or_default()
                    }
                };
                let param_diff = reload::ParamDiff::diff(&old_params, &new_config.params);
                for (param, value) in param_diff.added.iter().chain(param_diff.changed.iter()) {
                    record_reload_result(
                        &mut failures,
                        "reload_voice_param_update_failed",
                        self.voices.set_param(*id, param, *value).await,
                    );
                }
                if track_removals && !param_diff.removed.is_empty() {
                    // Reset removed script params to the synthdef's declared
                    // default so a hot reload matches a cold boot. Going
                    // through `voices.set_param` keeps the BEND-summer
                    // baseline forwarding intact for routed params; the key
                    // is then dropped from the stored config so future
                    // triggers use the synthdef default implicitly (exactly
                    // the cold-boot config). Recreating the voice instead
                    // would kill sounding notes, which is worse than a
                    // warn-and-skip when no default is known.
                    let defaults = vibelang_dsp::get_synthdef_param_defaults(&new_config.synthdef);
                    for param in &param_diff.removed {
                        match defaults.get(param) {
                            Some(default) => {
                                tracing::debug!(
                                    "Reload: voice {:?} param '{}' removed from script; resetting to synthdef default {}",
                                    id,
                                    param,
                                    default
                                );
                                record_reload_result(
                                    &mut failures,
                                    "reload_voice_param_reset_failed",
                                    self.voices.set_param(*id, param, *default).await,
                                );
                            }
                            None => {
                                tracing::warn!(
                                    "Reload: param '{}' removed from voice '{}' but synthdef '{}' declares no default; running nodes keep the last value {:?}",
                                    param,
                                    new_config.name,
                                    new_config.synthdef,
                                    old_params.get(param)
                                );
                            }
                        }
                        // Converge the stored config with the script either
                        // way (cold boot would not have the key at all).
                        let mut state = self.state.write().await;
                        if let Some(voice) = state.voices.get_mut(id) {
                            voice.config.params.remove(param);
                        }
                    }
                }
                // Update the stored config for non-param fields (name, polyphony, etc.)
                #[cfg_attr(not(feature = "midi"), allow(unused_variables))]
                let polyphony_changed = {
                    let mut state = self.state.write().await;
                    if let Some(voice) = state.voices.get_mut(id) {
                        let changed = voice.config.polyphony != new_config.polyphony;
                        voice.config.name = new_config.name.clone();
                        voice.config.polyphony = new_config.polyphony;
                        voice.config.round_robin_count = new_config.round_robin_count;
                        voice.config.choke_group = new_config.choke_group.clone();
                        voice.config.modulator_only = new_config.modulator_only;
                        voice.config.mono_legato = new_config.mono_legato;
                        #[cfg(feature = "midi")]
                        {
                            voice.config.midi_output = new_config.midi_output;
                            voice.config.midi_channel = new_config.midi_channel;
                            voice.config.param_cc_map = new_config.param_cc_map.clone();
                        }
                        changed
                    } else {
                        false
                    }
                };
                // A polyphony change on a MIDI-output voice resizes its
                // voice-allocation pool (NoteOff for notes that no longer fit).
                #[cfg(feature = "midi")]
                if polyphony_changed {
                    record_reload_result(
                        &mut failures,
                        "reload_voice_midi_pool_resize_failed",
                        self.voices
                            .resize_midi_pool(*id, new_config.polyphony as usize)
                            .await,
                    );
                }
            }
        }

        // Content swaps honour the script's `set_quantization(beats)` grid,
        // snapped to the discrete ChangeQuant boundaries: NextBar when the
        // script never sets one (None), Immediate for an explicit
        // `set_quantization(0)` — see ChangeQuant::from_grid.
        let swap_quant = reload::ChangeQuant::from_grid(new_state.quantization, new_state.time_sig);

        // Update patterns - queue content swap for seamless hot reload
        // Instead of delete/create cycle, queue new content to be applied at
        // the quantization boundary. The pattern tick applies it when the
        // lookahead window reaches the boundary, so the new bar's downbeat is
        // always scheduled from the new content.
        for (id, config) in &diff.patterns.updated {
            tracing::debug!("Reload: queuing pattern {:?} content swap", id);
            let new_content = crate::traits::PatternContent::arc_from_config(config);
            let mut state = self.state.write().await;
            let current_beat = state.current_beat;
            if let Some(pattern) = state.patterns.get_mut(id) {
                pattern.queue_content_swap(new_content, swap_quant);
                if !pattern.playing {
                    // Nothing is scheduled from a stopped pattern, so there is
                    // no boundary to wait for — swap now so a later start()
                    // (or a start in this very reload) plays the new content
                    // immediately, exactly like a cold boot would.
                    pattern.apply_pending_swap(current_beat);
                }
                tracing::debug!(
                    "Pattern {:?}: content swap {} (quant={:?})",
                    id,
                    if pattern.playing {
                        "queued"
                    } else {
                        "applied immediately (not playing)"
                    },
                    swap_quant
                );
            }
        }

        // Update melodies - same seamless content-swap path as patterns.
        for (id, config) in &diff.melodies.updated {
            tracing::debug!("Reload: queuing melody {:?} content swap", id);
            let new_content = crate::traits::MelodyContent::arc_from_config(config);
            let mut state = self.state.write().await;
            if let Some(melody) = state.melodies.get_mut(id) {
                melody.queue_content_swap(new_content, swap_quant);
                if !melody.playing {
                    melody.apply_pending_swap();
                }
                tracing::debug!(
                    "Melody {:?}: content swap {} (quant={:?})",
                    id,
                    if melody.playing {
                        "queued"
                    } else {
                        "applied immediately (not playing)"
                    },
                    swap_quant
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
            record_reload_result(
                &mut failures,
                "reload_sequence_update_delete_failed",
                self.sequences.delete(*id).await,
            );
            record_reload_result(
                &mut failures,
                "reload_sequence_update_create_failed",
                self.sequences.create(*id, config.clone()).await,
            );
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

        // NOTE: Effect updates are deferred until after routes finalize,
        // alongside effect creation, so freshly-(re)spawned effect synths sit
        // *after* the route mixers in SC tree order.
        failures
    }

    /// Materializes output route mixers and advances the current route snapshot.
    async fn phase_finalize_output_routes(&mut self, diff: &reload::ReloadDiff) -> Result<()> {
        // Spawned between the voice creation/update phase and the group
        // link-synth phase so the SC tree order is voices → routes → effects →
        // link synth → main bus. The diff is computed against the
        // last-applied [`State::current_routes`] snapshot.
        //
        // Story 5: the effective desired map is the union of count-based
        // defaults (installed in `state.default_routes` by VoicesHandler::create)
        // and the script-supplied user routes from `new_state.routes`, with
        // user entries winning on conflicts. We diff against — and persist —
        // this merged map so a later reload sees defaults as already-applied
        // and a removal of a user route correctly falls back to the default.
        self.apply_voice_roles(&diff.voice_roles).await;
        let merged_routes = diff.effective_output_routes.clone();
        let route_diff = diff.output_routes.clone();
        if !route_diff.is_empty() {
            tracing::debug!(
                "Reload: finalizing routes (additions={}, removals={})",
                route_diff.additions.len(),
                route_diff.removals.len(),
            );
            if let Err(e) = self.routes.finalize(&route_diff).await {
                tracing::error!(
                    "Reload: routes.finalize failed; aborting reload without advancing route snapshot: {}",
                    e
                );
                return Err(e);
            }
        }
        self.state.write().await.current_routes = merged_routes;
        Ok(())
    }

    /// Materializes named input routes against the last active input-route snapshot.
    async fn phase_finalize_input_routes(&mut self, input_routes: &InputRouteMap) -> Result<()> {
        // Script-side `voice.input("name").from(...)` calls populate explicit
        // entries, and every declared linkable input port defaults to the
        // shared silent bus when left unpatched. Reconcile that effective map
        // against the last materialized `State::input_routes` snapshot.
        if !input_routes.is_empty() || !self.state.read().await.input_routes.is_empty() {
            if let Err(e) = self.routes.finalize_input_routes(&input_routes).await {
                tracing::error!(
                    "Reload: routes.finalize_input_routes failed; aborting reload: {}",
                    e
                );
                return Err(e);
            }
        }
        Ok(())
    }

    /// Creates and updates effects after output route mixers have been materialized.
    async fn phase_apply_effects(
        &mut self,
        diff: &reload::ReloadDiff,
        new_state: &reload::ScriptState,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
        // Effects must be inserted into the SC tree *after* the route mixers
        // emitted by output route finalization so that an effect's `In.ar(group_audio_bus)`
        // sees the post-sum signal that routes have just deposited there.
        // On initial load (no link synth yet) we use `AddAction::Tail`, so
        // doing this strictly after output route finalization (which also Tail-adds mixers
        // to the same group node) places effects after routes in tree order.
        // On later reloads where the link synth exists, `EffectsHandler::add`
        // already inserts `AddAction::Before` link, which is also after the
        // routes. Either way: voices → routes → effects → link → main.

        // Create new effects in merged script evaluation order so effect
        // chains assembled across multiple group bodies stay deterministic at
        // runtime, not just in ScriptState snapshots.
        let mut created_effect_ids: Vec<_> = diff.effects.created.keys().copied().collect();
        created_effect_ids.sort_by_key(|id| {
            (
                new_state
                    .effect_order
                    .iter()
                    .position(|ordered| ordered == id)
                    .unwrap_or(usize::MAX),
                id.raw(),
            )
        });
        for id in created_effect_ids {
            let Some(config) = diff.effects.created.get(&id) else {
                continue;
            };
            tracing::debug!("Reload: creating effect {:?}", id);
            if let Err(e) = self
                .effects
                .add(id, config.group, &config.synthdef, &config.params)
                .await
            {
                tracing::error!("Reload: failed to create effect {:?}: {}", id, e);
                failures.push(ReloadPhaseFailure::new("reload_effect_create_failed", e));
            }
        }

        // Update effects - only recreate if synthdef or group changed
        // For param-only changes, use set_param to avoid audio gaps
        let mut updated_effect_ids: Vec<_> = diff.effects.updated.keys().copied().collect();
        updated_effect_ids.sort_by_key(|id| {
            (
                new_state
                    .effect_order
                    .iter()
                    .position(|ordered| ordered == id)
                    .unwrap_or(usize::MAX),
                id.raw(),
            )
        });
        for id in updated_effect_ids {
            let Some(new_config) = diff.effects.updated.get(&id) else {
                continue;
            };
            // Get current effect state to compare
            let (needs_recreate, old_script_params) = {
                let state = self.state.read().await;
                if let Some(current_effect) = state.effects.get(&id) {
                    // Recreate if synthdef or group changed, or if the
                    // synthdef's body changed (content hash mismatch).
                    (
                        current_effect.synthdef != new_config.synthdef
                            || current_effect.group != new_config.group
                            || reload::synthdef_body_changed(
                                &state,
                                new_state,
                                &new_config.synthdef,
                            ),
                        state.script_effect_params.get(&id).cloned(),
                    )
                } else {
                    // Effect not found - shouldn't happen, but recreate to be safe
                    (true, None)
                }
            };

            if needs_recreate {
                tracing::debug!(
                    "Reload: recreating effect {:?} (synthdef or group changed, synthdef='{}', {} params)",
                    id,
                    new_config.synthdef,
                    new_config.params.len()
                );
                record_reload_result(
                    &mut failures,
                    "reload_effect_recreate_remove_failed",
                    self.effects.remove(id).await,
                );
                if let Err(e) = self
                    .effects
                    .add(
                        id,
                        new_config.group,
                        &new_config.synthdef,
                        &new_config.params,
                    )
                    .await
                {
                    tracing::error!("Reload: failed to recreate effect {:?}: {}", id, e);
                    failures.push(ReloadPhaseFailure::new("reload_effect_recreate_failed", e));
                }
            } else {
                // Only params changed - update them without recreating the synth
                tracing::debug!(
                    "Reload: updating effect {:?} params only (synthdef='{}', {} params)",
                    id,
                    new_config.synthdef,
                    new_config.params.len()
                );
                let track_removals = old_script_params.is_some();
                let old_params = match old_script_params {
                    Some(params) => params,
                    None => {
                        let state = self.state.read().await;
                        state
                            .effects
                            .get(&id)
                            .map(|e| e.params.clone())
                            .unwrap_or_default()
                    }
                };
                let param_diff = reload::ParamDiff::diff(&old_params, &new_config.params);
                for (param, value) in param_diff.added.iter().chain(param_diff.changed.iter()) {
                    record_reload_result(
                        &mut failures,
                        "reload_effect_param_update_failed",
                        self.effects.set_param(id, param, *value).await,
                    );
                }
                if track_removals && !param_diff.removed.is_empty() {
                    // Reset removed script params to the effect's declared
                    // default (cold-boot value). Effects always come from
                    // `define_fx`, which registers the IR in the effect
                    // registry — a missing default means the param never
                    // existed on the synthdef, so a warn-and-skip beats a
                    // remove+re-add (which would move the effect to the tail
                    // of the chain and cut its tail through the grace-period
                    // fade).
                    let fx_defaults = vibelang_dsp::get_effect_param_defaults(&new_config.synthdef);
                    let synth_defaults =
                        vibelang_dsp::get_synthdef_param_defaults(&new_config.synthdef);
                    for param in &param_diff.removed {
                        match fx_defaults.get(param).or_else(|| synth_defaults.get(param)) {
                            Some(default) => {
                                tracing::debug!(
                                    "Reload: effect {:?} param '{}' removed from script; resetting to default {}",
                                    id,
                                    param,
                                    default
                                );
                                record_reload_result(
                                    &mut failures,
                                    "reload_effect_param_reset_failed",
                                    self.effects.set_param(id, param, *default).await,
                                );
                            }
                            None => {
                                tracing::warn!(
                                    "Reload: param '{}' removed from effect {:?} (synthdef '{}') but no default is registered; running node keeps the last value {:?}",
                                    param,
                                    id,
                                    new_config.synthdef,
                                    old_params.get(param)
                                );
                            }
                        }
                        // Converge stored state with the script either way.
                        let mut state = self.state.write().await;
                        if let Some(effect) = state.effects.get_mut(&id) {
                            effect.params.remove(param);
                        }
                    }
                }
            }
        }

        // Reconcile effect-chain ORDER. A script that reorders effects
        // within a group (same set, same per-effect configs) produces no
        // effect create/delete above, so without this step the live nodes
        // keep their stale tree order and the reload sounds different from
        // a cold boot. Move existing nodes into script order with /n_before
        // instead of recreating them: cheap, glitch-free, and effect state
        // (reverb tails, delay lines) survives. This also repairs the
        // recreate path above, which re-attaches an updated effect at the
        // chain tail.
        failures.extend(self.reconcile_effect_chain_order(new_state).await);
        failures
    }

    /// Moves live effect nodes into script-declared chain order via
    /// `/n_before`.
    ///
    /// Node-order invariant inside a group: route mixers first, then the
    /// effects in script order, then the link synth. Every effect node
    /// already sits between the mixers and the link synth, and each
    /// `/n_before` targets another effect node in the same window, so the
    /// moves cannot cross either boundary.
    async fn reconcile_effect_chain_order(
        &mut self,
        new_state: &reload::ScriptState,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
        // Compute moves under the state lock; dispatch after release.
        let mut moves: Vec<(crate::types::NodeId, crate::types::NodeId)> = Vec::new();
        {
            let mut state = self.state.write().await;
            // Story 4: hash-stable group order so dispatch is deterministic.
            let mut group_ids: Vec<_> = new_state.groups.keys().copied().collect();
            group_ids.sort_by_key(|id| id.raw());
            for group_id in group_ids {
                let Some(config) = new_state.groups.get(&group_id) else {
                    continue;
                };
                // Desired chain: script order, first occurrence wins for
                // repeated ids (matching the creation loop, which sorts by
                // first position in `effect_order`), restricted to effects
                // that actually exist in this group (creation can fail).
                let mut seen = std::collections::HashSet::new();
                let desired: Vec<crate::types::EffectId> = config
                    .effects
                    .iter()
                    .copied()
                    .filter(|id| seen.insert(*id))
                    .filter(|id| state.effects.get(id).map(|e| e.group == group_id) == Some(true))
                    .collect();
                let current = state.effect_ids_in_group(group_id);
                if current == desired {
                    continue;
                }
                tracing::debug!(
                    "Reload: reordering effect chain in group {:?}: {:?} -> {:?}",
                    group_id,
                    current,
                    desired
                );
                // Rebuild back-to-front: desired[len-1] stays where it is;
                // moving desired[i] immediately before desired[i+1] (already
                // in final relative position) leaves the chain contiguous
                // and in order around wherever the last effect sits.
                if desired.len() >= 2 {
                    for i in (0..desired.len() - 1).rev() {
                        let node = state.effects[&desired[i]].node_id;
                        let before = state.effects[&desired[i + 1]].node_id;
                        moves.push((node, before));
                    }
                }
                // Track the new live order. Any ids in the group but absent
                // from the script chain (degenerate — shouldn't survive the
                // delete phase) are kept at the tail so tracking stays
                // complete.
                let mut new_chain = desired;
                for id in current {
                    if !new_chain.contains(&id) {
                        new_chain.push(id);
                    }
                }
                state.group_effect_chain.insert(group_id, new_chain);
            }
        }
        for (node, before) in moves {
            if let Err(e) = self.backend.move_node_before(node, before).await {
                tracing::error!(
                    "Reload: failed to move effect node {:?} before {:?}: {}",
                    node,
                    before,
                    e
                );
                failures.push(ReloadPhaseFailure::new("reload_effect_reorder_failed", e));
            }
        }
        failures
    }

    /// Finalizes changed groups so link synths exist with the current routing configuration.
    async fn phase_finalize_groups(
        &mut self,
        diff: &reload::ReloadDiff,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
        // Finalize groups if any were created or updated — ensures link synths exist
        // and are properly configured. This handles group renames where the old group
        // is deleted and a new one is created.
        if !diff.groups.created.is_empty() || !diff.groups.updated.is_empty() {
            tracing::debug!("Reload: finalizing groups");
            record_reload_result(
                &mut failures,
                "reload_group_finalize_failed",
                self.groups.finalize().await,
            );

            // Brief sync to let link synths be created (non-blocking, quick timeout)
            if !self.sync_with_retry("after finalize").await {
                failures.push(ReloadPhaseFailure::new(
                    "reload_group_finalize_sync_failed",
                    "backend sync failed or timed out after group finalize",
                ));
            }
        }
        failures
    }

    /// Starts created and updated stateful fades and processes legacy pending fades.
    async fn phase_apply_fades(
        &mut self,
        diff: &reload::ReloadDiff,
        new_state: &reload::ScriptState,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
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
                    failures.push(ReloadPhaseFailure::new("reload_fade_start_failed", e));
                }
                // Track in runtime state for future diffing
                let mut state = self.state.write().await;
                state.fade_configs.insert(*id, config.clone());
            }
        }

        // Restart updated fades (already cancelled before deletion)
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
                    failures.push(ReloadPhaseFailure::new("reload_fade_restart_failed", e));
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
                    failures.push(ReloadPhaseFailure::new(
                        "reload_legacy_fade_start_failed",
                        e,
                    ));
                }
            }
        }
        failures
    }

    /// Starts or stops pattern, melody, and sequence playback requested by the script state.
    async fn phase_start_running_patterns(
        &mut self,
        diff: &reload::ReloadDiff,
        new_state: &reload::ScriptState,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
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
                        && p.owner != crate::state::PatternOwner::Looper
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
            record_reload_result(
                &mut failures,
                "reload_pattern_stop_failed",
                self.patterns.stop(id).await,
            );
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
            record_reload_result(
                &mut failures,
                "reload_melody_stop_failed",
                self.melodies.stop(id).await,
            );
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
            record_reload_result(
                &mut failures,
                "reload_sequence_stop_failed",
                self.sequences.stop(id).await,
            );
        }

        // Start patterns that should be playing (only if not already playing).
        // `start_on_grid` anchors the pattern to the song grid
        // (`start_beat = current_beat - current_beat % length`) and starts
        // the scheduling watermark at `current_beat`, so a reload-started
        // pattern plays exactly the phase a cold boot of the same script
        // would be playing at this beat: no first-tick burst of the loop's
        // tail, no phase-lock to the reload instant.
        for id in &new_state.playing_patterns {
            let should_start = {
                let state = self.state.read().await;
                state.patterns.get(id).is_some_and(|p| !p.playing)
            };
            if should_start {
                tracing::debug!("Reload: starting pattern {:?} on the song grid", id);
                record_reload_result(
                    &mut failures,
                    "reload_pattern_start_failed",
                    self.patterns.start_on_grid(*id).await,
                );
            }
        }

        // Start melodies that should be playing (only if not already playing)
        // Also sync their position to current_beat to avoid triggering past notes
        tracing::debug!(
            "Reload: processing {} melodies to start from playing_melodies",
            new_state.playing_melodies.len()
        );
        for id in &new_state.playing_melodies {
            let (should_start, melody_exists, notes_count) = {
                let state = self.state.read().await;
                let melody_exists = state.melodies.contains_key(id);
                let should_start = state.melodies.get(id).is_some_and(|m| !m.playing);
                let notes_count = state
                    .melodies
                    .get(id)
                    .map(|m| m.content.notes.len())
                    .unwrap_or(0);
                (should_start, melody_exists, notes_count)
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
                // Melodies run on absolute song time: `start` resumes at
                // `current_beat % length` with the scheduling watermark at
                // `current_beat` — identical phase to a cold boot, no burst,
                // no epsilon hacks.
                record_reload_result(
                    &mut failures,
                    "reload_melody_start_failed",
                    self.melodies.start(*id).await,
                );
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
                record_reload_result(
                    &mut failures,
                    "reload_sequence_start_failed",
                    self.sequences.start(*id, true).await,
                );
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
        failures
    }

    /// Stops removed running voices and triggers newly requested continuous voices.
    async fn phase_trigger_running_voices(
        &mut self,
        new_state: &reload::ScriptState,
    ) -> Vec<ReloadPhaseFailure> {
        let mut failures = Vec::new();
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
            record_reload_result(
                &mut failures,
                "reload_voice_stop_failed",
                self.voices.stop(voice_id).await,
            );
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
                    failures.push(ReloadPhaseFailure::new("reload_voice_trigger_failed", e));
                }
            }
        }
        failures
    }

    /// Materializes parameter routes after their source and target nodes exist.
    async fn phase_finalize_param_routes(
        &mut self,
        diff: &reload::ReloadDiff,
        new_state: &reload::ScriptState,
    ) -> Result<()> {
        // Param routes are not part of the entity diff, so they need an
        // explicit reload-time diff against State's active route registries.
        // Run this after running voices are triggered so `.run()` targets have
        // active nodes for `/n_map`, and after effect creation so fx targets
        // have node ids. The handler creates its dedicated summer group at the
        // root head, so summer/link synths still execute before target
        // voices/effects read their mapped params.
        if diff.param_routes_have_changes() {
            tracing::debug!(
                "Reload: finalizing param routes (set +{} -{}, bend +{} -{}, trigger +{} -{})",
                diff.param_routes_set.additions.len(),
                diff.param_routes_set.removals.len(),
                diff.param_routes_bend.additions.len(),
                diff.param_routes_bend.removals.len(),
                diff.param_routes_trigger.additions.len(),
                diff.param_routes_trigger.removals.len(),
            );
            if let Err(e) = self
                .routes
                .finalize_params_with_shaping(
                    &diff.param_routes_set,
                    &diff.param_routes_bend,
                    &diff.param_routes_trigger,
                    &new_state.param_route_set_shaping,
                    &new_state.param_route_bend_shaping,
                )
                .await
            {
                tracing::error!(
                    "Reload: routes.finalize_params failed; aborting reload: {}",
                    e
                );
                return Err(e);
            }
        }
        Ok(())
    }

    /// Applies MIDI routing and clock-output requests from the script state.
    async fn phase_apply_midi_routes(
        &mut self,
        new_state: &reload::ScriptState,
    ) -> Vec<ReloadPhaseFailure> {
        #[cfg(feature = "midi")]
        let mut failures = Vec::new();
        #[cfg(not(feature = "midi"))]
        let failures = Vec::new();
        #[cfg(not(feature = "midi"))]
        let _ = new_state;
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
                .apply_advanced_bend_routes(&new_state.advanced_bend_routes)
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
                let Some(device_id) = self
                    .midi_output_endpoints
                    .get(&clock_req.endpoint.stable_name)
                    .map(|resolved| resolved.id)
                else {
                    tracing::error!(
                        "Reload: clock output endpoint {:?} is not ONLINE; sending nothing",
                        clock_req.endpoint.stable_name
                    );
                    failures.push(ReloadPhaseFailure::new(
                        "reload_midi_clock_endpoint_offline",
                        format!(
                            "MIDI clock output endpoint {:?} is not online",
                            clock_req.endpoint.stable_name
                        ),
                    ));
                    continue;
                };
                tracing::debug!(
                    "Reload: {} MIDI clock output for {}",
                    if clock_req.enabled {
                        "enabling"
                    } else {
                        "disabling"
                    },
                    clock_req.endpoint
                );
                if clock_req.enabled {
                    if let Err(e) = self.midi.enable_clock_output(device_id).await {
                        tracing::error!(
                            "Reload: failed to enable clock output for {}: {}",
                            clock_req.endpoint,
                            e
                        );
                        failures.push(ReloadPhaseFailure::new(
                            "reload_midi_clock_enable_failed",
                            e,
                        ));
                    }
                } else if let Err(e) = self.midi.disable_clock_output(device_id).await {
                    tracing::error!(
                        "Reload: failed to disable clock output for {}: {}",
                        clock_req.endpoint,
                        e
                    );
                    failures.push(ReloadPhaseFailure::new(
                        "reload_midi_clock_disable_failed",
                        e,
                    ));
                }
            }

            self.midi.mark_script_routes_applied(new_state);
        }
        failures
    }
}

struct WorkFailure {
    code: &'static str,
    message: String,
    definitely_no_effect: bool,
    phase: FailurePhase,
}

fn finish_contextual_receipt(
    ledger: &MutationLedger,
    context: &MutationContext,
    component_path: &str,
    action: &str,
    failure: Option<WorkFailure>,
    confirmation: Confirmation,
) -> Result<MutationReceipt> {
    let current = ledger
        .receipt(context.attempt_id())
        .map_err(mutation_ledger_error)?;
    let previous = current.previous_confirmed_revision;
    let state = match failure {
        None => {
            let effective_at = EffectiveAt {
                observed_at: Timestamp::from_system_time(SystemTime::now()),
                musical_beat: None,
                backend_time_seconds: None,
            };
            ReceiptState::Terminal(TerminalOutcome::Applied(Applied {
                effective_at: effective_at.clone(),
                confirmations: vec![confirmation.clone()],
                components: vec![ComponentOutcome {
                    path: component_path.into(),
                    action: action.into(),
                    state: ComponentState::Applied,
                    effective_at: Some(effective_at),
                    confirmation: Some(confirmation),
                    diagnostic: None,
                }],
                audible_tail_until: None,
            }))
        }
        Some(failure) if failure.definitely_no_effect => {
            ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
                phase: failure.phase,
                code: failure.code.into(),
                message: failure.message,
                rollback: RollbackState::NotNeeded,
                preserved_revision: previous,
            }))
        }
        Some(failure) => ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: failure.phase,
            code: failure.code.into(),
            components: vec![ComponentOutcome {
                path: component_path.into(),
                action: action.into(),
                state: ComponentState::Uncertain,
                effective_at: None,
                confirmation: None,
                diagnostic: Some(Diagnostic {
                    severity: DiagnosticSeverity::Error,
                    code: failure.code.into(),
                    message: failure.message,
                    component_path: Some(component_path.into()),
                    source_span: None,
                }),
            }],
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: previous,
        })),
    };
    let now = SystemTime::now();
    let receipt = ledger
        .transition(context.attempt_id(), state, now)
        .map_err(mutation_ledger_error)?;
    publish_mutation_transition(ledger, context, &receipt, now);
    Ok(receipt)
}

fn finish_reload_receipt(
    ledger: &MutationLedger,
    context: &MutationContext,
    execution: &ReloadExecution,
) -> Result<MutationReceipt> {
    let now = SystemTime::now();
    let effective_at = EffectiveAt {
        observed_at: Timestamp::from_system_time(now),
        musical_beat: None,
        backend_time_seconds: None,
    };
    let mut components = Vec::with_capacity(execution.staging.len() + execution.phases.len());
    for staged in &execution.staging {
        components.push(ComponentOutcome {
            path: staged.path.clone(),
            action: staged.action.clone(),
            state: if staged.error.is_some() {
                ComponentState::Uncertain
            } else {
                ComponentState::Applied
            },
            effective_at: staged.error.is_none().then(|| effective_at.clone()),
            confirmation: staged
                .error
                .is_none()
                .then_some(Confirmation::RuntimeCommit),
            diagnostic: staged.error.as_ref().map(|message| Diagnostic {
                severity: DiagnosticSeverity::Error,
                code: "reload_staging_failed".into(),
                message: message.clone(),
                component_path: Some(staged.path.clone()),
                source_span: None,
            }),
        });
    }
    for phase in &execution.phases {
        let failure = phase.failures.first();
        components.push(ComponentOutcome {
            path: phase.path.into(),
            action: phase.action.into(),
            state: if !phase.started {
                ComponentState::NotStarted
            } else if failure.is_some() {
                ComponentState::Uncertain
            } else {
                ComponentState::Applied
            },
            effective_at: (phase.started && failure.is_none()).then(|| effective_at.clone()),
            confirmation: (phase.started && failure.is_none())
                .then_some(Confirmation::RuntimeCommit),
            diagnostic: failure.map(|failure| Diagnostic {
                severity: DiagnosticSeverity::Error,
                code: failure.code.into(),
                message: if phase.failures.len() == 1 {
                    failure.message.clone()
                } else {
                    format!(
                        "{} (and {} additional phase failure(s))",
                        failure.message,
                        phase.failures.len() - 1
                    )
                },
                component_path: Some(phase.path.into()),
                source_span: None,
            }),
        });
    }
    let first_failure = components.iter().find_map(|component| {
        component
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.clone())
    });
    let current = ledger
        .receipt(context.attempt_id())
        .map_err(mutation_ledger_error)?;
    let state = if let Some(code) = first_failure {
        ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: FailurePhase::Reconcile,
            code,
            components,
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: current.previous_confirmed_revision,
        }))
    } else {
        ReceiptState::Terminal(TerminalOutcome::Applied(Applied {
            effective_at,
            confirmations: vec![Confirmation::RuntimeCommit],
            components,
            audible_tail_until: None,
        }))
    };
    let receipt = ledger
        .transition(context.attempt_id(), state, now)
        .map_err(mutation_ledger_error)?;
    publish_mutation_transition(ledger, context, &receipt, now);
    Ok(receipt)
}

#[cfg(not(target_arch = "wasm32"))]
fn finish_lost_staged_reload(
    ledger: &MutationLedger,
    context: &MutationContext,
    staging: Vec<reload::StagedAssetOutcome>,
) -> Result<MutationReceipt> {
    let mut execution = ReloadExecution {
        result: Err(Error::ChannelClosed),
        staging,
        phases: RELOAD_PHASE_COMPONENTS
            .iter()
            .map(|(path, action)| ReloadPhaseOutcome::pending(path, action))
            .collect(),
    };
    execution.phases[15].started = true;
    execution.phases[15].failures.push(ReloadPhaseFailure {
        code: "staging_completion_lost",
        message: "the runtime queue closed before staged assets could be applied or reclaimed"
            .into(),
    });
    finish_reload_receipt(ledger, context, &execution)
}

#[cfg(not(target_arch = "wasm32"))]
fn finish_deferred_effect_after_fence(
    ledger: &MutationLedger,
    completion: &DeferredCompletion,
) -> Result<MutationReceipt> {
    let now = SystemTime::now();
    let effective_at = EffectiveAt {
        observed_at: Timestamp::from_system_time(now),
        musical_beat: None,
        backend_time_seconds: None,
    };
    let current = ledger
        .receipt(completion.context.attempt_id())
        .map_err(mutation_ledger_error)?;
    let receipt = ledger
        .transition(
            completion.context.attempt_id(),
            ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
                phase: FailurePhase::Reconcile,
                code: "effect_completed_after_runtime_fence".into(),
                components: vec![ComponentOutcome {
                    path: completion.component_path.clone(),
                    action: completion.action.clone(),
                    state: ComponentState::Applied,
                    effective_at: Some(effective_at),
                    confirmation: Some(Confirmation::RuntimeCommit),
                    diagnostic: Some(Diagnostic {
                        severity: DiagnosticSeverity::Error,
                        code: "effect_completed_after_runtime_fence".into(),
                        message: "the v1 effect completed while a lower revision became partial"
                            .into(),
                        component_path: Some(completion.component_path.clone()),
                        source_span: None,
                    }),
                }],
                rollback: RollbackState::Uncertain,
                fenced: true,
                last_confirmed_revision: current.previous_confirmed_revision,
            })),
            now,
        )
        .map_err(mutation_ledger_error)?;
    publish_mutation_transition(ledger, &completion.context, &receipt, now);
    Ok(receipt)
}

fn error_code(error: &Error) -> &'static str {
    match error {
        Error::Backend(_) => "backend_rejected",
        Error::BackendNotReady => "backend_not_ready",
        Error::SynthDefNotFound(_) => "synthdef_not_found",
        Error::SynthDefRejected { .. } => "synthdef_rejected",
        Error::SynthDefPreflightFailed { .. } => "synthdef_preflight_failed",
        Error::SampleNotFound(_) => "sample_not_found",
        Error::SampleLoadFailed { .. } => "sample_load_failed",
        Error::SfzNotFound(_) => "sfz_not_found",
        Error::SfzLoadFailed { .. } => "sfz_load_failed",
        Error::RecordingNotFound(_) => "recording_not_found",
        Error::RecordingAlreadyExists(_) => "recording_already_exists",
        Error::GroupNotFound(_) => "group_not_found",
        Error::VoiceNotFound(_) => "voice_not_found",
        Error::PatternNotFound(_) => "pattern_not_found",
        Error::MelodyNotFound(_) => "melody_not_found",
        Error::SequenceNotFound(_) => "sequence_not_found",
        Error::EffectNotFound(_) => "effect_not_found",
        Error::GroupExists(_) => "group_exists",
        Error::VoiceExists(_) => "voice_exists",
        Error::PatternExists(_) => "pattern_exists",
        Error::MelodyExists(_) => "melody_exists",
        Error::SequenceExists(_) => "sequence_exists",
        Error::EffectExists(_) => "effect_exists",
        Error::InvalidParam { .. } => "invalid_parameter",
        Error::InvalidConfig(_) => "invalid_configuration",
        Error::IdsExhausted(_) => "resource_ids_exhausted",
        Error::ChannelClosed => "queue_closed",
        Error::ChannelFull => "queue_full",
        Error::SyncTimeout => "sync_timeout",
        Error::AcknowledgementLost => "acknowledgement_lost",
        Error::MutationLedger(_) => "mutation_ledger_error",
        Error::RuntimeFenced(_) => "runtime_fenced",
        #[cfg(feature = "midi")]
        Error::MidiDeviceNotFound(_) => "midi_device_not_found",
        #[cfg(feature = "midi")]
        Error::MidiError(_) => "midi_error",
    }
}

fn error_is_pre_effect(error: &Error) -> bool {
    matches!(
        error,
        Error::BackendNotReady
            | Error::SynthDefNotFound(_)
            | Error::SampleNotFound(_)
            | Error::SfzNotFound(_)
            | Error::RecordingNotFound(_)
            | Error::RecordingAlreadyExists(_)
            | Error::GroupNotFound(_)
            | Error::VoiceNotFound(_)
            | Error::PatternNotFound(_)
            | Error::MelodyNotFound(_)
            | Error::SequenceNotFound(_)
            | Error::EffectNotFound(_)
            | Error::GroupExists(_)
            | Error::VoiceExists(_)
            | Error::PatternExists(_)
            | Error::MelodyExists(_)
            | Error::SequenceExists(_)
            | Error::EffectExists(_)
            | Error::InvalidParam { .. }
            | Error::InvalidConfig(_)
            | Error::ChannelClosed
            | Error::ChannelFull
            | Error::SyncTimeout
            | Error::AcknowledgementLost
            | Error::RuntimeFenced(_)
    )
}

/// A cloneable handle for sending messages to the runtime.
///
/// Handles are cheap to clone and can be shared across threads.
#[derive(Clone)]
pub struct RuntimeHandle {
    tx: Sender<ContextualMessage>,
    ledger: MutationLedger,
    policy: Arc<parking_lot::Mutex<MutationPolicy>>,
    async_mutation_in_flight: Arc<parking_lot::Mutex<Option<crate::mutation::AttemptId>>>,
}

impl RuntimeHandle {
    /// Send a message to the runtime.
    ///
    /// Returns an error if the runtime has been dropped.
    pub async fn send(&self, msg: Message) -> Result<()> {
        let submission = self.legacy_submission(&msg)?;
        let receipt = self.submit(msg, submission).await?;
        self.ensure_legacy_admitted(&receipt)
    }

    /// Submit a message through the canonical v1 best-effort receipt ledger.
    ///
    /// The returned receipt is `accepted` (pending) after queue admission. It
    /// is never a terminal success claim. Runtime handling publishes later
    /// transitions through the supplied sinks.
    pub async fn submit(&self, msg: Message, submission: Submission) -> Result<MutationReceipt> {
        self.submit_with_sinks(
            msg,
            submission,
            MutationReplySink::default(),
            MutationEventSink::default(),
        )
        .await
    }

    /// Submit with caller-owned receipt and event sinks.
    pub async fn submit_with_sinks(
        &self,
        msg: Message,
        submission: Submission,
        reply_sink: MutationReplySink,
        event_sink: MutationEventSink,
    ) -> Result<MutationReceipt> {
        self.ensure_receipt_bearing(&msg)?;
        let now = SystemTime::now();
        let submitted = self
            .ledger
            .submit(submission, now)
            .map_err(mutation_ledger_error)?;
        let receipt = submitted.receipt().clone();
        let context = MutationContext::new(
            receipt.attempt_id,
            receipt.runtime_epoch,
            receipt.request.idempotency_key_present,
            reply_sink,
            event_sink,
        );
        publish_mutation_transition(&self.ledger, &context, &receipt, now);
        match submitted {
            SubmissionResult::Rejected(_) | SubmissionResult::Replayed(_) => return Ok(receipt),
            SubmissionResult::New(_) => {}
        }
        if self.is_fenced() {
            let rejected = self.reject_before_admission(
                &context,
                "runtime_fenced",
                "a partial or unknown mutation must be acknowledged before continuing",
                now,
            )?;
            return Ok(rejected);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let tx = self.tx.clone();
            let permit = match tx.reserve().await {
                Ok(permit) => permit,
                Err(_) => {
                    self.reject_before_admission(
                        &context,
                        "queue_closed",
                        "the runtime mutation queue is closed",
                        SystemTime::now(),
                    )?;
                    return Err(Error::ChannelClosed);
                }
            };
            if self.is_fenced() {
                let rejected = self.reject_before_admission(
                    &context,
                    "runtime_fenced",
                    "a partial or unknown mutation must be acknowledged before continuing",
                    SystemTime::now(),
                )?;
                return Ok(rejected);
            }
            let accepted = self.accept_after_queue_admission(&context)?;
            if matches!(accepted.state, ReceiptState::Accepted { .. }) {
                permit.send(ContextualMessage::new(context, msg));
            }
            return Ok(accepted);
        }
        #[cfg(target_arch = "wasm32")]
        if self
            .tx
            .send_async(ContextualMessage::new(context.clone(), msg))
            .await
            .is_err()
        {
            self.reject_before_admission(
                &context,
                "queue_closed",
                "the runtime mutation queue is closed",
                SystemTime::now(),
            )?;
            return Err(Error::ChannelClosed);
        }
        #[cfg(target_arch = "wasm32")]
        self.accept_after_queue_admission(&context)
    }

    /// Try to send a message without waiting.
    ///
    /// Returns an error if the channel is full or closed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn try_send(&self, msg: Message) -> Result<()> {
        let submission = self.legacy_submission(&msg)?;
        let receipt = self.try_submit(msg, submission)?;
        self.ensure_legacy_admitted(&receipt)
    }

    /// Try to send a message without waiting.
    ///
    /// Returns an error if the channel is full or closed.
    #[cfg(target_arch = "wasm32")]
    pub fn try_send(&self, msg: Message) -> Result<()> {
        let submission = self.legacy_submission(&msg)?;
        let receipt = self.try_submit(msg, submission)?;
        self.ensure_legacy_admitted(&receipt)
    }

    /// Non-blocking receipt-bearing admission with distinct full/closed truth.
    pub fn try_submit(&self, msg: Message, submission: Submission) -> Result<MutationReceipt> {
        self.ensure_receipt_bearing(&msg)?;
        let now = SystemTime::now();
        let submitted = self
            .ledger
            .submit(submission, now)
            .map_err(mutation_ledger_error)?;
        let receipt = submitted.receipt().clone();
        let context = MutationContext::new(
            receipt.attempt_id,
            receipt.runtime_epoch,
            receipt.request.idempotency_key_present,
            MutationReplySink::default(),
            MutationEventSink::default(),
        );
        publish_mutation_transition(&self.ledger, &context, &receipt, now);
        match submitted {
            SubmissionResult::Rejected(_) | SubmissionResult::Replayed(_) => return Ok(receipt),
            SubmissionResult::New(_) => {}
        }
        if self.is_fenced() {
            return self.reject_before_admission(
                &context,
                "runtime_fenced",
                "a partial or unknown mutation must be acknowledged before continuing",
                now,
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let tx = self.tx.clone();
            let permit = match tx.try_reserve() {
                Ok(permit) => permit,
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    self.reject_before_admission(
                        &context,
                        "queue_full",
                        "the runtime mutation queue is full",
                        SystemTime::now(),
                    )?;
                    return Err(Error::ChannelFull);
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    self.reject_before_admission(
                        &context,
                        "queue_closed",
                        "the runtime mutation queue is closed",
                        SystemTime::now(),
                    )?;
                    return Err(Error::ChannelClosed);
                }
            };
            if self.is_fenced() {
                return self.reject_before_admission(
                    &context,
                    "runtime_fenced",
                    "a partial or unknown mutation must be acknowledged before continuing",
                    SystemTime::now(),
                );
            }
            let accepted = self.accept_after_queue_admission(&context)?;
            if matches!(accepted.state, ReceiptState::Accepted { .. }) {
                permit.send(ContextualMessage::new(context, msg));
            }
            Ok(accepted)
        }
        #[cfg(target_arch = "wasm32")]
        {
            use futures::Sink;
            let mut tx = self.tx.clone();
            match std::pin::Pin::new(&mut tx)
                .start_send(ContextualMessage::new(context.clone(), msg))
            {
                Ok(()) => self.accept_after_queue_admission(&context),
                Err(_) => {
                    self.reject_before_admission(
                        &context,
                        "queue_full_or_closed",
                        "the WASM runtime mutation queue is full or closed",
                        SystemTime::now(),
                    )?;
                    Err(Error::ChannelFull)
                }
            }
        }
    }

    /// Send a message, blocking the current thread until it's queued.
    ///
    /// This is useful when calling from synchronous code (like Rhai callbacks)
    /// where async is not available.
    ///
    /// Returns an error if the channel is closed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn blocking_send(&self, msg: Message) -> Result<()> {
        self.ensure_receipt_bearing(&msg)?;
        let submission = self.legacy_submission(&msg)?;
        let now = SystemTime::now();
        let submitted = self
            .ledger
            .submit(submission, now)
            .map_err(mutation_ledger_error)?;
        let receipt = submitted.receipt().clone();
        let context = MutationContext::new(
            receipt.attempt_id,
            receipt.runtime_epoch,
            receipt.request.idempotency_key_present,
            MutationReplySink::default(),
            MutationEventSink::default(),
        );
        publish_mutation_transition(&self.ledger, &context, &receipt, now);
        if !matches!(submitted, SubmissionResult::New(_)) {
            return Ok(());
        }
        if self.is_fenced() {
            self.reject_before_admission(
                &context,
                "runtime_fenced",
                "a partial or unknown mutation must be acknowledged before continuing",
                now,
            )?;
            return Err(Error::RuntimeFenced(receipt.attempt_id.to_string()));
        }
        let tx = self.tx.clone();
        let permit = match futures::executor::block_on(tx.reserve()) {
            Ok(permit) => permit,
            Err(_) => {
                self.reject_before_admission(
                    &context,
                    "queue_closed",
                    "the runtime mutation queue is closed",
                    SystemTime::now(),
                )?;
                return Err(Error::ChannelClosed);
            }
        };
        if self.is_fenced() {
            self.reject_before_admission(
                &context,
                "runtime_fenced",
                "a partial or unknown mutation must be acknowledged before continuing",
                SystemTime::now(),
            )?;
            return Err(Error::RuntimeFenced(receipt.attempt_id.to_string()));
        }
        let accepted = self.accept_after_queue_admission(&context)?;
        if matches!(accepted.state, ReceiptState::Accepted { .. }) {
            permit.send(ContextualMessage::new(context, msg));
        }
        Ok(())
    }

    /// Send a sync message and wait for the backend to complete all pending operations.
    ///
    /// This is a barrier that ensures:
    /// 1. All previously sent messages have been processed by the runtime
    /// 2. The backend has synced with scsynth (all d_recv, s_new, etc. completed)
    ///
    /// Use this after queueing synthdefs to ensure they're loaded before creating synths.
    pub async fn sync_and_wait(&self) -> Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return self.sync_and_wait_timeout(Duration::from_secs(30)).await;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let (tx, rx) = crate::compat::oneshot();
            self.send(Message::Sync(SyncMessage::SyncAndNotify { notify: tx }))
                .await?;
            rx.await
                .map_err(|_| Error::ChannelClosed)?
                .map_err(Error::backend_msg)
        }
    }

    /// Result-bearing backend barrier with an explicit native deadline.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn sync_and_wait_timeout(&self, deadline: Duration) -> Result<()> {
        let (tx, rx) = crate::compat::oneshot();
        let message = Message::Sync(SyncMessage::SyncAndNotify { notify: tx });
        let submission = self.legacy_submission(&message)?;
        let receipt = self.submit(message, submission).await?;
        self.ensure_legacy_admitted(&receipt)?;
        match timeout(deadline, rx).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => {
                let latest = self.mutation_receipt(receipt.attempt_id)?;
                if matches!(
                    &latest.state,
                    ReceiptState::Terminal(TerminalOutcome::Rejected(rejected))
                        if rejected.code == "runtime_fenced"
                ) {
                    Err(Error::RuntimeFenced(receipt.attempt_id.to_string()))
                } else {
                    Err(Error::backend_msg(message))
                }
            }
            Ok(Err(_)) => {
                self.finish_incomplete_sync(
                    receipt.attempt_id,
                    "acknowledgement_lost",
                    "the backend barrier completed without delivering its acknowledgement",
                )?;
                Err(Error::AcknowledgementLost)
            }
            Err(_) => {
                self.finish_incomplete_sync(
                    receipt.attempt_id,
                    "sync_timeout",
                    "the backend barrier did not complete before its deadline",
                )?;
                Err(Error::SyncTimeout)
            }
        }
    }

    /// Read the latest canonical receipt for an attempt.
    pub fn mutation_receipt(
        &self,
        attempt_id: crate::mutation::AttemptId,
    ) -> Result<MutationReceipt> {
        self.ledger
            .receipt(attempt_id)
            .map_err(mutation_ledger_error)
    }

    /// Read the current mutation/fence status.
    #[must_use]
    pub fn mutation_status(&self) -> crate::mutation::RuntimeMutationStatus {
        self.ledger.status(SystemTime::now())
    }

    /// Explicitly acknowledge the current fenced partial and permit continued
    /// v1 best-effort mutation. A later partial establishes a new fence.
    pub fn continue_best_effort(&self, partial_attempt: crate::mutation::AttemptId) -> Result<()> {
        let receipt = self
            .ledger
            .receipt(partial_attempt)
            .map_err(mutation_ledger_error)?;
        let is_fenced_partial = matches!(
            &receipt.state,
            ReceiptState::Terminal(TerminalOutcome::Partial(Partial { fenced: true, .. }))
        );
        let status_matches = match self.ledger.status(SystemTime::now()).live_state {
            LiveState::Partial { revision, fenced } => fenced && receipt.revision == Some(revision),
            LiveState::PreAdmissionPartial { attempt_id, fenced } => {
                fenced && attempt_id == partial_attempt
            }
            LiveState::Clean | LiveState::Unknown { .. } => false,
        };
        if !is_fenced_partial || !status_matches {
            return Err(Error::InvalidConfig(
                "continue_best_effort requires the current fenced partial receipt".into(),
            ));
        }
        self.policy.lock().acknowledged_fence = Some(partial_attempt);
        Ok(())
    }

    fn legacy_submission(&self, msg: &Message) -> Result<Submission> {
        let semantic = serde_json::json!({ "message_type": msg.type_name() });
        Ok(Submission {
            kind: MutationKind::Command {
                domain: msg.domain(),
                operation: msg.operation().to_lowercase(),
            },
            source: MutationSource::Rhai {
                engine_id: "compat.vibelang.v1.runtime_handle".into(),
            },
            caller_namespace: "compat.vibelang.v1.local".into(),
            idempotency_key: None,
            require_idempotency_key: false,
            retry_epoch: Some(self.ledger.runtime_epoch()),
            expected_revision: None,
            atomicity: Atomicity::BestEffort,
            supersession: SupersessionPolicy::Fifo,
            material: RequestMaterial::from_values(semantic, None),
        })
    }

    fn ensure_receipt_bearing(&self, msg: &Message) -> Result<()> {
        match msg.class() {
            MessageClass::ReceiptBearingMutation => Ok(()),
            MessageClass::ReceiptLinkedCompletion => Err(Error::InvalidConfig(format!(
                "{} is an internal completion and must retain its parent mutation context",
                msg.type_name()
            ))),
            MessageClass::Internal => Err(Error::InvalidConfig(format!(
                "{} is runtime maintenance and cannot create a mutation receipt",
                msg.type_name()
            ))),
        }
    }

    fn ensure_legacy_admitted(&self, receipt: &MutationReceipt) -> Result<()> {
        match &receipt.state {
            ReceiptState::Accepted { .. } => Ok(()),
            ReceiptState::Terminal(TerminalOutcome::Rejected(rejected))
                if rejected.code == "runtime_fenced" =>
            {
                Err(Error::RuntimeFenced(receipt.attempt_id.to_string()))
            }
            ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) => {
                Err(Error::MutationLedger(format!(
                    "mutation rejected during admission: {}",
                    rejected.code
                )))
            }
            state => Err(Error::MutationLedger(format!(
                "legacy mutation did not reach queue admission: {state:?}"
            ))),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_incomplete_sync(
        &self,
        attempt_id: crate::mutation::AttemptId,
        code: &'static str,
        message: &'static str,
    ) -> Result<()> {
        clear_async_mutation(&self.async_mutation_in_flight, attempt_id);
        let receipt = self
            .ledger
            .receipt(attempt_id)
            .map_err(mutation_ledger_error)?;
        if receipt.state.is_terminal() {
            return Ok(());
        }
        let context = MutationContext::new(
            receipt.attempt_id,
            receipt.runtime_epoch,
            receipt.request.idempotency_key_present,
            MutationReplySink::default(),
            MutationEventSink::default(),
        );
        let context = match receipt.revision {
            Some(revision) => context
                .with_revision(revision)
                .map_err(Error::MutationLedger)?,
            None => context,
        };
        match finish_contextual_receipt(
            &self.ledger,
            &context,
            "sync/backend_barrier",
            "sync_and_wait",
            Some(WorkFailure {
                code,
                message: message.into(),
                definitely_no_effect: false,
                phase: FailurePhase::BackendBarrier,
            }),
            Confirmation::BackendBarrier {
                backend: "runtime".into(),
                token: attempt_id.to_string(),
            },
        ) {
            Ok(_) => Ok(()),
            Err(error) => {
                let latest = self
                    .ledger
                    .receipt(attempt_id)
                    .map_err(mutation_ledger_error)?;
                if latest.state.is_terminal() {
                    Ok(())
                } else {
                    Err(error)
                }
            }
        }
    }

    fn accept_after_queue_admission(&self, context: &MutationContext) -> Result<MutationReceipt> {
        let now = SystemTime::now();
        let receipt = self
            .ledger
            .accept(context.attempt_id(), None, now)
            .map_err(mutation_ledger_error)?;
        publish_mutation_transition(&self.ledger, context, &receipt, now);
        Ok(receipt)
    }

    fn reject_before_admission(
        &self,
        context: &MutationContext,
        code: &str,
        message: &str,
        now: SystemTime,
    ) -> Result<MutationReceipt> {
        reject_contextual_admission(&self.ledger, context, code, message, now)
    }

    fn is_fenced(&self) -> bool {
        mutation_is_fenced(&self.ledger, &self.policy)
    }
}

fn reject_contextual_admission(
    ledger: &MutationLedger,
    context: &MutationContext,
    code: &str,
    message: &str,
    now: SystemTime,
) -> Result<MutationReceipt> {
    let previous = ledger
        .receipt(context.attempt_id())
        .map_err(mutation_ledger_error)?
        .previous_confirmed_revision;
    let receipt = ledger
        .transition(
            context.attempt_id(),
            ReceiptState::Terminal(TerminalOutcome::Rejected(Rejected {
                phase: FailurePhase::Admission,
                code: code.into(),
                message: message.into(),
                rollback: RollbackState::NotNeeded,
                preserved_revision: previous,
            })),
            now,
        )
        .map_err(mutation_ledger_error)?;
    publish_mutation_transition(ledger, context, &receipt, now);
    Ok(receipt)
}

fn mutation_is_fenced(
    ledger: &MutationLedger,
    policy: &parking_lot::Mutex<MutationPolicy>,
) -> bool {
    let status = ledger.status(SystemTime::now());
    let fenced = match status.live_state {
        LiveState::Partial { fenced, .. }
        | LiveState::PreAdmissionPartial { fenced, .. }
        | LiveState::Unknown { fenced, .. } => fenced,
        LiveState::Clean => false,
    };
    if !fenced {
        return false;
    }
    let acknowledged = policy.lock().acknowledged_fence;
    acknowledged.is_none_or(|attempt_id| {
        let Ok(receipt) = ledger.receipt(attempt_id) else {
            return true;
        };
        match status.live_state {
            LiveState::Partial { revision, fenced } => {
                !fenced || receipt.revision != Some(revision)
            }
            LiveState::PreAdmissionPartial {
                attempt_id: current,
                fenced,
            } => !fenced || current != attempt_id,
            LiveState::Unknown { .. } => true,
            LiveState::Clean => false,
        }
    })
}

fn clear_async_mutation(
    in_flight: &parking_lot::Mutex<Option<crate::mutation::AttemptId>>,
    attempt_id: crate::mutation::AttemptId,
) {
    let mut current = in_flight.lock();
    if *current == Some(attempt_id) {
        *current = None;
    }
}

fn mutation_ledger_error(error: crate::mutation::LedgerError) -> Error {
    Error::MutationLedger(error.to_string())
}

fn publish_mutation_transition(
    ledger: &MutationLedger,
    context: &MutationContext,
    receipt: &MutationReceipt,
    now: SystemTime,
) {
    context.reply(receipt.clone());
    let after = receipt
        .event_sequence
        .get()
        .checked_sub(1)
        .and_then(|sequence| crate::mutation::EventSequence::new(sequence).ok());
    if let EventQueryResult::Events { events } =
        ledger.events_after(receipt.runtime_epoch, after, now)
    {
        if let Some(event) = events
            .into_iter()
            .find(|event| event.event_sequence == receipt.event_sequence)
        {
            context.event(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{AddAction, BufferInfo};
    use crate::handlers::RouteDest;
    use crate::reload::ParamRouteKind;
    use crate::state::GroupState;
    use crate::types::{BufferId, BusId, GroupId, NodeId, ParamMap, VoiceId};
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

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

    #[derive(Clone, Default)]
    struct CapturedEvents {
        lines: Arc<Mutex<Vec<String>>>,
    }

    impl CapturedEvents {
        fn lines(&self) -> Vec<String> {
            self.lines.lock().unwrap().clone()
        }
    }

    struct CaptureLayer {
        sink: CapturedEvents,
    }

    impl<S> tracing_subscriber::Layer<S> for CaptureLayer
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            struct Visitor<'a>(&'a mut String);

            impl tracing::field::Visit for Visitor<'_> {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "message" {
                        let _ = std::fmt::Write::write_fmt(self.0, format_args!("{:?}", value));
                    }
                }

                fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                    if field.name() == "message" {
                        self.0.push_str(value);
                    }
                }
            }

            let mut line = String::new();
            event.record(&mut Visitor(&mut line));
            self.sink.lines.lock().unwrap().push(line);
        }
    }

    fn install_tracing_capture() -> (CapturedEvents, tracing::dispatcher::DefaultGuard) {
        let captured = CapturedEvents::default();
        let subscriber = tracing_subscriber::registry().with(CaptureLayer {
            sink: captured.clone(),
        });
        let guard = tracing::subscriber::set_default(subscriber);
        (captured, guard)
    }

    async fn register_input_synthdef(
        runtime: &Runtime<MockBackend>,
        synthdef: &str,
        inputs: Vec<vibelang_dsp::InputPort>,
    ) {
        let mut sd = vibelang_dsp::SynthDef::new(synthdef.to_string());
        sd.inputs = inputs;

        let mut state = runtime.state.write().await;
        state.synthdefs.insert(sd.name.clone());
        state.synthdef_inputs.insert(sd.name, sd.inputs);
    }

    async fn add_group(runtime: &Runtime<MockBackend>, group_id: GroupId) {
        let mut state = runtime.state.write().await;
        state.groups.insert(
            group_id,
            GroupState {
                id: group_id,
                name: format!("group_{}", group_id.raw()),
                parent: None,
                node_id: NodeId::new(group_id.raw()),
                audio_bus: BusId::new(32 + group_id.raw() * 2),
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );
    }

    fn script_state_with_voice(
        voice_id: VoiceId,
        synthdef: &str,
        group: GroupId,
    ) -> reload::ScriptState {
        let mut script_state = reload::ScriptState::new();
        script_state.add_voice(
            voice_id,
            crate::traits::VoiceConfig::new("target", synthdef, group),
        );
        script_state
    }

    #[tokio::test]
    async fn declared_in_autofeeds_from_parent_group_when_unrouted() {
        let runtime = Runtime::new(MockBackend);
        let voice_id = VoiceId::new(1);
        let group_id = GroupId::new(7);
        add_group(&runtime, group_id).await;
        register_input_synthdef(
            &runtime,
            "stereo_fx",
            vec![vibelang_dsp::InputPort::ar("in", 2)],
        )
        .await;

        let routes = runtime
            .effective_input_routes(&script_state_with_voice(voice_id, "stereo_fx", group_id))
            .await;

        assert_eq!(
            routes.get(&(voice_id, "in".to_string())),
            Some(&vec![InputRouteSrc::Group(group_id)])
        );
    }

    #[tokio::test]
    async fn declared_in_explicit_route_overrides_autofeed() {
        let runtime = Runtime::new(MockBackend);
        let target = VoiceId::new(1);
        let source = VoiceId::new(2);
        let group_id = GroupId::new(7);
        let explicit_src = InputRouteSrc::Voice(source, "out".to_string());
        add_group(&runtime, group_id).await;
        register_input_synthdef(
            &runtime,
            "stereo_fx",
            vec![vibelang_dsp::InputPort::ar("in", 2)],
        )
        .await;
        let mut script_state = script_state_with_voice(target, "stereo_fx", group_id);
        script_state.set_input_route(target, "in", explicit_src.clone());

        let routes = runtime.effective_input_routes(&script_state).await;

        assert_eq!(
            routes.get(&(target, "in".to_string())),
            Some(&vec![explicit_src])
        );
    }

    #[tokio::test]
    async fn declared_in_disconnect_clears_autofeed_and_input_is_silent() {
        let runtime = Runtime::new(MockBackend);
        let voice_id = VoiceId::new(1);
        let group_id = GroupId::new(7);
        add_group(&runtime, group_id).await;
        register_input_synthdef(
            &runtime,
            "stereo_fx",
            vec![vibelang_dsp::InputPort::ar("in", 2)],
        )
        .await;
        let mut script_state = script_state_with_voice(voice_id, "stereo_fx", group_id);
        script_state.set_input_route(voice_id, "in", InputRouteSrc::Silent);

        let routes = runtime.effective_input_routes(&script_state).await;

        assert_eq!(
            routes.get(&(voice_id, "in".to_string())),
            Some(&vec![InputRouteSrc::Silent])
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn declared_mono_in_does_not_autofeed_from_stereo_group_logs_warning() {
        let (captured, _guard) = install_tracing_capture();
        let runtime = Runtime::new(MockBackend);
        let voice_id = VoiceId::new(1);
        let group_id = GroupId::new(7);
        add_group(&runtime, group_id).await;
        register_input_synthdef(
            &runtime,
            "mono_fx",
            vec![vibelang_dsp::InputPort::ar("in", 1)],
        )
        .await;

        let routes = runtime
            .effective_input_routes(&script_state_with_voice(voice_id, "mono_fx", group_id))
            .await;

        assert_eq!(
            routes.get(&(voice_id, "in".to_string())),
            Some(&vec![InputRouteSrc::Silent])
        );
        assert!(
            captured.lines().iter().any(|line| {
                line.contains("Named input 'in'")
                    && line.contains("mono")
                    && line.contains("leaving input silent")
            }),
            "expected mono autofeed warning, got {:?}",
            captured.lines()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn declared_mono_in_with_explicit_route_skips_autofeed_warning() {
        let (captured, _guard) = install_tracing_capture();
        let runtime = Runtime::new(MockBackend);
        let target = VoiceId::new(1);
        let source = VoiceId::new(2);
        let group_id = GroupId::new(7);
        let explicit_src = InputRouteSrc::Voice(source, "out".to_string());
        add_group(&runtime, group_id).await;
        register_input_synthdef(
            &runtime,
            "mono_fx",
            vec![vibelang_dsp::InputPort::ar("in", 1)],
        )
        .await;
        let mut script_state = script_state_with_voice(target, "mono_fx", group_id);
        script_state.set_input_route(target, "in", explicit_src.clone());

        let routes = runtime.effective_input_routes(&script_state).await;

        assert_eq!(
            routes.get(&(target, "in".to_string())),
            Some(&vec![explicit_src])
        );
        assert!(
            !captured
                .lines()
                .iter()
                .any(|line| line.contains("Named input 'in'")),
            "explicit mono input route should not log autofeed warning: {:?}",
            captured.lines()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_reload_route_finalize_error_aborts_without_clean_complete_log() {
        let (captured, _guard) = install_tracing_capture();
        let mut runtime = Runtime::new(MockBackend);
        let voice_id = VoiceId::new(1);
        let group_id = GroupId::new(7);

        {
            let mut state = runtime.state.write().await;
            state.synthdefs.insert("kr_src".to_string());
            state.synthdef_outputs.insert(
                "kr_src".to_string(),
                vec![vibelang_dsp::OutputPort::kr("env", 1)],
            );
        }

        let mut script_state = reload::ScriptState::new();
        script_state.add_group(
            group_id,
            reload::GroupConfig {
                name: "mods".to_string(),
                ..Default::default()
            },
        );
        script_state.add_voice(
            voice_id,
            crate::traits::VoiceConfig::new("bad_mod", "kr_src", group_id),
        );
        script_state.set_route(voice_id, "env", RouteDest::Group(group_id));

        let err = runtime
            .apply_reload(script_state)
            .await
            .expect_err("kr-to-group route must abort reload");

        assert!(err.to_string().contains("kr-rate"), "err = {err}");
        assert!(
            runtime.state.read().await.current_routes.is_empty(),
            "failed route reload must not advance current_routes"
        );
        let lines = captured.lines();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("routes.finalize failed") && line.contains("aborting")),
            "expected abort log, got {lines:?}"
        );
        assert!(
            !lines.iter().any(|line| line.contains("Reload: complete")),
            "failed route reload must not log clean completion: {lines:?}"
        );
    }

    #[tokio::test]
    async fn hot_reload_toggle_renaming_port_to_and_from_in_updates_autofeed() {
        let runtime = Runtime::new(MockBackend);
        let voice_id = VoiceId::new(1);
        let group_id = GroupId::new(7);
        add_group(&runtime, group_id).await;
        register_input_synthdef(
            &runtime,
            "reload_fx",
            vec![vibelang_dsp::InputPort::ar("other", 2)],
        )
        .await;
        let script_state = script_state_with_voice(voice_id, "reload_fx", group_id);

        let other_routes = runtime.effective_input_routes(&script_state).await;
        assert_eq!(
            other_routes.get(&(voice_id, "other".to_string())),
            Some(&vec![InputRouteSrc::Silent])
        );
        assert!(!other_routes.contains_key(&(voice_id, "in".to_string())));

        register_input_synthdef(
            &runtime,
            "reload_fx",
            vec![vibelang_dsp::InputPort::ar("in", 2)],
        )
        .await;
        let in_routes = runtime.effective_input_routes(&script_state).await;
        assert_eq!(
            in_routes.get(&(voice_id, "in".to_string())),
            Some(&vec![InputRouteSrc::Group(group_id)])
        );
        assert!(!in_routes.contains_key(&(voice_id, "other".to_string())));

        register_input_synthdef(
            &runtime,
            "reload_fx",
            vec![vibelang_dsp::InputPort::ar("other", 2)],
        )
        .await;
        let renamed_away_routes = runtime.effective_input_routes(&script_state).await;
        assert_eq!(
            renamed_away_routes.get(&(voice_id, "other".to_string())),
            Some(&vec![InputRouteSrc::Silent])
        );
        assert!(!renamed_away_routes.contains_key(&(voice_id, "in".to_string())));
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
                    owner: crate::state::PatternOwner::Script,
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
                output_channels: None,
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

    #[tokio::test]
    async fn apply_reload_preserves_looper_owned_pattern_on_unrelated_script_change() {
        use crate::reload::ScriptState;
        use crate::state::{MelodyState, PatternOwner, PatternState};
        use crate::traits::{MelodyConfig, NoteEvent, PatternConfig};
        use crate::types::{MelodyId, PatternId};

        let looper_pattern_id = PatternId::new(101);
        let melody_id = MelodyId::new(202);
        let mut runtime = Runtime::new(MockBackend);

        let looper_config =
            PatternConfig::without_voice("__looper_1_1", crate::types::Beat::from_f64(4.0));
        let mut looper_pattern =
            PatternState::with_owner(looper_pattern_id, looper_config, PatternOwner::Looper);
        looper_pattern.playing = true;

        let original_melody =
            MelodyConfig::without_voice("script_melody", crate::types::Beat::from_f64(4.0))
                .with_note(NoteEvent::quarter(0.0, 60, 0.8));
        {
            let mut state = runtime.state.write().await;
            state.patterns.insert(looper_pattern_id, looper_pattern);
            state
                .melodies
                .insert(melody_id, MelodyState::new(melody_id, original_melody));
        }

        let mut new_state = ScriptState::new();
        let edited_melody =
            MelodyConfig::without_voice("script_melody", crate::types::Beat::from_f64(4.0))
                .with_note(NoteEvent::quarter(0.0, 64, 0.8));
        new_state.add_melody(melody_id, edited_melody);

        runtime.apply_reload(new_state).await.unwrap();

        let state = runtime.state.read().await;
        let looper_pattern = state
            .patterns
            .get(&looper_pattern_id)
            .expect("looper-owned pattern should survive reload");
        assert_eq!(looper_pattern.owner, PatternOwner::Looper);
        assert!(looper_pattern.playing);
    }

    #[tokio::test]
    #[cfg(feature = "midi")]
    async fn looper_only_reload_reaches_midi_route_reconciliation_phase() {
        use crate::reload::{LooperConfig, ScriptState};
        use crate::types::{MidiDeviceId, VoiceId};

        let mut runtime = Runtime::new(MockBackend);
        let mut old_state = ScriptState::new();
        old_state.loopers.push(LooperConfig {
            device_id: MidiDeviceId::new(1),
            voice_id: VoiceId::new(1),
            channel: Some(0),
            silence_bars: 1.0,
            quantize_beats: 0.0,
        });
        runtime.midi.mark_script_routes_applied(&old_state);

        let mut new_state = ScriptState::new();
        new_state.loopers.push(LooperConfig {
            device_id: MidiDeviceId::new(1),
            voice_id: VoiceId::new(1),
            channel: Some(1),
            silence_bars: 1.0,
            quantize_beats: 0.0,
        });

        assert!(runtime.midi.script_routes_changed(&new_state));

        runtime.apply_reload(new_state.clone()).await.unwrap();

        assert!(
            !runtime.midi.script_routes_changed(&new_state),
            "looper-only reload must not early-return before MIDI route reconciliation"
        );
    }

    // =========================================================================
    // Multi-output Story 7: post-mix invariant tests
    //
    // These tests pin the SC tree spawn order during apply_reload:
    //   voices' synth nodes (Head)
    //     -> routes' mixer synths (RoutesHandler::finalize)
    //     -> group's effect chain (EffectsHandler::add)
    //     -> group bus (link synth created by GroupsHandler::finalize)
    //     -> main bus
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
        create_group_log: std::sync::Mutex<Vec<NodeId>>,
        free_node_log: std::sync::Mutex<Vec<NodeId>>,
        map_param_log: std::sync::Mutex<Vec<RecordedMap>>,
        /// Unified create/free event stream — preserves ordering across both
        /// op kinds so reload-reconciler tests can resolve "what is alive at
        /// node N after free+respawn" even when the ID pool recycles a freed
        /// id back into the next create.
        events: std::sync::Mutex<Vec<BackendEvent>>,
        fail_create_group: std::sync::atomic::AtomicBool,
        sync_mode: std::sync::atomic::AtomicU8,
    }

    #[derive(Clone, Debug)]
    #[allow(dead_code)]
    enum BackendEvent {
        Create {
            def: String,
            node: NodeId,
            link_outbus: Option<f32>,
        },
        Free {
            node: NodeId,
        },
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
        /// `outbus` param from `system_link_audio*` synths (no underscore).
        link_outbus: Option<f32>,
        params: ParamMap,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct RecordedMap {
        node: NodeId,
        param: String,
        bus: u32,
    }

    impl RecordingBackend {
        fn new() -> Self {
            Self {
                create_synth_log: std::sync::Mutex::new(Vec::new()),
                create_group_log: std::sync::Mutex::new(Vec::new()),
                free_node_log: std::sync::Mutex::new(Vec::new()),
                map_param_log: std::sync::Mutex::new(Vec::new()),
                events: std::sync::Mutex::new(Vec::new()),
                fail_create_group: std::sync::atomic::AtomicBool::new(false),
                sync_mode: std::sync::atomic::AtomicU8::new(0),
            }
        }

        fn synths(&self) -> Vec<RecordedSynth> {
            self.create_synth_log.lock().unwrap().clone()
        }

        fn maps(&self) -> Vec<RecordedMap> {
            self.map_param_log.lock().unwrap().clone()
        }

        fn created_groups(&self) -> Vec<NodeId> {
            self.create_group_log.lock().unwrap().clone()
        }

        /// Link synths (`system_link_audio*`) that are still alive — i.e. the
        /// most recent op for each node id is a `Create`. Walks the unified
        /// event log so it stays correct across IDPool reuse (free + alloc
        /// can return the same id, and we still want to see the surviving
        /// synth attributed to the latest spawn).
        fn alive_link_synths(&self) -> Vec<(String, NodeId, Option<f32>)> {
            use std::collections::HashMap;
            let events = self.events.lock().unwrap();
            // Map node_id -> latest create attributes, cleared on Free.
            let mut latest: HashMap<NodeId, (String, Option<f32>)> = HashMap::new();
            for ev in events.iter() {
                match ev {
                    BackendEvent::Create {
                        def,
                        node,
                        link_outbus,
                    } => {
                        if def.starts_with("system_link_audio") {
                            latest.insert(*node, (def.clone(), *link_outbus));
                        }
                    }
                    BackendEvent::Free { node } => {
                        latest.remove(node);
                    }
                }
            }
            let mut alive: Vec<_> = latest
                .into_iter()
                .map(|(n, (def, outbus))| (def, n, outbus))
                .collect();
            alive.sort_by_key(|(_, n, _)| n.0);
            alive
        }

        fn alive_synths(&self) -> Vec<(String, NodeId)> {
            use std::collections::HashMap;
            let events = self.events.lock().unwrap();
            let mut latest: HashMap<NodeId, String> = HashMap::new();
            for ev in events.iter() {
                match ev {
                    BackendEvent::Create { def, node, .. } => {
                        latest.insert(*node, def.clone());
                    }
                    BackendEvent::Free { node } => {
                        latest.remove(node);
                    }
                }
            }
            let mut alive: Vec<_> = latest.into_iter().map(|(node, def)| (def, node)).collect();
            alive.sort_by_key(|(_, node)| node.0);
            alive
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
            let link_outbus = params.get("outbus").copied();
            self.create_synth_log.lock().unwrap().push(RecordedSynth {
                def: def.to_string(),
                node,
                target,
                action,
                in_bus: params.get("in_bus").copied(),
                out_bus: params.get("out_bus").copied(),
                fx_bus_in: params.get("__fx_bus_in").copied(),
                fx_bus_out: params.get("__fx_bus_out").copied(),
                link_outbus,
                params: params.clone(),
            });
            self.events.lock().unwrap().push(BackendEvent::Create {
                def: def.to_string(),
                node,
                link_outbus,
            });
            Ok(())
        }

        async fn create_group(
            &self,
            node: NodeId,
            _target: NodeId,
            _action: AddAction,
        ) -> std::result::Result<(), Self::Error> {
            if self
                .fail_create_group
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                return Err(MockError);
            }
            self.create_group_log.lock().unwrap().push(node);
            Ok(())
        }

        async fn free_node(&self, node: NodeId) -> std::result::Result<(), Self::Error> {
            self.free_node_log.lock().unwrap().push(node);
            self.events
                .lock()
                .unwrap()
                .push(BackendEvent::Free { node });
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
            node: NodeId,
            param: &str,
            bus: u32,
        ) -> std::result::Result<(), Self::Error> {
            self.map_param_log.lock().unwrap().push(RecordedMap {
                node,
                param: param.to_string(),
                bus,
            });
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

        async fn sync(&self) -> std::result::Result<(), Self::Error> {
            match self.sync_mode.load(std::sync::atomic::Ordering::SeqCst) {
                0 => Ok(()),
                1 => Err(MockError),
                2 => std::future::pending().await,
                mode => panic!("unsupported test sync mode {mode}"),
            }
        }
    }

    /// Pre-register a synthdef name with explicit OutputPort descriptors so
    /// that the voice handler allocates one audio bus per port.
    async fn register_voice_synthdef(
        runtime: &Runtime<RecordingBackend>,
        name: &str,
        ports: Vec<vibelang_dsp::OutputPort>,
    ) {
        vibelang_dsp::register_synthdef_outputs(name.to_string(), ports.clone());
        let mut state = runtime.state.write().await;
        state.synthdefs.insert(name.to_string());
        state.synthdef_outputs.insert(name.to_string(), ports);
    }

    /// Pre-register an effect synthdef name (no port descriptors needed).
    async fn register_effect_synthdef(runtime: &Runtime<RecordingBackend>, name: &str) {
        runtime
            .state
            .write()
            .await
            .synthdefs
            .insert(name.to_string());
    }

    #[tokio::test]
    async fn apply_reload_source_first_set_param_routes_emit_live_n_map() {
        use crate::handlers::{ParamRouteTarget, RouteDest};
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        let source_synth = "mixed_audio_cv_source";
        let target_synth = "param_target";
        register_voice_synthdef(
            &runtime,
            source_synth,
            vec![
                OutputPort {
                    name: "audio".to_string(),
                    channels: 2,
                    rate: PortRate::Ar,
                },
                OutputPort {
                    name: "env".to_string(),
                    channels: 1,
                    rate: PortRate::Kr,
                },
                OutputPort {
                    name: "dummy".to_string(),
                    channels: 1,
                    rate: PortRate::Kr,
                },
            ],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            target_synth,
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(10);
        let target_a = VoiceId::new(20);
        let target_b = VoiceId::new(21);

        let mut base = ScriptState::new();
        base.add_group(group, GroupConfig::default());
        base.add_voice(src, VoiceConfig::new("src", source_synth, group));
        base.add_voice(target_a, VoiceConfig::new("target_a", target_synth, group));
        base.add_voice(target_b, VoiceConfig::new("target_b", target_synth, group));
        base.set_route(src, "audio", RouteDest::Group(group));
        base.set_route(src, "dummy", RouteDest::Muted);

        runtime.apply_reload(base.clone()).await.unwrap();

        let mut routed = base;
        routed
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "env",
                ParamRouteTarget::Voice(target_a),
                "cutoff",
            )
            .unwrap();
        routed
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "env",
                ParamRouteTarget::Voice(target_b),
                "pitch",
            )
            .unwrap();
        routed.running_voices.insert(target_a);
        routed.running_voices.insert(target_b);

        runtime.apply_reload(routed).await.unwrap();

        let state = runtime.state.read().await;
        let target_a_node = state.voices.get(&target_a).unwrap().active_nodes[0];
        let target_b_node = state.voices.get(&target_b).unwrap().active_nodes[0];
        let bus_a = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target_a), "cutoff".to_string()))
            .expect("source-first SET route should create target_a summer")
            .bus
            .raw();
        let bus_b = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target_b), "pitch".to_string()))
            .expect("source-first SET fan-out should create target_b summer")
            .bus
            .raw();
        drop(state);

        let maps = runtime.backend.maps();
        assert!(maps.contains(&RecordedMap {
            node: target_a_node,
            param: "cutoff".to_string(),
            bus: bus_a,
        }));
        assert!(maps.contains(&RecordedMap {
            node: target_b_node,
            param: "pitch".to_string(),
            bus: bus_b,
        }));

        let summer_count = runtime
            .backend
            .synths()
            .iter()
            .filter(|s| s.def == "param_kr_modulate_1")
            .count();
        assert_eq!(
            summer_count, 2,
            "fan-out should spawn one SET summer per target param"
        );
    }

    #[tokio::test]
    async fn apply_reload_target_first_bend_routes_emit_live_n_map_with_fan_in() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        let source_synth = "kr_source";
        let target_synth = "bend_target";
        register_voice_synthdef(
            &runtime,
            source_synth,
            vec![OutputPort {
                name: "out".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            target_synth,
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src_a = VoiceId::new(30);
        let src_b = VoiceId::new(31);
        let target = VoiceId::new(40);

        let mut base = ScriptState::new();
        base.add_group(group, GroupConfig::default());
        base.add_voice(src_a, VoiceConfig::new("src_a", source_synth, group));
        base.add_voice(src_b, VoiceConfig::new("src_b", source_synth, group));
        base.add_voice(target, VoiceConfig::new("target", target_synth, group));

        runtime.apply_reload(base.clone()).await.unwrap();

        let mut routed = base;
        routed
            .add_param_route(
                ParamRouteKind::Bend,
                src_a,
                "out",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        routed
            .add_param_route(
                ParamRouteKind::Bend,
                src_b,
                "out",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        routed.running_voices.insert(target);

        runtime.apply_reload(routed).await.unwrap();

        let state = runtime.state.read().await;
        let target_node = state.voices.get(&target).unwrap().active_nodes[0];
        let summer_bus = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
            .expect("target-first BEND fan-in should create one summer")
            .bus
            .raw();
        drop(state);

        assert!(runtime.backend.maps().contains(&RecordedMap {
            node: target_node,
            param: "cutoff".to_string(),
            bus: summer_bus,
        }));
        assert_eq!(
            runtime
                .backend
                .synths()
                .iter()
                .filter(|s| s.def == "param_kr_modulate_2")
                .count(),
            1,
            "two target-first BEND sources should fan into one modulate_2 summer"
        );
    }

    #[tokio::test]
    async fn note_on_after_set_route_materialization_inherits_existing_param_summer() {
        use crate::handlers::ParamRouteTarget;
        use crate::message::VoiceMessage;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "late_set_source",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "late_set_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(10);
        let target = VoiceId::new(20);

        let mut routed = ScriptState::new();
        routed.add_group(group, GroupConfig::default());
        routed.add_voice(src, VoiceConfig::new("src", "late_set_source", group));
        routed.add_voice(target, VoiceConfig::new("target", "late_set_target", group));
        routed.running_voices.insert(src);
        routed.running_voices.insert(target);
        routed
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "out",
                ParamRouteTarget::Voice(target),
                "freq",
            )
            .unwrap();
        routed.set_param_route_set_scale(src, "out", ParamRouteTarget::Voice(target), "freq", 2.0);
        routed.set_param_route_set_offset(src, "out", ParamRouteTarget::Voice(target), "freq", 3.0);

        runtime.apply_reload(routed).await.unwrap();

        let (summer_bus, summer_node, initial_summer_count) = {
            let state = runtime.state.read().await;
            let summer = state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
                .expect("SET route should materialize one summer");
            assert_eq!(summer.sources[0].scale, 2.0);
            assert_eq!(summer.sources[0].offset, 3.0);
            (
                summer.bus.raw(),
                summer.node,
                runtime
                    .backend
                    .synths()
                    .iter()
                    .filter(|s| s.def == "param_kr_modulate_1")
                    .count(),
            )
        };

        runtime
            .send(
                VoiceMessage::NoteOn {
                    voice: target,
                    note: 60,
                    velocity: 0.8,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        let new_node = {
            let state = runtime.state.read().await;
            let summer = state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
                .expect("late note_on must reuse the existing summer");
            assert_eq!(summer.node, summer_node);
            state.voices.get(&target).unwrap().note_nodes[&60]
        };
        assert_eq!(
            runtime
                .backend
                .synths()
                .iter()
                .filter(|s| s.def == "param_kr_modulate_1")
                .count(),
            initial_summer_count,
            "late note_on must not respawn the voice-target-scoped summer"
        );
        assert!(runtime.backend.maps().contains(&RecordedMap {
            node: new_node,
            param: "freq".to_string(),
            bus: summer_bus,
        }));
    }

    #[tokio::test]
    async fn trigger_after_bend_route_materialization_inherits_existing_param_summer() {
        use crate::handlers::ParamRouteTarget;
        use crate::message::VoiceMessage;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "late_bend_source",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "late_bend_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(30);
        let target = VoiceId::new(40);

        let mut routed = ScriptState::new();
        routed.add_group(group, GroupConfig::default());
        routed.add_voice(src, VoiceConfig::new("src", "late_bend_source", group));
        let mut target_config = VoiceConfig::new("target", "late_bend_target", group);
        target_config.params.insert("freq".to_string(), 220.0);
        routed.add_voice(target, target_config);
        routed.running_voices.insert(src);
        routed.running_voices.insert(target);
        routed
            .add_param_route(
                ParamRouteKind::Bend,
                src,
                "out",
                ParamRouteTarget::Voice(target),
                "freq",
            )
            .unwrap();
        routed.set_param_route_bend_scale(src, "out", ParamRouteTarget::Voice(target), "freq", 4.0);
        routed.set_param_route_bend_offset(
            src,
            "out",
            ParamRouteTarget::Voice(target),
            "freq",
            5.0,
        );

        runtime.apply_reload(routed).await.unwrap();

        let (summer_bus, initial_summer_count) = {
            let state = runtime.state.read().await;
            let summer = state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
                .expect("BEND route should materialize one summer");
            assert_eq!(summer.sources[0].scale, 4.0);
            assert_eq!(summer.sources[0].offset, 5.0);
            (
                summer.bus.raw(),
                runtime
                    .backend
                    .synths()
                    .iter()
                    .filter(|s| s.def == "param_kr_modulate_1")
                    .count(),
            )
        };

        runtime
            .send(
                VoiceMessage::Trigger {
                    id: target,
                    params: ParamMap::new(),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        let new_node = {
            let state = runtime.state.read().await;
            *state
                .voices
                .get(&target)
                .unwrap()
                .active_nodes
                .last()
                .unwrap()
        };
        assert_eq!(
            runtime
                .backend
                .synths()
                .iter()
                .filter(|s| s.def == "param_kr_modulate_1")
                .count(),
            initial_summer_count,
            "late trigger must not respawn the voice-target-scoped summer"
        );
        assert!(runtime.backend.maps().contains(&RecordedMap {
            node: new_node,
            param: "freq".to_string(),
            bus: summer_bus,
        }));
    }

    #[tokio::test]
    async fn note_on_after_trigger_route_materialization_inherits_existing_trigger_link() {
        use crate::handlers::ParamRouteTarget;
        use crate::message::VoiceMessage;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "late_trigger_source",
            vec![OutputPort {
                name: "trig".to_string(),
                channels: 1,
                rate: PortRate::Tr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "late_trigger_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(50);
        let target = VoiceId::new(60);

        let mut routed = ScriptState::new();
        routed.add_group(group, GroupConfig::default());
        routed.add_voice(src, VoiceConfig::new("src", "late_trigger_source", group));
        routed.add_voice(
            target,
            VoiceConfig::new("target", "late_trigger_target", group),
        );
        routed.running_voices.insert(src);
        routed.running_voices.insert(target);
        routed
            .add_param_route(
                ParamRouteKind::Trigger,
                src,
                "trig",
                ParamRouteTarget::Voice(target),
                "gate",
            )
            .unwrap();

        runtime.apply_reload(routed).await.unwrap();

        let (link_bus, link_node, initial_link_count) = {
            let state = runtime.state.read().await;
            let (link_node, link_bus) = state
                .param_triggers
                .get(&(ParamRouteTarget::Voice(target), "gate".to_string()))
                .copied()
                .expect("TRIGGER route should materialize one link");
            (
                link_bus.raw(),
                link_node,
                runtime
                    .backend
                    .synths()
                    .iter()
                    .filter(|s| s.def == "port_tr_to_param_link_1")
                    .count(),
            )
        };

        runtime
            .send(
                VoiceMessage::NoteOn {
                    voice: target,
                    note: 67,
                    velocity: 0.7,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        let new_node = {
            let state = runtime.state.read().await;
            let (current_link, _) = state
                .param_triggers
                .get(&(ParamRouteTarget::Voice(target), "gate".to_string()))
                .copied()
                .expect("late note_on must reuse the existing trigger link");
            assert_eq!(current_link, link_node);
            state.voices.get(&target).unwrap().note_nodes[&67]
        };
        assert_eq!(
            runtime
                .backend
                .synths()
                .iter()
                .filter(|s| s.def == "port_tr_to_param_link_1")
                .count(),
            initial_link_count,
            "late note_on must not respawn the voice-target-scoped trigger link"
        );
        assert!(runtime.backend.maps().contains(&RecordedMap {
            node: new_node,
            param: "gate".to_string(),
            bus: link_bus,
        }));
    }

    #[tokio::test]
    async fn apply_reload_threads_param_route_shaping_and_route_only_updates() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "shape_source",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "shape_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(10);
        let target = VoiceId::new(20);

        let mut base = ScriptState::new();
        base.add_group(group, GroupConfig::default());
        base.add_voice(src, VoiceConfig::new("src", "shape_source", group));
        let mut target_config = VoiceConfig::new("target", "shape_target", group);
        target_config.params.insert("freq".to_string(), 220.0);
        base.add_voice(target, target_config);
        base.running_voices.insert(target);
        runtime.apply_reload(base.clone()).await.unwrap();

        let mut routed = base.clone();
        routed
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "out",
                ParamRouteTarget::Voice(target),
                "freq",
            )
            .unwrap();
        routed.set_param_route_set_scale(
            src,
            "out",
            ParamRouteTarget::Voice(target),
            "freq",
            700.0,
        );
        routed.set_param_route_set_offset(
            src,
            "out",
            ParamRouteTarget::Voice(target),
            "freq",
            110.0,
        );
        runtime.apply_reload(routed.clone()).await.unwrap();

        let first_summer = {
            let state = runtime.state.read().await;
            let summer = state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
                .expect("SET route should create a shaped summer");
            assert_eq!(summer.sources[0].scale, 700.0);
            assert_eq!(summer.sources[0].offset, 110.0);
            summer.node
        };
        let first_create = runtime
            .backend
            .synths()
            .into_iter()
            .rev()
            .find(|s| s.def == "param_kr_modulate_1")
            .expect("SET should spawn modulate_1");
        assert_eq!(first_create.params.get("scale_a"), Some(&700.0));
        assert_eq!(first_create.params.get("offset_a"), Some(&110.0));

        routed.set_param_route_set_scale(
            src,
            "out",
            ParamRouteTarget::Voice(target),
            "freq",
            500.0,
        );
        runtime.apply_reload(routed).await.unwrap();

        let updated_create = runtime
            .backend
            .synths()
            .into_iter()
            .rev()
            .find(|s| s.def == "param_kr_modulate_1")
            .expect("shaping-only reload should respawn modulate_1");
        assert_eq!(updated_create.params.get("scale_a"), Some(&500.0));
        assert_eq!(updated_create.params.get("offset_a"), Some(&110.0));
        assert!(
            runtime
                .backend
                .free_node_log
                .lock()
                .unwrap()
                .contains(&first_summer),
            "shaping-only reload should free the stale summer"
        );
    }

    #[tokio::test]
    async fn apply_reload_bend_fan_in_preserves_per_source_shaping_and_shrinks() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "bend_shape_source",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "bend_shape_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src_a = VoiceId::new(10);
        let src_b = VoiceId::new(11);
        let target = VoiceId::new(20);

        let mut base = ScriptState::new();
        base.add_group(group, GroupConfig::default());
        base.add_voice(src_a, VoiceConfig::new("src_a", "bend_shape_source", group));
        base.add_voice(src_b, VoiceConfig::new("src_b", "bend_shape_source", group));
        let mut target_config = VoiceConfig::new("target", "bend_shape_target", group);
        target_config.params.insert("freq".to_string(), 220.0);
        base.add_voice(target, target_config);
        base.running_voices.insert(target);
        runtime.apply_reload(base.clone()).await.unwrap();

        let mut two = base.clone();
        two.add_param_route(
            ParamRouteKind::Bend,
            src_a,
            "out",
            ParamRouteTarget::Voice(target),
            "freq",
        )
        .unwrap();
        two.set_param_route_bend_scale(src_a, "out", ParamRouteTarget::Voice(target), "freq", 30.0);
        two.set_param_route_bend_offset(src_a, "out", ParamRouteTarget::Voice(target), "freq", 3.0);
        two.add_param_route(
            ParamRouteKind::Bend,
            src_b,
            "out",
            ParamRouteTarget::Voice(target),
            "freq",
        )
        .unwrap();
        two.set_param_route_bend_scale(src_b, "out", ParamRouteTarget::Voice(target), "freq", 40.0);
        two.set_param_route_bend_offset(src_b, "out", ParamRouteTarget::Voice(target), "freq", 4.0);
        runtime.apply_reload(two).await.unwrap();

        let two_summer = {
            let state = runtime.state.read().await;
            let summer = state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
                .expect("two BEND sources should create a summer");
            assert_eq!(summer.sources.len(), 2);
            assert_eq!(
                summer
                    .sources
                    .iter()
                    .map(|source| (source.scale, source.offset))
                    .collect::<Vec<_>>(),
                vec![(30.0, 3.0), (40.0, 4.0)]
            );
            summer.node
        };
        let fan_in_create = runtime
            .backend
            .synths()
            .into_iter()
            .rev()
            .find(|s| s.def == "param_kr_modulate_2")
            .expect("BEND fan-in should spawn modulate_2");
        assert_eq!(fan_in_create.params.get("scale_a"), Some(&30.0));
        assert_eq!(fan_in_create.params.get("offset_a"), Some(&3.0));
        assert_eq!(fan_in_create.params.get("scale_b"), Some(&40.0));
        assert_eq!(fan_in_create.params.get("offset_b"), Some(&4.0));

        let mut one = base.clone();
        one.add_param_route(
            ParamRouteKind::Bend,
            src_a,
            "out",
            ParamRouteTarget::Voice(target),
            "freq",
        )
        .unwrap();
        one.set_param_route_bend_scale(src_a, "out", ParamRouteTarget::Voice(target), "freq", 30.0);
        one.set_param_route_bend_offset(src_a, "out", ParamRouteTarget::Voice(target), "freq", 3.0);
        runtime.apply_reload(one).await.unwrap();
        assert!(
            runtime
                .backend
                .free_node_log
                .lock()
                .unwrap()
                .contains(&two_summer),
            "N=2 to N=1 should free the old BEND summer"
        );
        let one_summer = {
            let state = runtime.state.read().await;
            state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "freq".to_string()))
                .expect("N=1 BEND route should keep one summer")
                .node
        };

        runtime.apply_reload(base).await.unwrap();
        let state = runtime.state.read().await;
        assert!(
            !state
                .param_summers
                .contains_key(&(ParamRouteTarget::Voice(target), "freq".to_string())),
            "N=1 to N=0 should remove the BEND summer"
        );
        drop(state);
        assert!(
            runtime
                .backend
                .free_node_log
                .lock()
                .unwrap()
                .contains(&one_summer),
            "N=1 to N=0 should free the stale BEND summer"
        );
    }

    #[tokio::test]
    async fn apply_reload_trigger_route_removal_unmaps_and_frees_link() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "trigger_source",
            vec![OutputPort {
                name: "trig".to_string(),
                channels: 1,
                rate: PortRate::Tr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "trigger_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(10);
        let target = VoiceId::new(20);

        let mut base = ScriptState::new();
        base.add_group(group, GroupConfig::default());
        base.add_voice(src, VoiceConfig::new("src", "trigger_source", group));
        base.add_voice(target, VoiceConfig::new("target", "trigger_target", group));
        base.running_voices.insert(target);
        runtime.apply_reload(base.clone()).await.unwrap();

        let mut routed = base.clone();
        routed
            .add_param_route(
                ParamRouteKind::Trigger,
                src,
                "trig",
                ParamRouteTarget::Voice(target),
                "trig",
            )
            .unwrap();
        runtime.apply_reload(routed).await.unwrap();

        let (target_node, link_node) = {
            let state = runtime.state.read().await;
            let target_node = state.voices.get(&target).unwrap().active_nodes[0];
            let (link_node, _) = state
                .param_triggers
                .get(&(ParamRouteTarget::Voice(target), "trig".to_string()))
                .copied()
                .expect("TRIGGER route should create a link");
            (target_node, link_node)
        };
        assert!(runtime
            .backend
            .synths()
            .iter()
            .any(|s| s.def == "port_tr_to_param_link_1"));

        runtime.apply_reload(base).await.unwrap();
        assert!(
            runtime.backend.maps().contains(&RecordedMap {
                node: target_node,
                param: "trig".to_string(),
                bus: u32::MAX,
            }),
            "TRIGGER route removal should unmap the target param"
        );
        assert!(
            runtime
                .backend
                .free_node_log
                .lock()
                .unwrap()
                .contains(&link_node),
            "TRIGGER route removal should free the link synth"
        );
    }

    #[tokio::test]
    async fn apply_reload_deleting_set_source_tears_down_summer_and_adapter() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "delete_set_source",
            vec![OutputPort {
                name: "env".to_string(),
                channels: 1,
                rate: PortRate::Ar,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "delete_set_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(110);
        let target = VoiceId::new(120);

        let mut routed = ScriptState::new();
        routed.add_group(group, GroupConfig::default());
        routed.add_voice(src, VoiceConfig::new("src", "delete_set_source", group));
        routed.add_voice(
            target,
            VoiceConfig::new("target", "delete_set_target", group),
        );
        routed.running_voices.insert(target);
        routed
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "env",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        runtime.apply_reload(routed).await.unwrap();

        let (target_node, summer_node, adapter_node) = {
            let state = runtime.state.read().await;
            let target_node = state.voices[&target].active_nodes[0];
            let summer_node = state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
                .expect("SET route should create a summer")
                .node;
            let (adapter_node, _) = state.ar_to_kr_adapters[&(src, "env".to_string())];
            (target_node, summer_node, adapter_node)
        };

        let mut source_deleted = ScriptState::new();
        source_deleted.add_group(group, GroupConfig::default());
        source_deleted.add_voice(
            target,
            VoiceConfig::new("target", "delete_set_target", group),
        );
        source_deleted.running_voices.insert(target);
        runtime.apply_reload(source_deleted).await.unwrap();

        let state = runtime.state.read().await;
        assert!(!state.voices.contains_key(&src));
        assert!(state.param_routes_set.is_empty());
        assert!(state.param_summers.is_empty());
        assert!(state.ar_to_kr_adapters.is_empty());
        drop(state);

        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(frees.contains(&summer_node));
        assert!(frees.contains(&adapter_node));
        assert!(runtime.backend.maps().contains(&RecordedMap {
            node: target_node,
            param: "cutoff".to_string(),
            bus: u32::MAX,
        }));
    }

    #[tokio::test]
    async fn apply_reload_deleting_one_bend_source_rebuilds_fan_in_summer() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "delete_bend_source",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "delete_bend_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src_a = VoiceId::new(130);
        let src_b = VoiceId::new(131);
        let target = VoiceId::new(140);

        let mut two_sources = ScriptState::new();
        two_sources.add_group(group, GroupConfig::default());
        two_sources.add_voice(
            src_a,
            VoiceConfig::new("src_a", "delete_bend_source", group),
        );
        two_sources.add_voice(
            src_b,
            VoiceConfig::new("src_b", "delete_bend_source", group),
        );
        two_sources.add_voice(
            target,
            VoiceConfig::new("target", "delete_bend_target", group),
        );
        two_sources.running_voices.insert(target);
        two_sources
            .add_param_route(
                ParamRouteKind::Bend,
                src_a,
                "out",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        two_sources
            .add_param_route(
                ParamRouteKind::Bend,
                src_b,
                "out",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        runtime.apply_reload(two_sources).await.unwrap();

        let old_summer_node = {
            let state = runtime.state.read().await;
            state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
                .expect("BEND fan-in should create a summer")
                .node
        };

        let mut one_source = ScriptState::new();
        one_source.add_group(group, GroupConfig::default());
        one_source.add_voice(
            src_b,
            VoiceConfig::new("src_b", "delete_bend_source", group),
        );
        one_source.add_voice(
            target,
            VoiceConfig::new("target", "delete_bend_target", group),
        );
        one_source.running_voices.insert(target);
        one_source
            .add_param_route(
                ParamRouteKind::Bend,
                src_b,
                "out",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        runtime.apply_reload(one_source).await.unwrap();

        let state = runtime.state.read().await;
        assert!(!state.voices.contains_key(&src_a));
        assert!(!state
            .param_routes_bend
            .contains_key(&(src_a, "out".to_string())));
        let src_b_bus = state.voices[&src_b]
            .output_buses
            .iter()
            .find(|(name, _)| name == "out")
            .map(|(_, bus)| *bus)
            .unwrap();
        let summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
            .expect("surviving BEND source should get a rebuilt summer");
        assert_eq!(summer.sources.len(), 1);
        assert_eq!(summer.sources[0].bus, src_b_bus);
        drop(state);

        assert!(runtime
            .backend
            .free_node_log
            .lock()
            .unwrap()
            .contains(&old_summer_node));
        assert!(runtime
            .backend
            .alive_synths()
            .iter()
            .any(|(def, _)| def == "param_kr_modulate_1"));
    }

    #[tokio::test]
    async fn apply_reload_deleting_trigger_target_frees_link_without_unmapping_freed_node() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "delete_trigger_source",
            vec![OutputPort {
                name: "trig".to_string(),
                channels: 1,
                rate: PortRate::Tr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "delete_trigger_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(150);
        let target = VoiceId::new(160);

        let mut routed = ScriptState::new();
        routed.add_group(group, GroupConfig::default());
        routed.add_voice(src, VoiceConfig::new("src", "delete_trigger_source", group));
        routed.add_voice(
            target,
            VoiceConfig::new("target", "delete_trigger_target", group),
        );
        routed.running_voices.insert(target);
        routed
            .add_param_route(
                ParamRouteKind::Trigger,
                src,
                "trig",
                ParamRouteTarget::Voice(target),
                "gate",
            )
            .unwrap();
        runtime.apply_reload(routed).await.unwrap();

        let (target_node, link_node) = {
            let state = runtime.state.read().await;
            let target_node = state.voices[&target].active_nodes[0];
            let (link_node, _) =
                state.param_triggers[&(ParamRouteTarget::Voice(target), "gate".to_string())];
            (target_node, link_node)
        };

        let mut target_deleted = ScriptState::new();
        target_deleted.add_group(group, GroupConfig::default());
        target_deleted.add_voice(src, VoiceConfig::new("src", "delete_trigger_source", group));
        runtime.apply_reload(target_deleted).await.unwrap();

        let state = runtime.state.read().await;
        assert!(!state.voices.contains_key(&target));
        assert!(state.param_routes_trigger.is_empty());
        assert!(state.param_triggers.is_empty());
        drop(state);

        assert!(runtime
            .backend
            .free_node_log
            .lock()
            .unwrap()
            .contains(&link_node));
        assert!(!runtime.backend.maps().contains(&RecordedMap {
            node: target_node,
            param: "gate".to_string(),
            bus: u32::MAX,
        }));
    }

    #[tokio::test]
    async fn apply_reload_structural_recreate_rebinds_unchanged_set_route() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        register_voice_synthdef(
            &runtime,
            "recreate_set_source",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "recreate_set_target",
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group_a = GroupId::new(1);
        let group_b = GroupId::new(2);
        let src = VoiceId::new(170);
        let target = VoiceId::new(180);

        let mut initial = ScriptState::new();
        initial.add_group(group_a, GroupConfig::default());
        initial.add_group(group_b, GroupConfig::default());
        initial.add_voice(src, VoiceConfig::new("src", "recreate_set_source", group_a));
        initial.add_voice(
            target,
            VoiceConfig::new("target", "recreate_set_target", group_a),
        );
        initial.running_voices.insert(target);
        initial
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "out",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        runtime.apply_reload(initial).await.unwrap();

        let old_summer_node = {
            let state = runtime.state.read().await;
            state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
                .expect("SET route should create a summer")
                .node
        };

        let mut recreated = ScriptState::new();
        recreated.add_group(group_a, GroupConfig::default());
        recreated.add_group(group_b, GroupConfig::default());
        recreated.add_voice(src, VoiceConfig::new("src", "recreate_set_source", group_b));
        recreated.add_voice(
            target,
            VoiceConfig::new("target", "recreate_set_target", group_b),
        );
        recreated.running_voices.insert(target);
        recreated
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "out",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        runtime.apply_reload(recreated).await.unwrap();

        let state = runtime.state.read().await;
        let target_node = state.voices[&target].active_nodes[0];
        let summer_bus = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
            .expect("unchanged SET route should be rebound after recreate")
            .bus
            .raw();
        drop(state);

        assert!(runtime
            .backend
            .free_node_log
            .lock()
            .unwrap()
            .contains(&old_summer_node));
        assert_eq!(
            runtime
                .backend
                .synths()
                .iter()
                .filter(|s| s.def == "param_kr_modulate_1")
                .count(),
            2,
            "hard recreate should respawn the unchanged SET route summer"
        );
        assert!(runtime.backend.maps().contains(&RecordedMap {
            node: target_node,
            param: "cutoff".to_string(),
            bus: summer_bus,
        }));
    }

    #[tokio::test]
    async fn apply_reload_output_port_flip_ar_to_kr_frees_audio_route_adapter_and_respawns_param_route(
    ) {
        use crate::handlers::{ParamRouteTarget, RouteDest};
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        let source_synth = "flip_ar_to_kr_source";
        let target_synth = "flip_ar_to_kr_target";
        register_voice_synthdef(
            &runtime,
            source_synth,
            vec![
                OutputPort {
                    name: "env".to_string(),
                    channels: 1,
                    rate: PortRate::Ar,
                },
                OutputPort {
                    name: "out".to_string(),
                    channels: 2,
                    rate: PortRate::Ar,
                },
            ],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            target_synth,
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(10);
        let target = VoiceId::new(20);

        let mut old_state = ScriptState::new();
        old_state.add_group(group, GroupConfig::default());
        old_state.add_voice(src, VoiceConfig::new("src", source_synth, group));
        old_state.add_voice(target, VoiceConfig::new("target", target_synth, group));
        old_state.running_voices.insert(target);
        old_state.set_route(src, "env", RouteDest::Group(group));
        old_state
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "env",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        runtime.apply_reload(old_state.clone()).await.unwrap();

        let (old_env_bus, old_route_node, old_adapter_node, old_summer_node) = {
            let state = runtime.state.read().await;
            let old_env_bus = state.voices[&src]
                .output_buses
                .iter()
                .find(|(name, _)| name == "env")
                .map(|(_, bus)| *bus)
                .unwrap();
            let old_route_node =
                state.route_synths[&(src, "env".to_string(), RouteDest::Group(group))];
            let (old_adapter_node, _) = state.ar_to_kr_adapters[&(src, "env".to_string())];
            let old_summer_node = state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
                .unwrap()
                .node;
            (
                old_env_bus,
                old_route_node,
                old_adapter_node,
                old_summer_node,
            )
        };
        assert!(old_env_bus.raw() < 1000);

        vibelang_dsp::register_synthdef_outputs(
            source_synth.to_string(),
            vec![
                OutputPort {
                    name: "env".to_string(),
                    channels: 1,
                    rate: PortRate::Kr,
                },
                OutputPort {
                    name: "out".to_string(),
                    channels: 2,
                    rate: PortRate::Ar,
                },
            ],
        );

        let mut new_state = old_state;
        new_state.routes.remove(&(src, "env".to_string()));
        new_state
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "env",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        runtime.apply_reload(new_state).await.unwrap();

        let state = runtime.state.read().await;
        let new_env_bus = state.voices[&src]
            .output_buses
            .iter()
            .find(|(name, _)| name == "env")
            .map(|(_, bus)| *bus)
            .unwrap();
        assert!(new_env_bus.raw() >= 1000);
        assert!(!state.route_synths.contains_key(&(
            src,
            "env".to_string(),
            RouteDest::Group(group)
        )));
        assert!(!state
            .ar_to_kr_adapters
            .contains_key(&(src, "env".to_string())));
        assert!(state
            .param_routes_set
            .contains_key(&(src, "env".to_string())));
        let new_summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
            .unwrap();
        assert_eq!(new_summer.sources[0].bus, new_env_bus);
        let registered = state.synthdef_outputs(source_synth);
        assert_eq!(
            registered.iter().find(|p| p.name == "env").unwrap().rate,
            PortRate::Kr
        );
        drop(state);

        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(frees.contains(&old_route_node));
        assert!(frees.contains(&old_adapter_node));
        assert!(frees.contains(&old_summer_node));
        assert!(runtime
            .backend
            .alive_synths()
            .iter()
            .any(|(def, _)| def == "param_kr_modulate_1"));
    }

    #[tokio::test]
    async fn apply_reload_output_port_flip_kr_to_ar_frees_control_summer_and_materializes_audio() {
        use crate::handlers::{ParamRouteTarget, RouteDest};
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        let source_synth = "flip_kr_to_ar_source";
        let target_synth = "flip_kr_to_ar_target";
        register_voice_synthdef(
            &runtime,
            source_synth,
            vec![OutputPort {
                name: "env".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            target_synth,
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(30);
        let target = VoiceId::new(40);
        let mut old_state = ScriptState::new();
        old_state.add_group(group, GroupConfig::default());
        old_state.add_voice(src, VoiceConfig::new("src", source_synth, group));
        old_state.add_voice(target, VoiceConfig::new("target", target_synth, group));
        old_state.running_voices.insert(target);
        old_state
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "env",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        runtime.apply_reload(old_state.clone()).await.unwrap();

        let (old_env_bus, old_summer_node) = {
            let state = runtime.state.read().await;
            let old_env_bus = state.voices[&src]
                .output_buses
                .iter()
                .find(|(name, _)| name == "env")
                .map(|(_, bus)| *bus)
                .unwrap();
            let old_summer_node = state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
                .unwrap()
                .node;
            (old_env_bus, old_summer_node)
        };
        assert!(old_env_bus.raw() >= 1000);

        vibelang_dsp::register_synthdef_outputs(
            source_synth.to_string(),
            vec![OutputPort {
                name: "env".to_string(),
                channels: 1,
                rate: PortRate::Ar,
            }],
        );

        let mut new_state = old_state;
        new_state.set_route(src, "env", RouteDest::Group(group));
        new_state
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "env",
                ParamRouteTarget::Voice(target),
                "cutoff",
            )
            .unwrap();
        runtime.apply_reload(new_state).await.unwrap();

        let state = runtime.state.read().await;
        let new_env_bus = state.voices[&src]
            .output_buses
            .iter()
            .find(|(name, _)| name == "env")
            .map(|(_, bus)| *bus)
            .unwrap();
        assert!(new_env_bus.raw() < 1000);
        assert!(state.route_synths.contains_key(&(
            src,
            "env".to_string(),
            RouteDest::Group(group)
        )));
        assert!(state
            .ar_to_kr_adapters
            .contains_key(&(src, "env".to_string())));
        assert!(state
            .param_routes_set
            .contains_key(&(src, "env".to_string())));
        let new_summer = state
            .param_summers
            .get(&(ParamRouteTarget::Voice(target), "cutoff".to_string()))
            .unwrap();
        assert_eq!(new_summer.sources.len(), 1);
        assert_eq!(
            state
                .synthdef_outputs(source_synth)
                .iter()
                .find(|p| p.name == "env")
                .unwrap()
                .rate,
            PortRate::Ar
        );
        drop(state);

        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(frees.contains(&old_summer_node));
        assert!(runtime
            .backend
            .alive_synths()
            .iter()
            .any(|(def, _)| def == "a2k_adapter_1"));
        assert!(runtime
            .backend
            .alive_synths()
            .iter()
            .any(|(def, _)| def.starts_with("port_to_group_link_")));
    }

    #[tokio::test]
    async fn apply_reload_output_port_flip_kr_to_tr_tears_down_summer_and_spawns_trigger_link() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        let source_synth = "flip_kr_to_tr_source";
        let target_synth = "flip_kr_to_tr_target";
        register_voice_synthdef(
            &runtime,
            source_synth,
            vec![OutputPort {
                name: "env".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            target_synth,
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(50);
        let target = VoiceId::new(60);
        let mut old_state = ScriptState::new();
        old_state.add_group(group, GroupConfig::default());
        old_state.add_voice(src, VoiceConfig::new("src", source_synth, group));
        old_state.add_voice(target, VoiceConfig::new("target", target_synth, group));
        old_state.running_voices.insert(target);
        old_state
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "env",
                ParamRouteTarget::Voice(target),
                "gate",
            )
            .unwrap();
        runtime.apply_reload(old_state.clone()).await.unwrap();

        let old_summer_node = {
            let state = runtime.state.read().await;
            state
                .param_summers
                .get(&(ParamRouteTarget::Voice(target), "gate".to_string()))
                .unwrap()
                .node
        };

        vibelang_dsp::register_synthdef_outputs(
            source_synth.to_string(),
            vec![OutputPort {
                name: "env".to_string(),
                channels: 1,
                rate: PortRate::Tr,
            }],
        );

        let mut new_state = old_state;
        new_state.param_routes_set.remove(&(src, "env".to_string()));
        new_state
            .add_param_route(
                ParamRouteKind::Trigger,
                src,
                "env",
                ParamRouteTarget::Voice(target),
                "gate",
            )
            .unwrap();
        runtime.apply_reload(new_state).await.unwrap();

        let state = runtime.state.read().await;
        assert!(!state
            .param_routes_set
            .contains_key(&(src, "env".to_string())));
        assert!(state
            .param_routes_trigger
            .contains_key(&(src, "env".to_string())));
        assert!(!state
            .param_summers
            .contains_key(&(ParamRouteTarget::Voice(target), "gate".to_string())));
        assert!(state
            .param_triggers
            .contains_key(&(ParamRouteTarget::Voice(target), "gate".to_string())));
        assert_eq!(
            state
                .synthdef_outputs(source_synth)
                .iter()
                .find(|p| p.name == "env")
                .unwrap()
                .rate,
            PortRate::Tr
        );
        drop(state);

        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(frees.contains(&old_summer_node));
        assert!(runtime
            .backend
            .alive_synths()
            .iter()
            .any(|(def, _)| def == "port_tr_to_param_link_1"));
    }

    #[tokio::test]
    async fn apply_reload_output_port_flip_tr_to_kr_tears_down_trigger_link_and_spawns_summer() {
        use crate::handlers::ParamRouteTarget;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};
        use vibelang_dsp::{OutputPort, PortRate};

        let mut runtime = Runtime::new(RecordingBackend::new());
        let source_synth = "flip_tr_to_kr_source";
        let target_synth = "flip_tr_to_kr_target";
        register_voice_synthdef(
            &runtime,
            source_synth,
            vec![OutputPort {
                name: "env".to_string(),
                channels: 1,
                rate: PortRate::Tr,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            target_synth,
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: PortRate::Ar,
            }],
        )
        .await;

        let group = GroupId::new(1);
        let src = VoiceId::new(70);
        let target = VoiceId::new(80);
        let mut old_state = ScriptState::new();
        old_state.add_group(group, GroupConfig::default());
        old_state.add_voice(src, VoiceConfig::new("src", source_synth, group));
        old_state.add_voice(target, VoiceConfig::new("target", target_synth, group));
        old_state.running_voices.insert(target);
        old_state
            .add_param_route(
                ParamRouteKind::Trigger,
                src,
                "env",
                ParamRouteTarget::Voice(target),
                "gate",
            )
            .unwrap();
        runtime.apply_reload(old_state.clone()).await.unwrap();

        let old_trigger_node = {
            let state = runtime.state.read().await;
            state
                .param_triggers
                .get(&(ParamRouteTarget::Voice(target), "gate".to_string()))
                .unwrap()
                .0
        };

        vibelang_dsp::register_synthdef_outputs(
            source_synth.to_string(),
            vec![OutputPort {
                name: "env".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            }],
        );

        let mut new_state = old_state;
        new_state
            .param_routes_trigger
            .remove(&(src, "env".to_string()));
        new_state
            .add_param_route(
                ParamRouteKind::Set,
                src,
                "env",
                ParamRouteTarget::Voice(target),
                "gate",
            )
            .unwrap();
        runtime.apply_reload(new_state).await.unwrap();

        let state = runtime.state.read().await;
        assert!(!state
            .param_routes_trigger
            .contains_key(&(src, "env".to_string())));
        assert!(state
            .param_routes_set
            .contains_key(&(src, "env".to_string())));
        assert!(!state
            .param_triggers
            .contains_key(&(ParamRouteTarget::Voice(target), "gate".to_string())));
        assert!(state
            .param_summers
            .contains_key(&(ParamRouteTarget::Voice(target), "gate".to_string())));
        assert_eq!(
            state
                .synthdef_outputs(source_synth)
                .iter()
                .find(|p| p.name == "env")
                .unwrap()
                .rate,
            PortRate::Kr
        );
        drop(state);

        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(frees.contains(&old_trigger_node));
        assert!(runtime
            .backend
            .alive_synths()
            .iter()
            .any(|(def, _)| def == "param_kr_modulate_1"));
    }

    fn add_body_contribution(
        state: &mut reload::ScriptState,
        target_group: GroupId,
        target_path: &str,
        ordinal: u64,
        source: &str,
    ) {
        state.body_contributions.push(reload::BodyContribution {
            id: ordinal,
            target_group,
            target_path: target_path.to_string(),
            ordinal,
            source: Some(source.to_string()),
            include_stack: vec![source.to_string()],
            line: None,
            column: None,
        });
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
        new_state.add_voice(voice_id, VoiceConfig::new("v", synthdef_name, group_id));
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
        // output-route finalization before effect creation guarantees.
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
                port_buses
                    .values()
                    .any(|&b| (b - in_bus).abs() < f32::EPSILON),
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

        // The link synth runs last and reads from the group bus.
        let link_idx = log
            .iter()
            .position(|r| r.def == "system_link_audio")
            .expect("link synth was created by group finalization");
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
        new_state.add_voice(voice_id, VoiceConfig::new("v", synthdef_name, main_id));
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

    #[tokio::test]
    async fn reload_running_voice_synthdef_change_frees_old_node() {
        use crate::message::ReloadMessage;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);

        register_voice_synthdef(
            &runtime,
            "line_in_stereo",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;
        register_voice_synthdef(
            &runtime,
            "line_in",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;

        let group_id = GroupId::new(1);
        let voice_id = VoiceId::new(1);

        let mut initial = ScriptState::new();
        initial.add_group(group_id, GroupConfig::default());
        initial.add_voice(
            voice_id,
            VoiceConfig::new("drums_in", "line_in_stereo", group_id),
        );
        initial.running_voices.insert(voice_id);
        runtime
            .send(ReloadMessage::Apply { state: initial }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let first_line_node = runtime
            .backend
            .alive_synths()
            .into_iter()
            .find_map(|(def, node)| (def == "line_in_stereo").then_some(node))
            .expect("initial running line_in_stereo node exists");

        let mut replacement = ScriptState::new();
        replacement.add_group(group_id, GroupConfig::default());
        replacement.add_voice(voice_id, VoiceConfig::new("drums_in", "line_in", group_id));
        replacement.running_voices.insert(voice_id);
        runtime
            .send(ReloadMessage::Apply { state: replacement }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(
            frees.contains(&first_line_node),
            "old running line_in_stereo node {:?} must be freed on synthdef reload; frees: {:?}",
            first_line_node,
            frees
        );

        let alive_line_nodes: Vec<_> = runtime
            .backend
            .alive_synths()
            .into_iter()
            .filter(|(def, _)| def == "line_in" || def == "line_in_stereo")
            .collect();
        assert!(
            matches!(alive_line_nodes.as_slice(), [(def, _)] if def == "line_in"),
            "only the replacement running voice should remain alive"
        );
    }

    #[tokio::test]
    async fn reload_merged_group_body_changes_keep_single_group_and_link() {
        use crate::message::ReloadMessage;
        use crate::reload::{EffectConfig, GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{EffectId, GroupId, VoiceId};

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);

        for synthdef in ["kick_synth", "snare_synth", "hat_synth"] {
            register_voice_synthdef(
                &runtime,
                synthdef,
                vec![vibelang_dsp::OutputPort {
                    name: "out".to_string(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                }],
            )
            .await;
        }
        for synthdef in ["body_a_fx", "body_b_fx", "body_c_fx"] {
            register_effect_synthdef(&runtime, synthdef).await;
        }

        let group_id = GroupId::new(10);
        let kick_id = VoiceId::new(1);
        let snare_id = VoiceId::new(2);
        let hat_id = VoiceId::new(3);
        let body_a_effect = EffectId::new(1);
        let body_b_effect = EffectId::new(2);
        let body_c_effect = EffectId::new(3);

        let mut initial = ScriptState::new();
        initial.add_group(
            group_id,
            GroupConfig {
                name: "Drums".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(&mut initial, group_id, "main/Drums", 0, "main.vibe");
        initial.add_voice(kick_id, VoiceConfig::new("kick", "kick_synth", group_id));
        initial.add_effect(
            body_a_effect,
            EffectConfig {
                group: group_id,
                synthdef: "body_a_fx".to_string(),
                params: ParamMap::new(),
            },
        );
        initial
            .groups
            .get_mut(&group_id)
            .unwrap()
            .effects
            .push(body_a_effect);
        add_body_contribution(&mut initial, group_id, "main/Drums", 1, "fills.vibe");
        initial.add_voice(snare_id, VoiceConfig::new("snare", "snare_synth", group_id));
        initial.add_effect(
            body_b_effect,
            EffectConfig {
                group: group_id,
                synthdef: "body_b_fx".to_string(),
                params: ParamMap::new(),
            },
        );
        initial
            .groups
            .get_mut(&group_id)
            .unwrap()
            .effects
            .push(body_b_effect);
        initial.running_voices.insert(kick_id);
        initial.running_voices.insert(snare_id);

        runtime
            .send(ReloadMessage::Apply { state: initial }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let (initial_group_node, initial_link_node) = {
            let state = runtime.state.read().await;
            let group = state.groups.get(&group_id).expect("merged group exists");
            (
                group.node_id,
                group
                    .link_synth_node_id
                    .expect("merged group has one link synth"),
            )
        };

        let initial_snare_node = runtime
            .backend
            .alive_synths()
            .into_iter()
            .find_map(|(def, node)| (def == "snare_synth").then_some(node))
            .expect("second body running voice is alive");

        let initial_synths = runtime.backend.synths();
        let body_a_fx_idx = initial_synths
            .iter()
            .position(|synth| synth.def == "body_a_fx")
            .expect("first body effect was created");
        let body_b_fx_idx = initial_synths
            .iter()
            .position(|synth| synth.def == "body_b_fx")
            .expect("second body effect was created");
        assert!(
            body_a_fx_idx < body_b_fx_idx,
            "merged-body effects should be created in ScriptState.effect_order"
        );

        let mut reloaded = ScriptState::new();
        reloaded.add_group(
            group_id,
            GroupConfig {
                name: "Drums".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(&mut reloaded, group_id, "main/Drums", 0, "main.vibe");
        reloaded.add_voice(kick_id, VoiceConfig::new("kick", "kick_synth", group_id));
        reloaded.add_effect(
            body_a_effect,
            EffectConfig {
                group: group_id,
                synthdef: "body_a_fx".to_string(),
                params: ParamMap::new(),
            },
        );
        reloaded
            .groups
            .get_mut(&group_id)
            .unwrap()
            .effects
            .push(body_a_effect);
        add_body_contribution(&mut reloaded, group_id, "main/Drums", 1, "fills.vibe");
        reloaded.add_voice(hat_id, VoiceConfig::new("hat", "hat_synth", group_id));
        reloaded.add_effect(
            body_c_effect,
            EffectConfig {
                group: group_id,
                synthdef: "body_c_fx".to_string(),
                params: ParamMap::new(),
            },
        );
        reloaded
            .groups
            .get_mut(&group_id)
            .unwrap()
            .effects
            .push(body_c_effect);
        reloaded.running_voices.insert(kick_id);
        reloaded.running_voices.insert(hat_id);

        runtime
            .send(ReloadMessage::Apply { state: reloaded }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let state = runtime.state.read().await;
        assert_eq!(
            state.groups.len(),
            1,
            "merged bodies keep one group identity"
        );
        let group = state.groups.get(&group_id).expect("merged group remains");
        assert_eq!(
            group.node_id, initial_group_node,
            "changing one body must not recreate the shared group"
        );
        assert_eq!(
            group.link_synth_node_id,
            Some(initial_link_node),
            "unchanged merged group must keep its single link synth"
        );
        drop(state);

        assert_eq!(
            runtime.backend.created_groups(),
            vec![initial_group_node],
            "reload must not spawn a duplicate group node for the second body"
        );
        assert_eq!(
            runtime.backend.alive_link_synths().len(),
            1,
            "only one group link synth should be alive after merged-body reload"
        );

        let alive_defs = runtime
            .backend
            .alive_synths()
            .into_iter()
            .map(|(def, _)| def)
            .collect::<Vec<_>>();
        assert!(
            alive_defs.iter().any(|def| def == "kick_synth"),
            "unchanged first body voice remains alive"
        );
        assert!(
            alive_defs.iter().any(|def| def == "hat_synth"),
            "new second body voice is alive"
        );
        assert!(
            !alive_defs.iter().any(|def| def == "snare_synth"),
            "removed second body voice must not stay alive"
        );

        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(
            frees.contains(&initial_snare_node),
            "removed body voice node {:?} must be freed on reload; frees: {:?}",
            initial_snare_node,
            frees
        );
    }

    #[tokio::test]
    async fn reload_repeated_saved_handle_bodies_inside_current_group_keep_one_runtime_group() {
        use crate::message::ReloadMessage;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);

        for synthdef in ["kick_synth", "snare_synth", "pad_synth"] {
            register_voice_synthdef(
                &runtime,
                synthdef,
                vec![vibelang_dsp::OutputPort {
                    name: "out".to_string(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                }],
            )
            .await;
        }

        let drums_id = GroupId::new(60);
        let song_id = GroupId::new(61);
        let kick_id = VoiceId::new(60);
        let snare_id = VoiceId::new(61);
        let pad_id = VoiceId::new(62);

        let mut state = ScriptState::new();
        state.add_group(
            drums_id,
            GroupConfig {
                name: "Drums".to_string(),
                ..GroupConfig::default()
            },
        );
        state.add_group(
            song_id,
            GroupConfig {
                name: "Song".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(&mut state, song_id, "main/Song", 0, "song.vibe");
        add_body_contribution(&mut state, drums_id, "main/Drums", 1, "song.vibe");
        state.add_voice(kick_id, VoiceConfig::new("kick", "kick_synth", drums_id));
        add_body_contribution(&mut state, drums_id, "main/Drums", 2, "song.vibe");
        state.add_voice(snare_id, VoiceConfig::new("snare", "snare_synth", drums_id));
        state.add_voice(pad_id, VoiceConfig::new("pad", "pad_synth", song_id));
        state.running_voices.insert(kick_id);
        state.running_voices.insert(snare_id);
        state.running_voices.insert(pad_id);

        runtime
            .send(ReloadMessage::Apply { state }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let state = runtime.state.read().await;
        assert_eq!(
            state.groups.len(),
            2,
            "saved Drums handle bodies inside Song must not create a sibling or child Drums group"
        );
        let drums = state.groups.get(&drums_id).expect("Drums group exists");
        let song = state.groups.get(&song_id).expect("Song group exists");
        assert_eq!(drums.name, "Drums");
        assert_eq!(song.name, "Song");
        assert_eq!(
            drums.parent, None,
            "Drums must remain anchored at main/Drums, not main/Song/Drums"
        );
        assert_eq!(song.parent, None);
        assert_eq!(state.voices.get(&kick_id).unwrap().config.group, drums_id);
        assert_eq!(state.voices.get(&snare_id).unwrap().config.group, drums_id);
        assert_eq!(state.voices.get(&pad_id).unwrap().config.group, song_id);
        assert!(drums.link_synth_node_id.is_some());
        assert!(song.link_synth_node_id.is_some());
        drop(state);

        assert_eq!(
            runtime.backend.created_groups().len(),
            2,
            "runtime should allocate one group node per logical group"
        );
        assert_eq!(
            runtime.backend.alive_link_synths().len(),
            2,
            "runtime should keep one group link synth per logical group"
        );

        let alive_route_links = runtime
            .backend
            .alive_synths()
            .into_iter()
            .filter(|(def, _)| def.starts_with("port_to_group_link_"))
            .count();
        assert_eq!(
            alive_route_links, 3,
            "each running voice should have exactly one default route into its resolved group"
        );
    }

    #[tokio::test]
    async fn reload_removing_effect_body_keeps_voice_body() {
        use crate::message::ReloadMessage;
        use crate::reload::{EffectConfig, GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{EffectId, GroupId, VoiceId};

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);

        register_voice_synthdef(
            &runtime,
            "kick_synth",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;
        register_effect_synthdef(&runtime, "body_fx").await;

        let group_id = GroupId::new(30);
        let voice_id = VoiceId::new(30);
        let effect_id = EffectId::new(30);

        let mut initial = ScriptState::new();
        initial.add_group(
            group_id,
            GroupConfig {
                name: "Drums".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(&mut initial, group_id, "main/Drums", 0, "voices.vibe");
        initial.add_voice(voice_id, VoiceConfig::new("kick", "kick_synth", group_id));
        initial.running_voices.insert(voice_id);
        add_body_contribution(&mut initial, group_id, "main/Drums", 1, "effects.vibe");
        initial.add_effect(
            effect_id,
            EffectConfig {
                group: group_id,
                synthdef: "body_fx".to_string(),
                params: ParamMap::new(),
            },
        );
        initial
            .groups
            .get_mut(&group_id)
            .unwrap()
            .effects
            .push(effect_id);

        runtime
            .send(ReloadMessage::Apply { state: initial }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let (initial_group_node, initial_link_node) = {
            let state = runtime.state.read().await;
            let group = state.groups.get(&group_id).expect("merged group exists");
            (
                group.node_id,
                group.link_synth_node_id.expect("group has link synth"),
            )
        };
        let initial_effect_node = runtime
            .backend
            .alive_synths()
            .into_iter()
            .find_map(|(def, node)| (def == "body_fx").then_some(node))
            .expect("effect body node is alive before removal");

        let mut reloaded = ScriptState::new();
        reloaded.add_group(
            group_id,
            GroupConfig {
                name: "Drums".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(&mut reloaded, group_id, "main/Drums", 0, "voices.vibe");
        reloaded.add_voice(voice_id, VoiceConfig::new("kick", "kick_synth", group_id));
        reloaded.running_voices.insert(voice_id);

        runtime
            .send(ReloadMessage::Apply { state: reloaded }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let state = runtime.state.read().await;
        let group = state
            .groups
            .get(&group_id)
            .expect("voice body group remains");
        assert_eq!(
            group.node_id, initial_group_node,
            "removing an effects body must not recreate the shared group"
        );
        assert_eq!(
            group.link_synth_node_id,
            Some(initial_link_node),
            "remaining voice body keeps the existing group link"
        );
        assert!(
            state.effects.get(&effect_id).is_none(),
            "removed effects body should remove its effect config"
        );
        assert!(
            state.voices.get(&voice_id).is_some(),
            "voice body should survive effect-body removal"
        );
        drop(state);

        assert!(
            runtime
                .backend
                .alive_synths()
                .into_iter()
                .any(|(def, _)| def == "kick_synth"),
            "remaining voice body keeps its running synth"
        );

        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        runtime.tick().await;
        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(
            frees.contains(&initial_effect_node),
            "removed effect node {:?} should be freed after its grace period; frees: {:?}",
            initial_effect_node,
            frees
        );
    }

    #[tokio::test]
    async fn reload_removing_voice_body_keeps_effect_body() {
        use crate::message::ReloadMessage;
        use crate::reload::{EffectConfig, GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{EffectId, GroupId, VoiceId};

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);

        register_voice_synthdef(
            &runtime,
            "snare_synth",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;
        register_effect_synthdef(&runtime, "shared_reverb").await;

        let group_id = GroupId::new(40);
        let voice_id = VoiceId::new(40);
        let effect_id = EffectId::new(40);

        let mut initial = ScriptState::new();
        initial.add_group(
            group_id,
            GroupConfig {
                name: "Drums".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(&mut initial, group_id, "main/Drums", 0, "voices.vibe");
        initial.add_voice(voice_id, VoiceConfig::new("snare", "snare_synth", group_id));
        initial.running_voices.insert(voice_id);
        add_body_contribution(&mut initial, group_id, "main/Drums", 1, "effects.vibe");
        initial.add_effect(
            effect_id,
            EffectConfig {
                group: group_id,
                synthdef: "shared_reverb".to_string(),
                params: ParamMap::new(),
            },
        );
        initial
            .groups
            .get_mut(&group_id)
            .unwrap()
            .effects
            .push(effect_id);

        runtime
            .send(ReloadMessage::Apply { state: initial }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let (initial_group_node, initial_link_node) = {
            let state = runtime.state.read().await;
            let group = state.groups.get(&group_id).expect("merged group exists");
            (
                group.node_id,
                group.link_synth_node_id.expect("group has link synth"),
            )
        };
        let initial_voice_node = runtime
            .backend
            .alive_synths()
            .into_iter()
            .find_map(|(def, node)| (def == "snare_synth").then_some(node))
            .expect("voice body node is alive before removal");
        let initial_effect_node = runtime
            .backend
            .alive_synths()
            .into_iter()
            .find_map(|(def, node)| (def == "shared_reverb").then_some(node))
            .expect("effect body node is alive before voice removal");

        let mut reloaded = ScriptState::new();
        reloaded.add_group(
            group_id,
            GroupConfig {
                name: "Drums".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(&mut reloaded, group_id, "main/Drums", 0, "effects.vibe");
        reloaded.add_effect(
            effect_id,
            EffectConfig {
                group: group_id,
                synthdef: "shared_reverb".to_string(),
                params: ParamMap::new(),
            },
        );
        reloaded
            .groups
            .get_mut(&group_id)
            .unwrap()
            .effects
            .push(effect_id);

        runtime
            .send(ReloadMessage::Apply { state: reloaded }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let state = runtime.state.read().await;
        let group = state
            .groups
            .get(&group_id)
            .expect("effect body group remains");
        assert_eq!(
            group.node_id, initial_group_node,
            "removing a voice body must not recreate the shared group"
        );
        assert_eq!(
            group.link_synth_node_id,
            Some(initial_link_node),
            "remaining effect body keeps the existing group link"
        );
        let effect = state
            .effects
            .get(&effect_id)
            .expect("effect body should survive voice-body removal");
        assert_eq!(
            effect.node_id, initial_effect_node,
            "unchanged effect body should not be respawned"
        );
        assert!(
            state.voices.get(&voice_id).is_none(),
            "removed voice body should delete its voice config"
        );
        drop(state);

        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(
            frees.contains(&initial_voice_node),
            "removed voice node {:?} should be freed; frees: {:?}",
            initial_voice_node,
            frees
        );
        assert!(
            runtime
                .backend
                .alive_synths()
                .into_iter()
                .any(|(def, node)| def == "shared_reverb" && node == initial_effect_node),
            "remaining effect body stays alive"
        );
    }

    #[tokio::test]
    async fn reload_removing_last_body_tears_down_group() {
        use crate::message::ReloadMessage;
        use crate::reload::{EffectConfig, GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{EffectId, GroupId, VoiceId};

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);

        register_voice_synthdef(
            &runtime,
            "line_in",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;
        register_effect_synthdef(&runtime, "last_fx").await;

        let group_id = GroupId::new(50);
        let voice_id = VoiceId::new(50);
        let effect_id = EffectId::new(50);

        let mut initial = ScriptState::new();
        initial.add_group(
            group_id,
            GroupConfig {
                name: "Drums".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(&mut initial, group_id, "main/Drums", 0, "main.vibe");
        initial.add_voice(voice_id, VoiceConfig::new("line", "line_in", group_id));
        initial.running_voices.insert(voice_id);
        initial.add_effect(
            effect_id,
            EffectConfig {
                group: group_id,
                synthdef: "last_fx".to_string(),
                params: ParamMap::new(),
            },
        );
        initial
            .groups
            .get_mut(&group_id)
            .unwrap()
            .effects
            .push(effect_id);

        runtime
            .send(ReloadMessage::Apply { state: initial }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let (group_node, link_node) = {
            let state = runtime.state.read().await;
            let group = state.groups.get(&group_id).expect("group exists");
            (
                group.node_id,
                group.link_synth_node_id.expect("group has link synth"),
            )
        };
        let voice_node = runtime
            .backend
            .alive_synths()
            .into_iter()
            .find_map(|(def, node)| (def == "line_in").then_some(node))
            .expect("voice node is alive before final body removal");
        let effect_node = runtime
            .backend
            .alive_synths()
            .into_iter()
            .find_map(|(def, node)| (def == "last_fx").then_some(node))
            .expect("effect node is alive before final body removal");

        runtime
            .send(
                ReloadMessage::Apply {
                    state: ScriptState::new(),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        let state = runtime.state.read().await;
        assert!(state.groups.get(&group_id).is_none());
        assert!(state.voices.get(&voice_id).is_none());
        assert!(state.effects.get(&effect_id).is_none());
        drop(state);

        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        runtime.tick().await;
        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        for node in [voice_node, effect_node, link_node, group_node] {
            assert!(
                frees.contains(&node),
                "removing the last body should free node {:?}; frees: {:?}",
                node,
                frees
            );
        }
    }

    #[tokio::test]
    async fn reload_group_rename_frees_old_group_link_and_running_voice_nodes() {
        use crate::message::ReloadMessage;
        use crate::reload::{GroupConfig, ScriptState};
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, VoiceId};

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);

        register_voice_synthdef(
            &runtime,
            "line_in",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;

        let old_group_id = GroupId::new(20);
        let new_group_id = GroupId::new(21);
        let old_voice_id = VoiceId::new(20);
        let new_voice_id = VoiceId::new(21);

        let mut initial = ScriptState::new();
        initial.add_group(
            old_group_id,
            GroupConfig {
                name: "Drums".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(&mut initial, old_group_id, "main/Drums", 0, "main.vibe");
        initial.add_voice(
            old_voice_id,
            VoiceConfig::new("drums_in", "line_in", old_group_id),
        );
        initial.running_voices.insert(old_voice_id);

        runtime
            .send(ReloadMessage::Apply { state: initial }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let (old_group_node, old_link_node) = {
            let state = runtime.state.read().await;
            let group = state.groups.get(&old_group_id).expect("old group exists");
            (
                group.node_id,
                group.link_synth_node_id.expect("old group has link synth"),
            )
        };
        let old_voice_node = runtime
            .backend
            .alive_synths()
            .into_iter()
            .find_map(|(def, node)| (def == "line_in").then_some(node))
            .expect("old running voice exists");

        let mut renamed = ScriptState::new();
        renamed.add_group(
            new_group_id,
            GroupConfig {
                name: "DrumsRenamed".to_string(),
                ..GroupConfig::default()
            },
        );
        add_body_contribution(
            &mut renamed,
            new_group_id,
            "main/DrumsRenamed",
            0,
            "main.vibe",
        );
        renamed.add_voice(
            new_voice_id,
            VoiceConfig::new("drums_in", "line_in", new_group_id),
        );
        renamed.running_voices.insert(new_voice_id);

        runtime
            .send(ReloadMessage::Apply { state: renamed }.into())
            .await
            .unwrap();
        runtime.tick().await;

        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        for node in [old_voice_node, old_link_node, old_group_node] {
            assert!(
                frees.contains(&node),
                "group rename reload must free stale node {:?}; frees: {:?}",
                node,
                frees
            );
        }

        let alive_links = runtime.backend.alive_link_synths();
        assert_eq!(
            alive_links.len(),
            1,
            "only the renamed group's link synth should survive"
        );

        let alive_line_nodes: Vec<_> = runtime
            .backend
            .alive_synths()
            .into_iter()
            .filter(|(def, _)| def == "line_in")
            .collect();
        assert_eq!(
            alive_line_nodes.len(),
            1,
            "only the renamed group's running line_in should survive"
        );

        let state = runtime.state.read().await;
        assert!(!state.groups.contains_key(&old_group_id));
        assert!(state.groups.contains_key(&new_group_id));
        assert!(!state.voices.contains_key(&old_voice_id));
        assert!(state.voices.contains_key(&new_voice_id));
    }

    // =========================================================================
    // Task D: reload reconciler handles `output_channels` delta
    //
    // These tests pin the four mono/stereo transitions exercised when a
    // hot-reload flips a group's hardware routing. The reload reconciler
    // (`apply_reload` group update) tears down the old link synth
    // on any (output_bus, output_channels) change; `groups.finalize`
    // then respawns the variant matching the new
    // channel count, routed at the new bus.
    //
    // After reload we assert: exactly one `system_link_audio*` synth is
    // alive, with the synthdef name and `outbus` matching the new
    // routing, and the live node id matches `GroupState.link_synth_node_id`.
    // =========================================================================

    fn build_routing_config(output_bus: u32, output_channels: u32) -> reload::GroupConfig {
        reload::GroupConfig {
            name: "g".to_string(),
            parent: None,
            params: ParamMap::new(),
            effects: Vec::new(),
            muted: false,
            soloed: false,
            output_bus: Some(output_bus),
            output_channels: Some(output_channels),
        }
    }

    fn expected_link_def(channels: u32) -> &'static str {
        match channels {
            1 => "system_link_audio_mono",
            _ => "system_link_audio",
        }
    }

    /// Drive a runtime through an initial reload (creating one routed group)
    /// then a second reload that flips its routing, and assert that exactly
    /// one link synth survives with the right variant and outbus.
    async fn assert_routing_reload_transition(
        initial_bus: u32,
        initial_channels: u32,
        new_bus: u32,
        new_channels: u32,
    ) {
        use crate::message::ReloadMessage;
        use crate::reload::ScriptState;
        use crate::types::GroupId;

        let backend = RecordingBackend::new();
        let mut runtime = Runtime::new(backend);
        let group_id = GroupId::new(1);

        // Initial reload — create the group with its starting routing.
        let mut s0 = ScriptState::new();
        s0.add_group(
            group_id,
            build_routing_config(initial_bus, initial_channels),
        );
        runtime
            .send(ReloadMessage::Apply { state: s0 }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // Sanity: initial finalize spawned the right starting variant.
        let initial_alive = runtime.backend.alive_link_synths();
        assert_eq!(
            initial_alive.len(),
            1,
            "initial finalize should spawn exactly one link synth, got {:?}",
            initial_alive
        );
        assert_eq!(
            initial_alive[0].0,
            expected_link_def(initial_channels),
            "initial variant must match starting output_channels"
        );
        assert_eq!(
            initial_alive[0].2,
            Some(initial_bus as f32),
            "initial outbus must match starting hardware bus"
        );
        let initial_link_node = initial_alive[0].1;

        // Second reload — flip routing on the same group.
        let mut s1 = ScriptState::new();
        s1.add_group(group_id, build_routing_config(new_bus, new_channels));
        runtime
            .send(ReloadMessage::Apply { state: s1 }.into())
            .await
            .unwrap();
        runtime.tick().await;

        // After reload: exactly one link synth alive, matching the new
        // routing. The old one has been freed (verified via the unified
        // event log inside `alive_link_synths`).
        let alive = runtime.backend.alive_link_synths();
        assert_eq!(
            alive.len(),
            1,
            "exactly one link synth alive after reload (transition {}ch@{} -> {}ch@{}); got {:?}",
            initial_channels,
            initial_bus,
            new_channels,
            new_bus,
            alive
        );
        let (def, node, outbus) = alive[0].clone();
        assert_eq!(
            def,
            expected_link_def(new_channels),
            "post-reload variant must match new output_channels"
        );
        assert_eq!(
            outbus,
            Some(new_bus as f32),
            "post-reload outbus must match new hardware bus"
        );

        // The old link node must have been freed at least once.
        let frees = runtime.backend.free_node_log.lock().unwrap().clone();
        assert!(
            frees.contains(&initial_link_node),
            "old link node {:?} must be freed during reload (frees: {:?})",
            initial_link_node,
            frees
        );

        // The live node id matches what's stored on GroupState.
        let state_link = {
            let s = runtime.state.read().await;
            s.groups
                .get(&group_id)
                .and_then(|g| g.link_synth_node_id)
                .expect("group has link_synth_node_id after reload")
        };
        assert_eq!(
            state_link, node,
            "GroupState.link_synth_node_id points at the alive link synth"
        );

        // GroupState reflects the new routing fields.
        let (state_bus, state_channels) = {
            let s = runtime.state.read().await;
            let g = s.groups.get(&group_id).unwrap();
            (g.output_bus, g.output_channels)
        };
        assert_eq!(state_bus, Some(new_bus));
        assert_eq!(state_channels, Some(new_channels));
    }

    #[tokio::test]
    async fn test_reload_stereo_bus_change() {
        // Regression: stereo [2,3] → stereo [4,5]. Same variant, new bus.
        assert_routing_reload_transition(2, 2, 4, 2).await;
    }

    #[tokio::test]
    async fn test_reload_stereo_to_mono_same_bus() {
        // Variant swap: stereo [2,3] → mono (2). Old `system_link_audio`
        // must be freed; only `system_link_audio_mono` survives at bus 2.
        assert_routing_reload_transition(2, 2, 2, 1).await;
    }

    #[tokio::test]
    async fn test_reload_mono_to_stereo_same_bus() {
        // Variant swap back: mono (2) → stereo [2,3]. Old
        // `system_link_audio_mono` must be freed; only `system_link_audio`
        // survives at bus 2.
        assert_routing_reload_transition(2, 1, 2, 2).await;
    }

    #[tokio::test]
    async fn test_reload_mono_bus_change() {
        // Bus change, same mono variant: (2) → (3). Even though the
        // variant is unchanged, the diff path still tears down + respawns
        // (collapses all four cases into one). Only one mono link
        // survives, at bus 3.
        assert_routing_reload_transition(2, 1, 3, 1).await;
    }

    // Regression: a poly(1) MIDI-output voice driven by a keyboard route must
    // forward its NoteOn through the runtime to the device's output sender —
    // i.e. the id `voice.config.midi_output` is registered as `note_on` ->
    // `send_midi_event_now` -> `get_midi_sender` look it up, not silently lost
    // to the (deploy-dead) synthdef fallback. This exercises one level above
    // the `handlers::voices` mock-sink unit tests: VoiceMessage::NoteOn (what a
    // keyboard route sends) -> Runtime dispatch -> VoicesHandler -> the sender
    // registered in the runtime's MIDI output-channel map.
    #[cfg(feature = "midi")]
    #[tokio::test]
    async fn midi_output_voice_note_on_reaches_device_sender() {
        use crate::message::{GroupMessage, VoiceMessage};
        use crate::midi::QueuedMidiEvent;
        use crate::traits::VoiceConfig;
        use crate::types::{GroupId, MidiDeviceId, VoiceId};

        let mut runtime = Runtime::new(MockBackend);

        // Register a fake output sender under the device id the voice will use.
        const DEV: u32 = 5;
        let (tx, rx) = crossbeam_channel::unbounded::<crate::midi::ScheduledMidiEvent>();
        runtime
            .midi
            .output_channels()
            .lock()
            .unwrap()
            .insert(MidiDeviceId::new(DEV), tx);

        // Group + a poly(1) MIDI-output voice on that device.
        runtime
            .send(
                GroupMessage::Create {
                    id: GroupId::new(1),
                    name: "g".to_string(),
                    parent: None,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        let vid = VoiceId::new(1);
        let cfg = VoiceConfig::new("lead", "", GroupId::new(1))
            .with_midi_output(MidiDeviceId::new(DEV), 0)
            .with_polyphony(1);
        runtime
            .send(
                VoiceMessage::Create {
                    id: vid,
                    config: Box::new(cfg),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        // Simulate the keyboard route firing (handlers::midi sends exactly this).
        runtime
            .send(
                VoiceMessage::NoteOn {
                    voice: vid,
                    note: 60,
                    velocity: 1.0,
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;

        let scheduled = rx
            .try_recv()
            .expect("poly(1) MIDI-output voice must forward its NoteOn to the device sender");
        match scheduled.event {
            QueuedMidiEvent::NoteOn {
                channel,
                note,
                velocity,
            } => {
                assert_eq!(channel, 0);
                assert_eq!(note, 60);
                assert_eq!(velocity, 127);
            }
            other => panic!("expected NoteOn, got {other:?}"),
        }
        assert!(
            rx.try_recv().is_err(),
            "a single keyboard note should produce exactly one device event"
        );
    }

    fn disable_test_midi_threads<B: Backend>(runtime: &mut Runtime<B>) {
        #[cfg(feature = "midi")]
        {
            runtime.clock_thread_started = true;
        }
    }

    #[tokio::test]
    async fn receipt_submission_returns_accepted_before_runtime_work_and_preserves_context() {
        let mut runtime = Runtime::new(MockBackend);
        disable_test_midi_threads(&mut runtime);
        let handle = runtime.handle();
        let replies = Arc::new(Mutex::new(Vec::<MutationReceipt>::new()));
        let events = Arc::new(Mutex::new(Vec::<crate::mutation::ReceiptEvent>::new()));
        let reply_capture = replies.clone();
        let event_capture = events.clone();
        let message = Message::Transport(TransportMessage::SetTempo { bpm: 137.0 });
        let submission = handle.legacy_submission(&message).unwrap();

        let accepted = handle
            .submit_with_sinks(
                message,
                submission,
                MutationReplySink::new(move |receipt| {
                    reply_capture.lock().unwrap().push(receipt);
                }),
                MutationEventSink::new(move |event| {
                    event_capture.lock().unwrap().push(event);
                }),
            )
            .await
            .unwrap();

        assert!(matches!(accepted.state, ReceiptState::Accepted { .. }));
        assert_eq!(
            handle.mutation_receipt(accepted.attempt_id).unwrap().state,
            accepted.state
        );
        runtime.tick().await;

        let replies = replies.lock().unwrap().clone();
        assert_eq!(replies.len(), 5);
        assert!(matches!(replies[0].state, ReceiptState::Evaluating { .. }));
        assert!(matches!(replies[1].state, ReceiptState::Accepted { .. }));
        assert!(matches!(replies[2].state, ReceiptState::Planning));
        assert!(matches!(replies[3].state, ReceiptState::Committing { .. }));
        assert!(matches!(
            replies[4].state,
            ReceiptState::Terminal(TerminalOutcome::Applied(_))
        ));
        assert!(replies
            .iter()
            .all(|receipt| receipt.attempt_id == accepted.attempt_id));
        assert!(replies[1..]
            .iter()
            .all(|receipt| receipt.revision == accepted.revision));
        let events = events.lock().unwrap().clone();
        assert_eq!(events.len(), replies.len());
        assert!(events
            .iter()
            .all(|event| event.receipt.attempt_id == accepted.attempt_id));
    }

    #[tokio::test]
    async fn queue_full_and_closed_are_distinct_and_never_allocate_failed_revision() {
        let runtime = Runtime::new_with_channel_capacity(MockBackend, 1);
        let handle = runtime.handle();
        let first = Message::Transport(TransportMessage::Start);
        let first_receipt = handle
            .try_submit(first.clone(), handle.legacy_submission(&first).unwrap())
            .unwrap();
        assert!(matches!(first_receipt.state, ReceiptState::Accepted { .. }));
        let accepted_through = handle.mutation_status().accepted_through;

        let full = Message::Transport(TransportMessage::Stop);
        assert!(matches!(
            handle.try_submit(full.clone(), handle.legacy_submission(&full).unwrap()),
            Err(Error::ChannelFull)
        ));
        assert_eq!(handle.mutation_status().accepted_through, accepted_through);

        let closed_runtime = Runtime::new_with_channel_capacity(MockBackend, 1);
        let closed_handle = closed_runtime.handle();
        drop(closed_runtime);
        let closed_replies = Arc::new(Mutex::new(Vec::<MutationReceipt>::new()));
        let reply_capture = closed_replies.clone();
        let closed = Message::Transport(TransportMessage::Start);
        let closed_submission = closed_handle.legacy_submission(&closed).unwrap();
        assert!(matches!(
            closed_handle
                .submit_with_sinks(
                    closed,
                    closed_submission,
                    MutationReplySink::new(move |receipt| {
                        reply_capture.lock().unwrap().push(receipt);
                    }),
                    MutationEventSink::default(),
                )
                .await,
            Err(Error::ChannelClosed)
        ));
        assert_eq!(closed_handle.mutation_status().accepted_through, None);
        let closed_replies = closed_replies.lock().unwrap();
        let ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) =
            &closed_replies.last().unwrap().state
        else {
            panic!("closed queue must publish a rejected receipt");
        };
        assert_eq!(rejected.code, "queue_closed");
    }

    #[cfg(feature = "midi")]
    #[test]
    fn internal_and_linked_messages_cannot_create_public_receipts() {
        let runtime = Runtime::new(MockBackend);
        let handle = runtime.handle();
        let internal = Message::Midi(MidiMessage::ReconcileInputs {
            present: std::collections::HashSet::new(),
        });
        assert!(matches!(
            handle.try_send(internal),
            Err(Error::InvalidConfig(_))
        ));
        let linked = Message::Reload(Box::new(ReloadMessage::ApplyStaged {
            state: reload::ScriptState::new(),
            assets: reload::StagedReloadAssets::default(),
        }));
        assert!(matches!(
            handle.try_send(linked),
            Err(Error::InvalidConfig(_))
        ));
        assert_eq!(handle.mutation_status().accepted_through, None);
    }

    #[tokio::test]
    async fn handler_failures_reject_pre_effect_or_fence_uncertain_backend_effects() {
        let mut pre_effect_runtime = Runtime::new(MockBackend);
        disable_test_midi_threads(&mut pre_effect_runtime);
        let pre_effect_handle = pre_effect_runtime.handle();
        let missing = Message::Group(GroupMessage::Delete {
            id: GroupId::new(404),
        });
        let missing_receipt = pre_effect_handle
            .submit(
                missing.clone(),
                pre_effect_handle.legacy_submission(&missing).unwrap(),
            )
            .await
            .unwrap();
        pre_effect_runtime.tick().await;
        let missing_receipt = pre_effect_handle
            .mutation_receipt(missing_receipt.attempt_id)
            .unwrap();
        let ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) = missing_receipt.state
        else {
            panic!("missing group must be rejected before effect");
        };
        assert_eq!(rejected.code, "group_not_found");
        assert!(matches!(
            pre_effect_handle.mutation_status().live_state,
            LiveState::Clean
        ));

        let mut runtime = Runtime::new(RecordingBackend::new());
        disable_test_midi_threads(&mut runtime);
        runtime
            .backend
            .fail_create_group
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let handle = runtime.handle();
        let create = Message::Group(GroupMessage::Create {
            id: GroupId::new(1),
            name: "faulted".into(),
            parent: None,
        });
        let accepted = handle
            .submit(create.clone(), handle.legacy_submission(&create).unwrap())
            .await
            .unwrap();
        runtime.tick().await;
        let partial = handle.mutation_receipt(accepted.attempt_id).unwrap();
        let ReceiptState::Terminal(TerminalOutcome::Partial(partial_outcome)) = &partial.state
        else {
            panic!("backend create failure must be partial");
        };
        assert!(partial_outcome.fenced);
        assert_eq!(partial_outcome.code, "backend_rejected");

        let blocked = Message::Transport(TransportMessage::Start);
        let blocked = handle
            .submit(blocked.clone(), handle.legacy_submission(&blocked).unwrap())
            .await
            .unwrap();
        let ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) = blocked.state else {
            panic!("mutation while fenced must be rejected");
        };
        assert_eq!(rejected.code, "runtime_fenced");
        let legacy_blocked = Message::Transport(TransportMessage::Stop);
        assert!(matches!(
            handle.try_send(legacy_blocked),
            Err(Error::RuntimeFenced(_))
        ));
        let legacy_blocked = Message::Transport(TransportMessage::Start);
        assert!(matches!(
            handle.send(legacy_blocked).await,
            Err(Error::RuntimeFenced(_))
        ));

        handle.continue_best_effort(partial.attempt_id).unwrap();
        runtime
            .backend
            .fail_create_group
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let resumed = Message::Transport(TransportMessage::SetTempo { bpm: 123.0 });
        let resumed = handle
            .submit(resumed.clone(), handle.legacy_submission(&resumed).unwrap())
            .await
            .unwrap();
        assert!(matches!(resumed.state, ReceiptState::Accepted { .. }));
        runtime.tick().await;
        assert!(matches!(
            handle.mutation_receipt(resumed.attempt_id).unwrap().state,
            ReceiptState::Terminal(TerminalOutcome::Applied(_))
        ));
    }

    #[tokio::test]
    async fn reload_failure_reports_exact_phase_components_and_fences() {
        let mut runtime = Runtime::new(RecordingBackend::new());
        disable_test_midi_threads(&mut runtime);
        runtime
            .backend
            .fail_create_group
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let handle = runtime.handle();
        let mut state = reload::ScriptState::new();
        state.add_group(GroupId::new(1), reload::GroupConfig::default());
        let reload = Message::Reload(Box::new(ReloadMessage::Apply { state }));
        let accepted = handle
            .submit(reload.clone(), handle.legacy_submission(&reload).unwrap())
            .await
            .unwrap();
        runtime.tick().await;
        let receipt = handle.mutation_receipt(accepted.attempt_id).unwrap();
        let ReceiptState::Terminal(TerminalOutcome::Partial(partial)) = receipt.state else {
            panic!("reload backend failure must be partial");
        };
        assert!(partial.fenced);
        assert_eq!(partial.components.len(), RELOAD_PHASE_COMPONENTS.len());
        for (component, (path, action)) in partial.components.iter().zip(RELOAD_PHASE_COMPONENTS) {
            assert_eq!(component.path, path);
            assert_eq!(component.action, action);
        }
        let create = partial
            .components
            .iter()
            .find(|component| component.path == "reload/create_entities")
            .unwrap();
        assert_eq!(create.state, ComponentState::Uncertain);
        assert_eq!(
            create.diagnostic.as_ref().unwrap().code,
            "reload_group_create_failed"
        );
    }

    async fn wait_for_one_pending(handle: &RuntimeHandle) -> crate::mutation::AttemptId {
        for _ in 0..100 {
            if let Some(pending) = handle.mutation_status().pending.first() {
                return pending.attempt_id;
            }
            tokio::task::yield_now().await;
        }
        panic!("mutation was not admitted");
    }

    #[tokio::test]
    async fn sync_and_wait_reports_backend_failure_timeout_and_ack_loss() {
        let mut failed_runtime = Runtime::new(RecordingBackend::new());
        disable_test_midi_threads(&mut failed_runtime);
        failed_runtime
            .backend
            .sync_mode
            .store(1, std::sync::atomic::Ordering::SeqCst);
        let failed_handle = failed_runtime.handle();
        let failed_waiter = {
            let handle = failed_handle.clone();
            tokio::spawn(async move { handle.sync_and_wait_timeout(Duration::from_secs(1)).await })
        };
        wait_for_one_pending(&failed_handle).await;
        let after_failed_sync = Message::Transport(TransportMessage::SetTempo { bpm: 111.0 });
        let after_failed_sync = failed_handle
            .submit(
                after_failed_sync.clone(),
                failed_handle.legacy_submission(&after_failed_sync).unwrap(),
            )
            .await
            .unwrap();
        failed_runtime.tick().await;
        assert!(matches!(
            failed_waiter.await.unwrap(),
            Err(Error::Backend(_))
        ));
        assert!(matches!(
            failed_handle.mutation_status().live_state,
            LiveState::Partial { fenced: true, .. }
        ));
        failed_runtime.tick().await;
        let after_failed_sync = failed_handle
            .mutation_receipt(after_failed_sync.attempt_id)
            .unwrap();
        assert!(matches!(
            after_failed_sync.state,
            ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
                ref code,
                fenced: true,
                ..
            })) if code == "effect_completed_after_runtime_fence"
        ));

        let mut timeout_runtime = Runtime::new(RecordingBackend::new());
        disable_test_midi_threads(&mut timeout_runtime);
        timeout_runtime
            .backend
            .sync_mode
            .store(2, std::sync::atomic::Ordering::SeqCst);
        let timeout_handle = timeout_runtime.handle();
        let timeout_waiter = {
            let handle = timeout_handle.clone();
            tokio::spawn(async move {
                handle
                    .sync_and_wait_timeout(Duration::from_millis(10))
                    .await
            })
        };
        wait_for_one_pending(&timeout_handle).await;
        timeout(Duration::from_millis(100), timeout_runtime.tick())
            .await
            .expect("a native backend barrier must not block the runtime tick");
        assert!(matches!(
            timeout_waiter.await.unwrap(),
            Err(Error::SyncTimeout)
        ));
        assert!(matches!(
            timeout_handle.mutation_status().live_state,
            LiveState::Partial { fenced: true, .. }
        ));

        let mut ack_runtime = Runtime::new(RecordingBackend::new());
        disable_test_midi_threads(&mut ack_runtime);
        let ack_handle = ack_runtime.handle();
        let ack_waiter = {
            let handle = ack_handle.clone();
            tokio::spawn(async move { handle.sync_and_wait_timeout(Duration::from_secs(1)).await })
        };
        let ack_attempt = wait_for_one_pending(&ack_handle).await;
        let _ = ack_handle.ledger.cancel(ack_attempt, SystemTime::now());
        ack_runtime.tick().await;
        assert!(matches!(
            ack_waiter.await.unwrap(),
            Err(Error::AcknowledgementLost)
        ));
    }
}
