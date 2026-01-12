//! MIDI handler implementation.
//!
//! This module provides MIDI device enumeration, input/output handling,
//! and routing of MIDI messages to voices and parameters.
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

use crate::backend::Backend;
use crate::compat::RwLock;
use crate::midi::{
    parse_midi_bytes as new_parse_midi_bytes,
    CallbackData,
    CallbackType,
    CcRouteBuilder,
    JitterCompensator,
    KeyboardRouteBuilder,
    MidiCallbacks,
    MidiClock,
    // New infrastructure
    MidiEventQueue,
    MidiEventSender,
    MidiMessage as NewMidiMessage,
    // Recording
    MidiRecording,
    MidiRecordingInfo,
    NoteRouteBuilder,
    QueuedMidiEvent,
    TimestampedMidiEvent,
};
use crate::state::State;
use crate::traits::{FadeTarget, Midi, MidiDeviceInfo};
use crate::types::ids::MidiDeviceId;
use crate::types::VoiceId;
use crate::{Error, Result};
use async_trait::async_trait;
use crossbeam_channel::{self, Receiver, Sender};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tokio::sync::mpsc;

/// A parsed MIDI message.
#[derive(Clone, Debug)]
#[allow(dead_code)] // All fields intentionally kept for future routing features
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    ControlChange {
        channel: u8,
        cc: u8,
        value: u8,
    },
    PitchBend {
        channel: u8,
        value: i16,
    },
    /// MIDI Clock (0xF8) - 24 pulses per quarter note.
    Clock,
    /// MIDI Start (0xFA) - Start playback from beginning.
    Start,
    /// MIDI Stop (0xFC) - Stop playback.
    Stop,
    /// MIDI Continue (0xFB) - Continue playback from current position.
    Continue,
}

/// Convert new infrastructure MidiMessage to legacy MidiMessage.
///
/// Returns None for message types that the legacy system doesn't support.
fn convert_new_to_legacy_message(msg: &NewMidiMessage) -> Option<MidiMessage> {
    match msg {
        NewMidiMessage::NoteOn {
            channel,
            note,
            velocity,
        } => {
            let vel = velocity.to_midi1();
            if vel > 0 {
                Some(MidiMessage::NoteOn {
                    channel: channel.get(),
                    note: *note,
                    velocity: vel,
                })
            } else {
                Some(MidiMessage::NoteOff {
                    channel: channel.get(),
                    note: *note,
                })
            }
        }
        NewMidiMessage::NoteOff { channel, note, .. } => Some(MidiMessage::NoteOff {
            channel: channel.get(),
            note: *note,
        }),
        NewMidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => Some(MidiMessage::ControlChange {
            channel: channel.get(),
            cc: *controller,
            value: *value,
        }),
        NewMidiMessage::PitchBend { channel, value } => Some(MidiMessage::PitchBend {
            channel: channel.get(),
            value: *value,
        }),
        NewMidiMessage::Clock => Some(MidiMessage::Clock),
        NewMidiMessage::Start => Some(MidiMessage::Start),
        NewMidiMessage::Continue => Some(MidiMessage::Continue),
        NewMidiMessage::Stop => Some(MidiMessage::Stop),
        // New message types not supported by legacy system
        NewMidiMessage::PolyAftertouch { .. }
        | NewMidiMessage::ProgramChange { .. }
        | NewMidiMessage::ChannelAftertouch { .. }
        | NewMidiMessage::SysEx(_)
        | NewMidiMessage::TimeCode { .. }
        | NewMidiMessage::SongPosition { .. }
        | NewMidiMessage::SongSelect { .. }
        | NewMidiMessage::TuneRequest
        | NewMidiMessage::ActiveSensing
        | NewMidiMessage::Reset => None,
    }
}

/// Keyboard routing configuration.
#[derive(Clone, Debug)]
struct KeyboardRoute {
    device_id: MidiDeviceId,
    channel: Option<u8>, // None = all channels
    voice_id: VoiceId,
}

/// CC routing configuration.
#[derive(Clone, Debug)]
struct CcRoute {
    device_id: MidiDeviceId,
    cc: u8,
    target: FadeTarget,
    param: String,
    /// Min value when CC is 0.
    min_value: f32,
    /// Max value when CC is 127.
    max_value: f32,
}

/// Internal routing state.
#[derive(Default)]
struct MidiRouting {
    keyboard_routes: Vec<KeyboardRoute>,
    cc_routes: Vec<CcRoute>,
    /// Advanced keyboard routes with full configuration.
    advanced_keyboard_routes: Vec<KeyboardRouteBuilder>,
    /// Advanced note routes (for drums/pads).
    note_routes: Vec<NoteRouteBuilder>,
    /// Advanced CC routes with curves.
    advanced_cc_routes: Vec<CcRouteBuilder>,
    /// Choke group tracking: group name -> active node IDs.
    choke_groups: HashMap<String, Vec<crate::types::NodeId>>,
}

/// State for a background MIDI output thread.
struct OutputThreadState {
    /// Thread handle.
    handle: JoinHandle<()>,
    /// Running flag.
    running: Arc<AtomicBool>,
}

/// A MIDI event notification sent to registered callbacks.
#[derive(Clone, Debug)]
pub struct MidiEventNotification {
    /// Callback ID that should handle this event.
    pub callback_id: u64,
    /// Device the event came from.
    pub device_id: MidiDeviceId,
    /// The MIDI message.
    pub message: MidiMessage,
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

    /// Open output connections (uses std::sync::Mutex because MidiOutputConnection is !Send).
    outputs: Arc<Mutex<HashMap<MidiDeviceId, MidiOutputConnection>>>,

    /// Incoming MIDI message channel (legacy).
    rx: Arc<Mutex<mpsc::Receiver<(MidiDeviceId, MidiMessage)>>>,

    /// Sender for incoming MIDI messages (legacy).
    tx: mpsc::Sender<(MidiDeviceId, MidiMessage)>,

    /// Routing configuration.
    routing: Arc<RwLock<MidiRouting>>,

    /// Output channel senders (for realtime service integration).
    /// Maps device ID to the sender channel.
    output_channels: Arc<Mutex<HashMap<MidiDeviceId, Sender<QueuedMidiEvent>>>>,

    /// Background output threads (for realtime service integration).
    output_threads: Arc<Mutex<HashMap<MidiDeviceId, OutputThreadState>>>,

    /// Callback storage.
    callbacks: Arc<RwLock<MidiCallbacks>>,

    /// Channel for sending callback notifications to the Rhai layer.
    callback_tx: mpsc::Sender<MidiEventNotification>,

    /// Receiver for callback notifications (to be polled by the Rhai layer).
    callback_rx: Arc<Mutex<mpsc::Receiver<MidiEventNotification>>>,

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
    // Recording
    // ========================================================================
    /// Active recordings by device ID.
    recordings: Arc<RwLock<HashMap<MidiDeviceId, MidiRecording>>>,

    // ========================================================================
    // Clock Output
    // ========================================================================
    /// Devices with clock output enabled.
    clock_output_devices: Arc<RwLock<std::collections::HashSet<MidiDeviceId>>>,

    /// Last beat position at which we sent a MIDI clock tick.
    /// Used to calculate how many clock ticks to send on each tick.
    last_clock_beat: Arc<RwLock<f64>>,
}

impl<B: Backend> MidiHandler<B> {
    /// Create a new MIDI handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        let (callback_tx, callback_rx) = mpsc::channel(1024);

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
            routing: Arc::new(RwLock::new(MidiRouting::default())),
            output_channels: Arc::new(Mutex::new(HashMap::new())),
            output_threads: Arc::new(Mutex::new(HashMap::new())),
            callbacks: Arc::new(RwLock::new(MidiCallbacks::new())),
            callback_tx,
            callback_rx: Arc::new(Mutex::new(callback_rx)),
            // New infrastructure
            event_queue,
            midi_clock,
            jitter_compensator,
            // Recording
            recordings: Arc::new(RwLock::new(HashMap::new())),
            // Clock output
            clock_output_devices: Arc::new(RwLock::new(std::collections::HashSet::new())),
            last_clock_beat: Arc::new(RwLock::new(0.0)),
        }
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
    // Callback Management
    // ========================================================================

    /// Register a callback for MIDI events.
    ///
    /// Returns a unique callback ID that can be used to unregister the callback.
    pub async fn register_callback(
        &self,
        device_id: MidiDeviceId,
        callback_type: CallbackType,
        channel: Option<u8>,
        callback_data: CallbackData,
    ) -> u64 {
        let mut callbacks = self.callbacks.write().await;
        let id = callbacks.register(device_id, callback_type, channel, callback_data);
        tracing::info!(
            "Registered MIDI callback {} for device {} ({:?})",
            id,
            device_id.0,
            callback_type
        );
        id
    }

    /// Unregister a callback by ID.
    ///
    /// Returns true if the callback was found and removed.
    pub async fn unregister_callback(&self, id: u64) -> bool {
        let mut callbacks = self.callbacks.write().await;
        let removed = callbacks.unregister(id);
        if removed {
            tracing::info!("Unregistered MIDI callback {}", id);
        }
        removed
    }

    /// Clear all callbacks.
    pub async fn clear_callbacks(&self) {
        let mut callbacks = self.callbacks.write().await;
        callbacks.clear();
        tracing::info!("Cleared all MIDI callbacks");
    }

    /// Clear all callbacks for a specific device.
    pub async fn clear_device_callbacks(&self, device_id: MidiDeviceId) {
        let mut callbacks = self.callbacks.write().await;
        callbacks.clear_device(device_id);
        tracing::info!("Cleared MIDI callbacks for device {}", device_id.0);
    }

    /// Get a receiver for callback notifications.
    ///
    /// This can be used by the Rhai layer to poll for incoming MIDI events.
    pub fn callback_receiver(&self) -> Arc<Mutex<mpsc::Receiver<MidiEventNotification>>> {
        Arc::clone(&self.callback_rx)
    }

    /// Try to receive callback notifications without blocking.
    ///
    /// Returns all pending notifications.
    pub fn poll_callbacks(&self) -> Vec<MidiEventNotification> {
        let mut rx = self.callback_rx.lock().unwrap();
        let mut notifications = Vec::new();
        while let Ok(notification) = rx.try_recv() {
            notifications.push(notification);
        }
        notifications
    }

    // ========================================================================
    // Advanced Routing
    // ========================================================================

    /// Add an advanced keyboard route.
    ///
    /// Returns the index of the route for later removal.
    pub async fn add_keyboard_route(&self, route: KeyboardRouteBuilder) -> usize {
        let mut routing = self.routing.write().await;
        let idx = routing.advanced_keyboard_routes.len();
        tracing::info!(
            "Added keyboard route {} for device {} (notes {}-{}, transpose {})",
            idx,
            route.device_id.0,
            route.note_min,
            route.note_max,
            route.transpose
        );
        routing.advanced_keyboard_routes.push(route);
        idx
    }

    /// Add an advanced note route (for drums/pads).
    ///
    /// Returns the index of the route for later removal.
    pub async fn add_note_route(&self, route: NoteRouteBuilder) -> usize {
        let mut routing = self.routing.write().await;
        let idx = routing.note_routes.len();
        tracing::info!(
            "Added note route {} for device {} note {}",
            idx,
            route.device_id.0,
            route.source_note
        );
        routing.note_routes.push(route);
        idx
    }

    /// Add an advanced CC route.
    ///
    /// Returns the index of the route for later removal.
    pub async fn add_cc_route(&self, route: CcRouteBuilder) -> usize {
        let mut routing = self.routing.write().await;
        let idx = routing.advanced_cc_routes.len();
        tracing::info!(
            "Added CC route {} for device {} CC {}",
            idx,
            route.device_id.0,
            route.cc
        );
        routing.advanced_cc_routes.push(route);
        idx
    }

    /// Remove a keyboard route by index.
    pub async fn remove_keyboard_route(&self, index: usize) {
        let mut routing = self.routing.write().await;
        if index < routing.advanced_keyboard_routes.len() {
            routing.advanced_keyboard_routes.remove(index);
            tracing::info!("Removed keyboard route at index {}", index);
        } else {
            tracing::warn!(
                "Attempted to remove keyboard route at invalid index {}",
                index
            );
        }
    }

    /// Get the number of active keyboard routes.
    pub async fn keyboard_route_count(&self) -> usize {
        let routing = self.routing.read().await;
        routing.advanced_keyboard_routes.len() + routing.keyboard_routes.len()
    }

    /// Clear all MIDI routes (both basic and advanced).
    pub async fn clear_routes(&self) {
        let mut routing = self.routing.write().await;
        routing.keyboard_routes.clear();
        routing.cc_routes.clear();
        routing.advanced_keyboard_routes.clear();
        routing.note_routes.clear();
        routing.advanced_cc_routes.clear();
        routing.choke_groups.clear();
        tracing::info!("Cleared all MIDI routes");
    }

    /// Apply MIDI CC routes from script state.
    ///
    /// This clears existing basic CC routes and adds the new ones.
    pub async fn apply_cc_routes(&self, routes: &[crate::reload::MidiCcRoute]) {
        let mut routing = self.routing.write().await;
        routing.cc_routes.clear();

        for route in routes {
            routing.cc_routes.push(CcRoute {
                device_id: route.device_id,
                cc: route.cc,
                target: route.target.clone(),
                param: route.param.clone(),
                min_value: route.min_value,
                max_value: route.max_value,
            });
            tracing::debug!(
                "Applied CC route: device={}, cc={}, target={:?}, param={}, min={}, max={}",
                route.device_id.0,
                route.cc,
                route.target,
                route.param,
                route.min_value,
                route.max_value
            );
        }

        if !routes.is_empty() {
            tracing::info!("Applied {} MIDI CC routes from script", routes.len());
        }
    }

    /// Create an output channel for a MIDI device.
    ///
    /// This opens the device and spawns a background thread to send MIDI events.
    /// Returns a channel sender that can be registered with `MidiRealtimeService`.
    ///
    /// # Arguments
    ///
    /// * `id` - The MIDI device ID
    ///
    /// # Returns
    ///
    /// A `Sender<QueuedMidiEvent>` that can be passed to `MidiRealtimeService::register_device()`.
    pub fn create_output_channel(&self, id: MidiDeviceId) -> Result<Sender<QueuedMidiEvent>> {
        // Check if we already have a channel for this device
        {
            let channels = self.output_channels.lock().unwrap();
            if let Some(sender) = channels.get(&id) {
                return Ok(sender.clone());
            }
        }

        // Open the MIDI output device
        let midi_out = MidiOutput::new("vibelang-core2-rt")
            .map_err(|e| Error::MidiError(format!("Failed to create MIDI output: {}", e)))?;

        let ports = midi_out.ports();
        let port = ports
            .get(id.0 as usize)
            .ok_or_else(|| Error::MidiError(format!("MIDI device {} not found", id.0)))?;

        let port_name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| format!("Unknown {}", id.0));

        let conn = midi_out
            .connect(port, "vibelang-rt-output")
            .map_err(|e| Error::MidiError(format!("Failed to connect MIDI output: {}", e)))?;

        // Create the channel
        let (tx, rx) = crossbeam_channel::bounded::<QueuedMidiEvent>(1024);

        // Spawn the background thread
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let device_id = id.0;

        let handle = thread::Builder::new()
            .name(format!("midi-out-{}", device_id))
            .spawn(move || {
                run_output_thread(conn, rx, running_clone, device_id);
            })
            .map_err(|e| Error::MidiError(format!("Failed to spawn output thread: {}", e)))?;

        // Store the sender and thread state
        self.output_channels.lock().unwrap().insert(id, tx.clone());
        self.output_threads
            .lock()
            .unwrap()
            .insert(id, OutputThreadState { handle, running });

        tracing::info!(
            "Created MIDI output channel for {} (id={})",
            port_name,
            id.0
        );

        Ok(tx)
    }

    /// Close an output channel and stop its background thread.
    pub fn close_output_channel(&self, id: MidiDeviceId) {
        // Remove the sender
        self.output_channels.lock().unwrap().remove(&id);

        // Stop the thread
        if let Some(state) = self.output_threads.lock().unwrap().remove(&id) {
            state.running.store(false, Ordering::SeqCst);
            // The thread will exit when it sees the running flag is false
            // or when the channel is disconnected
            let _ = state.handle.join();
            tracing::info!("Closed MIDI output channel for device {}", id.0);
        }
    }

    /// Get the output channel sender for a device (if it exists).
    pub fn get_output_channel(&self, id: MidiDeviceId) -> Option<Sender<QueuedMidiEvent>> {
        self.output_channels.lock().unwrap().get(&id).cloned()
    }

    /// Process incoming MIDI messages.
    ///
    /// Called by the runtime's tick loop.
    pub async fn tick(&self) {
        // Collect messages from the channel (holding the lock briefly)
        let messages: Vec<_> = {
            let mut rx = self.rx.lock().unwrap();
            let mut msgs = Vec::new();
            while let Ok(msg) = rx.try_recv() {
                msgs.push(msg);
            }
            msgs
        };

        // Process messages without holding the lock
        for (device_id, msg) in messages {
            self.handle_message(device_id, msg).await;
        }
    }

    /// Handle a single MIDI message.
    async fn handle_message(&self, device_id: MidiDeviceId, msg: MidiMessage) {
        // First, invoke callbacks
        self.invoke_callbacks(device_id, &msg).await;

        // Record events if recording is active for this device
        self.record_message(device_id, &msg).await;

        // Then process routing
        let routing = self.routing.read().await;

        match &msg {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => {
                // Process basic keyboard routes
                for route in &routing.keyboard_routes {
                    if route.device_id == device_id
                        && (route.channel.is_none() || route.channel == Some(*channel))
                    {
                        let mut state = self.state.write().await;
                        let node_id = state.alloc_node_id();
                        if let Some(voice) = state.voices.get_mut(&route.voice_id) {
                            tracing::debug!(
                                "MIDI note_on: voice={}, note={}, velocity={}",
                                route.voice_id.0,
                                note,
                                velocity
                            );
                            voice.note_nodes.insert(*note, node_id);
                        }
                    }
                }

                // Process advanced keyboard routes
                for route in &routing.advanced_keyboard_routes {
                    if route.device_id == device_id
                        && (route.channel.is_none() || route.channel == Some(*channel))
                        && route.note_in_range(*note)
                    {
                        if let Some(voice_id) = route.target_voice {
                            let transposed_note = route.apply_transpose(*note);
                            let curved_velocity = route.apply_velocity(*velocity);

                            let mut state = self.state.write().await;
                            let node_id = state.alloc_node_id();
                            if let Some(voice) = state.voices.get_mut(&voice_id) {
                                tracing::debug!(
                                    "MIDI advanced note_on: voice={}, note={}->{}, velocity={}->{}",
                                    voice_id.0,
                                    note,
                                    transposed_note,
                                    velocity,
                                    curved_velocity
                                );
                                voice.note_nodes.insert(transposed_note, node_id);
                            }
                        }
                    }
                }

                // Process note routes (drums/pads)
                for route in &routing.note_routes {
                    if route.device_id == device_id
                        && route.source_note == *note
                        && (route.channel.is_none() || route.channel == Some(*channel))
                    {
                        if let Some(voice_id) = route.target_voice {
                            let mut state = self.state.write().await;
                            let node_id = state.alloc_node_id();
                            if let Some(voice) = state.voices.get_mut(&voice_id) {
                                tracing::debug!(
                                    "MIDI note route: voice={}, note={}, velocity={}",
                                    voice_id.0,
                                    note,
                                    velocity
                                );
                                voice.note_nodes.insert(*note, node_id);

                                // Handle velocity-to-parameter mapping
                                if let Some((param, value)) = route.velocity_to_param(*velocity) {
                                    tracing::debug!("MIDI velocity mapping: {} = {}", param, value);
                                    // TODO: Apply parameter to voice
                                }
                            }
                        }
                    }
                }
            }
            MidiMessage::NoteOff { channel, note } => {
                // Process basic keyboard routes
                for route in &routing.keyboard_routes {
                    if route.device_id == device_id
                        && (route.channel.is_none() || route.channel == Some(*channel))
                    {
                        let mut state = self.state.write().await;
                        if let Some(voice) = state.voices.get_mut(&route.voice_id) {
                            tracing::debug!(
                                "MIDI note_off: voice={}, note={}",
                                route.voice_id.0,
                                note
                            );
                            voice.note_nodes.remove(note);
                        }
                    }
                }

                // Process advanced keyboard routes
                for route in &routing.advanced_keyboard_routes {
                    if route.device_id == device_id
                        && (route.channel.is_none() || route.channel == Some(*channel))
                        && route.note_in_range(*note)
                    {
                        if let Some(voice_id) = route.target_voice {
                            let transposed_note = route.apply_transpose(*note);

                            let mut state = self.state.write().await;
                            if let Some(voice) = state.voices.get_mut(&voice_id) {
                                tracing::debug!(
                                    "MIDI advanced note_off: voice={}, note={}->{}",
                                    voice_id.0,
                                    note,
                                    transposed_note
                                );
                                voice.note_nodes.remove(&transposed_note);
                            }
                        }
                    }
                }

                // Process note routes (drums/pads)
                for route in &routing.note_routes {
                    if route.device_id == device_id
                        && route.source_note == *note
                        && (route.channel.is_none() || route.channel == Some(*channel))
                    {
                        if let Some(voice_id) = route.target_voice {
                            let mut state = self.state.write().await;
                            if let Some(voice) = state.voices.get_mut(&voice_id) {
                                tracing::debug!(
                                    "MIDI note route off: voice={}, note={}",
                                    voice_id.0,
                                    note
                                );
                                voice.note_nodes.remove(note);
                            }
                        }
                    }
                }
            }
            MidiMessage::ControlChange { channel, cc, value } => {
                // Collect basic routes
                let basic_routes: Vec<_> = routing
                    .cc_routes
                    .iter()
                    .filter(|r| r.device_id == device_id && r.cc == *cc)
                    .cloned()
                    .collect();

                // Collect advanced routes
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

                drop(routing); // Release lock before backend calls

                // Process basic CC routes
                for route in basic_routes {
                    let normalized = *value as f32 / 127.0;
                    // Scale to min/max range
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
                    let param_value = route.cc_to_param(*value);

                    if let (Some(voice_id), Some(param)) = (route.target_voice, &route.target_param)
                    {
                        tracing::debug!(
                            "MIDI advanced CC: voice={}, param={}, value={}",
                            voice_id.0,
                            param,
                            param_value
                        );

                        let target = FadeTarget::Voice(voice_id);
                        if let Err(e) = self.apply_cc_to_target(&target, param, param_value).await {
                            tracing::warn!(
                                "Failed to apply advanced CC to voice {}: {}",
                                voice_id.0,
                                e
                            );
                        }
                    }
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

                drop(routing);

                let bend_semitones = (*value as f32 / 8192.0) * 2.0;

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
                            tracing::warn!(
                                "Failed to apply pitch bend to voice {}: {}",
                                voice_id.0,
                                e
                            );
                        }
                    }
                }
            }
            MidiMessage::Clock => {
                // MIDI Clock pulse (24 ppqn) - handled via callbacks only
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

    /// Invoke callbacks for a MIDI message.
    async fn invoke_callbacks(&self, device_id: MidiDeviceId, msg: &MidiMessage) {
        let callbacks = self.callbacks.read().await;

        // Determine the callback type and channel based on the message
        let (callback_type, channel) = match msg {
            MidiMessage::NoteOn { channel, .. } | MidiMessage::NoteOff { channel, .. } => {
                (CallbackType::KeyboardNote, Some(*channel))
            }
            MidiMessage::ControlChange { channel, cc, .. } => {
                (CallbackType::ControlChange(*cc), Some(*channel))
            }
            MidiMessage::PitchBend { channel, .. } => (CallbackType::PitchBend, Some(*channel)),
            MidiMessage::Clock | MidiMessage::Start | MidiMessage::Stop | MidiMessage::Continue => {
                (CallbackType::ClockSync, None)
            }
        };

        // Get matching callbacks - channel is optional now
        let matching = if let Some(ch) = channel {
            callbacks.get_matching(device_id, callback_type, ch)
        } else {
            // For clock messages, get all ClockSync callbacks for this device
            callbacks.get_matching_no_channel(device_id, callback_type)
        };

        // Send notifications for each matching callback
        for callback in matching {
            let notification = MidiEventNotification {
                callback_id: callback.id,
                device_id,
                message: msg.clone(),
            };

            // Try to send without blocking
            if let Err(e) = self.callback_tx.try_send(notification) {
                tracing::warn!("Failed to send MIDI callback notification: {}", e);
            }
        }
    }

    /// Apply a CC value to a target's parameter.
    async fn apply_cc_to_target(&self, target: &FadeTarget, param: &str, value: f32) -> Result<()> {
        use crate::types::NodeId;

        // Look up node ID(s) for the target
        let node_ids: Vec<NodeId> = {
            let state = self.state.read().await;
            match target {
                FadeTarget::Group(id) => {
                    if let Some(group) = state.groups.get(id) {
                        // For amp/pan, use link synth; otherwise use group node
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
                        // Apply to all active synth nodes
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
                // Patterns and Melodies don't have direct node IDs
                FadeTarget::Pattern(_) | FadeTarget::Melody(_) => {
                    tracing::debug!("CC to pattern/melody not supported directly");
                    vec![]
                }
            }
        };

        // Apply to each node
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
        // Get active note nodes
        let note_nodes: Vec<_> = {
            let state = self.state.read().await;
            if let Some(voice) = state.voices.get(&voice_id) {
                voice.note_nodes.values().copied().collect()
            } else {
                return Ok(());
            }
        };

        let node_count = note_nodes.len();

        // Apply pitch bend as a detune parameter
        // The synthdef should have a "detune" parameter in semitones
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

    /// Record a MIDI message if recording is active for this device.
    async fn record_message(&self, device_id: MidiDeviceId, msg: &MidiMessage) {
        let mut recordings = self.recordings.write().await;
        if let Some(recording) = recordings.get_mut(&device_id) {
            if recording.is_recording {
                // Get current beat from state
                let current_beat = {
                    let state = self.state.read().await;
                    state.current_beat
                };

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

    /// Send MIDI clock tick to all devices with clock output enabled.
    pub async fn send_clock_tick(&self) -> Result<()> {
        let clock_devices = self.clock_output_devices.read().await;
        if clock_devices.is_empty() {
            return Ok(());
        }

        let mut outputs = self.outputs.lock().unwrap();
        for device_id in clock_devices.iter() {
            if let Some(conn) = outputs.get_mut(device_id) {
                // MIDI Clock message is 0xF8
                if let Err(e) = conn.send(&[0xF8]) {
                    tracing::warn!("Failed to send MIDI clock to device {}: {}", device_id.0, e);
                }
            }
        }

        Ok(())
    }

    /// Get a reference to the recordings map for direct access.
    pub fn recordings(&self) -> Arc<RwLock<HashMap<MidiDeviceId, MidiRecording>>> {
        Arc::clone(&self.recordings)
    }

    /// Tick MIDI clock output based on current beat position.
    ///
    /// Sends the appropriate number of clock ticks (24 PPQN) based on
    /// the beat position change since the last tick.
    pub async fn tick_clock(&self, current_beat: f64, is_playing: bool) -> Result<()> {
        let clock_devices = self.clock_output_devices.read().await;
        if clock_devices.is_empty() {
            return Ok(());
        }

        // Only send clock when transport is playing
        if !is_playing {
            // Reset clock position when stopped
            let mut last_beat = self.last_clock_beat.write().await;
            *last_beat = current_beat;
            return Ok(());
        }

        let mut last_beat = self.last_clock_beat.write().await;

        // Calculate how many clock ticks to send
        // 24 PPQN = 24 pulses per quarter note = 24 pulses per beat
        const PPQN: f64 = 24.0;

        // Calculate ticks since last position
        let beat_diff = current_beat - *last_beat;

        // Handle negative beat diff (e.g., seek backwards)
        if beat_diff < 0.0 {
            *last_beat = current_beat;
            return Ok(());
        }

        let ticks_to_send = (beat_diff * PPQN).floor() as u32;

        if ticks_to_send > 0 {
            let mut outputs = self.outputs.lock().unwrap();

            for device_id in clock_devices.iter() {
                if let Some(conn) = outputs.get_mut(device_id) {
                    for _ in 0..ticks_to_send {
                        // MIDI Clock message is 0xF8
                        if let Err(e) = conn.send(&[0xF8]) {
                            tracing::warn!(
                                "Failed to send MIDI clock to device {}: {}",
                                device_id.0,
                                e
                            );
                        }
                    }
                }
            }

            // Update last beat position based on ticks sent
            *last_beat += (ticks_to_send as f64) / PPQN;
        }

        Ok(())
    }

    /// Reset the clock output position (e.g., when transport seeks).
    pub async fn reset_clock_position(&self, beat: f64) {
        let mut last_beat = self.last_clock_beat.write().await;
        *last_beat = beat;
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Midi for MidiHandler<B> {
    fn list_devices(&self) -> Vec<MidiDeviceInfo> {
        let mut devices = Vec::new();
        let mut seen_names = HashMap::new();

        // List input devices
        if let Ok(midi_in) = MidiInput::new("vibelang-core2-list") {
            for (idx, port) in midi_in.ports().iter().enumerate() {
                if let Ok(name) = midi_in.port_name(port) {
                    let id = MidiDeviceId::new(idx as u32);
                    devices.push(MidiDeviceInfo {
                        id,
                        name: name.clone(),
                        has_input: true,
                        has_output: false,
                    });
                    seen_names.insert(name, idx);
                }
            }
        }

        // List output devices
        if let Ok(midi_out) = MidiOutput::new("vibelang-core2-list") {
            for port in midi_out.ports().iter() {
                if let Ok(name) = midi_out.port_name(port) {
                    // Check if we already have this device from input
                    if let Some(&existing_idx) = seen_names.get(&name) {
                        devices[existing_idx].has_output = true;
                    } else {
                        let id = MidiDeviceId::new((devices.len()) as u32);
                        devices.push(MidiDeviceInfo {
                            id,
                            name,
                            has_input: false,
                            has_output: true,
                        });
                    }
                }
            }
        }

        devices
    }

    async fn open_input(&self, id: MidiDeviceId) -> Result<()> {
        let midi_in = MidiInput::new("vibelang-core2-in")
            .map_err(|e| Error::MidiError(format!("Failed to create MIDI input: {}", e)))?;

        let ports = midi_in.ports();
        let port = ports
            .get(id.0 as usize)
            .ok_or_else(|| Error::MidiError(format!("MIDI device {} not found", id.0)))?;

        let port_name = midi_in
            .port_name(port)
            .unwrap_or_else(|_| format!("Unknown {}", id.0));

        // Legacy channel for backward compatibility
        let tx = self.tx.clone();

        // New infrastructure: event queue sender and clock for timestamp calibration
        let event_sender = self.event_queue.sender();
        let midi_clock = Arc::clone(&self.midi_clock);
        let device_id = id;

        let conn = midi_in
            .connect(
                port,
                "vibelang-input",
                move |timestamp_us, data, _| {
                    let received_at = std::time::Instant::now();

                    // Calibrate clock on first message
                    midi_clock.calibrate(timestamp_us);

                    // Parse with new infrastructure (preserves all message types)
                    if let Some(new_msg) = new_parse_midi_bytes(data) {
                        // Send to new event queue with full timestamp info
                        let timestamped_event = TimestampedMidiEvent {
                            timestamp_us,
                            received_at,
                            device_id: crate::types::ids::MidiDeviceId::new(device_id.0),
                            message: new_msg.clone(),
                        };

                        if !event_sender.try_send(timestamped_event) {
                            tracing::warn!("[MIDI] Event queue full, dropping event");
                        }

                        // Also send to legacy channel for backward compatibility
                        if let Some(legacy_msg) = convert_new_to_legacy_message(&new_msg) {
                            let _ = tx.blocking_send((device_id, legacy_msg));
                        }
                    }
                },
                (),
            )
            .map_err(|e| Error::MidiError(format!("Failed to connect MIDI input: {}", e)))?;

        self.inputs.lock().unwrap().insert(id, conn);
        tracing::info!(
            "Opened MIDI input: {} (id={}) with timestamp preservation",
            port_name,
            id.0
        );

        Ok(())
    }

    async fn open_output(&self, id: MidiDeviceId) -> Result<()> {
        let midi_out = MidiOutput::new("vibelang-core2-out")
            .map_err(|e| Error::MidiError(format!("Failed to create MIDI output: {}", e)))?;

        let ports = midi_out.ports();
        let port = ports
            .get(id.0 as usize)
            .ok_or_else(|| Error::MidiError(format!("MIDI device {} not found", id.0)))?;

        let port_name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| format!("Unknown {}", id.0));

        let conn = midi_out
            .connect(port, "vibelang-output")
            .map_err(|e| Error::MidiError(format!("Failed to connect MIDI output: {}", e)))?;

        self.outputs.lock().unwrap().insert(id, conn);
        tracing::info!("Opened MIDI output: {} (id={})", port_name, id.0);

        Ok(())
    }

    async fn close(&self, id: MidiDeviceId) -> Result<()> {
        let mut removed = false;

        if self.inputs.lock().unwrap().remove(&id).is_some() {
            tracing::info!("Closed MIDI input: id={}", id.0);
            removed = true;
        }

        if self.outputs.lock().unwrap().remove(&id).is_some() {
            tracing::info!("Closed MIDI output: id={}", id.0);
            removed = true;
        }

        if removed {
            Ok(())
        } else {
            Err(Error::MidiError(format!("MIDI device {} not open", id.0)))
        }
    }

    async fn send_note_on(
        &self,
        device: MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
    ) -> Result<()> {
        let mut outputs = self.outputs.lock().unwrap();
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        let msg = [0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F];
        conn.send(&msg)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI: {}", e)))?;

        Ok(())
    }

    async fn send_note_off(&self, device: MidiDeviceId, channel: u8, note: u8) -> Result<()> {
        let mut outputs = self.outputs.lock().unwrap();
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        let msg = [0x80 | (channel & 0x0F), note & 0x7F, 0];
        conn.send(&msg)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI: {}", e)))?;

        Ok(())
    }

    async fn send_cc(&self, device: MidiDeviceId, channel: u8, cc: u8, value: u8) -> Result<()> {
        let mut outputs = self.outputs.lock().unwrap();
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        let msg = [0xB0 | (channel & 0x0F), cc & 0x7F, value & 0x7F];
        conn.send(&msg)
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI: {}", e)))?;

        Ok(())
    }

    async fn route_keyboard(&self, device: MidiDeviceId, voice: VoiceId) -> Result<()> {
        let mut routing = self.routing.write().await;
        routing.keyboard_routes.push(KeyboardRoute {
            device_id: device,
            channel: None, // All channels
            voice_id: voice,
        });

        tracing::info!("Routed MIDI keyboard {} to voice {}", device.0, voice.0);

        Ok(())
    }

    async fn route_cc(
        &self,
        device: MidiDeviceId,
        cc: u8,
        target: FadeTarget,
        param: &str,
    ) -> Result<()> {
        let mut routing = self.routing.write().await;
        routing.cc_routes.push(CcRoute {
            device_id: device,
            cc,
            target: target.clone(),
            param: param.to_string(),
            min_value: 0.0,
            max_value: 1.0,
        });

        tracing::info!(
            "Routed MIDI CC {} from device {} to {:?}.{}",
            cc,
            device.0,
            target,
            param
        );

        Ok(())
    }

    // =========================================================================
    // Recording
    // =========================================================================

    async fn start_recording(&self, device: MidiDeviceId) -> Result<()> {
        let current_beat = {
            let state = self.state.read().await;
            state.current_beat
        };

        let mut recordings = self.recordings.write().await;
        if recordings.contains_key(&device) {
            return Err(Error::MidiError(format!(
                "Already recording from MIDI device {}",
                device.0
            )));
        }

        let recording = MidiRecording::new(device, current_beat);
        recordings.insert(device, recording);

        tracing::info!(
            "Started MIDI recording from device {} at beat {}",
            device.0,
            current_beat.to_f64()
        );

        Ok(())
    }

    async fn start_recording_channel(&self, device: MidiDeviceId, channel: u8) -> Result<()> {
        let current_beat = {
            let state = self.state.read().await;
            state.current_beat
        };

        let mut recordings = self.recordings.write().await;
        if recordings.contains_key(&device) {
            return Err(Error::MidiError(format!(
                "Already recording from MIDI device {}",
                device.0
            )));
        }

        let recording = MidiRecording::new(device, current_beat).with_channel_filter(channel);
        recordings.insert(device, recording);

        tracing::info!(
            "Started MIDI recording from device {} channel {} at beat {}",
            device.0,
            channel,
            current_beat.to_f64()
        );

        Ok(())
    }

    async fn stop_recording(&self, device: MidiDeviceId) -> Result<MidiRecording> {
        let current_beat = {
            let state = self.state.read().await;
            state.current_beat
        };

        let mut recordings = self.recordings.write().await;
        let mut recording = recordings.remove(&device).ok_or_else(|| {
            Error::MidiError(format!("Not recording from MIDI device {}", device.0))
        })?;

        recording.stop(current_beat);

        tracing::info!(
            "Stopped MIDI recording from device {}: {} notes, {} CCs, {} beats",
            device.0,
            recording.note_count(),
            recording.cc_count(),
            recording.duration().to_f64()
        );

        Ok(recording)
    }

    async fn is_recording(&self, device: MidiDeviceId) -> bool {
        let recordings = self.recordings.read().await;
        recordings
            .get(&device)
            .map(|r| r.is_recording)
            .unwrap_or(false)
    }

    async fn recording_info(&self, device: MidiDeviceId) -> Option<MidiRecordingInfo> {
        let recordings = self.recordings.read().await;
        recordings.get(&device).map(MidiRecordingInfo::from)
    }

    // =========================================================================
    // Clock Output
    // =========================================================================

    async fn enable_clock_output(&self, device: MidiDeviceId) -> Result<()> {
        // Ensure output device is open
        {
            let outputs = self.outputs.lock().unwrap();
            if !outputs.contains_key(&device) {
                return Err(Error::MidiError(format!(
                    "MIDI output device {} not open",
                    device.0
                )));
            }
        }

        let mut clock_devices = self.clock_output_devices.write().await;
        clock_devices.insert(device);

        tracing::info!("Enabled MIDI clock output to device {}", device.0);

        Ok(())
    }

    async fn disable_clock_output(&self, device: MidiDeviceId) -> Result<()> {
        let mut clock_devices = self.clock_output_devices.write().await;
        clock_devices.remove(&device);

        tracing::info!("Disabled MIDI clock output to device {}", device.0);

        Ok(())
    }

    async fn is_clock_output_enabled(&self, device: MidiDeviceId) -> bool {
        let clock_devices = self.clock_output_devices.read().await;
        clock_devices.contains(&device)
    }

    async fn send_start(&self, device: MidiDeviceId) -> Result<()> {
        let mut outputs = self.outputs.lock().unwrap();
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        // MIDI Start message is 0xFA
        conn.send(&[0xFA])
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI Start: {}", e)))?;

        tracing::debug!("Sent MIDI Start to device {}", device.0);

        Ok(())
    }

    async fn send_stop(&self, device: MidiDeviceId) -> Result<()> {
        let mut outputs = self.outputs.lock().unwrap();
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        // MIDI Stop message is 0xFC
        conn.send(&[0xFC])
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI Stop: {}", e)))?;

        tracing::debug!("Sent MIDI Stop to device {}", device.0);

        Ok(())
    }

    async fn send_continue(&self, device: MidiDeviceId) -> Result<()> {
        let mut outputs = self.outputs.lock().unwrap();
        let conn = outputs
            .get_mut(&device)
            .ok_or_else(|| Error::MidiError(format!("MIDI output {} not open", device.0)))?;

        // MIDI Continue message is 0xFB
        conn.send(&[0xFB])
            .map_err(|e| Error::MidiError(format!("Failed to send MIDI Continue: {}", e)))?;

        tracing::debug!("Sent MIDI Continue to device {}", device.0);

        Ok(())
    }
}

/// Background thread for sending MIDI events to a device.
///
/// This thread receives `QueuedMidiEvent`s from the channel and sends them
/// to the MIDI device. It runs until the `running` flag is set to false
/// or the channel is disconnected.
fn run_output_thread(
    mut conn: MidiOutputConnection,
    rx: Receiver<QueuedMidiEvent>,
    running: Arc<AtomicBool>,
    device_id: u32,
) {
    tracing::info!("[MIDI_OUT_{}] Background thread started", device_id);

    while running.load(Ordering::Relaxed) {
        // Use recv_timeout with 1ms for low-latency MIDI output
        match rx.recv_timeout(std::time::Duration::from_millis(1)) {
            Ok(event) => {
                let bytes = event.to_bytes();
                if let Err(e) = conn.send(&bytes) {
                    tracing::warn!("[MIDI_OUT_{}] Failed to send MIDI: {}", device_id, e);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // Continue checking running flag
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                tracing::info!("[MIDI_OUT_{}] Channel disconnected, exiting", device_id);
                break;
            }
        }
    }

    tracing::info!("[MIDI_OUT_{}] Background thread exiting", device_id);
}
