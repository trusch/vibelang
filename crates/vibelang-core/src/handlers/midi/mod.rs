//! MIDI handler implementation.
//!
//! This module provides MIDI device enumeration, input/output handling,
//! and routing of MIDI messages to voices and parameters.
//!
//! ## Module Structure
//!
//! The MIDI handler is split into several submodules:
//! - `types` - Message types and routing configuration
//! - `callbacks` - Callback registration and invocation
//! - `clock` - MIDI clock output management
//! - `output` - Output thread and channel management
//! - `recording` - MIDI recording functionality
//! - `routing` - Route management for keyboard/CC/note mapping
//!
//! ## Realtime MIDI Output
//!
//! For sample-accurate MIDI output, use the `MidiRealtimeService` from the `midi` module.
//! This handler provides `create_output_channel()` to create channels that can be registered
//! with the realtime service.
//!
//! ## Callbacks
//!
//! Register callbacks to react to incoming MIDI data:
//! - `register_callback()` - Add a callback for specific MIDI events
//! - `unregister_callback()` - Remove a callback by ID
//! - `clear_callbacks()` - Remove all callbacks
//!
//! ## Advanced Routing
//!
//! Use routing builders for flexible MIDI-to-voice mapping:
//! - `add_keyboard_route()` - Route keyboard with range, transpose, velocity curves
//! - `add_note_route()` - Route single notes with choke groups
//! - `add_cc_route()` - Route CCs with parameter curves

mod callbacks;
mod clock;
#[cfg(not(target_arch = "wasm32"))]
mod clock_thread;
mod output;
mod recording;
mod routing;
mod types;

pub use callbacks::MidiCallbackManager;
pub use clock::MidiClockManager;
#[cfg(not(target_arch = "wasm32"))]
pub use clock_thread::MidiClockThread;
pub use output::MidiOutputManager;
pub use recording::MidiRecordingManager;
pub use routing::MidiRoutingManager;
pub use types::{CcRoute, KeyboardRoute, MidiEventNotification, MidiMessage, MidiRouting};

pub use types::{map_cc_to_range, map_per_note_to_range, Midi2ControllerType};

use crate::backend::Backend;
use crate::compat::RwLock;
#[cfg(feature = "pipewire-midi2")]
use crate::midi::PipeWireMidiInputConnection;
use crate::midi::{
    CallbackData, CallbackType, CcRouteBuilder, ControlValue, GroupChannel, KeyboardRouteBuilder,
    MidiClock, MidiEventQueue, MidiEventSender, MidiInputIntent, MidiInputIntentRuntime,
    MidiMessage as NewMidiMessage, MidiReadiness, MidiReadinessState, MidiRealtimeService,
    MidiRecording, NoteRouteBuilder, ScheduledMidiEvent, TimestampedMidiEvent,
};
use crate::midi::{LegacyInputAction, LegacyMidiPort};
#[cfg(not(target_arch = "wasm32"))]
use crate::transport_snapshot::TransportSnapshot;

use crate::handlers::ParamRouteTarget;
use crate::message::{Message, PatternMessage, VoiceMessage};
#[cfg(feature = "midi")]
use crate::midi::send_cc_for_param;
use crate::midi::PerNoteStateManager;
use crate::midi::{LooperAction, LooperManager};
use crate::reload::LooperConfig;
use crate::state::{PatternOwner, State};
#[cfg(feature = "pipewire-midi2")]
use crate::traits::Midi;
#[cfg(feature = "midi")]
use crate::traits::VoiceConfig;
use crate::traits::{FadeTarget, MidiOutputCapability};
use crate::types::ids::MidiDeviceId;
use crate::types::{NodeId, VoiceId};
use crate::{Error, Result};
use crossbeam_channel::Sender;
use midir::{MidiInputConnection, MidiOutputConnection};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Maximum number of runtime messages parked while the runtime channel is
/// full. When exceeded, the oldest non-protected message is dropped.
const PENDING_RUNTIME_CAP: usize = 16384;
const MIDI_INPUT_INTENT_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// A runtime message parked because the runtime channel was full.
struct PendingRuntimeMessage {
    msg: Message,
    /// Protected messages (note-offs and looper actions) are never dropped
    /// on overflow — losing one leaves stuck notes or orphaned patterns.
    protected: bool,
}

/// Outcome of a non-blocking send to the runtime channel.
enum RuntimeTrySend {
    Sent,
    Full(Message),
    Closed,
}

/// An open/close decision produced by [`plan_input_reconcile`].
#[cfg(any(feature = "pipewire-midi2", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputReconcileAction {
    /// A requested device is present but not open — connect it.
    Open(MidiDeviceId),
    /// A device should be torn down: a requested device that has been absent
    /// past the hysteresis threshold, or an open device no longer requested.
    Close(MidiDeviceId),
}

/// Pure decision for MIDI input hot-plug reconciliation.
///
/// Given the desired (`requested`), currently-present (`present`) and
/// currently-open (`open`) device sets — all PipeWire ids — decide which to
/// open and which to close, updating the per-device consecutive-absent
/// `counts` for hysteresis. Kept side-effect-free (no I/O, no locks) so the
/// tricky appear/disappear/hysteresis logic is unit-testable.
///
/// - requested & present & !open            → Open (power-on / replug)
/// - requested & !present & open            → bump absent; Close once it
///                                            reaches `close_threshold`
/// - open & !requested                      → Close (removed from script)
/// - present resets a device's absent count
#[cfg(any(feature = "pipewire-midi2", test))]
fn plan_input_reconcile(
    requested: &HashSet<MidiDeviceId>,
    present: &HashSet<MidiDeviceId>,
    open: &HashSet<MidiDeviceId>,
    counts: &mut HashMap<MidiDeviceId, u8>,
    close_threshold: u8,
) -> Vec<InputReconcileAction> {
    let mut actions = Vec::new();

    for id in requested {
        if present.contains(id) {
            counts.remove(id);
            if !open.contains(id) {
                actions.push(InputReconcileAction::Open(*id));
            }
        } else if open.contains(id) {
            let entry = counts.entry(*id).or_insert(0);
            *entry = entry.saturating_add(1);
            if *entry >= close_threshold {
                counts.remove(id);
                actions.push(InputReconcileAction::Close(*id));
            }
        }
    }

    // Open inputs the script no longer requests (disjoint from the loop above,
    // which only visits requested ids) are torn down immediately.
    for id in open {
        if !requested.contains(id) {
            counts.remove(id);
            actions.push(InputReconcileAction::Close(*id));
        }
    }

    actions
}

/// Try to send a runtime message without ever blocking.
fn try_send_runtime(tx: &crate::compat::Sender<Message>, msg: Message) -> RuntimeTrySend {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use tokio::sync::mpsc::error::TrySendError;
        match tx.try_send(msg) {
            Ok(()) => RuntimeTrySend::Sent,
            Err(TrySendError::Full(m)) => RuntimeTrySend::Full(m),
            Err(TrySendError::Closed(_)) => RuntimeTrySend::Closed,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut tx = tx.clone();
        match tx.try_send(msg) {
            Ok(()) => RuntimeTrySend::Sent,
            Err(e) if e.is_full() => RuntimeTrySend::Full(e.into_inner()),
            Err(_) => RuntimeTrySend::Closed,
        }
    }
}

#[derive(Default)]
struct VoiceCcTelemetry {
    total: u64,
    coalesced: u64,
    runtime_full_avoided: u64,
    last_info_total: u64,
    last_warn_total: u64,
    last_info_at: Option<Instant>,
    last_warn_at: Option<Instant>,
    coalesced_by_key: HashMap<(VoiceId, String), u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MidiRouteSnapshot {
    input_intents: Vec<MidiInputIntent>,
    keyboard_routes: Vec<crate::reload::MidiKeyboardRoute>,
    cc_routes: Vec<crate::reload::MidiCcRoute>,
    advanced_keyboard_routes: Vec<crate::reload::AdvancedMidiKeyboardRoute>,
    advanced_note_routes: Vec<crate::reload::AdvancedMidiNoteRoute>,
    advanced_cc_routes: Vec<crate::reload::AdvancedMidiCcRoute>,
    advanced_bend_routes: Vec<crate::reload::AdvancedMidiBendRoute>,
    midi2_keyboard_routes: Vec<crate::reload::Midi2KeyboardRoute>,
    midi2_per_note_routes: Vec<crate::reload::Midi2PerNoteRoute>,
    loopers: Vec<LooperConfig>,
    midi_clock_outputs: Vec<crate::reload::MidiClockOutputRequest>,
}

impl MidiRouteSnapshot {
    pub(crate) fn from_script_state(state: &crate::reload::ScriptState) -> Self {
        Self {
            input_intents: state.midi_input_intents.clone(),
            keyboard_routes: state.midi_keyboard_routes.clone(),
            cc_routes: state.midi_cc_routes.clone(),
            advanced_keyboard_routes: state.advanced_keyboard_routes.clone(),
            advanced_note_routes: state.advanced_note_routes.clone(),
            advanced_cc_routes: state.advanced_cc_routes.clone(),
            advanced_bend_routes: state.advanced_bend_routes.clone(),
            midi2_keyboard_routes: state.midi2_keyboard_routes.clone(),
            midi2_per_note_routes: state.midi2_per_note_routes.clone(),
            loopers: state.loopers.clone(),
            midi_clock_outputs: state.midi_clock_outputs.clone(),
        }
    }
}

/// Handler for MIDI operations.
///
/// Note: `inputs` and `outputs` use `std::sync::Mutex` instead of `tokio::sync::RwLock`
/// because midir's connection types contain raw pointers and are not `Send + Sync`.
/// We never hold these locks across await points, so `std::sync::Mutex` is safe.
pub struct MidiHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,

    /// Open input connections (uses std::sync::Mutex because MidiInputConnection is !Send).
    inputs: Arc<Mutex<HashMap<MidiDeviceId, MidiInputConnection<()>>>>,

    /// Stable logical MIDI-1 endpoints declared by the active script.
    input_intents: Mutex<Vec<MidiInputIntentRuntime>>,

    /// Last legacy-input discovery pass used to enforce the 250 ms cadence.
    last_input_intent_poll: Mutex<Option<Instant>>,

    /// Whether the active generation's script routes have been installed.
    input_routes_ready: AtomicBool,

    /// Open output connections (uses std::sync::Mutex because MidiOutputConnection is !Send).
    outputs: Arc<Mutex<HashMap<MidiDeviceId, MidiOutputConnection>>>,

    /// Open PipeWire raw UMP input connections.
    #[cfg(feature = "pipewire-midi2")]
    pipewire_inputs: Arc<Mutex<HashMap<MidiDeviceId, PipeWireMidiInputConnection>>>,

    /// Open ALSA raw UMP endpoints (Linux MIDI 2.0 without PipeWire).
    #[cfg(target_os = "linux")]
    alsa_ump_inputs: Arc<Mutex<HashMap<MidiDeviceId, crate::midi::AlsaUmpInputConnection>>>,

    /// Incoming MIDI message channel (legacy).
    rx: Arc<Mutex<mpsc::Receiver<(MidiDeviceId, MidiMessage)>>>,

    /// Sender for incoming MIDI messages (legacy).
    tx: mpsc::Sender<(MidiDeviceId, MidiMessage)>,

    /// Routing manager.
    routing_manager: MidiRoutingManager,

    /// Output manager.
    output_manager: MidiOutputManager,

    /// Callback manager.
    callback_manager: MidiCallbackManager,

    /// Recording manager.
    recording_manager: MidiRecordingManager,

    /// Clock manager.
    clock_manager: MidiClockManager,

    // ========================================================================
    // New Infrastructure
    // ========================================================================
    /// Lock-free event queue for high-performance MIDI processing.
    event_queue: Arc<MidiEventQueue>,

    /// MIDI clock for timestamp-to-frame conversion.
    midi_clock: Arc<MidiClock>,

    // ========================================================================
    // MIDI 2.0 Support
    // ========================================================================
    /// Cached device capabilities to avoid re-enumeration on every send.
    /// Updated when devices are opened/closed or on hot-plug events.
    capability_cache: Arc<parking_lot::RwLock<HashMap<MidiDeviceId, MidiOutputCapability>>>,

    /// Per-note state manager for MIDI 2.0 per-note expression.
    /// Reserved for future per-note pitch bend, per-note CC, etc.

    #[allow(dead_code)]
    per_note_state: Arc<RwLock<PerNoteStateManager>>,

    // ========================================================================
    // Realtime MIDI Output Service
    // ========================================================================
    /// MIDI realtime service for sample-accurate MIDI output.
    /// This listens for /tr OSC messages from SuperCollider and routes them
    /// to the appropriate MIDI output devices.
    realtime_service: Arc<parking_lot::RwLock<MidiRealtimeService>>,

    // ========================================================================
    // Clock Thread (Native Only)
    // ========================================================================
    /// Dedicated thread for MIDI clock output.
    /// Runs independently from the main loop for tighter timing.
    #[cfg(not(target_arch = "wasm32"))]
    clock_thread: Arc<parking_lot::RwLock<Option<MidiClockThread>>>,

    /// Sender for runtime messages, used to route MIDI note events
    /// through VoicesHandler for proper synth creation/destruction.
    runtime_tx: crate::compat::Sender<Message>,

    /// Latest voice CC values observed during the current MIDI tick.
    pending_voice_cc: Mutex<HashMap<(VoiceId, String), f32>>,

    /// FIFO of runtime messages that could not be sent because the runtime
    /// channel was full. Drained at the START of the next tick, before new
    /// events, with `try_send` again. A single global FIFO preserves order
    /// across all targets, so a parked NoteOn can never be overtaken by its
    /// own NoteOff.
    pending_runtime: Mutex<VecDeque<PendingRuntimeMessage>>,

    /// Overflow-drop telemetry for `pending_runtime`: (total dropped, last warn).
    pending_runtime_drops: Mutex<(u64, Option<Instant>)>,

    /// Events dropped in the midir input callback because both the event
    /// queue and the legacy channel were full (the callback must never block).
    input_callback_drops: Arc<AtomicU64>,

    /// Consumer-side reporting state for input callback drops:
    /// (last reported total, last warn instant).
    input_drops_reported: Mutex<(u64, Option<Instant>)>,

    /// Rate-limited counters for live-rig MIDI CC pressure diagnostics.
    voice_cc_telemetry: Mutex<VoiceCcTelemetry>,

    /// Looper manager.
    looper_manager: Mutex<LooperManager>,

    /// Last script MIDI route snapshot that was applied through reload.
    last_script_routes: Mutex<MidiRouteSnapshot>,

    /// PipeWire input devices the script/API has asked to keep open. The
    /// hot-plug watcher reopens these when they (re)appear on the system and
    /// closes them when they vanish, so inputs survive unplug/replug and
    /// power-on-after-start. Populated by `open_input` and reset on reload.
    requested_inputs: Arc<Mutex<HashSet<MidiDeviceId>>>,

    /// Consecutive hot-plug polls each requested input has been absent, for
    /// close hysteresis — a device must be missing for two scans before we
    /// tear its connection down, so one transient empty enumeration can't
    /// glitch a live input.
    #[cfg(feature = "pipewire-midi2")]
    input_absent_polls: Mutex<HashMap<MidiDeviceId, u8>>,

    /// Hot-plug watcher shutdown flag (thread stops when set true).
    #[cfg(feature = "pipewire-midi2")]
    hotplug_stop: Arc<AtomicBool>,

    /// Hot-plug watcher thread handle, joined on `stop_input_hotplug_watcher`.
    #[cfg(all(feature = "pipewire-midi2", not(target_arch = "wasm32")))]
    hotplug_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl<B: Backend> Drop for MidiHandler<B> {
    fn drop(&mut self) {
        // Signal the hot-plug watcher to stop so it doesn't outlive the
        // handler (its detached JoinHandle exits within one poll step). The
        // clock/realtime threads are torn down via their own explicit stops.
        #[cfg(feature = "pipewire-midi2")]
        self.hotplug_stop.store(true, Ordering::Relaxed);
    }
}

impl<B: Backend> MidiHandler<B> {
    /// Create a new MIDI handler.
    pub fn new(
        backend: Arc<B>,
        state: Arc<RwLock<State>>,
        runtime_tx: crate::compat::Sender<Message>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(1024);

        // Initialize new infrastructure
        let event_queue = Arc::new(MidiEventQueue::with_default_capacity());
        let midi_clock = Arc::new(MidiClock::default());

        Self {
            backend,
            state,
            inputs: Arc::new(Mutex::new(HashMap::new())),
            input_intents: Mutex::new(Vec::new()),
            last_input_intent_poll: Mutex::new(None),
            input_routes_ready: AtomicBool::new(true),
            outputs: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(feature = "pipewire-midi2")]
            pipewire_inputs: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(target_os = "linux")]
            alsa_ump_inputs: Arc::new(Mutex::new(HashMap::new())),
            rx: Arc::new(Mutex::new(rx)),
            tx,
            routing_manager: MidiRoutingManager::new(),
            output_manager: MidiOutputManager::new(),
            callback_manager: MidiCallbackManager::new(),
            recording_manager: MidiRecordingManager::new(),
            clock_manager: MidiClockManager::new(),
            // New infrastructure
            event_queue,
            midi_clock,
            // MIDI 2.0
            capability_cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),

            per_note_state: Arc::new(RwLock::new(PerNoteStateManager::new())),

            // Realtime service (not started yet - call start_realtime_service())
            realtime_service: Arc::new(parking_lot::RwLock::new(MidiRealtimeService::new())),

            // Clock thread (not started yet - call start_clock_thread())
            #[cfg(not(target_arch = "wasm32"))]
            clock_thread: Arc::new(parking_lot::RwLock::new(None)),

            runtime_tx,

            pending_voice_cc: Mutex::new(HashMap::new()),
            voice_cc_telemetry: Mutex::new(VoiceCcTelemetry::default()),

            pending_runtime: Mutex::new(VecDeque::new()),
            pending_runtime_drops: Mutex::new((0, None)),
            input_callback_drops: Arc::new(AtomicU64::new(0)),
            input_drops_reported: Mutex::new((0, None)),

            looper_manager: Mutex::new(LooperManager::new()),
            last_script_routes: Mutex::new(MidiRouteSnapshot::default()),

            requested_inputs: Arc::new(Mutex::new(HashSet::new())),
            #[cfg(feature = "pipewire-midi2")]
            input_absent_polls: Mutex::new(HashMap::new()),
            #[cfg(feature = "pipewire-midi2")]
            hotplug_stop: Arc::new(AtomicBool::new(false)),
            #[cfg(all(feature = "pipewire-midi2", not(target_arch = "wasm32")))]
            hotplug_handle: Mutex::new(None),
        }
    }

    /// Start the MIDI realtime service for sample-accurate MIDI output.
    ///
    /// This starts a dedicated high-priority thread that listens for /tr OSC messages
    /// from SuperCollider and routes them to MIDI output devices.
    ///
    /// Call this after the SuperCollider backend is connected.
    ///
    /// # Arguments
    ///
    /// * `scsynth_addr` - Optional scsynth address (defaults to "127.0.0.1:57110")
    pub fn start_realtime_service(&self, scsynth_addr: Option<&str>) -> Result<()> {
        let mut service = self.realtime_service.write();

        if service.is_running() {
            tracing::debug!("MIDI realtime service already running");
            return Ok(());
        }

        // Recreate with the correct address if specified
        if let Some(addr) = scsynth_addr {
            *service = MidiRealtimeService::with_scsynth_addr(addr);
        }

        service
            .start()
            .map_err(|e| Error::MidiError(format!("Failed to start MIDI realtime service: {}", e)))
    }

    /// Stop the MIDI realtime service.
    pub fn stop_realtime_service(&self) {
        let mut service = self.realtime_service.write();
        service.stop();
    }

    /// Check if the MIDI realtime service is running.
    pub fn is_realtime_service_running(&self) -> bool {
        self.realtime_service.read().is_running()
    }

    /// Record the set of PipeWire MIDI inputs the current script wants open.
    ///
    /// Called on reload so the hot-plug watcher stops reopening devices that
    /// were removed from the script and closes any still-open connection for
    /// them on the next reconcile.
    pub fn set_requested_inputs(&self, ids: &HashSet<MidiDeviceId>) {
        if let Ok(mut requested) = self.requested_inputs.lock() {
            *requested = ids
                .iter()
                .copied()
                .filter(|id| crate::midi::is_pipewire_midi_input_id(*id))
                .collect();
        }
    }

    /// Note that a device has been requested as an input, so the hot-plug
    /// watcher keeps (re)opening it even if the first open failed because it
    /// was not present yet.
    fn note_requested_input(&self, id: MidiDeviceId) {
        if crate::midi::is_pipewire_midi_input_id(id) {
            if let Ok(mut requested) = self.requested_inputs.lock() {
                requested.insert(id);
            }
        }
    }

    /// How many consecutive reconciles a device must be absent before its
    /// connection is torn down (hysteresis against a transient empty scan).
    #[cfg(feature = "pipewire-midi2")]
    const HOTPLUG_ABSENT_CLOSE_THRESHOLD: u8 = 2;

    /// Reconcile open PipeWire inputs against the devices currently present.
    ///
    /// Driven by [`Self::start_input_hotplug_watcher`]: reopens requested
    /// devices that have (re)appeared (power-on after start, unplug/replug —
    /// PipeWire ids are stable across replug because they hash the node name),
    /// tears down requested devices that have vanished (after a short absence
    /// hysteresis) so a replug reconnects cleanly, and closes open inputs the
    /// script no longer requests. All device open/close runs here on the
    /// runtime task, serialized with reload and tick.
    #[cfg(feature = "pipewire-midi2")]
    pub async fn reconcile_pipewire_inputs(&self, present: HashSet<MidiDeviceId>) {
        // Snapshot desired + open sets; never hold these locks across the
        // open_input/close awaits below (those take the same locks).
        let requested: HashSet<MidiDeviceId> = match self.requested_inputs.lock() {
            Ok(r) => r.clone(),
            Err(_) => return,
        };
        let open_ids: HashSet<MidiDeviceId> = match self.pipewire_inputs.lock() {
            Ok(inputs) => inputs.keys().copied().collect(),
            Err(_) => return,
        };

        // Decide open/close actions with the pure planner (holds the absent
        // counter only briefly), then perform the I/O without any lock held.
        let actions = {
            let mut counts = match self.input_absent_polls.lock() {
                Ok(c) => c,
                Err(_) => return,
            };
            plan_input_reconcile(
                &requested,
                &present,
                &open_ids,
                &mut counts,
                Self::HOTPLUG_ABSENT_CLOSE_THRESHOLD,
            )
        };

        for action in actions {
            match action {
                InputReconcileAction::Open(id) => match self.open_input(id).await {
                    Ok(()) => tracing::info!("MIDI input {:?} connected via hot-plug", id),
                    Err(e) => tracing::debug!("MIDI input {:?} present but open failed: {}", id, e),
                },
                InputReconcileAction::Close(id) => {
                    tracing::info!(
                        "MIDI input {:?} disconnected/removed; closing (reconnects when it returns)",
                        id
                    );
                    let _ = self.close(id).await;
                }
            }
        }
    }

    #[cfg(not(feature = "pipewire-midi2"))]
    pub async fn reconcile_pipewire_inputs(&self, _present: HashSet<MidiDeviceId>) {}

    /// Start the background MIDI input hot-plug watcher thread.
    ///
    /// Every couple of seconds it enumerates the PipeWire MIDI devices present
    /// on the system (a blocking scan, hence its own thread) and sends the
    /// snapshot to the runtime as [`crate::message::MidiMessage::ReconcileInputs`],
    /// which reopens/closes devices via [`Self::reconcile_pipewire_inputs`].
    /// Idempotent: a second call while the watcher is running is a no-op.
    #[cfg(all(feature = "pipewire-midi2", not(target_arch = "wasm32")))]
    pub fn start_input_hotplug_watcher(&self) {
        use std::time::Duration;

        {
            let handle = self.hotplug_handle.lock();
            if matches!(handle, Ok(ref h) if h.is_some()) {
                return;
            }
        }
        self.hotplug_stop.store(false, Ordering::Relaxed);

        let stop = Arc::clone(&self.hotplug_stop);
        let tx = self.runtime_tx.clone();
        const POLL_INTERVAL: Duration = Duration::from_secs(2);
        const STEP: Duration = Duration::from_millis(200);

        let handle = std::thread::Builder::new()
            .name("vibelang-midi-hotplug".to_string())
            .spawn(move || {
                tracing::info!("[MIDI HOTPLUG] input watcher started");
                while !stop.load(Ordering::Relaxed) {
                    // Sleep the poll interval in small steps for responsive shutdown.
                    let mut slept = Duration::ZERO;
                    while slept < POLL_INTERVAL {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(STEP);
                        slept += STEP;
                    }
                    let present = crate::midi::pipewire_midi2_input_ids();
                    let _ = try_send_runtime(
                        &tx,
                        Message::Midi(crate::message::MidiMessage::ReconcileInputs { present }),
                    );
                }
            });

        match handle {
            Ok(h) => {
                if let Ok(mut slot) = self.hotplug_handle.lock() {
                    *slot = Some(h);
                }
            }
            Err(e) => tracing::warn!("Failed to spawn MIDI hot-plug watcher: {}", e),
        }
    }

    /// Stop the MIDI input hot-plug watcher thread and join it.
    #[cfg(all(feature = "pipewire-midi2", not(target_arch = "wasm32")))]
    pub fn stop_input_hotplug_watcher(&self) {
        self.hotplug_stop.store(true, Ordering::Relaxed);
        let handle = self.hotplug_handle.lock().ok().and_then(|mut h| h.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }

    /// Start the MIDI clock thread for low-latency clock output.
    ///
    /// This starts a dedicated 1kHz thread that reads transport state from
    /// the provided snapshot and sends MIDI clock (24 PPQN) to registered devices.
    ///
    /// Call this after the runtime is initialized and transport is ready.
    ///
    /// # Arguments
    ///
    /// * `transport_snapshot` - Shared transport state for lock-free reading
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_clock_thread(&self, transport_snapshot: Arc<TransportSnapshot>) {
        let mut clock_thread_guard = self.clock_thread.write();

        if clock_thread_guard.is_some() {
            tracing::debug!("MIDI clock thread already initialized");
            return;
        }

        // Create the clock thread with output channels
        let mut clock_thread = MidiClockThread::new(
            transport_snapshot,
            self.output_manager.clock_channels.clone(),
        );

        // Start the thread
        clock_thread.start();

        *clock_thread_guard = Some(clock_thread);
        tracing::info!("MIDI clock thread started");
    }

    /// Stop the MIDI clock thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stop_clock_thread(&self) {
        let mut clock_thread_guard = self.clock_thread.write();
        if let Some(ref mut thread) = *clock_thread_guard {
            thread.stop();
        }
        *clock_thread_guard = None;
        tracing::info!("MIDI clock thread stopped");
    }

    /// Check if the MIDI clock thread is running.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn is_clock_thread_running(&self) -> bool {
        self.clock_thread
            .read()
            .as_ref()
            .map(|t| t.is_running())
            .unwrap_or(false)
    }

    /// Enable clock output for a device via the clock thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_clock_output_threaded(&self, device: MidiDeviceId) {
        if let Some(ref thread) = *self.clock_thread.read() {
            thread.enable_clock_output(device);
        }
    }

    /// Disable clock output for a device via the clock thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn disable_clock_output_threaded(&self, device: MidiDeviceId) {
        if let Some(ref thread) = *self.clock_thread.read() {
            thread.disable_clock_output(device);
        }
    }

    /// Queue a quantized MIDI Start for a device (sent at next bar boundary).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn queue_quantized_start(&self, device: MidiDeviceId) {
        if let Some(ref thread) = *self.clock_thread.read() {
            thread.queue_quantized_start(device);
        }
    }

    /// Set the beats per bar for quantization (from time signature).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_beats_per_bar(&self, beats: u8) {
        if let Some(ref thread) = *self.clock_thread.read() {
            thread.set_beats_per_bar(beats);
        }
    }

    /// Get cached capability for a device, detecting if not cached.
    pub fn get_device_capability(&self, device: MidiDeviceId) -> MidiOutputCapability {
        // Check cache first
        {
            let cache = self.capability_cache.read();
            if let Some(&cap) = cache.get(&device) {
                return cap;
            }
        }

        // Not cached - detect and cache
        self.detect_and_cache_capability(device)
    }

    /// Detect capability for a device and cache it.
    fn detect_and_cache_capability(&self, device: MidiDeviceId) -> MidiOutputCapability {
        // Find device name by enumerating
        let device_name = midir::MidiOutput::new("vibelang-cap-check")
            .ok()
            .and_then(|midi_out| {
                midi_out
                    .ports()
                    .get(device.0 as usize)
                    .and_then(|port| midi_out.port_name(port).ok())
            })
            .unwrap_or_else(|| format!("Unknown {}", device.0));

        let cap = output::detect_midi2_capability(&device_name);

        // Cache it
        self.capability_cache.write().insert(device, cap);

        tracing::info!(
            "Cached MIDI capability for device {} ('{}'): {:?}",
            device.0,
            device_name,
            cap
        );

        cap
    }

    /// Clear capability cache (call on device hot-plug).
    pub fn clear_capability_cache(&self) {
        self.capability_cache.write().clear();
        tracing::debug!("Cleared MIDI capability cache");
    }

    pub(crate) async fn set_input_intents(&self, intents: &[MidiInputIntent]) {
        self.input_routes_ready.store(false, Ordering::Release);

        let removed = {
            let mut current = self.input_intents.lock().unwrap_or_else(|e| e.into_inner());
            let mut previous = std::mem::take(&mut *current);
            let mut next = Vec::with_capacity(intents.len());

            for intent in intents {
                if let Some(index) = previous
                    .iter()
                    .position(|runtime| runtime.intent == *intent)
                {
                    next.push(previous.remove(index));
                } else {
                    next.push(MidiInputIntentRuntime::new(intent.clone()));
                }
            }
            *current = next;
            previous
        };

        for runtime in removed {
            if runtime.is_bound() {
                self.panic_and_close_input(runtime.intent.device_id).await;
            }
        }

        *self
            .last_input_intent_poll
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        self.publish_input_readiness().await;
        self.reconcile_input_intents(true).await;
    }

    async fn poll_input_intents(&self) {
        self.reconcile_input_intents(false).await;
    }

    async fn reconcile_input_intents(&self, force: bool) {
        if self
            .input_intents
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
        {
            return;
        }

        let now = Instant::now();
        {
            let mut last = self
                .last_input_intent_poll
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if !force
                && last
                    .is_some_and(|last| now.duration_since(last) < MIDI_INPUT_INTENT_POLL_INTERVAL)
            {
                return;
            }
            *last = Some(now);
        }

        let discovery = Self::enumerate_legacy_input_ports();
        let actions = {
            let mut runtimes = self.input_intents.lock().unwrap_or_else(|e| e.into_inner());
            runtimes
                .iter_mut()
                .enumerate()
                .map(|(index, runtime)| {
                    let action = match &discovery {
                        Ok(ports) => runtime.observe(Ok(ports)),
                        Err(error) => runtime.observe(Err(error)),
                    };
                    (index, runtime.intent.device_id, action)
                })
                .collect::<Vec<_>>()
        };

        for (index, logical_id, action) in actions {
            match action {
                LegacyInputAction::None => {}
                LegacyInputAction::Disconnect => {
                    self.panic_and_close_input(logical_id).await;
                }
                LegacyInputAction::Open {
                    port,
                    disconnect_first,
                } => {
                    if disconnect_first {
                        self.panic_and_close_input(logical_id).await;
                    }

                    let open_result = self.open_legacy_input_as(port.id, logical_id).await;
                    let mut runtimes = self.input_intents.lock().unwrap_or_else(|e| e.into_inner());
                    let Some(runtime) = runtimes.get_mut(index) else {
                        continue;
                    };
                    if runtime.intent.device_id != logical_id {
                        continue;
                    }
                    match open_result {
                        Ok(()) => runtime.mark_opened(port),
                        Err(error) => runtime.mark_open_failed(&port, &error.to_string()),
                    }
                }
            }
        }

        self.publish_input_readiness().await;
    }

    fn enumerate_legacy_input_ports() -> std::result::Result<Vec<LegacyMidiPort>, String> {
        let midi_in =
            midir::MidiInput::new("vibelang-intent-list").map_err(|error| error.to_string())?;
        let mut ports = Vec::new();
        for (index, port) in midi_in.ports().iter().enumerate() {
            let name = midi_in.port_name(port).map_err(|error| {
                format!("Failed to read MIDI input name at index {index}: {error}")
            })?;
            ports.push(LegacyMidiPort {
                id: MidiDeviceId::new(index as u32),
                name,
            });
        }
        Ok(ports)
    }

    async fn publish_input_readiness(&self) {
        let routes_ready = self.input_routes_ready.load(Ordering::Acquire);
        let mut endpoints = self
            .input_intents
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(MidiInputIntentRuntime::readiness)
            .collect::<Vec<_>>();

        if !routes_ready {
            for endpoint in &mut endpoints {
                if endpoint.state == MidiReadinessState::Connected {
                    endpoint.state = MidiReadinessState::Unavailable;
                    endpoint.detail = Some(
                        "MIDI routes are being installed for the active runtime generation"
                            .to_string(),
                    );
                }
            }
        }

        let readiness = MidiReadiness::from_endpoints(endpoints);
        let mut state = self.state.write().await;
        if state.midi_readiness != readiness {
            state.midi_readiness = readiness;
        }
    }

    async fn panic_and_close_input(&self, device_id: MidiDeviceId) {
        let routing = self.routing_manager.routing();
        let routing = routing.read().await;
        let mut target_voices = HashSet::new();
        target_voices.extend(
            routing
                .keyboard_routes
                .iter()
                .filter(|route| route.device_id == device_id)
                .map(|route| route.voice_id),
        );
        target_voices.extend(
            routing
                .advanced_keyboard_routes
                .iter()
                .filter(|route| route.device_id == device_id)
                .filter_map(|route| route.target_voice),
        );
        target_voices.extend(
            routing
                .note_routes
                .iter()
                .filter(|route| route.device_id == device_id)
                .filter_map(|route| route.target_voice),
        );
        drop(routing);

        let active_notes = {
            let state = self.state.read().await;
            target_voices
                .iter()
                .flat_map(|voice_id| {
                    state
                        .voices
                        .get(voice_id)
                        .into_iter()
                        .flat_map(|voice| voice.note_nodes.keys().copied())
                        .map(|note| (*voice_id, note))
                })
                .collect::<Vec<_>>()
        };

        for (voice, note) in active_notes {
            self.send_runtime_from_tick(
                Message::Voice(VoiceMessage::NoteOff { voice, note }),
                true,
                "input disconnect all-notes-off",
            );
        }
        self.pending_voice_cc
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(voice, _), _| !target_voices.contains(voice));

        self.close_legacy_input(device_id);
        tracing::info!(
            "MIDI input {} disconnected; issued all-notes-off for {} routed voices",
            device_id.raw(),
            target_voices.len()
        );
    }

    // ========================================================================
    // New Infrastructure Accessors
    // ========================================================================

    /// Get the event queue for direct access to timestamped events.
    pub fn event_queue(&self) -> Arc<MidiEventQueue> {
        Arc::clone(&self.event_queue)
    }

    /// Get the MIDI clock for timestamp conversion.
    pub fn midi_clock(&self) -> Arc<MidiClock> {
        Arc::clone(&self.midi_clock)
    }

    /// Get a sender for the event queue (for use in callbacks).
    pub fn event_sender(&self) -> MidiEventSender {
        self.event_queue.sender()
    }

    // ========================================================================
    // Callback Management (delegated)
    // ========================================================================

    /// Register a callback for MIDI events.
    pub async fn register_callback(
        &self,
        device_id: MidiDeviceId,
        callback_type: CallbackType,
        channel: Option<u8>,
        callback_data: CallbackData,
    ) -> u64 {
        self.callback_manager
            .register_callback(device_id, callback_type, channel, callback_data)
            .await
    }

    /// Unregister a callback by ID.
    pub async fn unregister_callback(&self, id: u64) -> bool {
        self.callback_manager.unregister_callback(id).await
    }

    /// Clear all callbacks.
    pub async fn clear_callbacks(&self) {
        self.callback_manager.clear_callbacks().await
    }

    /// Clear all callbacks for a specific device.
    pub async fn clear_device_callbacks(&self, device_id: MidiDeviceId) {
        self.callback_manager
            .clear_device_callbacks(device_id)
            .await
    }

    /// Get a receiver for callback notifications.
    pub fn callback_receiver(&self) -> Arc<Mutex<mpsc::Receiver<MidiEventNotification>>> {
        self.callback_manager.callback_receiver()
    }

    /// Try to receive callback notifications without blocking.
    pub fn poll_callbacks(&self) -> Vec<MidiEventNotification> {
        self.callback_manager.poll_callbacks()
    }

    // ========================================================================
    // Advanced Routing (delegated)
    // ========================================================================

    /// Add an advanced keyboard route.
    pub async fn add_keyboard_route(&self, route: KeyboardRouteBuilder) -> usize {
        self.routing_manager.add_keyboard_route(route).await
    }

    /// Add an advanced note route (for drums/pads).
    pub async fn add_note_route(&self, route: NoteRouteBuilder) -> usize {
        self.routing_manager.add_note_route(route).await
    }

    /// Add an advanced CC route.
    pub async fn add_cc_route(&self, route: CcRouteBuilder) -> usize {
        self.routing_manager.add_cc_route(route).await
    }

    /// Remove a keyboard route by index.
    pub async fn remove_keyboard_route(&self, index: usize) {
        self.routing_manager.remove_keyboard_route(index).await
    }

    /// Get the number of active keyboard routes.
    pub async fn keyboard_route_count(&self) -> usize {
        self.routing_manager.keyboard_route_count().await
    }

    /// Clear all MIDI routes.
    pub async fn clear_routes(&self) {
        self.routing_manager.clear_routes().await
    }

    pub(crate) fn script_routes_changed(&self, state: &crate::reload::ScriptState) -> bool {
        let desired = MidiRouteSnapshot::from_script_state(state);
        let current = self
            .last_script_routes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *current != desired
    }

    pub(crate) async fn mark_script_routes_applied(&self, state: &crate::reload::ScriptState) {
        {
            let mut current = self
                .last_script_routes
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *current = MidiRouteSnapshot::from_script_state(state);
        }
        self.input_routes_ready.store(true, Ordering::Release);
        self.publish_input_readiness().await;
    }

    /// Apply CC routes from script state.
    pub async fn apply_cc_routes(&self, routes: &[crate::reload::MidiCcRoute]) {
        self.routing_manager.apply_cc_routes(routes).await
    }

    /// Apply basic keyboard routes from script state.
    pub async fn apply_basic_keyboard_routes(&self, routes: &[crate::reload::MidiKeyboardRoute]) {
        self.routing_manager
            .apply_basic_keyboard_routes(routes)
            .await
    }

    /// Apply advanced keyboard routes from script state.
    pub async fn apply_advanced_keyboard_routes(
        &self,
        routes: &[crate::reload::AdvancedMidiKeyboardRoute],
    ) {
        self.routing_manager
            .apply_advanced_keyboard_routes(routes)
            .await
    }

    /// Apply advanced note routes from script state.
    pub async fn apply_advanced_note_routes(
        &self,
        routes: &[crate::reload::AdvancedMidiNoteRoute],
    ) {
        self.routing_manager
            .apply_advanced_note_routes(routes)
            .await
    }

    /// Apply advanced CC routes from script state.
    pub async fn apply_advanced_cc_routes(&self, routes: &[crate::reload::AdvancedMidiCcRoute]) {
        self.routing_manager.apply_advanced_cc_routes(routes).await
    }

    /// Apply advanced pitch-bend routes from script state.
    pub async fn apply_advanced_bend_routes(
        &self,
        routes: &[crate::reload::AdvancedMidiBendRoute],
    ) {
        self.routing_manager
            .apply_advanced_bend_routes(routes)
            .await
    }

    // ========================================================================
    // MIDI 2.0 Routing (delegated)
    // ========================================================================

    pub async fn apply_midi2_keyboard_routes(&self, routes: &[crate::reload::Midi2KeyboardRoute]) {
        self.routing_manager
            .apply_midi2_keyboard_routes(routes)
            .await
    }

    pub async fn apply_midi2_per_note_routes(&self, routes: &[crate::reload::Midi2PerNoteRoute]) {
        self.routing_manager
            .apply_midi2_per_note_routes(routes)
            .await
    }

    // ========================================================================
    // Output Channel Management (delegated)
    // ========================================================================

    /// Create an output channel for a MIDI device.
    ///
    /// This opens the device, creates a background output thread, and registers
    /// the device with the realtime service for sample-accurate MIDI output.
    ///
    /// Returns a sender for `ScheduledMidiEvent` which supports both immediate
    /// and timestamp-based scheduling for sub-millisecond timing precision.
    pub fn create_output_channel(&self, id: MidiDeviceId) -> Result<Sender<ScheduledMidiEvent>> {
        let (sender, capability) = self.output_manager.create_output_channel(id)?;

        // Register with the realtime service so /tr messages are routed to this device
        // Pass the capability so the service knows how to encode messages
        let service = self.realtime_service.read();
        service.register_device_with_capability(id.0, sender.clone(), capability);

        Ok(sender)
    }

    /// Close an output channel.
    ///
    /// This unregisters the device from the realtime service and closes the output thread.
    pub fn close_output_channel(&self, id: MidiDeviceId) {
        // Unregister from realtime service first
        let service = self.realtime_service.read();
        service.unregister_device(id.0);
        drop(service);

        self.output_manager.close_output_channel(id)
    }

    /// Get the output channel sender for a device.
    pub fn get_output_channel(&self, id: MidiDeviceId) -> Option<Sender<ScheduledMidiEvent>> {
        self.output_manager.get_output_channel(id)
    }

    /// Get the shared output channels map.
    ///
    /// This allows other handlers (like VoicesHandler) to access MIDI output
    /// channels for sending note-on/note-off messages to MIDI devices.
    pub fn output_channels(&self) -> Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>> {
        Arc::clone(&self.output_manager.output_channels)
    }

    // ========================================================================
    // Recording (delegated)
    // ========================================================================

    /// Get a reference to the recordings map.
    pub fn recordings(&self) -> Arc<RwLock<HashMap<MidiDeviceId, MidiRecording>>> {
        self.recording_manager.recordings()
    }

    // ========================================================================
    // Message Processing
    // ========================================================================

    /// Process incoming MIDI messages.
    ///
    /// Called by the runtime's tick loop.
    pub async fn tick(&self) {
        // First, retry runtime messages parked while the runtime channel was
        // full — before new events, so per-target ordering is preserved.
        self.drain_pending_runtime();

        // Report input-callback drops from the consumer side (rate-limited).
        self.report_input_callback_drops();

        // Collect messages from the channel (holding the lock briefly)
        let messages: Vec<_> = {
            if let Ok(mut rx) = self.rx.lock() {
                let mut msgs = Vec::new();
                while let Ok(msg) = rx.try_recv() {
                    msgs.push(msg);
                }
                msgs
            } else {
                tracing::warn!("MIDI rx mutex poisoned, skipping tick");
                Vec::new()
            }
        };

        // Process messages without holding the lock
        for (device_id, msg) in messages {
            self.handle_message_inner(device_id, msg).await;
        }

        // Process timestamped MIDI events from the event queue.
        self.process_timestamped_events().await;
        self.flush_pending_voice_cc().await;

        // Tick looper manager — detects silence and triggers playback conversion.
        let (current_beat, time_sig_num) = {
            let state = self.state.read().await;
            (state.current_beat.to_f64(), state.time_sig.numerator)
        };
        let looper_actions = {
            let mut mgr = self
                .looper_manager
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            mgr.tick(current_beat, time_sig_num)
        };
        self.dispatch_looper_actions(looper_actions);

        // Legacy ALSA ports have unstable numeric IDs. Re-resolve logical
        // intents after processing this tick's queued events so a disconnect
        // panic is ordered after any already-received note-on.
        self.poll_input_intents().await;
    }

    // ========================================================================
    // Non-blocking runtime channel access
    // ========================================================================

    /// Send a message to the runtime without ever awaiting channel capacity.
    ///
    /// The MIDI tick must never block on the bounded runtime channel: the
    /// runtime may itself be waiting on work that needs this tick to finish
    /// (self-backpressure deadlock). If the channel is full, the message is
    /// parked in `pending_runtime` and retried at the start of the next tick.
    /// If older messages are already parked, the new message queues behind
    /// them so global FIFO order is preserved.
    fn send_runtime_from_tick(&self, msg: Message, from_looper: bool, context: &'static str) {
        let mut pending = self
            .pending_runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let msg = if pending.is_empty() {
            match try_send_runtime(&self.runtime_tx, msg) {
                RuntimeTrySend::Sent => return,
                RuntimeTrySend::Closed => {
                    tracing::warn!("MIDI {}: runtime channel closed, dropping message", context);
                    return;
                }
                RuntimeTrySend::Full(m) => m,
            }
        } else {
            // Messages are already parked: append behind them instead of
            // sending directly, so a parked NoteOn cannot be overtaken by
            // its own NoteOff.
            msg
        };

        let protected = from_looper || matches!(msg, Message::Voice(VoiceMessage::NoteOff { .. }));
        pending.push_back(PendingRuntimeMessage { msg, protected });

        if pending.len() > PENDING_RUNTIME_CAP {
            // Drop the oldest non-protected message. Note-offs and looper
            // actions are never dropped, so the queue may exceed the cap if
            // every parked message is protected.
            if let Some(idx) = pending.iter().position(|p| !p.protected) {
                pending.remove(idx);
                drop(pending);
                self.note_pending_runtime_drop(context);
            }
        }
    }

    /// Retry parked runtime messages in FIFO order until the channel fills
    /// up again. Called at the start of every tick, before new events.
    fn drain_pending_runtime(&self) {
        let mut pending = self
            .pending_runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        while let Some(entry) = pending.pop_front() {
            match try_send_runtime(&self.runtime_tx, entry.msg) {
                RuntimeTrySend::Sent => {}
                RuntimeTrySend::Full(msg) => {
                    pending.push_front(PendingRuntimeMessage {
                        msg,
                        protected: entry.protected,
                    });
                    break;
                }
                RuntimeTrySend::Closed => {
                    let dropped = pending.len() + 1;
                    pending.clear();
                    tracing::warn!(
                        "MIDI: runtime channel closed, dropping {} pending messages",
                        dropped
                    );
                    break;
                }
            }
        }
    }

    /// Count an overflow drop from the pending runtime queue (warn rate-limited).
    fn note_pending_runtime_drop(&self, context: &'static str) {
        let now = Instant::now();
        let mut drops = self
            .pending_runtime_drops
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        drops.0 = drops.0.saturating_add(1);
        if drops
            .1
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
        {
            drops.1 = Some(now);
            tracing::warn!(
                "MIDI pending runtime queue overflow (cap={}): dropped oldest non-protected message; total_dropped={}, latest_context={}",
                PENDING_RUNTIME_CAP,
                drops.0,
                context
            );
        }
    }

    /// Report events dropped in the midir input callback (rate-limited).
    ///
    /// The callback itself must never block or log; it only increments an
    /// atomic counter, which we report here from the consumer side.
    fn report_input_callback_drops(&self) {
        let total = self.input_callback_drops.load(Ordering::Relaxed);
        let mut reported = self
            .input_drops_reported
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if total > reported.0 {
            let now = Instant::now();
            if reported
                .1
                .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
            {
                tracing::warn!(
                    "MIDI input callback dropped {} events since last report (total={}): event queue and legacy channel both full",
                    total - reported.0,
                    total
                );
                reported.0 = total;
                reported.1 = Some(now);
            }
        }
    }

    // ========================================================================
    // Looper Management
    // ========================================================================

    /// Reconcile looper instances against the new config list.
    ///
    /// Called on script reload. Stops patterns from removed loopers.
    pub async fn reconcile_loopers(&self, configs: &[LooperConfig]) {
        let actions = {
            let mut mgr = self
                .looper_manager
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            mgr.reconcile(configs)
        };
        self.dispatch_looper_actions(actions);
    }

    /// Dispatch a batch of looper actions to the runtime (non-blocking).
    ///
    /// Looper messages are dispatched as protected: they are parked (never
    /// dropped) when the runtime channel is full.
    fn dispatch_looper_actions(&self, actions: Vec<LooperAction>) {
        for action in actions {
            match action {
                LooperAction::NoteOn {
                    voice_id,
                    note,
                    velocity,
                } => {
                    self.send_runtime_from_tick(
                        Message::Voice(VoiceMessage::NoteOn {
                            voice: voice_id,
                            note,
                            velocity: velocity as f32 / 127.0,
                        }),
                        true,
                        "looper note_on",
                    );
                }
                LooperAction::NoteOff { voice_id, note } => {
                    self.send_runtime_from_tick(
                        Message::Voice(VoiceMessage::NoteOff {
                            voice: voice_id,
                            note,
                        }),
                        true,
                        "looper note_off",
                    );
                }
                LooperAction::StopPattern { pattern_id } => {
                    self.send_runtime_from_tick(
                        Message::Pattern(PatternMessage::Stop { id: pattern_id }),
                        true,
                        "looper stop_pattern",
                    );
                    self.send_runtime_from_tick(
                        Message::Pattern(PatternMessage::Delete { id: pattern_id }),
                        true,
                        "looper delete_pattern",
                    );
                }
                LooperAction::StartPattern { config, pattern_id } => {
                    self.send_runtime_from_tick(
                        Message::Pattern(PatternMessage::Create {
                            id: pattern_id,
                            config,
                            owner: PatternOwner::Looper,
                        }),
                        true,
                        "looper create_pattern",
                    );
                    self.send_runtime_from_tick(
                        // Grid-anchored, not "now"-anchored: the captured loop
                        // drops in phase-locked to the transport bar grid so it
                        // sits with the rest of the song instead of free-running
                        // at whatever fractional beat the silence timer fired.
                        Message::Pattern(PatternMessage::StartOnGrid { id: pattern_id }),
                        true,
                        "looper start_pattern",
                    );
                }
            }
        }
    }

    /// Process timestamped MIDI events from the event queue.
    async fn process_timestamped_events(&self) {
        // Drain events from the queue
        let events = self.event_queue.drain();

        for event in events {
            if event.message.is_midi2() {
                if let Some(msg) = types::convert_new_to_legacy_message(&event.message) {
                    let event_beat = self.event_arrival_beat(&event).await;
                    let route_advanced_cc =
                        !matches!(&event.message, NewMidiMessage::Midi2ControlChange { .. });
                    self.handle_message_at_beat(
                        event.device_id,
                        msg,
                        Some(event_beat),
                        route_advanced_cc,
                    )
                    .await;
                }
                self.handle_midi2_message(event.device_id, &event.message)
                    .await;
            } else if let Some(msg) = types::convert_new_to_legacy_message(&event.message) {
                let event_beat = self.event_arrival_beat(&event).await;
                self.handle_message_at_beat(event.device_id, msg, Some(event_beat), true)
                    .await;
            }
        }
    }

    async fn event_arrival_beat(&self, event: &TimestampedMidiEvent) -> f64 {
        let state = self.state.read().await;
        beat_at_event_arrival(
            event.received_at,
            Instant::now(),
            state.current_beat.to_f64(),
            state.tempo,
            state.playing,
        )
    }

    /// Handle a single MIDI message.
    #[cfg(test)]
    async fn handle_message(&self, device_id: MidiDeviceId, msg: MidiMessage) {
        self.handle_message_inner(device_id, msg).await;
        self.flush_pending_voice_cc().await;
    }

    async fn handle_message_inner(&self, device_id: MidiDeviceId, msg: MidiMessage) {
        self.handle_message_at_beat(device_id, msg, None, true)
            .await;
    }

    async fn handle_message_at_beat(
        &self,
        device_id: MidiDeviceId,
        msg: MidiMessage,
        event_beat: Option<f64>,
        route_advanced_cc: bool,
    ) {
        // First, invoke callbacks
        self.callback_manager
            .invoke_callbacks(device_id, &msg)
            .await;

        // Record events if recording is active for this device
        if let Some(event_beat) = event_beat {
            self.recording_manager
                .record_message_at_beat(device_id, &msg, crate::types::Beat::from_f64(event_beat))
                .await;
        } else {
            self.recording_manager
                .record_message(device_id, &msg, &self.state)
                .await;
        }

        // Then process routing
        let routing_arc = self.routing_manager.routing();
        let routing = routing_arc.read().await;

        match &msg {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => {
                self.handle_note_on(&routing, device_id, *channel, *note, *velocity, event_beat)
                    .await;
            }
            MidiMessage::NoteOff { channel, note } => {
                self.handle_note_off(&routing, device_id, *channel, *note, event_beat)
                    .await;
            }
            MidiMessage::ControlChange { channel, cc, value } => {
                // Collect routes and drop routing lock before backend calls
                let basic_routes: Vec<_> = routing
                    .cc_routes
                    .iter()
                    .filter(|r| r.device_id == device_id && r.cc == *cc)
                    .cloned()
                    .collect();

                drop(routing);

                self.handle_basic_cc(basic_routes, *value).await;
                if route_advanced_cc {
                    self.handle_advanced_cc(
                        device_id,
                        None,
                        *channel,
                        *cc,
                        ControlValue::from_7bit(*value),
                    )
                    .await;
                }
            }
            MidiMessage::PitchBend { channel, value } => {
                let basic_routes: Vec<_> = routing
                    .keyboard_routes
                    .iter()
                    .filter(|r| {
                        r.device_id == device_id
                            && (r.channel.is_none() || r.channel == Some(*channel))
                    })
                    .cloned()
                    .collect();

                let advanced_routes: Vec<_> = routing
                    .advanced_keyboard_routes
                    .iter()
                    .filter(|r| {
                        r.device_id == device_id
                            && (r.channel.is_none() || r.channel == Some(*channel))
                    })
                    .cloned()
                    .collect();

                let advanced_bend_routes: Vec<_> = routing
                    .advanced_bend_routes
                    .iter()
                    .filter(|r| {
                        r.device_id == device_id
                            && (r.channel.is_none() || r.channel == Some(*channel))
                    })
                    .cloned()
                    .collect();

                drop(routing);

                self.handle_pitch_bend(basic_routes, advanced_routes, advanced_bend_routes, *value)
                    .await;
            }
            // TODO: external MIDI clock sync is unimplemented — Clock/Start/
            // Stop/Continue from devices are only logged and never drive the
            // transport or tempo.
            MidiMessage::Clock => {
                tracing::trace!("MIDI Clock pulse from device {}", device_id.0);
            }
            MidiMessage::Start => {
                tracing::debug!("MIDI Start from device {}", device_id.0);
            }
            MidiMessage::Stop => {
                tracing::debug!("MIDI Stop from device {}", device_id.0);
            }
            MidiMessage::Continue => {
                tracing::debug!("MIDI Continue from device {}", device_id.0);
            }
        }
    }

    async fn handle_note_on(
        &self,
        routing: &MidiRouting,
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
        event_beat: Option<f64>,
    ) {
        // Defensive: a "NoteOn velocity 0" is the running-status idiom for a
        // note-off. The legacy input path already maps it via
        // `convert_new_to_legacy_message`, but normalise again here at the
        // routing boundary so no future caller can sneak a vel-0 NoteOn through
        // — letting it fall through would retrigger the voice (and, for a
        // MIDI-output voice, re-send `NoteOn vel 0`) instead of releasing it,
        // which on some encoders/devices leaves the gate stuck open.
        if velocity == 0 {
            tracing::debug!(
                "MIDI: NoteOn velocity 0 (note={}, ch={}) treated as NoteOff",
                note,
                channel
            );
            self.handle_note_off(routing, device_id, channel, note, event_beat)
                .await;
            return;
        }

        // If a looper is configured for this device/channel, route exclusively through it.
        if self
            .looper_manager
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .has_route_for_event(device_id, channel)
        {
            let (current_beat, time_sig_num) = {
                let state = self.state.read().await;
                (state.current_beat.to_f64(), state.time_sig.numerator)
            };
            let capture_beat = event_beat.unwrap_or(current_beat);
            let actions = {
                let mut mgr = self
                    .looper_manager
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                mgr.handle_note_on(
                    device_id,
                    channel,
                    note,
                    velocity,
                    capture_beat,
                    time_sig_num,
                )
            };
            self.dispatch_looper_actions(actions);
            return;
        }

        let vel_f32 = velocity as f32 / 127.0;

        // Process basic keyboard routes
        for route in &routing.keyboard_routes {
            if route.device_id == device_id
                && (route.channel.is_none() || route.channel == Some(channel))
            {
                tracing::debug!(
                    "MIDI note_on: voice={}, note={}, velocity={}",
                    route.voice_id.0,
                    note,
                    velocity
                );
                self.send_runtime_from_tick(
                    Message::Voice(VoiceMessage::NoteOn {
                        voice: route.voice_id,
                        note,
                        velocity: vel_f32,
                    }),
                    false,
                    "keyboard note_on",
                );
            }
        }

        // Process advanced keyboard routes
        for route in &routing.advanced_keyboard_routes {
            if route.device_id == device_id
                && (route.channel.is_none() || route.channel == Some(channel))
                && route.note_in_range(note)
            {
                if let Some(voice_id) = route.target_voice {
                    let transposed_note = route.apply_transpose(note);
                    let curved_velocity = route.apply_velocity(velocity);
                    let vel = curved_velocity as f32 / 127.0;

                    tracing::debug!(
                        "MIDI advanced note_on: voice={}, note={}->{}, velocity={}->{}",
                        voice_id.0,
                        note,
                        transposed_note,
                        velocity,
                        curved_velocity
                    );
                    self.send_runtime_from_tick(
                        Message::Voice(VoiceMessage::NoteOn {
                            voice: voice_id,
                            note: transposed_note,
                            velocity: vel,
                        }),
                        false,
                        "advanced keyboard note_on",
                    );
                }
            }
        }

        // Process note routes (drums/pads)
        for route in &routing.note_routes {
            if route.device_id == device_id
                && route.source_note == note
                && (route.channel.is_none() || route.channel == Some(channel))
            {
                if let Some(voice_id) = route.target_voice {
                    let curved_velocity = route.apply_velocity(velocity);
                    let vel_curved = curved_velocity as f32 / 127.0;
                    tracing::debug!(
                        "MIDI note route: voice={}, note={}, velocity={}, curved={}",
                        voice_id.0,
                        note,
                        velocity,
                        curved_velocity
                    );

                    // Handle velocity-to-parameter mapping before triggering the note
                    if let Some((param, value)) = route.velocity_to_param(curved_velocity) {
                        tracing::debug!(
                            "MIDI velocity mapping: param={}, value={} (voice={}, note={})",
                            param,
                            value,
                            voice_id.0,
                            note
                        );
                        self.send_runtime_from_tick(
                            Message::Voice(VoiceMessage::SetParam {
                                id: voice_id,
                                param: param.to_string(),
                                value: value as f32,
                            }),
                            false,
                            "note route velocity param",
                        );
                    }

                    self.send_runtime_from_tick(
                        Message::Voice(VoiceMessage::NoteOn {
                            voice: voice_id,
                            note,
                            velocity: vel_curved,
                        }),
                        false,
                        "note route note_on",
                    );
                }
            }
        }
    }

    async fn handle_note_off(
        &self,
        routing: &MidiRouting,
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
        event_beat: Option<f64>,
    ) {
        // If a looper is configured for this device/channel, route exclusively through it.
        if self
            .looper_manager
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .has_route_for_event(device_id, channel)
        {
            let current_beat = {
                let state = self.state.read().await;
                state.current_beat.to_f64()
            };
            let capture_beat = event_beat.unwrap_or(current_beat);
            let actions = {
                let mut mgr = self
                    .looper_manager
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                mgr.handle_note_off(device_id, channel, note, capture_beat)
            };
            self.dispatch_looper_actions(actions);
            return;
        }

        // Process basic keyboard routes
        for route in &routing.keyboard_routes {
            if route.device_id == device_id
                && (route.channel.is_none() || route.channel == Some(channel))
            {
                tracing::debug!("MIDI note_off: voice={}, note={}", route.voice_id.0, note);
                self.send_runtime_from_tick(
                    Message::Voice(VoiceMessage::NoteOff {
                        voice: route.voice_id,
                        note,
                    }),
                    false,
                    "keyboard note_off",
                );
            }
        }

        // Process advanced keyboard routes
        for route in &routing.advanced_keyboard_routes {
            if route.device_id == device_id
                && (route.channel.is_none() || route.channel == Some(channel))
                && route.note_in_range(note)
            {
                if let Some(voice_id) = route.target_voice {
                    let transposed_note = route.apply_transpose(note);

                    tracing::debug!(
                        "MIDI advanced note_off: voice={}, note={}->{}",
                        voice_id.0,
                        note,
                        transposed_note
                    );
                    self.send_runtime_from_tick(
                        Message::Voice(VoiceMessage::NoteOff {
                            voice: voice_id,
                            note: transposed_note,
                        }),
                        false,
                        "advanced keyboard note_off",
                    );
                }
            }
        }

        // Process note routes (drums/pads)
        for route in &routing.note_routes {
            if route.device_id == device_id
                && route.source_note == note
                && (route.channel.is_none() || route.channel == Some(channel))
            {
                if let Some(voice_id) = route.target_voice {
                    tracing::debug!("MIDI note route off: voice={}, note={}", voice_id.0, note);
                    self.send_runtime_from_tick(
                        Message::Voice(VoiceMessage::NoteOff {
                            voice: voice_id,
                            note,
                        }),
                        false,
                        "note route note_off",
                    );
                }
            }
        }
    }

    async fn handle_basic_cc(&self, basic_routes: Vec<CcRoute>, value: u8) {
        // Process basic CC routes
        for route in basic_routes {
            let normalized = value as f32 / 127.0;
            let scaled = route.min_value + normalized * (route.max_value - route.min_value);
            tracing::debug!(
                "MIDI CC: target={:?}, param={}, value={} (raw={}, min={}, max={})",
                route.target,
                route.param,
                scaled,
                normalized,
                route.min_value,
                route.max_value
            );

            if let Err(e) = self
                .apply_cc_to_target(&route.target, &route.param, scaled)
                .await
            {
                tracing::warn!("Failed to apply CC to {:?}: {}", route.target, e);
            }
        }
    }

    async fn handle_advanced_cc(
        &self,
        device_id: MidiDeviceId,
        group_channel: Option<GroupChannel>,
        channel: u8,
        cc: u8,
        value: ControlValue,
    ) {
        let routes: Vec<_> = {
            let routing_arc = self.routing_manager.routing();
            let routing = routing_arc.read().await;
            routing
                .advanced_cc_routes
                .iter()
                .filter(|route| {
                    let group_matches = match (route.group, group_channel) {
                        (None, _) => true,
                        (Some(expected), Some(actual)) => expected == actual.group(),
                        (Some(_), None) => false,
                    };
                    route.device_id == device_id
                        && route.cc == cc
                        && route.channel.is_none_or(|expected| expected == channel)
                        && group_matches
                })
                .cloned()
                .collect()
        };

        let normalized = value.as_f32();
        for route in routes {
            let param_value =
                map_cc_to_range(normalized, 0.0, 1.0, route.min, route.max, &route.curve);

            tracing::debug!(
                "MIDI CC: target={:?}, cc={}, param={}={:.4}",
                route.target,
                cc,
                route.param,
                param_value
            );

            if let Err(e) = self
                .apply_cc_to_target(&route.target, &route.param, param_value)
                .await
            {
                tracing::warn!("Failed to apply MIDI CC to {:?}: {}", route.target, e);
            }
        }
    }

    async fn handle_pitch_bend(
        &self,
        basic_routes: Vec<KeyboardRoute>,
        advanced_routes: Vec<KeyboardRouteBuilder>,
        advanced_bend_routes: Vec<CcRouteBuilder>,
        value: i16,
    ) {
        let bend_semitones = (value as f32 / 8192.0) * 2.0;

        for route in basic_routes {
            if let Err(e) = self.apply_pitch_bend(route.voice_id, bend_semitones).await {
                tracing::warn!(
                    "Failed to apply pitch bend to voice {}: {}",
                    route.voice_id.0,
                    e
                );
            }
        }

        for route in advanced_routes {
            if let Some(voice_id) = route.target_voice {
                if let Err(e) = self.apply_pitch_bend(voice_id, bend_semitones).await {
                    tracing::warn!("Failed to apply pitch bend to voice {}: {}", voice_id.0, e);
                }
            }
        }

        // Pitch bend remains additive: keyboard routes keep detuning held
        // notes, while map_bend routes can also drive arbitrary live params.
        // Avoid routing the same device's keyboard bend and joystick-param
        // bend to the same voice unless that double effect is intentional.
        for route in advanced_bend_routes {
            let param_value = route.bend_to_param(value);

            if let (Some(target), Some(param)) = (route.target.as_ref(), &route.target_param) {
                tracing::debug!(
                    "MIDI advanced pitch bend: target={:?}, param={}, value={}",
                    target,
                    param,
                    param_value
                );

                if let Err(e) = self.apply_cc_to_target(target, param, param_value).await {
                    tracing::warn!("Failed to apply advanced pitch bend to {:?}: {}", target, e);
                }
            }
        }
    }

    /// Apply a CC value to a target's parameter.
    async fn apply_cc_to_target(&self, target: &FadeTarget, param: &str, value: f32) -> Result<()> {
        if let FadeTarget::Voice(id) = target {
            let voice_exists = {
                let state = self.state.read().await;
                state.voices.contains_key(id)
            };

            if voice_exists {
                let replaced = self
                    .pending_voice_cc
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert((*id, param.to_string()), value)
                    .is_some();
                self.observe_voice_cc(*id, param, replaced);
            }

            return Ok(());
        }

        // Look up node ID(s) for the target
        let node_ids: Vec<NodeId> = {
            let state = self.state.read().await;
            match target {
                FadeTarget::Group(id) => {
                    if let Some(group) = state.groups.get(id) {
                        let node = if matches!(param, "amp" | "pan") {
                            group.link_synth_node_id.unwrap_or(group.node_id)
                        } else {
                            group.node_id
                        };
                        vec![node]
                    } else {
                        vec![]
                    }
                }
                FadeTarget::Voice(_) => unreachable!("voice targets return before node lookup"),
                FadeTarget::Effect(id) => {
                    if let Some(effect) = state.effects.get(id) {
                        vec![effect.node_id]
                    } else {
                        vec![]
                    }
                }
                FadeTarget::Pattern(_) | FadeTarget::Melody(_) => {
                    tracing::debug!("CC to pattern/melody not supported directly");
                    vec![]
                }
            }
        };

        for node_id in node_ids {
            self.backend
                .set_param(node_id, param, value)
                .await
                .map_err(Error::backend)?;
        }

        Ok(())
    }

    fn observe_voice_cc(&self, id: VoiceId, param: &str, coalesced: bool) {
        let channel = self.runtime_channel_stats();
        let now = Instant::now();
        let mut telemetry = self
            .voice_cc_telemetry
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        telemetry.total = telemetry.total.saturating_add(1);
        if coalesced {
            telemetry.coalesced = telemetry.coalesced.saturating_add(1);
            *telemetry
                .coalesced_by_key
                .entry((id, param.to_string()))
                .or_insert(0) += 1;
        }

        let runtime_full = channel.is_some_and(|(_, remaining, _)| remaining == 0);
        if runtime_full {
            telemetry.runtime_full_avoided = telemetry.runtime_full_avoided.saturating_add(1);
        }

        let should_info = telemetry
            .last_info_at
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
            || telemetry.total.saturating_sub(telemetry.last_info_total) >= 1024;
        if should_info {
            telemetry.last_info_at = Some(now);
            telemetry.last_info_total = telemetry.total;

            let key_coalesced = telemetry
                .coalesced_by_key
                .get(&(id, param.to_string()))
                .copied()
                .unwrap_or(0);
            if let Some((depth, remaining, max)) = channel {
                let near_full = max > 0 && remaining <= (max / 8).max(1);
                tracing::info!(
                    "MIDI voice CC coalescing: total={}, coalesced={}, current_key=({}, '{}'), current_key_coalesced={}, runtime_queue_depth={}/{}, runtime_queue_remaining={}, runtime_queue_near_full={}",
                    telemetry.total,
                    telemetry.coalesced,
                    id.0,
                    param,
                    key_coalesced,
                    depth,
                    max,
                    remaining,
                    near_full
                );
            } else {
                tracing::info!(
                    "MIDI voice CC coalescing: total={}, coalesced={}, current_key=({}, '{}'), current_key_coalesced={}, runtime_queue_depth=unavailable",
                    telemetry.total,
                    telemetry.coalesced,
                    id.0,
                    param,
                    key_coalesced
                );
            }
        }

        let should_warn = runtime_full
            && (telemetry
                .last_warn_at
                .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
                || telemetry
                    .runtime_full_avoided
                    .saturating_sub(telemetry.last_warn_total)
                    >= 1024);
        if should_warn {
            telemetry.last_warn_at = Some(now);
            telemetry.last_warn_total = telemetry.runtime_full_avoided;
            tracing::warn!(
                "MIDI voice CC avoided blocking on full runtime queue: avoided_count={}, total={}, coalesced={}, current_key=({}, '{}')",
                telemetry.runtime_full_avoided,
                telemetry.total,
                telemetry.coalesced,
                id.0,
                param
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn runtime_channel_stats(&self) -> Option<(usize, usize, usize)> {
        let remaining = self.runtime_tx.capacity();
        let max = self.runtime_tx.max_capacity();
        Some((max.saturating_sub(remaining), remaining, max))
    }

    #[cfg(target_arch = "wasm32")]
    fn runtime_channel_stats(&self) -> Option<(usize, usize, usize)> {
        None
    }

    async fn flush_pending_voice_cc(&self) {
        let updates: Vec<_> = {
            let mut pending = self
                .pending_voice_cc
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            pending.drain().collect()
        };

        for ((voice_id, param), value) in updates {
            if let Err(e) = self.apply_voice_cc(voice_id, &param, value).await {
                tracing::warn!(
                    "Failed to apply MIDI CC to voice {} param '{}': {}",
                    voice_id.0,
                    param,
                    e
                );
            }
        }
    }

    async fn apply_voice_cc(&self, id: VoiceId, param: &str, value: f32) -> Result<()> {
        #[cfg(feature = "midi")]
        let (nodes, voice_config, summer_node, set_routed): (
            Vec<NodeId>,
            VoiceConfig,
            Option<NodeId>,
            bool,
        ) = {
            let mut state = self.state.write().await;
            let Some(voice) = state.voices.get_mut(&id) else {
                return Ok(());
            };

            voice.config.params.insert(param.to_string(), value);

            let mut nodes = voice.active_nodes.clone();
            nodes.extend(voice.note_nodes.values().copied());
            let voice_config = voice.config.clone();

            let target = ParamRouteTarget::Voice(id);
            let key = (target.clone(), param.to_string());
            let summer_node = state.param_summers.get(&key).map(|s| s.node);
            let set_routed = state
                .param_routes_set
                .values()
                .any(|targets| targets.iter().any(|(t, tp)| *t == target && tp == param));

            (nodes, voice_config, summer_node, set_routed)
        };

        #[cfg(not(feature = "midi"))]
        let (nodes, summer_node, set_routed): (Vec<NodeId>, Option<NodeId>, bool) = {
            let mut state = self.state.write().await;
            let Some(voice) = state.voices.get_mut(&id) else {
                return Ok(());
            };

            voice.config.params.insert(param.to_string(), value);

            let mut nodes = voice.active_nodes.clone();
            nodes.extend(voice.note_nodes.values().copied());

            let target = ParamRouteTarget::Voice(id);
            let key = (target.clone(), param.to_string());
            let summer_node = state.param_summers.get(&key).map(|s| s.node);
            let set_routed = state
                .param_routes_set
                .values()
                .any(|targets| targets.iter().any(|(t, tp)| *t == target && tp == param));

            (nodes, summer_node, set_routed)
        };

        #[cfg(feature = "midi")]
        {
            let sent = send_cc_for_param(&voice_config, param, value, |device_id| {
                self.get_output_channel(device_id)
            });
            if sent {
                tracing::debug!(
                    "Voice {:?}: sent MIDI CC for MIDI input param '{}' = {}",
                    id,
                    param,
                    value
                );
            }
        }

        if set_routed {
            tracing::debug!(
                "MIDI CC set_param on voice {:?} param '{}': param is routed via .to_param \
                 (SET), so the mapped source overrides this value until the route is removed",
                id,
                param
            );
        }

        for node_id in nodes {
            self.backend
                .set_param(node_id, param, value)
                .await
                .map_err(Error::backend)?;
        }

        if let Some(summer) = summer_node {
            if !set_routed {
                self.backend
                    .set_param(summer, "baseline", value)
                    .await
                    .map_err(Error::backend)?;
            }
        }

        Ok(())
    }

    /// Apply pitch bend to a voice's active notes.
    async fn apply_pitch_bend(&self, voice_id: VoiceId, bend_semitones: f32) -> Result<()> {
        let note_nodes: Vec<_> = {
            let state = self.state.read().await;
            if let Some(voice) = state.voices.get(&voice_id) {
                voice.note_nodes.values().copied().collect()
            } else {
                return Ok(());
            }
        };

        let node_count = note_nodes.len();

        for node_id in note_nodes {
            self.backend
                .set_param(node_id, "detune", bend_semitones)
                .await
                .map_err(Error::backend)?;
        }

        tracing::debug!(
            "Applied pitch bend {} semitones to voice {} ({} nodes)",
            bend_semitones,
            voice_id.0,
            node_count
        );

        Ok(())
    }

    /// Send MIDI clock tick.
    pub async fn send_clock_tick(&self) -> Result<()> {
        self.clock_manager.send_clock_tick(&self.outputs).await
    }

    /// Tick MIDI clock output based on current beat position.
    pub async fn tick_clock(&self, current_beat: f64, is_playing: bool) -> Result<()> {
        self.clock_manager
            .tick_clock(current_beat, is_playing, &self.outputs)
            .await
    }

    /// Reset the clock output position.
    pub async fn reset_clock_position(&self, beat: f64) {
        self.clock_manager.reset_clock_position(beat).await
    }

    // ========================================================================
    // MIDI 2.0 Message Handling
    // ========================================================================

    async fn handle_midi2_message(&self, device_id: MidiDeviceId, msg: &NewMidiMessage) {
        match msg {
            NewMidiMessage::Midi2PerNotePitchBend {
                group_channel,
                note,
                value,
            } => {
                self.handle_per_note_pitch_bend(device_id, *group_channel, *note, *value)
                    .await;
            }
            NewMidiMessage::Midi2PerNoteController {
                group_channel,
                note,
                controller,
                value,
            } => {
                self.handle_per_note_controller(
                    device_id,
                    *group_channel,
                    *note,
                    *controller,
                    *value,
                )
                .await;
            }
            NewMidiMessage::Midi2NoteOn {
                group_channel,
                note,
                velocity,
                ..
            } => {
                self.handle_midi2_note_on(device_id, *group_channel, *note, *velocity)
                    .await;
            }
            NewMidiMessage::Midi2NoteOff {
                group_channel,
                note,
                ..
            } => {
                self.handle_midi2_note_off(device_id, *group_channel, *note)
                    .await;
            }
            NewMidiMessage::Midi2ControlChange {
                group_channel,
                controller,
                value,
            } => {
                self.handle_advanced_cc(
                    device_id,
                    Some(*group_channel),
                    group_channel.channel(),
                    *controller,
                    *value,
                )
                .await;
            }
            NewMidiMessage::Midi2PitchBend {
                group_channel,
                value,
            } => {
                self.handle_midi2_pitch_bend(device_id, *group_channel, *value)
                    .await;
            }
            _ => {}
        }
    }

    async fn handle_per_note_pitch_bend(
        &self,
        device_id: MidiDeviceId,
        group_channel: crate::midi::GroupChannel,
        note: u8,
        value: u32,
    ) {
        let routing_arc = self.routing_manager.routing();
        let routing = routing_arc.read().await;

        for route in &routing.midi2_per_note_routes {
            if route.device_id != device_id {
                continue;
            }

            if let Some(g) = route.group {
                if g != group_channel.group() {
                    continue;
                }
            }
            if let Some(c) = route.channel {
                if c != group_channel.channel() {
                    continue;
                }
            }

            if let Midi2ControllerType::PitchBend { range } = route.controller_type {
                let centered = (value as i64) - 0x8000_0000i64;
                let normalized = centered as f64 / 0x8000_0000u64 as f64;
                let semitones = (normalized * range as f64) as f32;

                let param_value = map_per_note_to_range(
                    semitones,
                    -(range as f32),
                    range as f32,
                    route.min_value,
                    route.max_value,
                    &route.curve,
                );

                if let Err(e) = self
                    .apply_per_note_param(route.voice_id, note, &route.param, param_value)
                    .await
                {
                    tracing::warn!(
                        "Failed to apply per-note pitch bend to voice {}: {}",
                        route.voice_id.0,
                        e
                    );
                }

                tracing::trace!(
                    "Per-note pitch bend: voice={}, note={}, semitones={:.2}, param={}={:.3}",
                    route.voice_id.0,
                    note,
                    semitones,
                    route.param,
                    param_value
                );
            }
        }
    }

    async fn handle_per_note_controller(
        &self,
        device_id: MidiDeviceId,
        group_channel: crate::midi::GroupChannel,
        note: u8,
        controller: u8,
        value: crate::midi::ControlValue,
    ) {
        let routing_arc = self.routing_manager.routing();
        let routing = routing_arc.read().await;

        for route in &routing.midi2_per_note_routes {
            if route.device_id != device_id {
                continue;
            }

            if let Some(g) = route.group {
                if g != group_channel.group() {
                    continue;
                }
            }
            if let Some(c) = route.channel {
                if c != group_channel.channel() {
                    continue;
                }
            }

            let matches = match &route.controller_type {
                Midi2ControllerType::Pressure => controller == 0x7B,
                Midi2ControllerType::Timbre => controller == 74,
                Midi2ControllerType::Controller(cc) => controller == *cc,
                _ => false,
            };

            if matches {
                let normalized = value.as_f32();
                let param_value = map_per_note_to_range(
                    normalized,
                    0.0,
                    1.0,
                    route.min_value,
                    route.max_value,
                    &route.curve,
                );

                if let Err(e) = self
                    .apply_per_note_param(route.voice_id, note, &route.param, param_value)
                    .await
                {
                    tracing::warn!(
                        "Failed to apply per-note controller to voice {}: {}",
                        route.voice_id.0,
                        e
                    );
                }

                tracing::trace!(
                    "Per-note controller: voice={}, note={}, cc={}, param={}={:.3}",
                    route.voice_id.0,
                    note,
                    controller,
                    route.param,
                    param_value
                );
            }
        }
    }

    async fn handle_midi2_note_on(
        &self,
        device_id: MidiDeviceId,
        group_channel: crate::midi::GroupChannel,
        note: u8,
        velocity: crate::midi::Velocity,
    ) {
        let routing_arc = self.routing_manager.routing();
        let routing = routing_arc.read().await;

        for route in &routing.midi2_keyboard_routes {
            if route.device_id != device_id {
                continue;
            }

            if let Some(g) = route.group {
                if g != group_channel.group() {
                    continue;
                }
            }
            if let Some(c) = route.channel {
                if c != group_channel.channel() {
                    continue;
                }
            }

            if note < route.note_min || note > route.note_max {
                continue;
            }

            let transposed_note = (note as i16 + route.transpose as i16).clamp(0, 127) as u8;

            tracing::debug!(
                "MIDI 2.0 note_on: voice={}, note={}->{}, velocity={}",
                route.voice_id.0,
                note,
                transposed_note,
                velocity.as_f32()
            );
            self.send_runtime_from_tick(
                Message::Voice(VoiceMessage::NoteOn {
                    voice: route.voice_id,
                    note: transposed_note,
                    velocity: velocity.as_f32(),
                }),
                false,
                "midi2 note_on",
            );
        }
    }

    async fn handle_midi2_note_off(
        &self,
        device_id: MidiDeviceId,
        group_channel: crate::midi::GroupChannel,
        note: u8,
    ) {
        let routing_arc = self.routing_manager.routing();
        let routing = routing_arc.read().await;

        for route in &routing.midi2_keyboard_routes {
            if route.device_id != device_id {
                continue;
            }

            if let Some(g) = route.group {
                if g != group_channel.group() {
                    continue;
                }
            }
            if let Some(c) = route.channel {
                if c != group_channel.channel() {
                    continue;
                }
            }

            if note < route.note_min || note > route.note_max {
                continue;
            }

            let transposed_note = (note as i16 + route.transpose as i16).clamp(0, 127) as u8;

            tracing::debug!(
                "MIDI 2.0 note_off: voice={}, note={}->{}",
                route.voice_id.0,
                note,
                transposed_note
            );
            self.send_runtime_from_tick(
                Message::Voice(VoiceMessage::NoteOff {
                    voice: route.voice_id,
                    note: transposed_note,
                }),
                false,
                "midi2 note_off",
            );
        }
    }

    async fn handle_midi2_pitch_bend(
        &self,
        device_id: MidiDeviceId,
        group_channel: crate::midi::GroupChannel,
        value: u32,
    ) {
        let routes: Vec<_> = {
            let routing_arc = self.routing_manager.routing();
            let routing = routing_arc.read().await;
            routing
                .midi2_keyboard_routes
                .iter()
                .filter(|r| {
                    r.device_id == device_id
                        && r.group.is_none_or(|g| g == group_channel.group())
                        && r.channel.is_none_or(|c| c == group_channel.channel())
                })
                .cloned()
                .collect()
        };

        let centered = (value as i64) - 0x8000_0000i64;
        let normalized = centered as f64 / 0x8000_0000u64 as f64;
        let bend_semitones = (normalized * 2.0) as f32;

        for route in routes {
            if let Err(e) = self.apply_pitch_bend(route.voice_id, bend_semitones).await {
                tracing::warn!(
                    "Failed to apply MIDI 2.0 pitch bend to voice {}: {}",
                    route.voice_id.0,
                    e
                );
            }
        }
    }

    async fn apply_per_note_param(
        &self,
        voice_id: VoiceId,
        note: u8,
        param: &str,
        value: f32,
    ) -> Result<()> {
        let node_id = {
            let state = self.state.read().await;
            if let Some(voice) = state.voices.get(&voice_id) {
                voice.note_nodes.get(&note).copied()
            } else {
                None
            }
        };

        if let Some(node_id) = node_id {
            self.backend
                .set_param(node_id, param, value)
                .await
                .map_err(Error::backend)?;
        }

        Ok(())
    }
}

fn beat_at_event_arrival(
    received_at: Instant,
    processing_at: Instant,
    processing_beat: f64,
    tempo: f64,
    playing: bool,
) -> f64 {
    if !playing {
        return processing_beat;
    }

    let queued_secs = processing_at
        .checked_duration_since(received_at)
        .unwrap_or_default()
        .as_secs_f64();
    (processing_beat - queued_secs * tempo / 60.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_input_reconcile_appear_disappear_hysteresis_and_deregister() {
        let a = MidiDeviceId::new(1);
        let empty = HashSet::new();
        let req: HashSet<MidiDeviceId> = [a].into_iter().collect();
        let present: HashSet<MidiDeviceId> = [a].into_iter().collect();
        let open: HashSet<MidiDeviceId> = [a].into_iter().collect();
        let mut counts = HashMap::new();

        // Requested + present + not open -> Open (power-on / replug).
        assert_eq!(
            plan_input_reconcile(&req, &present, &empty, &mut counts, 2),
            vec![InputReconcileAction::Open(a)]
        );

        // Requested + open + absent: first poll waits (hysteresis), second closes.
        assert!(plan_input_reconcile(&req, &empty, &open, &mut counts, 2).is_empty());
        assert_eq!(
            plan_input_reconcile(&req, &empty, &open, &mut counts, 2),
            vec![InputReconcileAction::Close(a)]
        );

        // Reappearing before the threshold resets the absent counter.
        let mut counts = HashMap::new();
        assert!(plan_input_reconcile(&req, &empty, &open, &mut counts, 2).is_empty()); // absent=1
        assert!(plan_input_reconcile(&req, &present, &open, &mut counts, 2).is_empty()); // reset
        assert!(plan_input_reconcile(&req, &empty, &open, &mut counts, 2).is_empty()); // absent=1 again, no close

        // Open but no longer requested (removed from script) -> immediate close.
        let mut counts = HashMap::new();
        assert_eq!(
            plan_input_reconcile(&empty, &empty, &open, &mut counts, 2),
            vec![InputReconcileAction::Close(a)]
        );

        // Steady state (requested + present + open) -> no actions.
        let mut counts = HashMap::new();
        assert!(plan_input_reconcile(&req, &present, &open, &mut counts, 2).is_empty());
    }

    use crate::backend::{AddAction, Backend, BufferInfo};
    use crate::compat::{channel, timeout, RwLock};
    use crate::handlers::VoicesHandler;
    use crate::midi::{Channel, NoteRouteBuilder, Velocity};
    use crate::reload::{
        AdvancedMidiBendRoute, AdvancedMidiCcRoute, LooperConfig, Midi2PerNoteControllerType,
        Midi2PerNoteRoute,
    };
    use crate::state::GroupState;
    use crate::traits::{VoiceConfig, Voices};
    use crate::types::{Beat, BufferId, BusId, GroupId, NodeId, ParamMap, VoiceId};
    use async_trait::async_trait;
    use std::path::Path;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;
    use tracing_subscriber::layer::SubscriberExt;

    struct WarningCapture {
        messages: Arc<StdMutex<Vec<String>>>,
    }

    impl<S> tracing_subscriber::Layer<S> for WarningCapture
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if *event.metadata().level() != tracing::Level::WARN {
                return;
            }

            #[derive(Default)]
            struct MessageVisitor(String);

            impl tracing::field::Visit for MessageVisitor {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "message" {
                        self.0 = format!("{value:?}");
                    }
                }

                fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                    if field.name() == "message" {
                        self.0 = value.to_string();
                    }
                }
            }

            let mut visitor = MessageVisitor::default();
            event.record(&mut visitor);
            self.messages.lock().unwrap().push(visitor.0);
        }
    }

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock error")
        }
    }

    impl std::error::Error for MockError {}

    #[derive(Clone, Debug)]
    struct SetParamCall {
        node: NodeId,
        param: String,
        value: f32,
    }

    struct MockBackend {
        set_param_calls: StdMutex<Vec<SetParamCall>>,
        synth_create_params: StdMutex<Vec<ParamMap>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                set_param_calls: StdMutex::new(Vec::new()),
                synth_create_params: StdMutex::new(Vec::new()),
            }
        }

        fn set_param_calls(&self) -> Vec<SetParamCall> {
            self.set_param_calls.lock().unwrap().clone()
        }

        fn clear_set_param_calls(&self) {
            self.set_param_calls.lock().unwrap().clear();
        }

        fn synth_create_params(&self) -> Vec<ParamMap> {
            self.synth_create_params.lock().unwrap().clone()
        }
    }

    #[async_trait]
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
            params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            self.synth_create_params
                .lock()
                .unwrap()
                .push(params.clone());
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
            node: NodeId,
            param: &str,
            value: f32,
        ) -> std::result::Result<(), Self::Error> {
            self.set_param_calls.lock().unwrap().push(SetParamCall {
                node,
                param: param.to_string(),
                value,
            });
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

    async fn setup_voice_state(state: &Arc<RwLock<State>>) {
        let mut state = state.write().await;
        state.synthdefs.insert("test_synth".to_string());

        let group_id = GroupId::new(1);
        state.groups.insert(
            group_id,
            GroupState {
                id: group_id,
                name: "test_group".to_string(),
                parent: None,
                node_id: NodeId(100),
                audio_bus: BusId(16),
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );
    }

    async fn create_test_voice<B: Backend>(
        voices: &VoicesHandler<B>,
        voice_id: VoiceId,
    ) -> VoiceConfig {
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        voices.create(voice_id, config.clone()).await.unwrap();
        config
    }

    fn advanced_cc_route(
        device_id: MidiDeviceId,
        cc: u8,
        group: Option<u8>,
        curve: &str,
        target: FadeTarget,
        param: &str,
        min: f32,
        max: f32,
    ) -> AdvancedMidiCcRoute {
        AdvancedMidiCcRoute {
            device_id,
            cc,
            channel: None,
            group,
            curve: curve.to_string(),
            target,
            param: param.to_string(),
            min,
            max,
        }
    }

    async fn send_ump_cc(
        midi: &MidiHandler<MockBackend>,
        device_id: MidiDeviceId,
        sequence: u64,
        group: u8,
        cc: u8,
        value: ControlValue,
    ) {
        assert!(midi.event_sender().try_send(TimestampedMidiEvent::new(
            sequence,
            Instant::now(),
            device_id,
            NewMidiMessage::Midi2ControlChange {
                group_channel: GroupChannel::new(group, 0),
                controller: cc,
                value,
            },
        )));
        midi.tick().await;
    }

    #[test]
    fn midi_input_intent_changes_are_part_of_reload_snapshot() {
        let mut before = crate::reload::ScriptState::new();
        before
            .midi_input_intents
            .push(MidiInputIntent::new("gamma", "gamma"));
        let mut after = before.clone();
        after.midi_input_intents[0] = MidiInputIntent::new("gamma", "renamed-gamma");

        assert_ne!(
            MidiRouteSnapshot::from_script_state(&before),
            MidiRouteSnapshot::from_script_state(&after)
        );
    }

    #[tokio::test]
    async fn midi_input_intent_routes_channel_velocity_and_note_off() {
        let intent = MidiInputIntent::new("gamma", "gamma");
        let note_voice = VoiceId::new(11);
        let chord_voice = VoiceId::new(12);
        let state = Arc::new(RwLock::new(State::default()));
        let (runtime_tx, mut runtime_rx) = channel(8);
        let handler = MidiHandler::new(Arc::new(MockBackend::new()), state, runtime_tx);
        handler
            .add_keyboard_route(
                KeyboardRouteBuilder::new(intent.device_id)
                    .channel(1)
                    .velocity_curve_name("linear")
                    .to(note_voice),
            )
            .await;
        handler
            .add_keyboard_route(
                KeyboardRouteBuilder::new(intent.device_id)
                    .channel(2)
                    .velocity_curve_name("linear")
                    .to(chord_voice),
            )
            .await;

        assert!(handler.event_sender().try_send(TimestampedMidiEvent::new(
            1,
            Instant::now(),
            intent.device_id,
            NewMidiMessage::NoteOn {
                channel: Channel::new(0),
                note: 60,
                velocity: Velocity::from_midi1(96),
            },
        )));
        handler.tick().await;

        assert!(matches!(
            runtime_rx.try_recv(),
            Ok(Message::Voice(VoiceMessage::NoteOn {
                voice,
                note: 60,
                velocity,
            })) if voice == note_voice && (velocity - 96.0 / 127.0).abs() < f32::EPSILON
        ));
        assert!(runtime_rx.try_recv().is_err());

        assert!(handler.event_sender().try_send(TimestampedMidiEvent::new(
            2,
            Instant::now(),
            intent.device_id,
            NewMidiMessage::NoteOff {
                channel: Channel::new(0),
                note: 60,
                velocity: Velocity::from_midi1(0),
            },
        )));
        handler.tick().await;

        assert!(matches!(
            runtime_rx.try_recv(),
            Ok(Message::Voice(VoiceMessage::NoteOff { voice, note: 60 })) if voice == note_voice
        ));
        assert!(runtime_rx.try_recv().is_err());
    }

    #[test]
    fn arrival_beat_subtracts_handler_delay_when_transport_is_playing() {
        let processing_at = Instant::now();
        let received_at = processing_at - Duration::from_millis(100);

        let beat = beat_at_event_arrival(received_at, processing_at, 4.2, 120.0, true);

        assert!((beat - 4.0).abs() < 0.001, "beat was {beat}");
    }

    #[tokio::test]
    async fn timestamped_input_uses_arrival_beat_for_looper_capture() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let state = Arc::new(RwLock::new(State::default()));
        let (runtime_tx, mut runtime_rx) = channel(32);
        let handler =
            MidiHandler::new(Arc::new(MockBackend::new()), Arc::clone(&state), runtime_tx);

        handler
            .reconcile_loopers(&[LooperConfig {
                device_id,
                voice_id,
                channel: None,
                silence_bars: 0.25,
                quantize_beats: 0.0,
            }])
            .await;

        {
            let mut state = state.write().await;
            state.current_beat = Beat::from_f64(4.20);
            state.tempo = 120.0;
            state.playing = true;
        }

        let note_on_received_at = Instant::now() - Duration::from_millis(100);
        assert!(handler.event_sender().try_send(TimestampedMidiEvent::new(
            1,
            note_on_received_at,
            device_id,
            NewMidiMessage::NoteOn {
                channel: Channel::new(0),
                note: 60,
                velocity: Velocity::from_midi1(100),
            },
        )));
        handler.tick().await;

        {
            let mut state = state.write().await;
            state.current_beat = Beat::from_f64(4.55);
        }

        assert!(handler.event_sender().try_send(TimestampedMidiEvent::new(
            2,
            Instant::now(),
            device_id,
            NewMidiMessage::NoteOff {
                channel: Channel::new(0),
                note: 60,
                velocity: Velocity::ZERO,
            },
        )));
        handler.tick().await;

        {
            let mut state = state.write().await;
            state.current_beat = Beat::from_f64(5.70);
        }
        handler.tick().await;

        let mut pattern = None;
        while let Ok(msg) = runtime_rx.try_recv() {
            if let Message::Pattern(PatternMessage::Create { config, .. }) = msg {
                pattern = Some(config);
            }
        }
        let pattern = pattern.expect("looper should create a pattern");
        let gate = pattern.steps[0].params["gate"];

        assert!(
            gate > 0.45,
            "gate {gate} should include delayed note-on arrival time"
        );
        assert!(
            gate < 0.65,
            "gate {gate} should stay near the timestamp-derived duration"
        );
    }

    #[tokio::test]
    async fn ump_note_downconversion_still_feeds_legacy_looper() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let state = Arc::new(RwLock::new(State::default()));
        let (runtime_tx, mut runtime_rx) = channel(32);
        let handler =
            MidiHandler::new(Arc::new(MockBackend::new()), Arc::clone(&state), runtime_tx);
        handler
            .reconcile_loopers(&[LooperConfig {
                device_id,
                voice_id,
                channel: None,
                silence_bars: 0.25,
                quantize_beats: 0.0,
            }])
            .await;
        {
            let mut state = state.write().await;
            state.current_beat = Beat::from_f64(4.0);
            state.tempo = 120.0;
            state.playing = true;
        }

        assert!(handler.event_sender().try_send(TimestampedMidiEvent::new(
            1,
            Instant::now(),
            device_id,
            NewMidiMessage::Midi2NoteOn {
                group_channel: GroupChannel::new(2, 0),
                note: 60,
                velocity: Velocity::from_midi2(0xC000),
                attribute_type: 0,
                attribute_value: 0,
            },
        )));
        handler.tick().await;
        state.write().await.current_beat = Beat::from_f64(4.5);
        assert!(handler.event_sender().try_send(TimestampedMidiEvent::new(
            2,
            Instant::now(),
            device_id,
            NewMidiMessage::Midi2NoteOff {
                group_channel: GroupChannel::new(2, 0),
                note: 60,
                velocity: Velocity::ZERO,
                attribute_type: 0,
                attribute_value: 0,
            },
        )));
        handler.tick().await;
        state.write().await.current_beat = Beat::from_f64(6.0);
        handler.tick().await;

        let mut saw_pattern = false;
        while let Ok(message) = runtime_rx.try_recv() {
            saw_pattern |= matches!(message, Message::Pattern(PatternMessage::Create { .. }));
        }
        assert!(saw_pattern);
    }

    #[tokio::test]
    async fn advanced_cc_is_transport_transparent_for_voice_and_group_targets() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let group_id = GroupId::new(1);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;

        let backend = Arc::new(MockBackend::new());
        let voices = VoicesHandler::new(Arc::clone(&backend), Arc::clone(&state));
        create_test_voice(&voices, voice_id).await;
        let (runtime_tx, _runtime_rx) = channel(32);
        let midi = MidiHandler::new(Arc::clone(&backend), Arc::clone(&state), runtime_tx);
        let mut sequence = 1;

        for curve in ["linear", "exponential", "logarithmic", "s_curve"] {
            for value in [0, 1, 63, 64, 126, 127] {
                let voice_param = format!("voice_{curve}_{value}");
                midi.apply_advanced_cc_routes(&[advanced_cc_route(
                    device_id,
                    74,
                    None,
                    curve,
                    FadeTarget::Voice(voice_id),
                    &voice_param,
                    200.0,
                    8000.0,
                )])
                .await;

                midi.handle_message(
                    device_id,
                    MidiMessage::ControlChange {
                        channel: 0,
                        cc: 74,
                        value,
                    },
                )
                .await;
                let midi1_value = {
                    let state = state.read().await;
                    state.voices[&voice_id].config.params[&voice_param]
                };

                send_ump_cc(
                    &midi,
                    device_id,
                    sequence,
                    7,
                    74,
                    ControlValue::from_7bit(value),
                )
                .await;
                sequence += 1;
                let ump_value = {
                    let state = state.read().await;
                    state.voices[&voice_id].config.params[&voice_param]
                };
                assert_eq!(
                    midi1_value.to_bits(),
                    ump_value.to_bits(),
                    "voice {curve} value {value}"
                );

                midi.apply_advanced_cc_routes(&[advanced_cc_route(
                    device_id,
                    74,
                    None,
                    curve,
                    FadeTarget::Group(group_id),
                    "cutoff",
                    200.0,
                    8000.0,
                )])
                .await;
                backend.clear_set_param_calls();
                midi.handle_message(
                    device_id,
                    MidiMessage::ControlChange {
                        channel: 0,
                        cc: 74,
                        value,
                    },
                )
                .await;
                let midi1_calls = backend.set_param_calls();
                assert_eq!(midi1_calls.len(), 1);

                backend.clear_set_param_calls();
                send_ump_cc(
                    &midi,
                    device_id,
                    sequence,
                    7,
                    74,
                    ControlValue::from_7bit(value),
                )
                .await;
                sequence += 1;
                let ump_calls = backend.set_param_calls();
                assert_eq!(
                    ump_calls.len(),
                    1,
                    "UMP group route must apply exactly once"
                );
                assert_eq!(
                    midi1_calls[0].value.to_bits(),
                    ump_calls[0].value.to_bits(),
                    "group {curve} value {value}"
                );
            }
        }
    }

    #[tokio::test]
    async fn midi2_per_note_controller_and_pressure_keep_legacy_log_curve() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;

        let backend = Arc::new(MockBackend::new());
        let voices = VoicesHandler::new(Arc::clone(&backend), Arc::clone(&state));
        create_test_voice(&voices, voice_id).await;
        voices.note_on(voice_id, 60, 0.5).await.unwrap();

        let (runtime_tx, _runtime_rx) = channel(32);
        let midi = MidiHandler::new(Arc::clone(&backend), state, runtime_tx);
        midi.apply_midi2_per_note_routes(&[
            Midi2PerNoteRoute {
                device_id,
                group: Some(3),
                channel: Some(0),
                controller_type: Midi2PerNoteControllerType::Controller(74),
                voice: voice_id,
                param: "timbre".to_string(),
                min_value: 200.0,
                max_value: 8000.0,
                curve: "logarithmic".to_string(),
            },
            Midi2PerNoteRoute {
                device_id,
                group: Some(3),
                channel: Some(0),
                controller_type: Midi2PerNoteControllerType::Pressure,
                voice: voice_id,
                param: "pressure".to_string(),
                min_value: 200.0,
                max_value: 8000.0,
                curve: "logarithmic".to_string(),
            },
        ])
        .await;

        backend.clear_set_param_calls();
        for (sequence, controller) in [74, 0x7B].into_iter().enumerate() {
            assert!(midi.event_sender().try_send(TimestampedMidiEvent::new(
                sequence as u64,
                Instant::now(),
                device_id,
                NewMidiMessage::Midi2PerNoteController {
                    group_channel: GroupChannel::new(3, 0),
                    note: 60,
                    controller,
                    value: ControlValue::from_32bit(0x8000_0000),
                },
            )));
            midi.tick().await;
        }

        let calls = backend.set_param_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].param, "timbre");
        assert_eq!(calls[1].param, "pressure");
        for call in calls {
            assert!((call.value - 6_918.690_4).abs() < 0.001);
        }
    }

    #[tokio::test]
    async fn advanced_cc_preserves_native_ump_precision_and_exact_midi1_steps() {
        let device_id = MidiDeviceId::new(1);
        let group_id = GroupId::new(1);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;
        let backend = Arc::new(MockBackend::new());
        let (runtime_tx, _runtime_rx) = channel(32);
        let midi = MidiHandler::new(Arc::clone(&backend), state, runtime_tx);
        midi.apply_advanced_cc_routes(&[advanced_cc_route(
            device_id,
            74,
            None,
            "linear",
            FadeTarget::Group(group_id),
            "cutoff",
            0.0,
            1.0,
        )])
        .await;

        backend.clear_set_param_calls();
        let seven_bit_step = ControlValue::from_7bit(64).to_32bit();
        let next_f32_value = seven_bit_step + 256;
        for (sequence, value) in [
            ControlValue::from_32bit(0),
            ControlValue::from_32bit(seven_bit_step),
            ControlValue::from_32bit(next_f32_value),
            ControlValue::from_32bit(u32::MAX),
        ]
        .into_iter()
        .enumerate()
        {
            send_ump_cc(&midi, device_id, sequence as u64, 0, 74, value).await;
        }
        let ump_values: Vec<_> = backend
            .set_param_calls()
            .into_iter()
            .map(|call| call.value)
            .collect();
        assert_eq!(ump_values.len(), 4);
        assert_eq!(ump_values[0].to_bits(), 0.0f32.to_bits());
        assert!(ump_values[2] > ump_values[1]);
        assert!(ump_values[2] - ump_values[1] < 1.0 / 127.0);
        assert_eq!(ump_values[3].to_bits(), 1.0f32.to_bits());

        backend.clear_set_param_calls();
        for value in 0..=127 {
            midi.handle_message(
                device_id,
                MidiMessage::ControlChange {
                    channel: 0,
                    cc: 74,
                    value,
                },
            )
            .await;
        }
        let midi1_values: Vec<_> = backend
            .set_param_calls()
            .into_iter()
            .map(|call| call.value)
            .collect();
        assert_eq!(midi1_values.len(), 128);
        assert_eq!(midi1_values[0].to_bits(), 0.0f32.to_bits());
        assert_eq!(midi1_values[127].to_bits(), 1.0f32.to_bits());
        assert!(midi1_values.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[tokio::test]
    async fn advanced_cc_group_filters_only_matching_ump_groups() {
        let device_id = MidiDeviceId::new(1);
        let group_id = GroupId::new(1);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;
        let backend = Arc::new(MockBackend::new());
        let (runtime_tx, _runtime_rx) = channel(32);
        let midi = MidiHandler::new(Arc::clone(&backend), state, runtime_tx);

        midi.apply_advanced_cc_routes(&[advanced_cc_route(
            device_id,
            74,
            Some(2),
            "linear",
            FadeTarget::Group(group_id),
            "cutoff",
            0.0,
            1.0,
        )])
        .await;
        backend.clear_set_param_calls();
        midi.handle_message(
            device_id,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 74,
                value: 127,
            },
        )
        .await;
        for group in [1, 2, 3] {
            send_ump_cc(&midi, device_id, group as u64, group, 74, ControlValue::MAX).await;
        }
        assert_eq!(backend.set_param_calls().len(), 1);

        midi.apply_advanced_cc_routes(&[advanced_cc_route(
            device_id,
            74,
            None,
            "linear",
            FadeTarget::Group(group_id),
            "cutoff",
            0.0,
            1.0,
        )])
        .await;
        backend.clear_set_param_calls();
        midi.handle_message(
            device_id,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 74,
                value: 127,
            },
        )
        .await;
        send_ump_cc(&midi, device_id, 4, 7, 74, ControlValue::MAX).await;
        assert_eq!(backend.set_param_calls().len(), 2);
    }

    #[tokio::test]
    async fn group_filter_diagnostic_is_apply_time_only_and_repeats_on_reload() {
        let messages = Arc::new(StdMutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(WarningCapture {
            messages: Arc::clone(&messages),
        });
        let _guard = tracing::subscriber::set_default(subscriber);

        let device_id = MidiDeviceId::new(1);
        let group_id = GroupId::new(1);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;
        let backend = Arc::new(MockBackend::new());
        let (runtime_tx, _runtime_rx) = channel(32);
        let midi = MidiHandler::new(backend, state, runtime_tx);
        let routes = [advanced_cc_route(
            device_id,
            74,
            Some(2),
            "linear",
            FadeTarget::Group(group_id),
            "cutoff",
            0.0,
            1.0,
        )];

        midi.apply_advanced_cc_routes(&routes).await;
        midi.apply_advanced_cc_routes(&routes).await;
        midi.handle_message(
            device_id,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 74,
                value: 127,
            },
        )
        .await;

        let warnings = messages.lock().unwrap();
        assert_eq!(
            warnings
                .iter()
                .filter(|message| message.contains("the route will never fire"))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn cc_curve_diagnostics_are_apply_time_only_with_finite_linear_fallbacks() {
        let messages = Arc::new(StdMutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(WarningCapture {
            messages: Arc::clone(&messages),
        });
        let _guard = tracing::subscriber::set_default(subscriber);

        let device_id = MidiDeviceId::new(1);
        let group_id = GroupId::new(1);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;
        let backend = Arc::new(MockBackend::new());
        let (runtime_tx, _runtime_rx) = channel(32);
        let midi = MidiHandler::new(Arc::clone(&backend), state, runtime_tx);
        midi.apply_advanced_cc_routes(&[
            advanced_cc_route(
                device_id,
                74,
                None,
                "unknown",
                FadeTarget::Group(group_id),
                "unknown_curve",
                0.0,
                1.0,
            ),
            advanced_cc_route(
                device_id,
                75,
                None,
                "logarithmic",
                FadeTarget::Group(group_id),
                "invalid_log",
                0.0,
                1.0,
            ),
        ])
        .await;

        for cc in [74, 75] {
            midi.handle_message(
                device_id,
                MidiMessage::ControlChange {
                    channel: 0,
                    cc,
                    value: 64,
                },
            )
            .await;
        }

        let calls = backend.set_param_calls();
        assert_eq!(calls.len(), 2);
        let expected = ControlValue::from_7bit(64).as_f32();
        assert!(calls
            .iter()
            .all(|call| call.value.to_bits() == expected.to_bits() && call.value.is_finite()));

        let warnings = messages.lock().unwrap();
        assert_eq!(
            warnings
                .iter()
                .filter(|message| message.contains("Unknown MIDI CC curve"))
                .count(),
            1
        );
        assert_eq!(
            warnings
                .iter()
                .filter(|message| message.contains("is not positive"))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn ump_cc_routes_once_while_legacy_callback_and_recording_are_preserved() {
        let device_id = MidiDeviceId::new(1);
        let group_id = GroupId::new(1);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;
        let backend = Arc::new(MockBackend::new());
        let (runtime_tx, _runtime_rx) = channel(32);
        let midi = MidiHandler::new(Arc::clone(&backend), state, runtime_tx);
        midi.apply_advanced_cc_routes(&[advanced_cc_route(
            device_id,
            74,
            None,
            "linear",
            FadeTarget::Group(group_id),
            "cutoff",
            0.0,
            1.0,
        )])
        .await;
        midi.register_callback(
            device_id,
            CallbackType::ControlChange(74),
            None,
            CallbackData::External,
        )
        .await;
        midi.recordings()
            .write()
            .await
            .insert(device_id, MidiRecording::new(device_id, Beat::ZERO));

        backend.clear_set_param_calls();
        send_ump_cc(&midi, device_id, 1, 3, 74, ControlValue::from_7bit(96)).await;
        assert_eq!(backend.set_param_calls().len(), 1);

        let notifications = midi.callback_manager.poll_callbacks();
        assert_eq!(notifications.len(), 1);
        assert!(matches!(
            notifications[0].message,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 74,
                value: 96
            }
        ));
        let recordings = midi.recordings();
        let recordings = recordings.read().await;
        let events = &recordings[&device_id].cc_events;
        assert_eq!(events.len(), 1);
        assert_eq!(
            (events[0].cc, events[0].value, events[0].channel),
            (74, 96, 0)
        );
    }

    #[tokio::test]
    async fn advanced_cc_reload_replaces_routes_without_reset_and_last_voice_route_wins() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;
        let backend = Arc::new(MockBackend::new());
        let voices = VoicesHandler::new(Arc::clone(&backend), Arc::clone(&state));
        create_test_voice(&voices, voice_id).await;
        let (runtime_tx, _runtime_rx) = channel(32);
        let midi = MidiHandler::new(backend, Arc::clone(&state), runtime_tx);

        midi.apply_advanced_cc_routes(&[
            advanced_cc_route(
                device_id,
                74,
                None,
                "linear",
                FadeTarget::Voice(voice_id),
                "cutoff",
                0.0,
                1.0,
            ),
            advanced_cc_route(
                device_id,
                74,
                None,
                "linear",
                FadeTarget::Voice(voice_id),
                "cutoff",
                0.0,
                2.0,
            ),
        ])
        .await;
        midi.handle_message(
            device_id,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 74,
                value: 127,
            },
        )
        .await;
        assert_eq!(
            state.read().await.voices[&voice_id].config.params["cutoff"],
            2.0
        );

        midi.apply_advanced_cc_routes(&[advanced_cc_route(
            device_id,
            75,
            None,
            "linear",
            FadeTarget::Voice(voice_id),
            "cutoff",
            10.0,
            20.0,
        )])
        .await;
        assert_eq!(
            state.read().await.voices[&voice_id].config.params["cutoff"],
            2.0
        );
        midi.handle_message(
            device_id,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 74,
                value: 0,
            },
        )
        .await;
        assert_eq!(
            state.read().await.voices[&voice_id].config.params["cutoff"],
            2.0
        );
        midi.handle_message(
            device_id,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 75,
                value: 0,
            },
        )
        .await;
        assert_eq!(
            state.read().await.voices[&voice_id].config.params["cutoff"],
            10.0
        );
    }

    #[cfg(not(feature = "pipewire-midi2"))]
    #[tokio::test]
    async fn unified_cc_route_stays_reachable_without_ump_transport() {
        let device_id = MidiDeviceId::new(1);
        let group_id = GroupId::new(1);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;
        let backend = Arc::new(MockBackend::new());
        let (runtime_tx, _runtime_rx) = channel(32);
        let midi = MidiHandler::new(Arc::clone(&backend), state, runtime_tx);
        midi.apply_advanced_cc_routes(&[advanced_cc_route(
            device_id,
            74,
            None,
            "linear",
            FadeTarget::Group(group_id),
            "cutoff",
            0.0,
            1.0,
        )])
        .await;
        midi.handle_message(
            device_id,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 74,
                value: 127,
            },
        )
        .await;
        assert_eq!(backend.set_param_calls().len(), 1);
    }

    #[tokio::test]
    async fn cc_voice_route_updates_note_node_and_future_note_defaults() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;

        let backend = Arc::new(MockBackend::new());
        let voices = VoicesHandler::new(Arc::clone(&backend), Arc::clone(&state));
        create_test_voice(&voices, voice_id).await;
        voices.note_on(voice_id, 60, 0.75).await.unwrap();

        let note_node = {
            let state = state.read().await;
            *state
                .voices
                .get(&voice_id)
                .unwrap()
                .note_nodes
                .get(&60)
                .expect("note_on should create a note node")
        };

        let (runtime_tx, mut runtime_rx) = channel(32);
        let midi = MidiHandler::new(Arc::clone(&backend), Arc::clone(&state), runtime_tx);
        midi.add_cc_route(
            CcRouteBuilder::new(device_id, 70).to_voice(voice_id, "cutoff", 100.0, 2000.0),
        )
        .await;

        midi.handle_message(
            device_id,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 70,
                value: 127,
            },
        )
        .await;

        assert!(
            runtime_rx.try_recv().is_err(),
            "voice CC should not enqueue SetParam into the runtime channel"
        );

        let calls = backend.set_param_calls();
        assert!(
            calls.iter().any(|call| {
                call.node == note_node && call.param == "cutoff" && call.value == 2000.0
            }),
            "CC SetParam should update held note node {note_node:?}; calls={calls:?}"
        );

        {
            let state = state.read().await;
            let voice = state.voices.get(&voice_id).unwrap();
            assert_eq!(voice.config.params.get("cutoff"), Some(&2000.0));
        }

        voices.note_on(voice_id, 61, 0.5).await.unwrap();
        let create_params = backend.synth_create_params();
        let next_note_params = create_params
            .last()
            .expect("next note should create a synth");
        assert_eq!(next_note_params.get("cutoff"), Some(&2000.0));
    }

    #[tokio::test]
    async fn dense_voice_cc_tick_does_not_block_concurrent_note_on() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;

        let backend = Arc::new(MockBackend::new());
        let voices = VoicesHandler::new(Arc::clone(&backend), Arc::clone(&state));
        create_test_voice(&voices, voice_id).await;

        let (runtime_tx, mut runtime_rx) = channel(2);
        let midi = MidiHandler::new(Arc::clone(&backend), Arc::clone(&state), runtime_tx);
        midi.add_cc_route(
            CcRouteBuilder::new(device_id, 70).to_voice(voice_id, "cutoff", 100.0, 2000.0),
        )
        .await;
        midi.add_note_route(NoteRouteBuilder::new(device_id, 36).to(voice_id))
            .await;

        for sequence in 1..=64 {
            assert!(midi.event_sender().try_send(TimestampedMidiEvent::new(
                sequence,
                Instant::now(),
                device_id,
                NewMidiMessage::ControlChange {
                    channel: Channel::new(0),
                    controller: 70,
                    value: 127,
                },
            )));
        }
        assert!(midi.event_sender().try_send(TimestampedMidiEvent::new(
            65,
            Instant::now(),
            device_id,
            NewMidiMessage::NoteOn {
                channel: Channel::new(0),
                note: 36,
                velocity: Velocity::from_midi1(100),
            },
        )));

        timeout(Duration::from_millis(100), midi.tick())
            .await
            .expect("dense voice CC input must not deadlock the MIDI tick");

        let msg = runtime_rx
            .try_recv()
            .expect("NoteOn should still reach the runtime queue");
        match msg {
            Message::Voice(VoiceMessage::NoteOn {
                voice,
                note,
                velocity,
            }) => {
                assert_eq!(voice, voice_id);
                assert_eq!(note, 36);
                assert!((velocity - (100.0 / 127.0)).abs() < f32::EPSILON);
                voices.note_on(voice, note, velocity).await.unwrap();
            }
            other => panic!("expected Voice::NoteOn, got {other:?}"),
        }

        assert!(
            runtime_rx.try_recv().is_err(),
            "voice CC flood should not leave queued SetParam messages behind"
        );

        {
            let state = state.read().await;
            let voice = state.voices.get(&voice_id).unwrap();
            assert_eq!(voice.config.params.get("cutoff"), Some(&2000.0));
            assert!(
                voice.note_nodes.contains_key(&36),
                "queued NoteOn should be handleable after the CC flood"
            );
        }

        let create_params = backend.synth_create_params();
        let note_params = create_params.last().expect("NoteOn should create a synth");
        assert_eq!(note_params.get("cutoff"), Some(&2000.0));
    }

    #[tokio::test]
    async fn note_flood_against_full_runtime_channel_does_not_block_or_lose_note_offs() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;

        let backend = Arc::new(MockBackend::new());
        let voices = VoicesHandler::new(Arc::clone(&backend), Arc::clone(&state));
        create_test_voice(&voices, voice_id).await;

        // Tiny runtime channel: fills after two messages, like a busy runtime.
        let (runtime_tx, mut runtime_rx) = channel(2);
        let midi = MidiHandler::new(Arc::clone(&backend), Arc::clone(&state), runtime_tx);
        midi.add_note_route(NoteRouteBuilder::new(device_id, 36).to(voice_id))
            .await;

        const PAIRS: usize = 32;
        let mut sequence = 0;
        for _ in 0..PAIRS {
            sequence += 1;
            assert!(midi.event_sender().try_send(TimestampedMidiEvent::new(
                sequence,
                Instant::now(),
                device_id,
                NewMidiMessage::NoteOn {
                    channel: Channel::new(0),
                    note: 36,
                    velocity: Velocity::from_midi1(100),
                },
            )));
            sequence += 1;
            assert!(midi.event_sender().try_send(TimestampedMidiEvent::new(
                sequence,
                Instant::now(),
                device_id,
                NewMidiMessage::NoteOff {
                    channel: Channel::new(0),
                    note: 36,
                    velocity: Velocity::ZERO,
                },
            )));
        }

        // The runtime channel is full after two messages; the tick must park
        // the rest instead of awaiting channel capacity (self-backpressure
        // deadlock).
        timeout(Duration::from_millis(100), midi.tick())
            .await
            .expect("note flood against a full runtime channel must not block the MIDI tick");

        // Drain the runtime channel tick-by-tick, as the real runtime would.
        let mut received = Vec::new();
        for _ in 0..1000 {
            while let Ok(msg) = runtime_rx.try_recv() {
                received.push(msg);
            }
            if received.len() == PAIRS * 2 {
                break;
            }
            timeout(Duration::from_millis(100), midi.tick())
                .await
                .expect("draining parked runtime messages must not block the MIDI tick");
        }

        assert_eq!(
            received.len(),
            PAIRS * 2,
            "all NoteOn/NoteOff messages must reach the runtime once the channel drains"
        );

        let mut ons = 0;
        let mut offs = 0;
        for (i, msg) in received.iter().enumerate() {
            match msg {
                Message::Voice(VoiceMessage::NoteOn { voice, note, .. }) => {
                    assert_eq!(*voice, voice_id);
                    assert_eq!(*note, 36);
                    assert_eq!(i % 2, 0, "NoteOn overtaken by a NoteOff at index {i}");
                    ons += 1;
                }
                Message::Voice(VoiceMessage::NoteOff { voice, note }) => {
                    assert_eq!(*voice, voice_id);
                    assert_eq!(*note, 36);
                    assert_eq!(i % 2, 1, "NoteOff overtook its NoteOn at index {i}");
                    offs += 1;
                }
                other => panic!("expected only NoteOn/NoteOff, got {other:?}"),
            }
        }
        assert_eq!(ons, PAIRS, "no NoteOn may be lost");
        assert_eq!(offs, PAIRS, "no NoteOff may be lost");
    }

    #[tokio::test]
    async fn cc_voice_route_matches_no_channel_and_channel_one() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;

        let backend = Arc::new(MockBackend::new());
        let voices = VoicesHandler::new(Arc::clone(&backend), Arc::clone(&state));
        create_test_voice(&voices, voice_id).await;

        let (runtime_tx, mut runtime_rx) = channel(32);
        let midi = MidiHandler::new(backend, state, runtime_tx);
        midi.add_cc_route(CcRouteBuilder::new(device_id, 71).to_voice(
            voice_id,
            "no_channel",
            0.0,
            1.0,
        ))
        .await;
        midi.add_cc_route(CcRouteBuilder::new(device_id, 71).channel(1).to_voice(
            voice_id,
            "channel_one",
            0.0,
            1.0,
        ))
        .await;

        midi.handle_message(
            device_id,
            MidiMessage::ControlChange {
                channel: 0,
                cc: 71,
                value: 127,
            },
        )
        .await;

        assert!(
            runtime_rx.try_recv().is_err(),
            "voice CC should not enqueue SetParam into the runtime channel"
        );

        let params = {
            let state = midi.state.read().await;
            state.voices.get(&voice_id).unwrap().config.params.clone()
        };

        assert!(
            params.contains_key("no_channel"),
            "route without a channel filter should match channel 0"
        );
        assert!(
            params.contains_key("channel_one"),
            ".channel(1) should match internal MIDI channel 0"
        );
    }

    #[tokio::test]
    async fn pitch_bend_voice_route_maps_signed_range_to_param_range() {
        let device_id = MidiDeviceId::new(1);
        let voice_id = VoiceId::new(7);
        let state = Arc::new(RwLock::new(State::default()));
        setup_voice_state(&state).await;

        let backend = Arc::new(MockBackend::new());
        let voices = VoicesHandler::new(Arc::clone(&backend), Arc::clone(&state));
        create_test_voice(&voices, voice_id).await;

        let (runtime_tx, mut runtime_rx) = channel(32);
        let midi = MidiHandler::new(backend, state, runtime_tx);
        midi.apply_advanced_bend_routes(&[AdvancedMidiBendRoute {
            device_id,
            channel: Some(0),
            curve: "linear".to_string(),
            target: FadeTarget::Voice(voice_id),
            param: "morph".to_string(),
            min: 0.0,
            max: 1.0,
        }])
        .await;

        for (raw, expected) in [(-8192, 0.0), (0, 8192.0 / 16383.0), (8191, 1.0)] {
            midi.handle_message(
                device_id,
                MidiMessage::PitchBend {
                    channel: 0,
                    value: raw,
                },
            )
            .await;

            let value = {
                let state = midi.state.read().await;
                *state
                    .voices
                    .get(&voice_id)
                    .unwrap()
                    .config
                    .params
                    .get("morph")
                    .expect("pitch bend should update morph")
            };

            assert!(
                (value - expected).abs() < 0.0001,
                "raw bend {raw} should map to {expected}, got {value}"
            );
        }

        assert!(
            runtime_rx.try_recv().is_err(),
            "voice bend mapping should not enqueue SetParam into the runtime channel"
        );
    }
}

// Include the Midi trait implementation in a separate file to keep mod.rs manageable
mod trait_impl;
