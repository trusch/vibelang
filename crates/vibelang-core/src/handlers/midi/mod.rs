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

pub use types::{map_to_range, Midi2ControllerType};

use crate::backend::Backend;
use crate::compat::RwLock;
use crate::midi::{
    CallbackData, CallbackType, CcRouteBuilder, JitterCompensator, KeyboardRouteBuilder, MidiClock,
    MidiEventQueue, MidiEventSender, MidiMessage as NewMidiMessage, MidiRealtimeService,
    MidiRecording, NoteRouteBuilder, ScheduledMidiEvent,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::transport_snapshot::TransportSnapshot;

use crate::compat::SenderExt;
use crate::message::{Message, PatternMessage, VoiceMessage};
use crate::midi::PerNoteStateManager;
use crate::midi::{LooperAction, LooperManager};
use crate::reload::LooperConfig;
use crate::state::State;
use crate::traits::{FadeTarget, MidiOutputCapability};
use crate::types::ids::MidiDeviceId;
use crate::types::{NodeId, VoiceId};
use crate::{Error, Result};
use crossbeam_channel::Sender;
use midir::{MidiInputConnection, MidiOutputConnection};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

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

    /// Open output connections (uses std::sync::Mutex because MidiOutputConnection is !Send).
    outputs: Arc<Mutex<HashMap<MidiDeviceId, MidiOutputConnection>>>,

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

    /// Jitter compensator for stable timing.
    /// Reserved for advanced MIDI timing compensation - not yet integrated into event processing.
    #[allow(dead_code)]
    jitter_compensator: Arc<parking_lot::RwLock<JitterCompensator>>,

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

    /// Looper manager — one instance per configured MIDI device.
    looper_manager: Mutex<LooperManager>,
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
        let jitter_compensator = Arc::new(parking_lot::RwLock::new(JitterCompensator::new()));

        Self {
            backend,
            state,
            inputs: Arc::new(Mutex::new(HashMap::new())),
            outputs: Arc::new(Mutex::new(HashMap::new())),
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
            jitter_compensator,
            // MIDI 2.0
            capability_cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),

            per_note_state: Arc::new(RwLock::new(PerNoteStateManager::new())),

            // Realtime service (not started yet - call start_realtime_service())
            realtime_service: Arc::new(parking_lot::RwLock::new(MidiRealtimeService::new())),

            // Clock thread (not started yet - call start_clock_thread())
            #[cfg(not(target_arch = "wasm32"))]
            clock_thread: Arc::new(parking_lot::RwLock::new(None)),

            runtime_tx,

            looper_manager: Mutex::new(LooperManager::new()),
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
            self.output_manager.output_channels.clone(),
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

    pub async fn apply_midi2_cc_routes(&self, routes: &[crate::reload::Midi2CcRoute]) {
        self.routing_manager.apply_midi2_cc_routes(routes).await
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
            self.handle_message(device_id, msg).await;
        }

        // Process MIDI 2.0 messages from the event queue

        self.process_midi2_events().await;

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
        self.dispatch_looper_actions(looper_actions).await;
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
        self.dispatch_looper_actions(actions).await;
    }

    /// Dispatch a batch of looper actions to the runtime.
    async fn dispatch_looper_actions(&self, actions: Vec<LooperAction>) {
        for action in actions {
            match action {
                LooperAction::NoteOn {
                    voice_id,
                    note,
                    velocity,
                } => {
                    if let Err(e) = self
                        .runtime_tx
                        .send_async(Message::Voice(VoiceMessage::NoteOn {
                            voice: voice_id,
                            note,
                            velocity: velocity as f32 / 127.0,
                        }))
                        .await
                    {
                        tracing::warn!("Looper: failed to send NoteOn: {}", e);
                    }
                }
                LooperAction::NoteOff { voice_id, note } => {
                    if let Err(e) = self
                        .runtime_tx
                        .send_async(Message::Voice(VoiceMessage::NoteOff {
                            voice: voice_id,
                            note,
                        }))
                        .await
                    {
                        tracing::warn!("Looper: failed to send NoteOff: {}", e);
                    }
                }
                LooperAction::StopPattern { pattern_id } => {
                    let _ = self
                        .runtime_tx
                        .send_async(Message::Pattern(PatternMessage::Stop { id: pattern_id }))
                        .await;
                    let _ = self
                        .runtime_tx
                        .send_async(Message::Pattern(PatternMessage::Delete { id: pattern_id }))
                        .await;
                }
                LooperAction::StartPattern { config, pattern_id } => {
                    let _ = self
                        .runtime_tx
                        .send_async(Message::Pattern(PatternMessage::Create {
                            id: pattern_id,
                            config,
                        }))
                        .await;
                    let _ = self
                        .runtime_tx
                        .send_async(Message::Pattern(PatternMessage::Start { id: pattern_id }))
                        .await;
                }
            }
        }
    }

    /// Process MIDI 2.0 events from the event queue.
    async fn process_midi2_events(&self) {
        // Drain events from the queue
        let events = self.event_queue.drain();

        for event in events {
            // Only process MIDI 2.0 specific messages that weren't converted to legacy
            if event.message.is_midi2() {
                self.handle_midi2_message(event.device_id, &event.message)
                    .await;
            }
        }
    }

    /// Handle a single MIDI message.
    async fn handle_message(&self, device_id: MidiDeviceId, msg: MidiMessage) {
        // First, invoke callbacks
        self.callback_manager
            .invoke_callbacks(device_id, &msg)
            .await;

        // Record events if recording is active for this device
        self.recording_manager
            .record_message(device_id, &msg, &self.state)
            .await;

        // Then process routing
        let routing_arc = self.routing_manager.routing();
        let routing = routing_arc.read().await;

        match &msg {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => {
                self.handle_note_on(&routing, device_id, *channel, *note, *velocity)
                    .await;
            }
            MidiMessage::NoteOff { channel, note } => {
                self.handle_note_off(&routing, device_id, *channel, *note)
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

                let advanced_routes: Vec<_> = routing
                    .advanced_cc_routes
                    .iter()
                    .filter(|r| {
                        r.device_id == device_id
                            && r.cc == *cc
                            && (r.channel.is_none() || r.channel == Some(*channel))
                    })
                    .cloned()
                    .collect();

                drop(routing);

                self.handle_cc(basic_routes, advanced_routes, *cc, *value)
                    .await;
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

                drop(routing);

                self.handle_pitch_bend(basic_routes, advanced_routes, *value)
                    .await;
            }
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
            self.handle_note_off(routing, device_id, channel, note)
                .await;
            return;
        }

        // If a looper is configured for this device, route exclusively through it.
        if self
            .looper_manager
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .has_device(device_id)
        {
            let (current_beat, time_sig_num) = {
                let state = self.state.read().await;
                (state.current_beat.to_f64(), state.time_sig.numerator)
            };
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
                    current_beat,
                    time_sig_num,
                )
            };
            self.dispatch_looper_actions(actions).await;
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
                if let Err(e) = self
                    .runtime_tx
                    .send_async(Message::Voice(VoiceMessage::NoteOn {
                        voice: route.voice_id,
                        note,
                        velocity: vel_f32,
                    }))
                    .await
                {
                    tracing::warn!("Failed to send MIDI note_on to runtime: {}", e);
                }
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
                    if let Err(e) = self
                        .runtime_tx
                        .send_async(Message::Voice(VoiceMessage::NoteOn {
                            voice: voice_id,
                            note: transposed_note,
                            velocity: vel,
                        }))
                        .await
                    {
                        tracing::warn!("Failed to send MIDI advanced note_on to runtime: {}", e);
                    }
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
                        if let Err(e) = self
                            .runtime_tx
                            .send_async(Message::Voice(VoiceMessage::SetParam {
                                id: voice_id,
                                param: param.to_string(),
                                value: value as f32,
                            }))
                            .await
                        {
                            tracing::warn!("Failed to send MIDI velocity param to runtime: {}", e);
                        }
                    }

                    if let Err(e) = self
                        .runtime_tx
                        .send_async(Message::Voice(VoiceMessage::NoteOn {
                            voice: voice_id,
                            note,
                            velocity: vel_curved,
                        }))
                        .await
                    {
                        tracing::warn!("Failed to send MIDI note route on to runtime: {}", e);
                    }
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
    ) {
        // If a looper is configured for this device, route exclusively through it.
        if self
            .looper_manager
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .has_device(device_id)
        {
            let current_beat = {
                let state = self.state.read().await;
                state.current_beat.to_f64()
            };
            let actions = {
                let mut mgr = self
                    .looper_manager
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                mgr.handle_note_off(device_id, channel, note, current_beat)
            };
            self.dispatch_looper_actions(actions).await;
            return;
        }

        // Process basic keyboard routes
        for route in &routing.keyboard_routes {
            if route.device_id == device_id
                && (route.channel.is_none() || route.channel == Some(channel))
            {
                tracing::debug!("MIDI note_off: voice={}, note={}", route.voice_id.0, note);
                if let Err(e) = self
                    .runtime_tx
                    .send_async(Message::Voice(VoiceMessage::NoteOff {
                        voice: route.voice_id,
                        note,
                    }))
                    .await
                {
                    tracing::warn!("Failed to send MIDI note_off to runtime: {}", e);
                }
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
                    if let Err(e) = self
                        .runtime_tx
                        .send_async(Message::Voice(VoiceMessage::NoteOff {
                            voice: voice_id,
                            note: transposed_note,
                        }))
                        .await
                    {
                        tracing::warn!("Failed to send MIDI advanced note_off to runtime: {}", e);
                    }
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
                    if let Err(e) = self
                        .runtime_tx
                        .send_async(Message::Voice(VoiceMessage::NoteOff {
                            voice: voice_id,
                            note,
                        }))
                        .await
                    {
                        tracing::warn!("Failed to send MIDI note route off to runtime: {}", e);
                    }
                }
            }
        }
    }

    async fn handle_cc(
        &self,
        basic_routes: Vec<CcRoute>,
        advanced_routes: Vec<CcRouteBuilder>,
        _cc: u8,
        value: u8,
    ) {
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

        // Process advanced CC routes
        for route in advanced_routes {
            let param_value = route.cc_to_param(value);

            if let (Some(target), Some(param)) = (route.target.as_ref(), &route.target_param) {
                tracing::debug!(
                    "MIDI advanced CC: target={:?}, param={}, value={}",
                    target,
                    param,
                    param_value
                );

                if let Err(e) = self.apply_cc_to_target(target, param, param_value).await {
                    tracing::warn!("Failed to apply advanced CC to {:?}: {}", target, e);
                }
            }
        }
    }

    async fn handle_pitch_bend(
        &self,
        basic_routes: Vec<KeyboardRoute>,
        advanced_routes: Vec<KeyboardRouteBuilder>,
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
    }

    /// Apply a CC value to a target's parameter.
    async fn apply_cc_to_target(&self, target: &FadeTarget, param: &str, value: f32) -> Result<()> {
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
                FadeTarget::Voice(id) => {
                    if let Some(voice) = state.voices.get(id) {
                        voice.active_nodes.clone()
                    } else {
                        vec![]
                    }
                }
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
                self.handle_midi2_cc(device_id, *group_channel, *controller, *value)
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

                let param_value = map_to_range(
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
                let param_value = map_to_range(
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
            if let Err(e) = self
                .runtime_tx
                .send_async(Message::Voice(VoiceMessage::NoteOn {
                    voice: route.voice_id,
                    note: transposed_note,
                    velocity: velocity.as_f32(),
                }))
                .await
            {
                tracing::warn!("Failed to send MIDI 2.0 note_on to runtime: {}", e);
            }
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
            if let Err(e) = self
                .runtime_tx
                .send_async(Message::Voice(VoiceMessage::NoteOff {
                    voice: route.voice_id,
                    note: transposed_note,
                }))
                .await
            {
                tracing::warn!("Failed to send MIDI 2.0 note_off to runtime: {}", e);
            }
        }
    }

    async fn handle_midi2_cc(
        &self,
        device_id: MidiDeviceId,
        group_channel: crate::midi::GroupChannel,
        controller: u8,
        value: crate::midi::ControlValue,
    ) {
        let routes: Vec<_> = {
            let routing_arc = self.routing_manager.routing();
            let routing = routing_arc.read().await;
            routing
                .midi2_cc_routes
                .iter()
                .filter(|r| {
                    r.device_id == device_id
                        && r.cc == controller
                        && r.group.is_none_or(|g| g == group_channel.group())
                        && r.channel.is_none_or(|c| c == group_channel.channel())
                })
                .cloned()
                .collect()
        };

        for route in routes {
            let normalized = value.as_f32();
            let param_value = map_to_range(
                normalized,
                0.0,
                1.0,
                route.min_value,
                route.max_value,
                &route.curve,
            );

            tracing::debug!(
                "MIDI 2.0 CC: voice={}, cc={}, param={}={:.4}",
                route.voice_id.0,
                controller,
                route.param,
                param_value
            );

            let target = FadeTarget::Voice(route.voice_id);
            if let Err(e) = self
                .apply_cc_to_target(&target, &route.param, param_value)
                .await
            {
                tracing::warn!(
                    "Failed to apply MIDI 2.0 CC to voice {}: {}",
                    route.voice_id.0,
                    e
                );
            }
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

// Include the Midi trait implementation in a separate file to keep mod.rs manageable
mod trait_impl;
