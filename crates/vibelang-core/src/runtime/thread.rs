//! Runtime thread for VibeLang.
//!
//! The runtime thread is the heart of VibeLang. It:
//! - Owns the state manager
//! - Runs the beat scheduler
//! - Processes state messages
//! - Communicates with SuperCollider

use crate::audio_device::AudioConfig;
use crate::events::{BeatEvent, FadeTargetType};
use crate::midi::{MidiMessage, MidiRouting};
use crate::osc_sender::{OscSender, OscTiming};
use crate::reload::{ChangeOp, EntityKind, ReloadManager, StateSnapshot};
use crate::scheduler::{EventScheduler, LoopKind, LoopSnapshot};
use crate::scsynth::{AddAction, BufNum, NodeId, Scsynth, Target};
use crate::scsynth_process::ScsynthProcess;
use rosc::{OscMessage, OscPacket, OscType};
use crate::state::{
    ActiveFadeJob, ActiveSequence, ActiveSynth, EffectState, GroupState, LoopStatus,
    MelodyState, PatternState, SampleInfo, ScheduledEvent, ScheduledNoteOff,
    ScriptState, SequenceRunLog, StateManager, StateMessage, VoiceState,
};
use crate::timing::{BeatTime, TimeSignature, TransportClock};
use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const EPSILON: f64 = 1e-6;
const LOOKAHEAD_MS: u64 = 250;

/// Handle to the running VibeLang runtime.
///
/// This is the main interface for interacting with VibeLang from the API layer.
/// It provides thread-safe access to state and message sending.
#[derive(Clone)]
pub struct RuntimeHandle {
    /// Sender for state messages.
    message_tx: Sender<StateMessage>,
    /// Shared state manager for read access.
    state_manager: StateManager,
    /// Reference to the SuperCollider client.
    scsynth: Scsynth,
    /// Flag to signal shutdown.
    shutdown: Arc<AtomicBool>,
    /// Receiver for sequence completion notifications.
    completion_rx: Option<crossbeam_channel::Receiver<String>>,
    /// Sender for MIDI messages to the runtime thread.
    midi_tx: Sender<MidiMessage>,
}

impl RuntimeHandle {
    /// Send a message to the runtime thread.
    pub fn send(&self, msg: StateMessage) -> Result<()> {
        self.message_tx
            .send(msg)
            .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))
    }

    /// Get the state manager for read access.
    pub fn state(&self) -> &StateManager {
        &self.state_manager
    }

    /// Get the SuperCollider client.
    pub fn scsynth(&self) -> &Scsynth {
        &self.scsynth
    }

    /// Read the current state with a closure.
    pub fn with_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&ScriptState) -> R,
    {
        self.state_manager.with_state_read(f)
    }

    /// Write to the state with a closure.
    pub fn with_state_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ScriptState) -> R,
    {
        self.state_manager.with_state_write(f)
    }

    /// Get a clone of the message sender.
    pub fn message_sender(&self) -> Sender<StateMessage> {
        self.message_tx.clone()
    }

    /// Signal the runtime to shut down.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Check if shutdown has been requested.
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    /// Wait for a sequence to complete (for play_once sequences).
    /// Returns true if the sequence completed, false if timeout or no channel.
    pub fn wait_for_sequence(&self, name: &str, timeout: Option<Duration>) -> bool {
        let rx = match &self.completion_rx {
            Some(rx) => rx,
            None => return false,
        };

        let deadline = timeout.map(|t| Instant::now() + t);

        loop {
            let remaining = match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        return false;
                    }
                    Some(d - now)
                }
                None => None,
            };

            let result = match remaining {
                Some(t) => rx.recv_timeout(t),
                None => rx.recv().map_err(|_| crossbeam_channel::RecvTimeoutError::Disconnected),
            };

            match result {
                Ok(completed_name) if completed_name == name => return true,
                Ok(_) => continue, // Different sequence completed, keep waiting
                Err(_) => return false, // Timeout or channel closed
            }
        }
    }

    /// Try to receive a sequence completion notification without blocking.
    /// Returns Some(sequence_name) if a sequence completed, None otherwise.
    pub fn try_recv_completion(&self) -> Option<String> {
        self.completion_rx.as_ref()?.try_recv().ok()
    }

    /// Check if a specific sequence has completed (non-blocking).
    pub fn is_sequence_completed(&self, name: &str) -> bool {
        self.with_state(|state| {
            // If it's in active_sequences and marked completed, it's done
            if let Some(active) = state.active_sequences.get(name) {
                return active.completed;
            }
            // If it's not in active_sequences but the definition has play_once,
            // it was started and has now finished
            if let Some(def) = state.sequences.get(name) {
                if def.play_once {
                    // If play_once and not active, it either never started or completed
                    // We can't distinguish without more tracking, so just return false
                    return false;
                }
            }
            false
        })
    }

    /// Get the MIDI message sender for forwarding MIDI events.
    /// This is used by the API layer when opening MIDI devices.
    pub fn midi_sender(&self) -> Sender<MidiMessage> {
        self.midi_tx.clone()
    }

    /// Create a RuntimeHandle for validation mode (no runtime thread).
    ///
    /// This is used by the validation engine to execute scripts without
    /// starting a full runtime. The handle can receive messages but
    /// they won't be processed by a runtime thread.
    ///
    /// # Arguments
    /// * `message_tx` - Sender for state messages (will be received by caller)
    /// * `state_manager` - State manager for read/write access
    /// * `scsynth` - SuperCollider client (use `Scsynth::noop()` for validation)
    /// * `midi_tx` - Sender for MIDI messages (can be unused)
    pub fn new_validation(
        message_tx: Sender<StateMessage>,
        state_manager: StateManager,
        scsynth: Scsynth,
        midi_tx: Sender<MidiMessage>,
    ) -> Self {
        Self {
            message_tx,
            state_manager,
            scsynth,
            shutdown: Arc::new(AtomicBool::new(false)),
            completion_rx: None,
            midi_tx,
        }
    }
}

/// The VibeLang runtime.
///
/// Manages the SuperCollider process and runtime thread.
pub struct Runtime {
    /// The scsynth process (owned, will be killed on drop).
    _process: ScsynthProcess,
    /// Handle for interacting with the runtime.
    handle: RuntimeHandle,
    /// Join handle for the runtime thread.
    thread_handle: Option<JoinHandle<()>>,
}

impl Runtime {
    /// Start the VibeLang runtime with default settings.
    ///
    /// Uses port 57110, default audio configuration, and generates system synthdefs automatically.
    pub fn start_default() -> Result<Self> {
        Self::start_with_audio_config(AudioConfig::default())
    }

    /// Start the VibeLang runtime with custom audio configuration.
    ///
    /// Uses port 57110 and generates system synthdefs automatically.
    ///
    /// # Arguments
    ///
    /// * `audio_config` - Audio device and channel configuration
    pub fn start_with_audio_config(audio_config: AudioConfig) -> Result<Self> {
        // Generate system_link_audio synthdef bytes
        let system_synthdef_bytes = create_system_link_audio_bytes()?;
        Self::start_full(57110, &system_synthdef_bytes, audio_config)
    }

    /// Start the VibeLang runtime with default audio configuration.
    ///
    /// This will:
    /// 1. Start the scsynth process
    /// 2. Connect to scsynth via OSC
    /// 3. Load system synthdefs
    /// 4. Start the runtime thread
    ///
    /// # Arguments
    ///
    /// * `port` - UDP port for scsynth (default: 57110)
    /// * `system_synthdef_bytes` - Pre-compiled bytes for system_link_audio synthdef
    pub fn start(port: u16, system_synthdef_bytes: &[u8]) -> Result<Self> {
        Self::start_full(port, system_synthdef_bytes, AudioConfig::default())
    }

    /// Start the VibeLang runtime with full configuration options.
    ///
    /// This will:
    /// 1. Start the scsynth process with the specified audio configuration
    /// 2. Connect to scsynth via OSC
    /// 3. Load system synthdefs
    /// 4. Start the runtime thread
    ///
    /// # Arguments
    ///
    /// * `port` - UDP port for scsynth (default: 57110)
    /// * `system_synthdef_bytes` - Pre-compiled bytes for system_link_audio synthdef
    /// * `audio_config` - Audio device and channel configuration
    pub fn start_full(port: u16, system_synthdef_bytes: &[u8], audio_config: AudioConfig) -> Result<Self> {
        // Start scsynth with audio configuration
        // This now waits for scsynth to be ready by polling /status
        log::info!("1. Starting scsynth server...");
        let process = ScsynthProcess::start_with_config(port, &audio_config)?;

        // Connect to scsynth (no additional sleep needed - start_with_config waits for readiness)
        log::info!("2. Connecting to scsynth...");
        let addr = format!("127.0.0.1:{}", port);
        let scsynth = Scsynth::new(&addr)?;
        log::info!("   Connected to scsynth");

        // Load system synthdefs and collect bytes for later storage in state
        let mut system_synthdefs: Vec<(String, Vec<u8>)> = Vec::new();

        scsynth.d_recv_bytes(system_synthdef_bytes.to_vec())?;
        system_synthdefs.push(("system_link_audio".to_string(), system_synthdef_bytes.to_vec()));
        log::info!("   Loaded system_link_audio synthdef");

        // Load SFZ synthdefs
        for (name, bytes) in vibelang_sfz::create_sfz_synthdefs() {
            scsynth.d_recv_bytes(bytes.clone())?;
            system_synthdefs.push((name.clone(), bytes));
            log::info!("   Loaded {} synthdef", name);
        }

        // Load sample voice synthdefs (PlayBuf and Warp1 based)
        for (name, bytes) in crate::sample_synthdef::create_sample_synthdefs() {
            scsynth.d_recv_bytes(bytes.clone())?;
            system_synthdefs.push((name.clone(), bytes));
            log::info!("   Loaded {} synthdef", name);
        }

        // Load MIDI trigger synthdefs (for SC-managed MIDI output)
        for (name, bytes) in crate::midi_synthdefs::create_midi_synthdefs() {
            scsynth.d_recv_bytes(bytes.clone())?;
            system_synthdefs.push((name.clone(), bytes));
            log::info!("   Loaded {} synthdef", name);
        }

        // Free all existing groups
        log::info!("   Freeing existing groups...");
        if let Err(e) = scsynth.g_free_all(0) {
            log::warn!("   Failed to free existing groups: {}", e);
        }
        std::thread::sleep(Duration::from_millis(100));

        // Create state manager and message channel
        let state_manager = StateManager::new();
        let (message_tx, message_rx) = unbounded();
        let shutdown = Arc::new(AtomicBool::new(false));

        // Store system synthdefs in state for score capture
        state_manager.with_state_write(|state| {
            for (name, bytes) in system_synthdefs {
                state.synthdefs.insert(name, bytes);
            }
        });

        // Create the main group in SuperCollider (node 1 at root)
        log::info!("   Creating main group (node 1)...");
        if let Err(e) = scsynth.g_new(NodeId::new(1), AddAction::AddToTail, Target::root()) {
            log::error!("Failed to create main group: {}", e);
        }

        // Register the main group in state with bus 0 (the main output)
        // This is the root of the group hierarchy - all other groups are children of main
        state_manager.with_state_write(|state| {
            let mut main_group = GroupState::new(
                "main".to_string(),
                "main".to_string(),
                None,  // No parent - this is the root
                0,     // Bus 0 is the main output (hardware output)
            );
            main_group.node_id = Some(1);
            state.groups.insert("main".to_string(), main_group);
        });

        // Create completion notification channel for play_once sequences
        let (completion_tx, completion_rx) = crossbeam_channel::unbounded();

        // Create MIDI message channel
        let (midi_tx, midi_rx) = unbounded();

        // Create runtime handle
        let handle = RuntimeHandle {
            message_tx,
            state_manager: state_manager.clone(),
            scsynth: scsynth.clone(),
            shutdown: shutdown.clone(),
            completion_rx: Some(completion_rx),
            midi_tx,
        };

        // Start runtime thread
        log::info!("3. Starting runtime thread...");
        let thread_scsynth = scsynth.clone();
        let thread_state = state_manager.clone();
        let thread_shutdown = shutdown.clone();
        let thread_handle = thread::spawn(move || {
            let mut rt = RuntimeThread::with_completion_channel(
                thread_scsynth,
                thread_state,
                message_rx,
                completion_tx,
                midi_rx,
            );
            rt.run(thread_shutdown);
        });

        // Note: Scheduler is NOT started here - the CLI starts it after script evaluation
        // This ensures sequences started during initial evaluation anchor at beat 0.0
        log::info!("   Runtime started (scheduler not yet started)");

        Ok(Self {
            _process: process,
            handle,
            thread_handle: Some(thread_handle),
        })
    }

    /// Get a handle to interact with the runtime.
    pub fn handle(&self) -> &RuntimeHandle {
        &self.handle
    }

    /// Shut down the runtime gracefully.
    pub fn shutdown(mut self) {
        self.handle.shutdown();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.handle.shutdown();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// The runtime thread that processes messages and runs the scheduler.
struct RuntimeThread {
    /// Centralized OSC sender - handles all scsynth communication and score capture.
    osc_sender: OscSender,
    /// Scsynth connection for receiving (the OscSender wraps a clone of this).
    sc: Scsynth,
    shared: StateManager,
    message_rx: Receiver<StateMessage>,
    scheduler: EventScheduler,
    transport: TransportClock,
    last_tick: Instant,
    /// Manages live reload state transitions.
    reload_manager: ReloadManager,
    /// Channel for notifying when play_once sequences complete.
    completion_tx: Option<crossbeam_channel::Sender<String>>,
    /// MIDI message receiver.
    midi_rx: Receiver<MidiMessage>,
    /// Handler for MIDI triggers from scsynth SendTrig messages.
    midi_osc_handler: crate::midi_osc_handler::MidiOscHandler,
    /// Node ID for the SC-managed MIDI clock synth (None = not running).
    sc_midi_clock_node_id: Option<i32>,
}

impl RuntimeThread {
    fn with_completion_channel(
        sc: Scsynth,
        shared: StateManager,
        message_rx: Receiver<StateMessage>,
        completion_tx: crossbeam_channel::Sender<String>,
        midi_rx: Receiver<MidiMessage>,
    ) -> Self {
        let osc_sender = OscSender::new(sc.clone());
        Self {
            osc_sender,
            sc,
            shared,
            message_rx,
            scheduler: EventScheduler::new(),
            transport: TransportClock::new(),
            last_tick: Instant::now(),
            reload_manager: ReloadManager::new(),
            completion_tx: Some(completion_tx),
            midi_rx,
            midi_osc_handler: crate::midi_osc_handler::MidiOscHandler::new(),
            sc_midi_clock_node_id: None,
        }
    }

    fn run(&mut self, shutdown: Arc<AtomicBool>) {
        let interval = Duration::from_millis(1);

        while !shutdown.load(Ordering::Relaxed) {
            self.drain_messages();
            self.drain_midi_messages();
            self.poll_osc_messages();
            self.tick();
            thread::sleep(interval);
        }
    }

    /// Process all pending MIDI messages.
    fn drain_midi_messages(&mut self) {
        // Get current routing configuration from state
        let routing = self.shared.with_state_read(|state| {
            state.midi_config.routing.clone()
        });

        // Process all available MIDI messages
        while let Ok(msg) = self.midi_rx.try_recv() {
            self.process_midi_message(&routing, msg);
        }
    }

    /// Send a MIDI message to all or a specific MIDI output device.
    fn send_midi_clock_message(&self, event: crate::midi::QueuedMidiEvent) {
        self.shared.with_state_read(|state| {
            let config = &state.midi_output_config;
            if let Some(target_id) = config.clock_device_id {
                // Send to specific device
                if let Some(device) = config.devices.get(&target_id) {
                    let _ = device.event_tx.send(event.clone());
                }
            } else {
                // Send to all devices
                for device in config.devices.values() {
                    let _ = device.event_tx.send(event.clone());
                }
            }
        });
    }

    /// Start the SC-managed MIDI clock synth.
    ///
    /// The clock synth uses Impulse at the clock frequency to send clock pulses
    /// at 24 PPQN (Pulses Per Quarter Note).
    fn start_sc_midi_clock(&mut self, device_id: u32, bpm: f64) {
        // Stop existing clock synth if running
        self.stop_sc_midi_clock();

        // Calculate clock frequency: BPM / 60 * 24 = Hz
        let clock_freq = bpm / 60.0 * 24.0;

        // Allocate node ID for the clock synth
        let node_id = self.shared.with_state_write(|state| state.allocate_synth_node());

        log::info!(
            "[SC-MIDI CLOCK] Starting clock synth: node={} device={} bpm={} freq={}Hz",
            node_id, device_id, bpm, clock_freq
        );

        // Create the clock synth
        if let Err(e) = self.osc_sender.s_new(
            OscTiming::Now,
            "vibelang_midi_clock",
            NodeId::new(node_id),
            AddAction::AddToTail,
            Target::new(0),
            &[
                ("device_id", device_id as f32),
                ("freq", clock_freq as f32),
            ],
            self.transport.beat_at(Instant::now()).to_float(),
        ) {
            log::error!("[SC-MIDI CLOCK] Failed to create clock synth: {}", e);
            return;
        }

        self.sc_midi_clock_node_id = Some(node_id);
    }

    /// Stop the SC-managed MIDI clock synth.
    fn stop_sc_midi_clock(&mut self) {
        if let Some(node_id) = self.sc_midi_clock_node_id.take() {
            log::info!("[SC-MIDI CLOCK] Stopping clock synth: node={}", node_id);

            if let Err(e) = self.osc_sender.n_free(
                OscTiming::Now,
                NodeId::new(node_id),
                self.transport.beat_at(Instant::now()).to_float(),
            ) {
                log::error!("[SC-MIDI CLOCK] Failed to free clock synth: {}", e);
            }
        }
    }

    /// Update the SC-managed MIDI clock synth tempo.
    fn update_sc_midi_clock_tempo(&mut self, bpm: f64) {
        if let Some(node_id) = self.sc_midi_clock_node_id {
            let clock_freq = bpm / 60.0 * 24.0;

            log::debug!(
                "[SC-MIDI CLOCK] Updating clock tempo: node={} bpm={} freq={}Hz",
                node_id, bpm, clock_freq
            );

            if let Err(e) = self.osc_sender.n_set(
                OscTiming::Now,
                NodeId::new(node_id),
                &[("freq", clock_freq as f32)],
                self.transport.beat_at(Instant::now()).to_float(),
            ) {
                log::error!("[SC-MIDI CLOCK] Failed to update clock tempo: {}", e);
            }
        }
    }

    /// Process a single MIDI message according to routing configuration.
    fn process_midi_message(&mut self, routing: &MidiRouting, msg: MidiMessage) {
        // Log if monitoring is enabled
        if routing.monitor_enabled {
            log::info!("[MIDI] {:?}", msg);
        }

        match msg {
            MidiMessage::NoteOn { channel, note, velocity, .. } => {
                self.handle_midi_note_on(routing, channel, note, velocity);
            }
            MidiMessage::NoteOff { channel, note, .. } => {
                self.handle_midi_note_off(routing, channel, note);
            }
            MidiMessage::ControlChange { channel, controller, value, .. } => {
                self.handle_midi_cc(routing, channel, controller, value);
            }
            MidiMessage::PitchBend { channel, value, .. } => {
                self.handle_midi_pitch_bend(routing, channel, value);
            }
            MidiMessage::ChannelAftertouch { channel, pressure, .. } => {
                self.handle_midi_aftertouch(routing, channel, pressure);
            }
            // Ignore other messages for now
            _ => {}
        }
    }

    /// Handle MIDI note on event.
    fn handle_midi_note_on(&mut self, routing: &MidiRouting, channel: u8, note: u8, velocity: u8) {
        // First check for note callbacks and queue them
        let callback_ids: Vec<u64> = routing
            .find_note_callbacks(channel, note, true)
            .iter()
            .map(|cb| cb.callback_id)
            .collect();

        // Queue callbacks for execution by the script thread
        if !callback_ids.is_empty() {
            self.shared.with_state_write(|state| {
                for id in &callback_ids {
                    state.midi_config.routing.queue_callback(*id, velocity as i64);
                }
            });
        }

        // Then check for note-specific routes (drum pads)
        if let Some(note_route) = routing.find_note_route(channel, note) {
            let vel = note_route.velocity_curve.apply(velocity);

            // Record the note-on for this voice
            self.record_midi_note_on(channel, note, velocity, &note_route.voice_name);

            // Handle choke groups
            if let Some(choke_group) = &note_route.choke_group {
                self.handle_choke_group(choke_group, &note_route.voice_name);
            }

            // Build parameters
            let mut params = vec![
                ("note".to_string(), note as f32),
                ("freq".to_string(), 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)),
                ("velocity".to_string(), vel),
                ("gate".to_string(), 1.0),
            ];

            // Add velocity-mapped parameters
            for (param_name, min, max) in &note_route.velocity_params {
                let value = min + vel * (max - min);
                params.push((param_name.clone(), value));
            }

            self.trigger_voice(&note_route.voice_name, None, None, params);
            return;
        }

        // Then check keyboard routes
        let keyboard_routes = routing.find_keyboard_routes(channel, note);
        for route in keyboard_routes {
            let transposed_note = route.transpose_note(note);
            let vel = route.velocity_curve.apply(velocity);

            // Record the note-on for this voice
            self.record_midi_note_on(channel, note, velocity, &route.voice_name);

            // Send note on to the voice
            self.handle_note_on(&route.voice_name, transposed_note, (vel * 127.0) as u8, None);
        }
    }

    /// Handle MIDI note off event.
    fn handle_midi_note_off(&mut self, routing: &MidiRouting, channel: u8, note: u8) {
        // First check for note callbacks (note-off) and queue them
        let callback_ids: Vec<u64> = routing
            .find_note_callbacks(channel, note, false)
            .iter()
            .map(|cb| cb.callback_id)
            .collect();

        // Queue callbacks for execution by the script thread
        if !callback_ids.is_empty() {
            self.shared.with_state_write(|state| {
                for id in &callback_ids {
                    state.midi_config.routing.queue_callback(*id, 0); // Note-off doesn't have velocity
                }
            });
        }

        // Then check for note-specific routes (drum pads usually don't need note off, but handle it)
        if let Some(note_route) = routing.find_note_route(channel, note) {
            // Complete the recorded note for this voice
            self.record_midi_note_off(channel, note, &note_route.voice_name);

            // Look up tracked node IDs for this note
            let node_ids: Vec<i32> = self.shared.with_state_read(|state| {
                if let Some(voice) = state.voices.get(&note_route.voice_name) {
                    voice.active_notes.get(&note).cloned().unwrap_or_default()
                } else {
                    Vec::new()
                }
            });

            if node_ids.is_empty() {
                // Fallback to legacy behavior
                self.handle_note_off(&note_route.voice_name, note, None);
            } else {
                for node_id in node_ids {
                    self.handle_note_off(&note_route.voice_name, note, Some(node_id));
                }
            }
            return;
        }

        // Then check keyboard routes
        let keyboard_routes = routing.find_keyboard_routes(channel, note);
        for route in keyboard_routes {
            let transposed_note = route.transpose_note(note);

            // Complete the recorded note for this voice
            self.record_midi_note_off(channel, note, &route.voice_name);

            // Look up tracked node IDs for this note
            let node_ids: Vec<i32> = self.shared.with_state_read(|state| {
                if let Some(voice) = state.voices.get(&route.voice_name) {
                    voice.active_notes.get(&transposed_note).cloned().unwrap_or_default()
                } else {
                    Vec::new()
                }
            });

            if node_ids.is_empty() {
                // Fallback to legacy behavior
                self.handle_note_off(&route.voice_name, transposed_note, None);
            } else {
                for node_id in node_ids {
                    self.handle_note_off(&route.voice_name, transposed_note, Some(node_id));
                }
            }
        }
    }

    /// Record a MIDI note-on event for pattern/melody export.
    fn record_midi_note_on(&mut self, channel: u8, note: u8, velocity: u8, voice_name: &str) {
        // Get current recording state and transport position
        let (recording_enabled, quantization, beats_per_bar) = self.shared.with_state_read(|state| {
            (
                state.midi_recording.recording_enabled,
                state.midi_recording.quantization,
                state.time_signature.beats_per_bar(),
            )
        });

        if !recording_enabled {
            return;
        }

        // Get current beat position
        let raw_beat = self.transport.beat_at(Instant::now()).to_float();

        // Calculate quantized beat
        let grid_size = beats_per_bar / quantization as f64;
        let quantized_beat = (raw_beat / grid_size).round() * grid_size;

        // Store pending note-on (keyed by channel, note, voice)
        let voice = voice_name.to_string();
        self.shared.with_state_write(|state| {
            state
                .midi_recording
                .pending_notes
                .insert((channel, note, voice), (quantized_beat, velocity, raw_beat));
        });
    }

    /// Record a MIDI note-off event, completing the note's duration.
    fn record_midi_note_off(&mut self, channel: u8, note: u8, voice_name: &str) {
        use crate::state::RecordedMidiNote;

        // Try to find a pending note-on for this channel/note/voice
        let voice = voice_name.to_string();
        let pending = self.shared.with_state_write(|state| {
            state
                .midi_recording
                .pending_notes
                .remove(&(channel, note, voice))
        });

        let Some((start_beat, velocity, raw_start)) = pending else {
            return; // No pending note-on, nothing to record
        };

        // Get recording state
        let (recording_enabled, quantization, beats_per_bar, current_beat) =
            self.shared.with_state_read(|state| {
                (
                    state.midi_recording.recording_enabled,
                    state.midi_recording.quantization,
                    state.time_signature.beats_per_bar(),
                    self.transport.beat_at(Instant::now()).to_float(),
                )
            });

        if !recording_enabled {
            return;
        }

        // Calculate quantized end beat
        let grid_size = beats_per_bar / quantization as f64;
        let quantized_end = (current_beat / grid_size).round() * grid_size;

        // Calculate duration (minimum of one grid step)
        let duration = (quantized_end - start_beat).max(grid_size);

        // Create and store the recorded note
        let recorded_note = RecordedMidiNote {
            beat: start_beat,
            note,
            velocity,
            duration,
            raw_beat: raw_start,
            channel,
            voice_name: voice_name.to_string(),
        };

        self.shared.with_state_write(|state| {
            state
                .midi_recording
                .add_note(recorded_note, current_beat, beats_per_bar);
        });
    }

    /// Handle MIDI control change event.
    fn handle_midi_cc(&mut self, routing: &MidiRouting, channel: u8, controller: u8, value: u8) {
        // Check for CC callbacks and queue any that should trigger
        self.shared.with_state_write(|state| {
            state.midi_config.routing.check_and_queue_cc_callbacks(channel, controller, value);
        });

        // Then process CC routes
        let cc_routes = routing.find_cc_routes(channel, controller);

        for route in cc_routes {
            let param_value = route.apply(value);

            match &route.target {
                crate::midi::CcTarget::Voice(voice_name) => {
                    self.shared.with_state_write(|state| {
                        if let Some(voice) = state.voices.get_mut(voice_name) {
                            voice.params.insert(route.param_name.clone(), param_value);
                            state.bump_version();
                        }
                    });

                    // Also update any currently playing synths for this voice
                    let nodes: Vec<i32> = self.shared.with_state_read(|state| {
                        state.active_synths
                            .iter()
                            .filter(|(_, s)| s.voice_names.contains(voice_name))
                            .map(|(id, _)| *id)
                            .collect()
                    });

                    let current_beat = self.transport.beat_at(Instant::now()).to_float();
                    for node_id in nodes {
                        let _ = self.osc_sender.n_set(
                            OscTiming::Now,
                            NodeId::new(node_id),
                            &[(&route.param_name, param_value)],
                            current_beat,
                        );
                    }
                }
                crate::midi::CcTarget::Effect(effect_id) => {
                    let node_id = self.shared.with_state_read(|state| {
                        state.effects.get(effect_id).and_then(|e| e.node_id)
                    });

                    if let Some(node_id) = node_id {
                        let current_beat = self.transport.beat_at(Instant::now()).to_float();
                        let _ = self.osc_sender.n_set(
                            OscTiming::Now,
                            NodeId::new(node_id),
                            &[(&route.param_name, param_value)],
                            current_beat,
                        );
                    }
                }
                crate::midi::CcTarget::Group(group_path) => {
                    self.handle_set_group_param(group_path, &route.param_name, param_value);
                }
                crate::midi::CcTarget::Global(param_name) => {
                    // Handle global parameters (e.g., tempo)
                    match param_name.as_str() {
                        "tempo" | "bpm" => {
                            let now = Instant::now();
                            self.transport.set_bpm(param_value as f64, now);
                            self.shared.with_state_write(|state| {
                                state.tempo = param_value as f64;
                                state.bump_version();
                            });
                            self.osc_sender.set_tempo(param_value as f64);
                        }
                        _ => {
                            log::debug!("Unknown global parameter: {}", param_name);
                        }
                    }
                }
            }
        }
    }

    /// Handle MIDI pitch bend event.
    fn handle_midi_pitch_bend(&mut self, routing: &MidiRouting, channel: u8, value: i16) {
        let routes = routing.find_pitch_bend_routes(channel);

        // Convert pitch bend to 0.0-1.0 range
        let normalized = (value as f32 + 8192.0) / 16383.0;

        for route in routes {
            let param_value = route.curve.apply(normalized, route.min_value, route.max_value);

            match &route.target {
                crate::midi::CcTarget::Voice(voice_name) => {
                    // Update voice parameter
                    self.shared.with_state_write(|state| {
                        if let Some(voice) = state.voices.get_mut(voice_name) {
                            voice.params.insert(route.param_name.clone(), param_value);
                            state.bump_version();
                        }
                    });

                    // Update running synths
                    let nodes: Vec<i32> = self.shared.with_state_read(|state| {
                        state.active_synths
                            .iter()
                            .filter(|(_, s)| s.voice_names.contains(voice_name))
                            .map(|(id, _)| *id)
                            .collect()
                    });

                    let current_beat = self.transport.beat_at(Instant::now()).to_float();
                    for node_id in nodes {
                        let _ = self.osc_sender.n_set(
                            OscTiming::Now,
                            NodeId::new(node_id),
                            &[(&route.param_name, param_value)],
                            current_beat,
                        );
                    }
                }
                _ => {
                    // Handle other targets if needed
                }
            }
        }
    }

    /// Handle MIDI channel aftertouch.
    fn handle_midi_aftertouch(&mut self, routing: &MidiRouting, channel: u8, pressure: u8) {
        // Use aftertouch routes if configured
        if let Some(routes) = routing.aftertouch_routes.get(&channel)
            .or_else(|| routing.aftertouch_routes.get(&255))
        {
            let normalized = pressure as f32 / 127.0;

            for route in routes {
                let param_value = route.curve.apply(normalized, route.min_value, route.max_value);

                if let crate::midi::CcTarget::Voice(voice_name) = &route.target {
                    // Update running synths
                    let nodes: Vec<i32> = self.shared.with_state_read(|state| {
                        state.active_synths
                            .iter()
                            .filter(|(_, s)| s.voice_names.contains(voice_name))
                            .map(|(id, _)| *id)
                            .collect()
                    });

                    let current_beat = self.transport.beat_at(Instant::now()).to_float();
                    for node_id in nodes {
                        let _ = self.osc_sender.n_set(
                            OscTiming::Now,
                            NodeId::new(node_id),
                            &[(&route.param_name, param_value)],
                            current_beat,
                        );
                    }
                }
            }
        }
    }

    /// Handle choke groups - stop all notes in the same choke group.
    fn handle_choke_group(&mut self, _choke_group: &str, _voice_name: &str) {
        // TODO: Implement choke group logic
        // This would stop all currently playing notes from voices in the same choke group
    }

    // Note: MIDI callbacks are now queued for execution by the script thread.
    // See the callback execution mechanism in the API layer.

    /// Poll for OSC messages from scsynth (e.g., /n_end notifications)
    fn poll_osc_messages(&mut self) {
        // Process all available OSC messages
        loop {
            match self.sc.osc.try_recv_msg() {
                Ok(Some(packet)) => {
                    self.handle_osc_packet(packet);
                }
                Ok(None) => break, // No more messages
                Err(e) => {
                    log::trace!("OSC recv error (usually harmless): {}", e);
                    break;
                }
            }
        }
    }

    /// Handle an incoming OSC packet from scsynth
    fn handle_osc_packet(&mut self, packet: rosc::OscPacket) {
        match packet {
            rosc::OscPacket::Message(msg) => {
                match msg.addr.as_str() {
                    "/n_go" => {
                        // /n_go node_id group_id prev_node_id next_node_id is_group
                        if msg.args.len() >= 5 {
                            if let (
                                Some(rosc::OscType::Int(node_id)),
                                Some(rosc::OscType::Int(group_id)),
                                _,
                                _,
                                Some(rosc::OscType::Int(is_group)),
                            ) = (
                                msg.args.first(),
                                msg.args.get(1),
                                msg.args.get(2),
                                msg.args.get(3),
                                msg.args.get(4),
                            ) {
                                self.handle_message(StateMessage::NodeCreated {
                                    node_id: *node_id,
                                    group_id: *group_id,
                                    is_group: *is_group != 0,
                                });
                            }
                        }
                    }
                    "/n_end" => {
                        // /n_end node_id group_id prev_node_id next_node_id is_group
                        if let Some(rosc::OscType::Int(node_id)) = msg.args.first() {
                            log::trace!("[OSC] Node {} ended", node_id);
                            self.handle_message(StateMessage::NodeDestroyed {
                                node_id: *node_id,
                            });
                        }
                    }
                    "/done" => {
                        // /done /command_name [args...]
                        // For /b_allocRead: /done /b_allocRead bufnum
                        if msg.args.len() >= 2 {
                            if let (
                                Some(rosc::OscType::String(cmd)),
                                Some(rosc::OscType::Int(buffer_id)),
                            ) = (msg.args.first(), msg.args.get(1))
                            {
                                if cmd == "/b_allocRead" {
                                    log::debug!("[OSC] Buffer {} loaded", buffer_id);
                                    self.handle_message(StateMessage::BufferLoaded {
                                        buffer_id: *buffer_id,
                                    });
                                }
                            }
                        }
                    }
                    "/fail" => {
                        // Log failures - temporarily at debug level to diagnose MIDI issues
                        log::debug!("[OSC] scsynth failure: {:?}", msg.args);
                    }
                    "/tr" => {
                        // /tr node_id trig_id value
                        // Handle both meter data and MIDI triggers from SendTrig
                        // Meter Trig IDs: 0=peak_left, 1=peak_right, 2=rms_left, 3=rms_right
                        // MIDI Trig IDs: 100+ (see midi_synthdefs::trigger_ids)
                        if msg.args.len() >= 3 {
                            // Log all incoming triggers at debug level for debugging
                            if let (
                                Some(rosc::OscType::Int(node_id)),
                                Some(rosc::OscType::Int(trig_id)),
                                Some(rosc::OscType::Float(value)),
                            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2)) {
                                if *trig_id >= 100 {
                                    log::debug!("[/tr] MIDI trigger: node={} trig_id={} value={}", node_id, trig_id, value);
                                }
                            }
                            // First try to handle as MIDI trigger
                            if self.midi_osc_handler.handle_osc(&msg.addr, &msg.args) {
                                // Was a MIDI trigger, already handled
                            } else if let (
                                Some(rosc::OscType::Int(node_id)),
                                Some(rosc::OscType::Int(trig_id)),
                                Some(rosc::OscType::Float(value)),
                            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
                            {
                                // Not a MIDI trigger, try meter trigger
                                self.handle_meter_trigger(*node_id, *trig_id, *value);
                            }
                        }
                    }
                    _ => {
                        // Ignore other messages
                    }
                }
            }
            rosc::OscPacket::Bundle(_) => {
                // Ignore bundles for now
            }
        }
    }

    /// Handle meter trigger data from link synths.
    /// Called when we receive /tr messages from SendTrig UGens.
    /// Trig IDs: 0=peak_left, 1=peak_right, 2=rms_left, 3=rms_right
    fn handle_meter_trigger(&mut self, node_id: i32, trig_id: i32, value: f32) {
        // Find the group path that has this node_id as its link_synth_node_id
        let group_path = self.shared.with_state_read(|state| {
            state
                .groups
                .iter()
                .find(|(_, g)| g.link_synth_node_id == Some(node_id))
                .map(|(path, _)| path.clone())
        });

        if let Some(path) = group_path {
            self.shared.with_state_write(|state| {
                let meter = state.meter_levels.entry(path).or_insert_with(|| {
                    crate::state::MeterLevel::default()
                });

                // All 4 triggers fire on the same Impulse, representing one meter update cycle.
                // Store L/R values separately for stereo meter display.
                match trig_id {
                    0 => {
                        // peak_left
                        meter.peak_left = value;
                    }
                    1 => {
                        // peak_right
                        meter.peak_right = value;
                    }
                    2 => {
                        // rms_left
                        meter.rms_left = value;
                    }
                    3 => {
                        // rms_right - this completes the cycle
                        meter.rms_right = value;
                        meter.last_update = Some(std::time::Instant::now());
                    }
                    _ => {}
                }
            });
        }
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.message_rx.try_recv() {
            self.handle_message(msg);
        }
    }

    fn handle_message(&mut self, msg: StateMessage) {
        match msg {
            // === Transport ===
            StateMessage::SetBpm { bpm } => {
                let now = Instant::now();
                self.transport.set_bpm(bpm, now);
                self.shared.with_state_write(|state| {
                    state.tempo = bpm;
                    state.bump_version();
                });
                // Also update OscSender's tempo for score capture timing
                self.osc_sender.set_tempo(bpm);

                // Update SC-managed MIDI clock synth tempo if running
                self.update_sc_midi_clock_tempo(bpm);
            }
            StateMessage::SetQuantization { beats } => {
                self.shared.with_state_write(|state| {
                    state.quantization_beats = beats.max(EPSILON);
                    state.bump_version();
                });
            }
            StateMessage::SetTimeSignature {
                numerator,
                denominator,
            } => {
                self.transport
                    .set_time_signature(numerator, denominator, Instant::now());
                self.shared.with_state_write(|state| {
                    state.time_signature = TimeSignature::new(numerator, denominator);
                    state.bump_version();
                });
            }
            StateMessage::StartScheduler => {
                let now = Instant::now();
                self.transport.start(now);
                // Don't reset scheduler here - seek handles that
                self.shared.with_state_write(|state| {
                    state.transport_running = true;
                    state.bump_version();
                });

                // Send MIDI start message if clock output is enabled
                let clock_enabled = self.shared.with_state_read(|state| {
                    state.midi_output_config.clock_output_enabled
                });
                if clock_enabled {
                    self.send_midi_clock_message(crate::midi::QueuedMidiEvent::start());
                    log::info!("[MIDI CLOCK] Sent START message");
                }
            }
            StateMessage::StopScheduler => {
                let now = Instant::now();
                let current_beat = self.transport.beat_at(now).to_float();

                // Send MIDI stop message if clock output is enabled
                let clock_enabled = self.shared.with_state_read(|state| {
                    state.midi_output_config.clock_output_enabled
                });
                if clock_enabled {
                    self.send_midi_clock_message(crate::midi::QueuedMidiEvent::stop());
                    log::info!("[MIDI CLOCK] Sent STOP message");
                }

                // Stop transport and prevent new events
                self.transport.stop(now);
                self.scheduler.sync_to_beat(current_beat);
                self.shared.with_state_write(|state| {
                    state.transport_running = false;
                    state.bump_version();
                });

                // Collect all nodes that need gate=0 (active notes + pending)
                let nodes_to_release: Vec<i32> = self.shared.with_state_write(|state| {
                    use std::collections::HashSet;
                    let mut nodes: HashSet<i32> = HashSet::new();

                    // All active notes
                    for voice in state.voices.values_mut() {
                        for node_ids in voice.active_notes.values() {
                            nodes.extend(node_ids.iter().copied());
                        }
                        voice.active_notes.clear();
                    }

                    // All active synths
                    for &node_id in state.active_synths.keys() {
                        nodes.insert(node_id);
                    }

                    // All pending nodes (scheduled but may not have started yet)
                    for &node_id in state.pending_nodes.keys() {
                        nodes.insert(node_id);
                    }

                    // Clear scheduled note-offs since we're releasing everything
                    state.scheduled_note_offs.clear();

                    nodes.into_iter().collect()
                });

                // Send gate=0 to release all notes
                log::debug!(
                    "[STOP] Pausing at beat {}, sending gate=0 to {} nodes",
                    current_beat,
                    nodes_to_release.len()
                );
                for node_id in nodes_to_release {
                    let _ = self.osc_sender.n_set(
                        OscTiming::Now,
                        NodeId::new(node_id),
                        &[("gate", 0.0f32)],
                        current_beat,
                    );
                }
            }
            StateMessage::SeekTransport { beat } => {
                let now = Instant::now();
                let target_beat = beat.max(0.0);
                self.transport.seek(BeatTime::from_float(target_beat), now);
                // Reset scheduler to target beat to prevent event burst
                self.scheduler.reset_to_beat(target_beat);
                self.shared.with_state_write(|state| {
                    state.current_beat = target_beat;
                    // Reset sequence anchors so they restart cleanly from target beat
                    for active in state.active_sequences.values_mut() {
                        active.anchor_beat = target_beat;
                        active.triggered_clips.clear();
                        active.last_iteration = 0;
                        active.completed = false;
                    }
                    state.bump_version();
                });
            }
            StateMessage::BeginReload => {
                // Capture a snapshot of current state BEFORE incrementing generation.
                // This snapshot will be used to diff against the new state after script execution.
                let snapshot = self.capture_state_snapshot();
                self.reload_manager.begin_reload(snapshot);

                // Update quantization from state
                let quantization = self.shared.with_state_read(|s| s.quantization_beats);
                self.reload_manager.set_quantization(quantization);

                // Increment generation for tracking which entities were touched
                // Also clear MIDI routing (but keep devices connected) so scripts can re-register routes
                self.shared.with_state_write(|state| {
                    state.reload_generation += 1;
                    // Clear MIDI routing but keep devices - routes will be re-registered by script
                    state.midi_config.routing.clear();
                    state.midi_config.callbacks.clear();
                    state.bump_version();
                });
                log::debug!("[MIDI] Cleared routing on reload (devices preserved)");
            }
            StateMessage::SetScrubMute { muted } => {
                self.shared.with_state_write(|state| {
                    state.scrub_muted = muted;
                    state.bump_version();
                });
            }

            // === SynthDefs ===
            StateMessage::LoadSynthDef { name, bytes } => {
                log::debug!("Loading synthdef '{}'", name);
                // Store bytes in state for score capture
                self.shared.with_state_write(|state| {
                    state.synthdefs.insert(name.clone(), bytes.clone());
                });

                // Capture to score if enabled - add /d_recv at time 0
                if let Some(writer) = self.osc_sender.score_writer_mut() {
                    let packet = rosc::OscPacket::Message(rosc::OscMessage {
                        addr: "/d_recv".to_string(),
                        args: vec![rosc::OscType::Blob(bytes.clone())],
                    });
                    // Synthdefs should be at time 0 (before any notes play)
                    writer.add_packet(0.0, packet);
                    log::debug!("[SCORE] Captured synthdef '{}' at time 0", name);
                }

                if let Err(e) = self.sc.d_recv_bytes(bytes) {
                    log::error!("Failed to load synthdef '{}': {}", name, e);
                }
            }

            // === Groups ===
            StateMessage::RegisterGroup {
                name,
                path,
                parent_path,
                node_id,
                source_location,
            } => {
                self.handle_register_group(name, path, parent_path, node_id, source_location);
            }
            StateMessage::UnregisterGroup { path } => {
                self.shared.with_state_write(|state| {
                    state.groups.remove(&path);
                    state.bump_version();
                });
            }
            StateMessage::SetGroupParam { path, param, value } => {
                self.handle_set_group_param(&path, &param, value);
            }
            StateMessage::MuteGroup { path } => {
                self.set_group_run_state(&path, false);
            }
            StateMessage::UnmuteGroup { path } => {
                self.set_group_run_state(&path, true);
            }
            StateMessage::SoloGroup { path, solo } => {
                self.shared.with_state_write(|state| {
                    if let Some(group) = state.groups.get_mut(&path) {
                        group.soloed = solo;
                        state.bump_version();
                    }
                });
            }
            StateMessage::FinalizeGroups => {
                self.finalize_groups();
            }

            // === Voices ===
            StateMessage::UpsertVoice {
                name,
                group_path,
                group_name,
                synth_name,
                polyphony,
                gain,
                muted,
                soloed,
                output_bus,
                params,
                sfz_instrument,
                vst_instrument,
                source_location,
                midi_output_device_id,
                midi_channel,
                cc_mappings,
            } => {
                let generation = self.shared.with_state_read(|s| s.reload_generation);
                // Check if gain changed and get running node if any
                let (gain_changed, running_node) = self.shared.with_state_read(|state| {
                    if let Some(voice) = state.voices.get(&name) {
                        let changed = (voice.gain - gain).abs() > 0.0001;
                        (changed, voice.running_node_id)
                    } else {
                        (false, None)
                    }
                });

                self.shared.with_state_write(|state| {
                    let voice = state.voices.entry(name.clone()).or_insert_with(|| {
                        VoiceState::new(name.clone(), group_path.clone())
                    });
                    voice.group_path = group_path;
                    voice.group_name = group_name;
                    voice.synth_name = synth_name;
                    voice.polyphony = polyphony;
                    voice.gain = gain;
                    voice.muted = muted;
                    voice.soloed = soloed;
                    voice.output_bus = output_bus;
                    voice.params = params;
                    voice.sfz_instrument = sfz_instrument;
                    voice.vst_instrument = vst_instrument;
                    voice.generation = generation;
                    voice.source_location = source_location;
                    voice.midi_output_device_id = midi_output_device_id;
                    voice.midi_channel = midi_channel;
                    voice.cc_mappings = cc_mappings;
                    state.bump_version();
                });

                // If gain changed and voice has a running synth, update it
                if gain_changed {
                    if let Some(node_id) = running_node {
                        let current_beat = self.transport.beat_at(Instant::now()).to_float();
                        let _ = self.osc_sender.n_set(
                            OscTiming::Now,
                            NodeId::new(node_id),
                            &[("amp", gain as f32)],
                            current_beat,
                        );
                        log::debug!("[VOICE] Updated running node {} gain to {}", node_id, gain);
                    }
                }
            }
            StateMessage::DeleteVoice { name } => {
                self.shared.with_state_write(|state| {
                    state.voices.remove(&name);
                    state.bump_version();
                });
            }
            StateMessage::SetVoiceParam { name, param, value } => {
                // Check if this voice has MIDI CC mapping for this param
                let midi_cc_info = self.shared.with_state_read(|state| {
                    if let Some(voice) = state.voices.get(&name) {
                        if let Some(device_id) = voice.midi_output_device_id {
                            if let Some(&cc_num) = voice.cc_mappings.get(&param) {
                                let channel = voice.midi_channel.unwrap_or(0);
                                if let Some(device) = state.midi_output_config.devices.get(&device_id) {
                                    return Some((device.event_tx.clone(), channel, cc_num));
                                }
                            }
                        }
                    }
                    None
                });

                // Send MIDI CC if mapped
                if let Some((event_tx, channel, cc_num)) = midi_cc_info {
                    // Convert 0.0-1.0 to 0-127
                    let cc_value = (value.clamp(0.0, 1.0) * 127.0) as u8;
                    let midi_event = crate::midi::QueuedMidiEvent::control_change(channel, cc_num, cc_value);
                    let _ = event_tx.send(midi_event);
                    log::debug!("[MIDI_OUT] Voice '{}' CC: {}={} (param='{}', ch={})",
                        name, cc_num, cc_value, param, channel + 1);
                }

                // Always update the local state too
                self.shared.with_state_write(|state| {
                    if let Some(voice) = state.voices.get_mut(&name) {
                        voice.params.insert(param, value);
                        state.bump_version();
                    }
                });
            }
            StateMessage::MuteVoice { name } => {
                self.shared.with_state_write(|state| {
                    if let Some(voice) = state.voices.get_mut(&name) {
                        voice.muted = true;
                        state.bump_version();
                    }
                });
            }
            StateMessage::UnmuteVoice { name } => {
                self.shared.with_state_write(|state| {
                    if let Some(voice) = state.voices.get_mut(&name) {
                        voice.muted = false;
                        state.bump_version();
                    }
                });
            }
            StateMessage::TriggerVoice {
                name,
                synth_name,
                group_path,
                params,
            } => {
                self.trigger_voice(&name, synth_name, group_path, params);
            }
            StateMessage::NoteOn {
                voice_name,
                note,
                velocity,
                duration,
            } => {
                self.handle_note_on(&voice_name, note, velocity, duration);
            }
            StateMessage::NoteOff { voice_name, note } => {
                self.handle_note_off(&voice_name, note, None);
            }

            // === Patterns ===
            StateMessage::CreatePattern {
                name,
                group_path,
                voice_name,
                pattern,
                source_location,
                step_pattern,
            } => {
                let generation = self.shared.with_state_read(|s| s.reload_generation);
                self.shared.with_state_write(|state| {
                    let ps = state.patterns.entry(name.clone()).or_insert_with(|| {
                        PatternState::new(name.clone(), group_path.clone(), voice_name.clone())
                    });
                    ps.loop_pattern = Some(pattern);
                    ps.generation = generation;
                    ps.group_path = group_path;
                    ps.voice_name = voice_name;
                    ps.source_location = source_location;
                    ps.step_pattern = step_pattern;
                    state.bump_version();
                });
            }
            StateMessage::DeletePattern { name } => {
                // Reset scheduler tracking to prevent ghost events when pattern is recreated
                self.scheduler.reset_loop(&name);
                self.shared.with_state_write(|state| {
                    state.patterns.remove(&name);
                    state.bump_version();
                });
            }
            StateMessage::SetPatternParam { name, param, value } => {
                self.shared.with_state_write(|state| {
                    if let Some(p) = state.patterns.get_mut(&name) {
                        p.params.insert(param, value);
                        state.bump_version();
                    }
                });
            }
            StateMessage::StartPattern { name } => {
                self.queue_loop_start(&name, LoopKind::Pattern);
            }
            StateMessage::StopPattern { name } => {
                self.stop_loop(&name, LoopKind::Pattern);
            }

            // === Melodies ===
            StateMessage::CreateMelody {
                name,
                group_path,
                voice_name,
                pattern,
                source_location,
                notes_patterns,
            } => {
                let generation = self.shared.with_state_read(|s| s.reload_generation);
                self.shared.with_state_write(|state| {
                    let ms = state.melodies.entry(name.clone()).or_insert_with(|| {
                        MelodyState::new(name.clone(), group_path.clone(), voice_name.clone())
                    });
                    ms.loop_pattern = Some(pattern);
                    ms.generation = generation;
                    ms.group_path = group_path;
                    ms.voice_name = voice_name;
                    ms.source_location = source_location;
                    ms.notes_patterns = notes_patterns;
                    state.bump_version();
                });
            }
            StateMessage::DeleteMelody { name } => {
                self.shared.with_state_write(|state| {
                    state.melodies.remove(&name);
                    state.bump_version();
                });
            }
            StateMessage::SetMelodyParam { name, param, value } => {
                self.shared.with_state_write(|state| {
                    if let Some(m) = state.melodies.get_mut(&name) {
                        m.params.insert(param, value);
                        state.bump_version();
                    }
                });
            }
            StateMessage::StartMelody { name } => {
                self.queue_loop_start(&name, LoopKind::Melody);
            }
            StateMessage::StopMelody { name } => {
                self.stop_loop(&name, LoopKind::Melody);
            }

            // === Sequences ===
            StateMessage::CreateSequence { sequence } => {
                use crate::sequences::ClipSource;

                log::trace!("Creating sequence '{}' with {} clips", sequence.name, sequence.clips.len());

                // Collect names of removed clips (patterns, melodies, fades) from old sequence
                let (removed_patterns, removed_melodies, fade_names_to_remove): (Vec<String>, Vec<String>, Vec<String>) =
                    self.shared.with_state_read(|state| {
                        if let Some(old_seq) = state.sequences.get(&sequence.name) {
                            // Collect names from old sequence
                            let old_patterns: std::collections::HashSet<_> = old_seq.clips.iter().filter_map(|clip| {
                                if let ClipSource::Pattern(name) = &clip.source { Some(name.clone()) } else { None }
                            }).collect();
                            let old_melodies: std::collections::HashSet<_> = old_seq.clips.iter().filter_map(|clip| {
                                if let ClipSource::Melody(name) = &clip.source { Some(name.clone()) } else { None }
                            }).collect();
                            let old_fades: Vec<_> = old_seq.clips.iter().filter_map(|clip| {
                                if let ClipSource::Fade(name) = &clip.source { Some(name.clone()) } else { None }
                            }).collect();

                            // Collect names from new sequence
                            let new_patterns: std::collections::HashSet<_> = sequence.clips.iter().filter_map(|clip| {
                                if let ClipSource::Pattern(name) = &clip.source { Some(name.clone()) } else { None }
                            }).collect();
                            let new_melodies: std::collections::HashSet<_> = sequence.clips.iter().filter_map(|clip| {
                                if let ClipSource::Melody(name) = &clip.source { Some(name.clone()) } else { None }
                            }).collect();

                            // Find removed clips (in old but not in new)
                            let removed_patterns: Vec<_> = old_patterns.difference(&new_patterns).cloned().collect();
                            let removed_melodies: Vec<_> = old_melodies.difference(&new_melodies).cloned().collect();

                            (removed_patterns, removed_melodies, old_fades)
                        } else {
                            (Vec::new(), Vec::new(), Vec::new())
                        }
                    });

                // Stop synths triggered by removed pattern/melody clips
                // This prevents "hanging notes" when clips are removed from a sequence
                if !removed_patterns.is_empty() || !removed_melodies.is_empty() {
                    let nodes_to_release: Vec<i32> = self.shared.with_state_write(|state| {
                        let nodes: Vec<i32> = state.active_synths.iter().filter(|(_, synth)| {
                            // Check if this synth was triggered by a removed pattern or melody
                            synth.pattern_names.iter().any(|p| removed_patterns.contains(p)) ||
                            synth.melody_names.iter().any(|m| removed_melodies.contains(m))
                        }).map(|(id, _)| *id).collect();

                        // Remove from tracking
                        for &node_id in &nodes {
                            state.active_synths.remove(&node_id);
                            state.pending_nodes.remove(&node_id);
                        }

                        nodes
                    });

                    // Release the synths by setting gate=0
                    if !nodes_to_release.is_empty() {
                        log::info!(
                            "[SEQUENCE] Releasing {} synths from removed clips (patterns: {:?}, melodies: {:?})",
                            nodes_to_release.len(),
                            removed_patterns,
                            removed_melodies
                        );
                        let current_beat = self.transport.beat_at(Instant::now()).to_float();
                        for node_id in nodes_to_release {
                            let _ = self.osc_sender.n_set(OscTiming::Now, NodeId::new(node_id), &[("gate", 0.0f32)], current_beat);
                        }
                    }
                }

                let generation = self.shared.with_state_read(|s| s.reload_generation);
                self.shared.with_state_write(|state| {
                    // Remove fades that match the old sequence's fade clips
                    if !fade_names_to_remove.is_empty() {
                        let removed_count = state.fades.len();
                        state.fades.retain(|fade| {
                            // Check if this fade was triggered by one of the removed clips
                            // by matching fade target with fade definitions
                            !fade_names_to_remove.iter().any(|fade_name| {
                                if let Some(fade_def) = state.fade_defs.get(fade_name) {
                                    fade_def.target_name == fade.target_name &&
                                    fade_def.param_name == fade.param_name &&
                                    fade_def.target_type == fade.target_type
                                } else {
                                    false
                                }
                            })
                        });
                        let removed_count = removed_count - state.fades.len();
                        if removed_count > 0 {
                            log::debug!("[SEQUENCE] Removed {} stale fades from old sequence '{}'",
                                removed_count, sequence.name);
                        }
                    }

                    let mut seq = sequence;
                    seq.generation = generation;
                    state.sequences.insert(seq.name.clone(), seq);
                    state.bump_version();
                });
            }
            StateMessage::StartSequence { name } => {
                log::trace!("Starting sequence '{}'", name);
                self.start_sequence(&name, false);
            }
            StateMessage::StartSequenceOnce { name } => {
                log::trace!("Starting sequence '{}' (once)", name);
                self.start_sequence(&name, true);
            }
            StateMessage::StopSequence { name } => {
                self.shared.with_state_write(|state| {
                    state.active_sequences.remove(&name);
                    state.bump_version();
                });
            }
            StateMessage::DeleteSequence { name } => {
                self.shared.with_state_write(|state| {
                    state.sequences.remove(&name);
                    state.active_sequences.remove(&name);
                    state.bump_version();
                });
            }
            StateMessage::SequenceCompleted { name } => {
                // This message is mainly for notification purposes.
                // The sequence is already marked as completed and removed from active_sequences.
                log::info!("Sequence '{}' completed", name);
                // Notify via completion channel if set
                if let Some(ref tx) = self.completion_tx {
                    let _ = tx.send(name);
                }
            }

            // === Fades ===
            StateMessage::CreateFadeDefinition { fade } => {
                self.shared.with_state_write(|state| {
                    state.fade_defs.insert(fade.name.clone(), fade);
                    state.bump_version();
                });
            }
            StateMessage::FadeGroupParam { .. }
            | StateMessage::FadeVoiceParam { .. }
            | StateMessage::FadePatternParam { .. }
            | StateMessage::FadeMelodyParam { .. }
            | StateMessage::FadeEffectParam { .. } => {
                // TODO: Implement fade automation
                log::debug!("Fade automation not yet implemented");
            }

            StateMessage::CancelFade { target_type, target_name, param_name } => {
                self.shared.with_state_write(|state| {
                    let before_count = state.fades.len();
                    state.fades.retain(|fade| {
                        !(fade.target_type == target_type &&
                          fade.target_name == target_name &&
                          fade.param_name == param_name)
                    });
                    let removed = before_count - state.fades.len();
                    if removed > 0 {
                        log::debug!("Cancelled {} fade(s) on {}.{}", removed, target_name, param_name);
                        state.bump_version();
                    }
                });
            }

            // === Effects ===
            StateMessage::AddEffect {
                id,
                synthdef,
                group_path,
                params,
                bus_in: _,
                bus_out: _,
                source_location,
            } => {
                self.handle_add_effect(id, synthdef, group_path, params, source_location);
            }
            StateMessage::RemoveEffect { id } => {
                let node_to_free = self.shared.with_state_write(|state| {
                    let node = state.effects.remove(&id).and_then(|e| e.node_id);
                    state.bump_version();
                    node
                });
                if let Some(node_id) = node_to_free {
                    let current_beat = self.transport.beat_at(Instant::now()).to_float();
                    let _ = self.osc_sender.n_free(OscTiming::Now, NodeId::new(node_id), current_beat);
                }
            }
            StateMessage::SetEffectParam { id, param, value } => {
                let node_to_update = self.shared.with_state_write(|state| {
                    let node_id = state.effects.get_mut(&id).and_then(|effect| {
                        effect.params.insert(param.clone(), value);
                        effect.node_id
                    });
                    state.bump_version();
                    node_id
                });
                if let Some(node_id) = node_to_update {
                    let current_beat = self.transport.beat_at(Instant::now()).to_float();
                    let _ = self.osc_sender.n_set(OscTiming::Now, NodeId::new(node_id), &[(param.as_str(), value)], current_beat);
                }
            }

            // === Samples ===
            StateMessage::LoadSample { id, path, resolved_path, .. } => {
                // Use the pre-resolved path if available, otherwise fall back to the raw path
                let actual_path = resolved_path.unwrap_or(path);
                self.handle_load_sample(id, actual_path);
            }
            StateMessage::FreeSample { id } => {
                let buffer_to_free = self.shared.with_state_write(|state| {
                    let buf = state.samples.remove(&id).map(|s| s.buffer_id);
                    state.bump_version();
                    buf
                });
                if let Some(buffer_id) = buffer_to_free {
                    let current_beat = self.transport.beat_at(Instant::now()).to_float();
                    let _ = self.osc_sender.b_free(OscTiming::Now, BufNum::new(buffer_id), current_beat);
                }
            }

            // === SFZ ===
            StateMessage::LoadSfzInstrument { id, sfz_path } => {
                log::info!("Loading SFZ instrument '{}' from {:?}", id, sfz_path);

                // Get the next buffer ID from state
                let mut next_buffer_id = self.shared.with_state_read(|state| state.next_buffer_id);

                // Clone sc for the closure
                let sc_clone = self.sc.clone();

                // Load the SFZ instrument using the callback-based loader
                let result = vibelang_sfz::load_sfz_instrument(
                    &sfz_path,
                    id.clone(),
                    &mut |path, buffer_id| {
                        sc_clone.b_alloc_read(BufNum::new(buffer_id), path)
                            .map_err(|e| anyhow::anyhow!("Buffer load failed: {}", e))
                    },
                    &mut next_buffer_id,
                );

                match result {
                    Ok(instrument) => {
                        log::info!(
                            "Loaded SFZ instrument '{}' with {} regions",
                            id,
                            instrument.num_regions()
                        );

                        // Give SuperCollider time to load buffers
                        std::thread::sleep(std::time::Duration::from_millis(500));

                        // Store in state
                        self.shared.with_state_write(|state| {
                            state.sfz_instruments.insert(id.clone(), instrument);
                            state.next_buffer_id = next_buffer_id;
                            state.bump_version();
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to load SFZ instrument '{}': {}", id, e);
                    }
                }
            }

            // === VST ===
            StateMessage::LoadVstInstrument { .. }
            | StateMessage::VstNoteOn { .. }
            | StateMessage::VstNoteOff { .. }
            | StateMessage::SetVstParam { .. }
            | StateMessage::SetVstParamByName { .. } => {
                // TODO: Implement VST support
                log::debug!("VST not yet implemented");
            }

            // === Events ===
            StateMessage::ScheduleEvent { event, start_beat } => {
                self.shared.with_state_write(|state| {
                    state.scheduled_events.push(ScheduledEvent {
                        beat: start_beat,
                        event,
                    });
                    state.bump_version();
                });
            }
            StateMessage::RegisterSequenceRun { name, anchor_beat } => {
                self.shared.with_state_write(|state| {
                    state.sequence_runs.push(SequenceRunLog {
                        name,
                        anchor_beat,
                        started_at: std::time::SystemTime::now(),
                    });
                    state.bump_version();
                });
            }

            // === Control Change ===
            StateMessage::ControlChange { .. } => {
                // TODO: Implement MIDI CC
            }
            StateMessage::StopVoice { .. } => {
                // TODO: Implement voice stopping
            }
            StateMessage::PauseSequence { .. } | StateMessage::ResumeSequence { .. } => {
                // TODO: Implement sequence pause/resume
            }

            // === Running Voices (for continuous processing like line-in) ===
            StateMessage::RunVoice { name } => {
                self.handle_run_voice(name);
            }

            // === MIDI Device Management ===
            StateMessage::MidiOpenDevice {
                device_id,
                info,
                backend,
            } => {
                self.shared.with_state_write(|state| {
                    let device_state = crate::state::MidiDeviceState {
                        id: device_id,
                        info,
                        backend,
                        generation: state.reload_generation,
                    };
                    state.midi_config.devices.insert(device_id, device_state);
                    state.bump_version();
                });
                log::info!("[MIDI] Device {} registered in state", device_id);
            }

            StateMessage::MidiCloseDevice { device_id } => {
                self.shared.with_state_write(|state| {
                    state.midi_config.devices.remove(&device_id);
                    state.bump_version();
                });
                log::info!("[MIDI] Device {} removed from state", device_id);
            }

            StateMessage::MidiCloseAllDevices => {
                self.shared.with_state_write(|state| {
                    state.midi_config.devices.clear();
                    state.bump_version();
                });
                log::info!("[MIDI] All devices removed from state");
            }

            // === MIDI Routing ===
            StateMessage::MidiAddKeyboardRoute { route } => {
                self.shared.with_state_write(|state| {
                    state.midi_config.routing.add_keyboard_route(route);
                    state.bump_version();
                });
            }

            StateMessage::MidiAddNoteRoute {
                channel,
                note,
                route,
            } => {
                self.shared.with_state_write(|state| {
                    state.midi_config.routing.add_note_route(channel, note, route);
                    state.bump_version();
                });
            }

            StateMessage::MidiAddCcRoute {
                channel,
                cc_number,
                route,
            } => {
                self.shared.with_state_write(|state| {
                    state.midi_config.routing.add_cc_route(channel, cc_number, route);
                    state.bump_version();
                });
            }

            StateMessage::MidiAddPitchBendRoute { channel, route } => {
                self.shared.with_state_write(|state| {
                    state.midi_config.routing.add_pitch_bend_route(channel, route);
                    state.bump_version();
                });
            }

            StateMessage::MidiClearRouting => {
                self.shared.with_state_write(|state| {
                    state.midi_config.clear_routing();
                    state.bump_version();
                });
                log::info!("[MIDI] All routing cleared");
            }

            // === MIDI Callbacks ===
            StateMessage::MidiRegisterNoteCallback {
                callback_id,
                channel,
                note,
                on_note_on,
                on_note_off,
            } => {
                self.shared.with_state_write(|state| {
                    // Add to routing
                    let callback = crate::midi::NoteCallback {
                        channel,
                        note,
                        on_note_on,
                        on_note_off,
                        callback_id,
                    };
                    state.midi_config.routing.add_note_callback(callback);

                    // Track callback metadata
                    state.midi_config.callbacks.insert(
                        callback_id,
                        crate::state::MidiCallbackInfo {
                            id: callback_id,
                            callback_type: crate::state::MidiCallbackType::Note {
                                channel,
                                note,
                                on_note_on,
                                on_note_off,
                            },
                            generation: state.reload_generation,
                        },
                    );
                    state.bump_version();
                });
            }

            StateMessage::MidiRegisterCcCallback {
                callback_id,
                channel,
                cc_number,
                threshold,
                above_threshold,
            } => {
                self.shared.with_state_write(|state| {
                    // Add to routing
                    let callback = crate::midi::CcCallback {
                        channel,
                        cc_number,
                        threshold,
                        above_threshold,
                        callback_id,
                    };
                    state.midi_config.routing.add_cc_callback(callback);

                    // Track callback metadata
                    state.midi_config.callbacks.insert(
                        callback_id,
                        crate::state::MidiCallbackInfo {
                            id: callback_id,
                            callback_type: crate::state::MidiCallbackType::Cc {
                                channel,
                                cc_number,
                                threshold,
                                above_threshold,
                            },
                            generation: state.reload_generation,
                        },
                    );
                    state.bump_version();
                });
            }

            StateMessage::MidiSetMonitoring { enabled } => {
                self.shared.with_state_write(|state| {
                    state.midi_config.monitor_enabled = enabled;
                    state.midi_config.routing.monitor_enabled = enabled;
                    state.bump_version();
                });
                log::info!("[MIDI] Monitoring set to {}", enabled);
            }

            // === MIDI Recording ===
            StateMessage::MidiSetRecordingQuantization { positions_per_bar } => {
                // Validate: must be 4, 8, 16, 32, or 64
                if [4, 8, 16, 32, 64].contains(&positions_per_bar) {
                    self.shared.with_state_write(|state| {
                        state.midi_recording.quantization = positions_per_bar;
                        state.bump_version();
                    });
                    log::debug!("[MIDI] Recording quantization set to 1/{}", positions_per_bar);
                } else {
                    log::warn!(
                        "[MIDI] Invalid recording quantization {}, must be 4, 8, 16, 32, or 64",
                        positions_per_bar
                    );
                }
            }
            StateMessage::MidiSetRecordingEnabled { enabled } => {
                self.shared.with_state_write(|state| {
                    state.midi_recording.recording_enabled = enabled;
                    state.bump_version();
                });
                log::info!("[MIDI] Recording {}", if enabled { "enabled" } else { "disabled" });
            }
            StateMessage::MidiClearRecording => {
                self.shared.with_state_write(|state| {
                    state.midi_recording.clear();
                    state.bump_version();
                });
                log::info!("[MIDI] Recording history cleared");
            }

            // === MIDI Output ===
            StateMessage::MidiOutputOpenDevice { device_id, info, event_tx } => {
                // Create a MidiOutputHandle for the OSC handler
                // The OSC handler uses event_tx for immediate delivery when SC triggers fire
                let handle = crate::midi::MidiOutputHandle::new(
                    device_id,
                    info.clone(),
                    event_tx.clone(),
                );

                // Register with the MIDI OSC handler for SC-managed MIDI
                self.midi_osc_handler.register_device(device_id, handle);

                self.shared.with_state_write(|state| {
                    let device_state = crate::state::MidiOutputDeviceState::new(
                        device_id,
                        info,
                        event_tx,
                    );
                    state.midi_output_config.devices.insert(device_id, device_state);
                    state.bump_version();
                });
                log::info!("[MIDI OUTPUT] Opened device {} (registered with SC-managed MIDI)", device_id);
            }

            StateMessage::MidiOutputCloseDevice { device_id } => {
                // Unregister from MIDI OSC handler
                self.midi_osc_handler.unregister_device(device_id);

                self.shared.with_state_write(|state| {
                    state.midi_output_config.devices.remove(&device_id);
                    state.bump_version();
                });
                log::info!("[MIDI OUTPUT] Closed device {}", device_id);
            }

            StateMessage::MidiOutputCloseAllDevices => {
                self.shared.with_state_write(|state| {
                    state.midi_output_config.devices.clear();
                    state.bump_version();
                });
                log::info!("[MIDI OUTPUT] Closed all devices");
            }

            StateMessage::MidiOutputNoteOn { device_id, channel, note, velocity } => {
                if let Some(device) = self.shared.with_state_read(|state| {
                    state.midi_output_config.devices.get(&device_id).cloned()
                }) {
                    let _ = device.event_tx.send(crate::midi::QueuedMidiEvent::note_on(channel, note, velocity));
                }
            }

            StateMessage::MidiOutputNoteOff { device_id, channel, note } => {
                if let Some(device) = self.shared.with_state_read(|state| {
                    state.midi_output_config.devices.get(&device_id).cloned()
                }) {
                    let _ = device.event_tx.send(crate::midi::QueuedMidiEvent::note_off(channel, note));
                }
            }

            StateMessage::MidiOutputControlChange { device_id, channel, controller, value } => {
                if let Some(device) = self.shared.with_state_read(|state| {
                    state.midi_output_config.devices.get(&device_id).cloned()
                }) {
                    let _ = device.event_tx.send(crate::midi::QueuedMidiEvent::control_change(channel, controller, value));
                }
            }

            StateMessage::MidiOutputPitchBend { device_id, channel, value } => {
                if let Some(device) = self.shared.with_state_read(|state| {
                    state.midi_output_config.devices.get(&device_id).cloned()
                }) {
                    let _ = device.event_tx.send(crate::midi::QueuedMidiEvent::pitch_bend(channel, value));
                }
            }

            StateMessage::MidiOutputSetClockEnabled { enabled } => {
                // Get clock device ID and tempo before updating state
                let (device_id, bpm) = self.shared.with_state_read(|state| {
                    let device_id = state.midi_output_config.clock_device_id
                        .or_else(|| state.midi_output_config.devices.keys().next().copied());
                    (device_id, state.tempo)
                });

                self.shared.with_state_write(|state| {
                    state.midi_output_config.clock_output_enabled = enabled;
                    state.bump_version();
                });

                // Start or stop the SC-managed MIDI clock synth
                if enabled {
                    if let Some(device_id) = device_id {
                        self.start_sc_midi_clock(device_id, bpm);
                    } else {
                        log::warn!("[SC-MIDI CLOCK] No MIDI output device available for clock");
                    }
                } else {
                    self.stop_sc_midi_clock();
                }

                log::info!("[MIDI OUTPUT] Clock output {} (SC-managed)", if enabled { "enabled" } else { "disabled" });
            }

            StateMessage::MidiOutputSetClockDevice { device_id } => {
                self.shared.with_state_write(|state| {
                    state.midi_output_config.clock_device_id = device_id;
                    state.bump_version();
                });
            }

            StateMessage::MidiOutputSendStart => {
                self.send_midi_clock_message(crate::midi::QueuedMidiEvent::start());
            }

            StateMessage::MidiOutputSendStop => {
                self.send_midi_clock_message(crate::midi::QueuedMidiEvent::stop());
            }

            StateMessage::MidiOutputSendContinue => {
                self.send_midi_clock_message(crate::midi::QueuedMidiEvent::continue_msg());
            }

            // === OSC Feedback ===
            StateMessage::NodeCreated { .. } => {}
            StateMessage::NodeDestroyed { node_id } => {
                self.shared.with_state_write(|state| {
                    state.active_synths.remove(&node_id);
                    // Also clean up node from voices' active_notes
                    for voice in state.voices.values_mut() {
                        for node_ids in voice.active_notes.values_mut() {
                            node_ids.retain(|&id| id != node_id);
                        }
                        // Clean up empty entries
                        voice.active_notes.retain(|_, ids| !ids.is_empty());
                    }
                    state.bump_version();
                });
            }
            StateMessage::BufferLoaded { .. } => {}

            // === Score Capture ===
            StateMessage::EnableScoreCapture { path } => {
                log::info!(
                    "[SCORE] Enabling score capture to {} (all events will be captured from beat 0)",
                    path.display()
                );

                // Reset transport to beat 0 for clean recording
                // This ensures sequences start at beat 0 (or quantized from beat 0)
                let now = std::time::Instant::now();
                self.transport.seek(crate::timing::BeatTime::ZERO, now);
                log::info!("[SCORE] Transport reset to beat 0");

                // Update OscSender's tempo
                let tempo = self.shared.with_state_read(|s| s.tempo);
                self.osc_sender.set_tempo(tempo);

                // Enable capture in OscSender
                self.osc_sender.enable_capture(path);

                // Add all loaded synthdefs at time 0
                let synthdefs: Vec<(String, Vec<u8>)> = self.shared.with_state_read(|s| {
                    s.synthdefs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                });
                if let Some(writer) = self.osc_sender.score_writer_mut() {
                    for (name, bytes) in &synthdefs {
                        log::debug!("[SCORE] Adding synthdef '{}' ({} bytes) at time 0", name, bytes.len());
                        writer.add_message(0.0, "/d_recv", vec![rosc::OscType::Blob(bytes.clone())]);
                    }
                }
                log::info!("[SCORE] Added {} synthdefs at time 0", synthdefs.len());

                // Add group creation messages at time 0
                let groups: Vec<(String, i32, i32)> = self.shared.with_state_read(|s| {
                    let mut result = Vec::new();
                    for (path, g) in &s.groups {
                        // Skip groups without a node ID
                        let Some(node_id) = g.node_id else { continue };
                        let parent_id = match &g.parent_path {
                            Some(parent_path) => {
                                match s.groups.get(parent_path).and_then(|p| p.node_id) {
                                    Some(id) => id,
                                    None => continue, // Skip if parent not found or has no node_id
                                }
                            }
                            None => 0, // Root group (parent is scsynth root node 0)
                        };
                        result.push((path.clone(), node_id, parent_id));
                    }
                    result
                });
                if let Some(writer) = self.osc_sender.score_writer_mut() {
                    for (path, node_id, parent_id) in &groups {
                        log::debug!("[SCORE] Adding group '{}' (node {}) under parent {}", path, node_id, parent_id);
                        // /g_new node_id add_action target_id
                        // add_action 0 = add to head
                        writer.add_message(0.0, "/g_new", vec![
                            rosc::OscType::Int(*node_id),
                            rosc::OscType::Int(0), // add to head
                            rosc::OscType::Int(*parent_id),
                        ]);
                    }
                }
                log::info!("[SCORE] Added {} groups at time 0", groups.len());

                // Add link synths (system_link_audio) that route audio from group buses to output
                // These are created by FinalizeGroups and are essential for audio routing
                let link_synths: Vec<(String, i32, i32, i32, i32)> = self.shared.with_state_read(|s| {
                    let mut result = Vec::new();
                    for (path, g) in &s.groups {
                        // Only include groups that have link synths
                        let Some(link_node_id) = g.link_synth_node_id else { continue };
                        let Some(group_node_id) = g.node_id else { continue };
                        let in_bus = g.audio_bus;

                        // Determine output bus (parent's bus or 0 for main output)
                        let out_bus = g.parent_path.as_ref()
                            .and_then(|pp| s.groups.get(pp).map(|pg| pg.audio_bus))
                            .unwrap_or(0);

                        result.push((path.clone(), link_node_id, group_node_id, in_bus, out_bus));
                    }
                    result
                });
                if let Some(writer) = self.osc_sender.score_writer_mut() {
                    for (path, link_node_id, group_node_id, in_bus, out_bus) in &link_synths {
                        log::debug!("[SCORE] Adding link synth for '{}' (node {}, inbus={}, outbus={})",
                            path, link_node_id, in_bus, out_bus);
                        // /s_new synthdef_name node_id add_action target_id [control_pairs...]
                        // add_action 1 = add to tail
                        writer.add_message(0.0, "/s_new", vec![
                            rosc::OscType::String("system_link_audio".to_string()),
                            rosc::OscType::Int(*link_node_id),
                            rosc::OscType::Int(1), // add to tail
                            rosc::OscType::Int(*group_node_id),
                            rosc::OscType::String("inbus".to_string()),
                            rosc::OscType::Float(*in_bus as f32),
                            rosc::OscType::String("outbus".to_string()),
                            rosc::OscType::Float(*out_bus as f32),
                        ]);
                    }
                }
                log::info!("[SCORE] Added {} link synths at time 0", link_synths.len());
            }
            StateMessage::DisableScoreCapture => {
                if self.osc_sender.is_capturing() {
                    let tempo = self.shared.with_state_read(|s| s.tempo);
                    let current_beat = self.shared.with_state_read(|s| s.current_beat);
                    let current_time = crate::score::beats_to_seconds(current_beat, tempo);

                    // Add tail time (2 seconds) for reverb decay
                    let tail_time = 2.0;
                    let end_time = current_time + tail_time;

                    // Add final events to the score writer before disabling
                    if let Some(writer) = self.osc_sender.score_writer_mut() {
                        // Free all synths in the default group (node 1) at end_time
                        // This ensures all synths stop and scsynth can finish rendering
                        writer.add_message(end_time, "/g_freeAll", vec![
                            rosc::OscType::Int(1), // Default group node ID
                        ]);

                        // Add a dummy event at the final time to mark the end of rendering
                        // SuperCollider's Score class uses /c_set 0 0 as a harmless end marker
                        writer.add_message(end_time + 0.5, "/c_set", vec![
                            rosc::OscType::Int(0),    // Control bus index
                            rosc::OscType::Float(0.0), // Value
                        ]);

                        log::info!(
                            "[SCORE] Added tail time: {:.1}s, end time: {:.1}s",
                            tail_time,
                            end_time + 0.5
                        );
                    }

                    // Disable capture - this writes the score file
                    if let Some(path) = self.osc_sender.disable_capture() {
                        log::info!("[SCORE] Score file written successfully to {}", path.display());
                    }
                } else {
                    log::warn!("[SCORE] DisableScoreCapture called but no capture was active");
                }
            }
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();

        // Skip if transport not running
        if !self.shared.with_state_read(|s| s.transport_running) {
            self.last_tick = now;
            return;
        }

        // Clear expired pending nodes - those whose live_instant has passed
        self.shared.with_state_write(|state| {
            state.pending_nodes.retain(|_, live_instant| *live_instant > now);
        });

        // Get current beat
        let current_beat = self.transport.beat_at(now).to_float();
        self.shared.with_state_write(|state| {
            state.current_beat = current_beat;
        });

        // Note: MIDI clock is now managed by SC via the clock synth (start_sc_midi_clock)

        // Process active sequences - start/stop patterns based on current beat
        self.process_active_sequences(current_beat);

        // Process pending reload changes at quantization boundary
        self.process_pending_reload(current_beat);

        // Collect loops that need event expansion
        let loops = self.collect_active_loops();

        // Log active patterns for debugging (only every ~100 ticks to reduce spam)
        static TICK_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let tick = TICK_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if tick % 100 == 0 {
            for lp in &loops {
                if matches!(lp.kind, LoopKind::Pattern) {
                    log::debug!("[TICK] Active pattern '{}' start_beat={:.3} loop_length={:.3} events={}",
                        lp.name, lp.start_beat, lp.pattern.loop_length_beats, lp.pattern.events.len());
                }
            }
        }

        // Collect scheduled events from state
        let scheduled_events: Vec<(BeatEvent, BeatTime)> = self.shared.with_state_read(|state| {
            state
                .scheduled_events
                .iter()
                .map(|e| (e.event.clone(), BeatTime::from_float(e.beat)))
                .collect()
        });

        // Collect due events from the scheduler
        let due_events = self.scheduler.collect_due_events(
            &self.transport,
            now,
            &loops,
            &scheduled_events,
            LOOKAHEAD_MS,
        );

        // Log all due events for debugging
        for (beat_time, events) in &due_events {
            for event in events {
                if event.fade.is_none() {
                    log::debug!("[SCHEDULER] Due event at beat {:.3}: pattern={:?} synth={}",
                        beat_time.to_float(),
                        event.pattern_name,
                        event.synth_def);
                }
            }
        }

        // Fire due events using timed OSC bundles for precise scheduling
        for (beat_time, events) in due_events {
            // Separate fades from synth events
            let mut synth_events = Vec::new();
            for event in events {
                if let Some(fade) = event.fade {
                    // Handle fades immediately (they update internal state)
                    log::info!("[FADE] Starting fade '{}' on {}:{} from {} to {} over {} beats",
                        fade.name, fade.target_name, fade.param_name,
                        fade.start_value, fade.target_value, fade.duration_beats);
                    self.start_fade_from_clip(fade);
                } else {
                    synth_events.push(event);
                }
            }

            // Build and send synth events as a timed bundle
            if !synth_events.is_empty() {
                self.fire_events_bundled(beat_time, synth_events, now);
            }
        }

        // Process scheduled note-offs
        self.process_scheduled_note_offs(current_beat);

        // Update active fades
        self.update_fades(now);

        self.last_tick = now;
    }

    fn collect_active_loops(&mut self) -> Vec<LoopSnapshot> {
        let mut loops = Vec::new();
        let mut clips_to_mark: Vec<(String, String, u64)> = Vec::new(); // (seq_name, clip_id, iteration)
        let mut completed_sequences: Vec<String> = Vec::new(); // Sequences that completed (play_once)
        let current_beat = self.transport.beat_at(Instant::now()).to_float();

        // Calculate lookahead in beats for early completion detection
        let tempo = self.shared.with_state_read(|s| s.tempo);
        let lookahead_seconds = LOOKAHEAD_MS as f64 / 1000.0;
        let lookahead_beats = lookahead_seconds * tempo / 60.0;
        let lookahead_beat = current_beat + lookahead_beats;

        // First pass: update iteration tracking, detect play_once completion, and clear triggered_clips on new iterations
        self.shared.with_state_write(|state| {
            for (seq_name, active) in state.active_sequences.iter_mut() {
                if active.paused || active.completed {
                    continue;
                }
                if let Some(seq_def) = state.sequences.get(seq_name) {
                    if seq_def.loop_beats > EPSILON {
                        let elapsed = (current_beat - active.anchor_beat).max(0.0);
                        let current_iteration = (elapsed / seq_def.loop_beats).floor() as u64;

                        if current_iteration > active.last_iteration {
                            log::debug!(
                                "[SEQUENCE] '{}' entering iteration {} (was {}), clearing triggered_clips",
                                seq_name, current_iteration, active.last_iteration
                            );
                            active.triggered_clips.clear();
                            active.last_iteration = current_iteration;

                            // Check if play_once sequence has completed
                            if seq_def.play_once && current_iteration >= 1 {
                                log::info!("[SEQUENCE] '{}' completed (play_once mode)", seq_name);
                                active.completed = true;
                                completed_sequences.push(seq_name.clone());
                                // NOTE: Don't remove from active_sequences - keep it so is_sequence_completed() works
                            }
                        }

                        // Early completion detection for play_once: if lookahead would reach iteration 1,
                        // mark as completed early to prevent scheduling events in the next iteration
                        if seq_def.play_once && !active.completed {
                            let end_beat = active.anchor_beat + seq_def.loop_beats;
                            if lookahead_beat >= end_beat {
                                log::info!("[SEQUENCE] '{}' completing early (lookahead reached end)", seq_name);
                                active.completed = true;
                                completed_sequences.push(seq_name.clone());
                            }
                        }
                    }
                }
            }
        });

        self.shared.with_state_read(|state| {
            // Collect patterns that are directly playing (via pattern.start())
            for (name, pattern) in &state.patterns {
                if let LoopStatus::Playing { start_beat } = pattern.status {
                    if let Some(ref lp) = pattern.loop_pattern {
                        loops.push(LoopSnapshot {
                            kind: LoopKind::Pattern,
                            name: name.clone(),
                            pattern: lp.clone(),
                            start_beat,
                            voice_name: pattern.voice_name.clone(),
                            group_path: Some(pattern.group_path.clone()),
                        });
                    }
                }
            }

            // Collect melodies that are directly playing (via melody.start())
            for (name, melody) in &state.melodies {
                if let LoopStatus::Playing { start_beat } = melody.status {
                    if let Some(ref lp) = melody.loop_pattern {
                        loops.push(LoopSnapshot {
                            kind: LoopKind::Melody,
                            name: name.clone(),
                            pattern: lp.clone(),
                            start_beat,
                            voice_name: melody.voice_name.clone(),
                            group_path: Some(melody.group_path.clone()),
                        });
                    }
                }
            }

            // Collect active sequences - materialize their clips into events
            if !state.active_sequences.is_empty() {
                log::trace!("[LOOPS] {} active sequences", state.active_sequences.len());
            }
            for (seq_name, active) in &state.active_sequences {
                if active.paused {
                    log::trace!("[LOOPS] Sequence '{}' is paused, skipping", seq_name);
                    continue;
                }
                if active.completed {
                    log::trace!("[LOOPS] Sequence '{}' is completed, skipping", seq_name);
                    continue;
                }
                if let Some(seq_def) = state.sequences.get(seq_name) {
                    // Pass the root sequence's triggered_clips for clip_once tracking
                    let triggered_set = active.triggered_clips.clone();
                    let current_iteration = active.last_iteration;
                    let mut newly_triggered: Vec<String> = Vec::new();
                    if let Some(pattern) = Self::materialize_sequence(
                        seq_def, state, &mut Vec::new(), &triggered_set, &mut newly_triggered
                    ) {
                        let fade_count = pattern.events.iter().filter(|e| e.fade.is_some()).count();
                        log::trace!("[LOOPS] Adding sequence '{}' with {} events ({} fades)",
                            seq_name, pattern.events.len(), fade_count);
                        loops.push(LoopSnapshot {
                            kind: LoopKind::Sequence,
                            name: seq_name.clone(),
                            pattern,
                            start_beat: active.anchor_beat,
                            voice_name: None,
                            group_path: None,
                        });
                        // Track newly triggered clips to mark after state lock
                        for clip_id in newly_triggered {
                            clips_to_mark.push((seq_name.clone(), clip_id, current_iteration));
                        }
                    }
                }
            }
        });

        // Mark triggered clips after releasing state lock
        if !clips_to_mark.is_empty() {
            self.shared.with_state_write(|state| {
                for (seq_name, clip_id, iteration) in &clips_to_mark {
                    if let Some(seq) = state.active_sequences.get_mut(seq_name) {
                        seq.triggered_clips.insert(clip_id.clone(), *iteration);
                    }
                }
            });
        }

        // Send completion notifications for play_once sequences
        for seq_name in completed_sequences {
            if let Some(ref tx) = self.completion_tx {
                let _ = tx.send(seq_name);
            }
        }

        loops
    }

    /// Materialize a sequence definition into a LoopPattern containing all events from its clips.
    fn materialize_sequence(
        def: &crate::sequences::SequenceDefinition,
        state: &crate::state::ScriptState,
        stack: &mut Vec<String>,
        triggered_set: &HashMap<String, u64>,
        newly_triggered: &mut Vec<String>,
    ) -> Option<crate::events::Pattern> {
        use crate::sequences::ClipSource;

        const EPSILON: f64 = 0.0001;

        // Detect cycles
        if stack.contains(&def.name) {
            log::warn!("[SEQUENCE] Cycle detected in '{}', skipping", def.name);
            return None;
        }

        if def.loop_beats <= EPSILON {
            return None;
        }

        stack.push(def.name.clone());

        let mut events: Vec<BeatEvent> = Vec::new();

        for clip in &def.clips {
            let clip_start = clip.start.max(0.0);
            let clip_end = clip.end.min(def.loop_beats);
            if clip_end - clip_start <= EPSILON {
                continue;
            }

            // Check if this is a clip_once that has already been triggered
            // NOTE: Fade clips are excluded from this check because they need to be
            // materialized every tick until the scheduler actually schedules them.
            // The scheduler's loop_last_scheduled handles duplicate prevention for fades.
            if matches!(clip.mode, crate::sequences::ClipMode::Once) && !matches!(clip.source, ClipSource::Fade(_)) {
                let clip_id = match &clip.source {
                    ClipSource::Pattern(name) => format!("pattern:{}", name),
                    ClipSource::Melody(name) => format!("melody:{}", name),
                    ClipSource::Fade(_) => unreachable!(), // Excluded above
                    ClipSource::Sequence(name) => format!("sequence:{}", name),
                };

                if triggered_set.contains_key(&clip_id) {
                    log::debug!(
                        "[SEQUENCE] Skipping clip_once '{}' - already triggered for sequence '{}'",
                        clip_id,
                        def.name
                    );
                    continue;
                }
            }

            match &clip.source {
                ClipSource::Pattern(name) => {
                    if let Some(pat) = state.patterns.get(name).and_then(|p| p.loop_pattern.as_ref()) {
                        Self::append_looping_events(
                            &mut events,
                            &pat.events,
                            pat.loop_length_beats,
                            clip_start,
                            clip_end,
                            &clip.mode,
                            pat.phase_offset,
                            (
                                Some(name.clone()),
                                None,
                                state.patterns.get(name).map(|p| p.group_path.clone()),
                                state.patterns.get(name).and_then(|p| p.voice_name.clone()),
                            ),
                        );
                        // Mark as triggered if clip_once
                        if matches!(clip.mode, crate::sequences::ClipMode::Once) {
                            newly_triggered.push(format!("pattern:{}", name));
                        }
                    } else {
                        log::warn!("[SEQUENCE] Pattern '{}' not found for clip in '{}'", name, def.name);
                    }
                }
                ClipSource::Melody(name) => {
                    if let Some(mel) = state.melodies.get(name).and_then(|m| m.loop_pattern.as_ref()) {
                        let melody_group_path = state.melodies.get(name).map(|m| m.group_path.clone());
                        let voice_name = state.melodies.get(name).and_then(|m| m.voice_name.clone());
                        log::trace!("[SEQUENCE] Melody '{}' group_path={:?} voice={:?} events={}",
                            name, melody_group_path, voice_name, mel.events.len());
                        Self::append_looping_events(
                            &mut events,
                            &mel.events,
                            mel.loop_length_beats,
                            clip_start,
                            clip_end,
                            &clip.mode,
                            mel.phase_offset,
                            (
                                None,
                                Some(name.clone()),
                                melody_group_path,
                                voice_name,
                            ),
                        );
                        // Mark as triggered if clip_once
                        if matches!(clip.mode, crate::sequences::ClipMode::Once) {
                            newly_triggered.push(format!("melody:{}", name));
                        }
                    } else {
                        log::warn!("[SEQUENCE] Melody '{}' not found for clip in '{}'", name, def.name);
                    }
                }
                ClipSource::Sequence(name) => {
                    if let Some(nested_def) = state.sequences.get(name) {
                        // Pass through triggered_set and newly_triggered to nested sequences
                        if let Some(nested_pat) = Self::materialize_sequence(
                            nested_def, state, stack, triggered_set, newly_triggered
                        ) {
                            let fade_count = nested_pat.events.iter().filter(|e| e.fade.is_some()).count();
                            log::trace!("[SEQUENCE] Nested sequence '{}' has {} events ({} fades)",
                                name, nested_pat.events.len(), fade_count);
                            Self::append_looping_events(
                                &mut events,
                                &nested_pat.events,
                                nested_pat.loop_length_beats,
                                clip_start,
                                clip_end,
                                &clip.mode,
                                nested_pat.phase_offset,
                                (None, None, None, None),
                            );
                            // Mark as triggered if clip_once
                            if matches!(clip.mode, crate::sequences::ClipMode::Once) {
                                newly_triggered.push(format!("sequence:{}", name));
                            }
                        }
                    } else {
                        log::warn!("[SEQUENCE] Nested sequence '{}' not found", name);
                    }
                }
                ClipSource::Fade(name) => {
                    // Convert fade clips into BeatEvents with fade info
                    // NOTE: We do NOT mark fades as triggered here. The scheduler's
                    // loop_last_scheduled tracking handles duplicate prevention.
                    // Marking fades as triggered during materialization would prevent
                    // them from being scheduled when their beat actually arrives.
                    log::trace!("[SEQUENCE] Processing fade clip '{}' in sequence '{}'", name, def.name);
                    if let Some(fade_def) = state.fade_defs.get(name) {
                        log::trace!("[SEQUENCE] Found fade def '{}', adding events at {}-{}", name, clip_start, clip_end);
                        Self::append_fade_events(
                            &mut events,
                            fade_def,
                            clip_start,
                            clip_end,
                            &clip.mode,
                            &def.name,
                        );
                    } else {
                        log::warn!("[SEQUENCE] Fade '{}' not found for clip", name);
                    }
                }
            }
        }

        stack.pop();

        // Sort events by beat, but ensure fades come BEFORE notes at the same beat.
        // This is critical: fades must set voice.params before synths are created,
        // so that synths pick up the correct initial amp value.
        events.sort_by(|a, b| {
            match a.beat.partial_cmp(&b.beat) {
                Some(std::cmp::Ordering::Equal) => {
                    // At the same beat, fades come first
                    match (a.fade.is_some(), b.fade.is_some()) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => std::cmp::Ordering::Equal,
                    }
                }
                Some(ord) => ord,
                None => std::cmp::Ordering::Equal,
            }
        });

        let fade_count = events.iter().filter(|e| e.fade.is_some()).count();
        log::trace!("[SEQUENCE] Returning materialized '{}' with {} total events ({} fades)",
            def.name, events.len(), fade_count);

        Some(crate::events::Pattern {
            name: def.name.clone(),
            events,
            loop_length_beats: def.loop_beats,
            phase_offset: 0.0,
        })
    }

    /// Append events from a source pattern to the destination, looping/repeating as needed.
    fn append_looping_events(
        dest: &mut Vec<BeatEvent>,
        source_events: &[BeatEvent],
        source_loop: f64,
        clip_start: f64,
        clip_end: f64,
        mode: &crate::sequences::ClipMode,
        phase_offset: f64,
        meta: (Option<String>, Option<String>, Option<String>, Option<String>),
    ) {
        use crate::sequences::ClipMode;

        const EPSILON: f64 = 0.0001;

        if source_loop <= EPSILON {
            return;
        }

        let mut iteration: usize = 0;
        let max_iterations = match mode {
            ClipMode::Loop => None,
            ClipMode::Once => Some(1),
            ClipMode::LoopCount(n) if *n > 0 => Some(*n as usize),
            ClipMode::LoopCount(_) => Some(0),
        };

        loop {
            if let Some(max) = max_iterations {
                if iteration >= max {
                    break;
                }
            }

            let iter_start = clip_start + (iteration as f64 * source_loop);
            if iter_start >= clip_end - EPSILON {
                break;
            }

            for ev in source_events {
                let beat = iter_start + phase_offset + ev.beat;
                if beat + EPSILON >= clip_end {
                    continue;
                }
                let mut ev = ev.clone();
                if ev.pattern_name.is_none() {
                    ev.pattern_name = meta.0.clone();
                }
                if ev.melody_name.is_none() {
                    ev.melody_name = meta.1.clone();
                }
                if ev.group_path.is_none() {
                    ev.group_path = meta.2.clone();
                }
                if ev.voice_name.is_none() {
                    ev.voice_name = meta.3.clone();
                }
                ev.beat = beat;
                dest.push(ev);
            }

            iteration += 1;
            if matches!(mode, ClipMode::Once) {
                break;
            }
        }
    }

    /// Append fade events from a FadeDefinition to the destination.
    fn append_fade_events(
        dest: &mut Vec<BeatEvent>,
        fade: &crate::sequences::FadeDefinition,
        clip_start: f64,
        clip_end: f64,
        mode: &crate::sequences::ClipMode,
        sequence_name: &str,
    ) {
        use crate::events::FadeClip;
        use crate::sequences::ClipMode;

        const EPSILON: f64 = 0.0001;

        if fade.duration_beats <= EPSILON {
            return;
        }

        let mut iteration: usize = 0;
        let max_iterations = match mode {
            ClipMode::Loop => None,
            ClipMode::Once => Some(1),
            ClipMode::LoopCount(n) if *n > 0 => Some(*n as usize),
            ClipMode::LoopCount(_) => Some(0),
        };

        loop {
            if let Some(max) = max_iterations {
                if iteration >= max {
                    break;
                }
            }

            let start = clip_start + (iteration as f64 * fade.duration_beats);
            if start >= clip_end - EPSILON {
                break;
            }

            dest.push(BeatEvent {
                beat: start,
                synth_def: String::new(),
                controls: Vec::new(),
                group_path: None,
                pattern_name: None,
                melody_name: None,
                voice_name: None,
                fade: Some(FadeClip {
                    name: fade.name.clone(),
                    sequence_name: Some(sequence_name.to_string()),
                    target_type: fade.target_type.clone(),
                    target_name: fade.target_name.clone(),
                    param_name: fade.param_name.clone(),
                    start_value: fade.from,
                    target_value: fade.to,
                    duration_beats: fade.duration_beats,
                }),
            });

            iteration += 1;
            if matches!(mode, ClipMode::Once) {
                break;
            }
        }
    }

    /// Fire multiple events at a specific beat using a timed OSC bundle.
    /// This ensures sample-accurate timing by scheduling with scsynth's timestamp mechanism.
    fn fire_events_bundled(&mut self, beat_time: BeatTime, events: Vec<BeatEvent>, now: Instant) {
        log::debug!("[FIRE_EVENTS] Processing {} events at beat {:.2}", events.len(), beat_time.to_float());
        for event in &events {
            log::debug!("[FIRE_EVENTS]   synth_def='{}' voice={:?} group={:?}",
                event.synth_def, event.voice_name, event.group_path);
        }

        // Skip if scrub muted
        if self.shared.with_state_read(|s| s.scrub_muted) {
            return;
        }

        // Get the Instant when synths will be live (OscSender computes the OSC timestamp internally)
        let (live_instant, _) = self.transport.beat_to_timestamp_and_instant(beat_time, now);

        // Build OSC packets for each event
        let mut packets: Vec<OscPacket> = Vec::new();
        let mut note_offs_to_schedule: Vec<(String, u8, i32, f32)> = Vec::new(); // (voice_name, note, node_id, duration)

        for event in events {
            // Check if this event's voice is routed to MIDI output
            let midi_output_info = event.voice_name.as_ref().and_then(|voice_name| {
                self.shared.with_state_read(|state| {
                    if let Some(voice) = state.voices.get(voice_name) {
                        if let Some(device_id) = voice.midi_output_device_id {
                            let channel = voice.midi_channel.unwrap_or(0);
                            // Check device exists in the midi_osc_handler's registered devices
                            if state.midi_output_config.devices.contains_key(&device_id) {
                                return Some((device_id, channel, voice_name.clone()));
                            }
                        }
                    }
                    None
                })
            });

            // If this is a MIDI voice, create SC-managed MIDI trigger synths
            // These synths use SendTrig to fire OSC messages at sample-accurate times
            if let Some((device_id, channel, voice_name)) = midi_output_info {
                // Extract note and velocity from event controls
                let freq = event.controls.iter()
                    .find(|(k, _)| k == "freq")
                    .map(|(_, v)| *v as f64)
                    .unwrap_or(440.0);
                let note = (69.0 + 12.0 * (freq / 440.0).log2()).round() as u8;
                let velocity = event.controls.iter()
                    .find(|(k, _)| k == "amp")
                    .map(|(_, v)| (*v * 127.0).clamp(0.0, 127.0) as u8)
                    .unwrap_or(100);

                // Get duration for note-off scheduling
                let duration = event.controls.iter()
                    .find(|(k, _)| k == "gate")
                    .map(|(_, v)| *v)
                    .unwrap_or(0.25);

                let off_beat = BeatTime::from_float(beat_time.to_float() + duration as f64);

                log::info!(
                    "[SC-MIDI] Creating MIDI trigger synths: voice='{}' ch={} note={} vel={} on_beat={:.2} off_beat={:.2}",
                    voice_name, channel + 1, note, velocity,
                    beat_time.to_float(), off_beat.to_float()
                );

                // Check polyphony limit and steal oldest voice if needed
                let voice_to_steal = self.shared.with_state_read(|state| {
                    if let Some(voice) = state.voices.get(&voice_name) {
                        let active_count: usize = voice.active_notes.values().map(|v| v.len()).sum();
                        if voice.polyphony > 0 && active_count >= voice.polyphony as usize {
                            // Find oldest note to steal
                            let oldest = voice.active_notes.iter()
                                .flat_map(|(n, ids)| ids.iter().map(move |id| (*n, *id)))
                                .next();
                            if let Some((steal_note, _)) = oldest {
                                log::debug!(
                                    "[SC-MIDI-STEAL] Voice '{}' at polyphony limit ({}), will steal note {}",
                                    voice_name, voice.polyphony, steal_note
                                );
                                return Some(steal_note);
                            }
                        }
                    }
                    None
                });

                // If we need to steal a voice, create note-off packet FIRST (in same bundle, before note-on)
                if let Some(steal_note) = voice_to_steal {
                    // Pack format: (device << 14) | (channel << 7) | note
                    let packed_steal = ((device_id as u32) << 14)
                        | ((channel as u32) << 7)
                        | (steal_note as u32);

                    let steal_node_id = self.shared.with_state_write(|state| state.allocate_synth_node());
                    let steal_packet = rosc::OscPacket::Message(rosc::OscMessage {
                        addr: "/s_new".to_string(),
                        args: vec![
                            rosc::OscType::String("vibelang_midi_note_off".to_string()),
                            rosc::OscType::Int(steal_node_id),
                            rosc::OscType::Int(0), // addToHead
                            rosc::OscType::Int(0), // default group
                            rosc::OscType::String("packed_data".to_string()),
                            rosc::OscType::Float(packed_steal as f32),
                        ],
                    });
                    log::debug!("[SC-MIDI-STEAL] Adding stolen note_off packet: note={} packed={}", steal_note, packed_steal);
                    packets.push(steal_packet);

                    // Remove from active notes (specifically look for -2 marker for SC-managed MIDI)
                    // AND cancel any orphaned scheduled note-off for this note
                    self.shared.with_state_write(|state| {
                        if let Some(voice) = state.voices.get_mut(&voice_name) {
                            if let Some(notes) = voice.active_notes.get_mut(&steal_note) {
                                // Remove one -2 marker (SC-managed MIDI)
                                if let Some(pos) = notes.iter().position(|&id| id == -2) {
                                    notes.remove(pos);
                                } else {
                                    // Fallback: pop any marker
                                    notes.pop();
                                }
                                if notes.is_empty() {
                                    voice.active_notes.remove(&steal_note);
                                }
                            }
                        }
                        // Cancel the orphaned scheduled note-off for the stolen note
                        // This prevents duplicate note-offs when the scheduled time arrives
                        let stolen_voice_name = voice_name.clone();
                        state.scheduled_note_offs.retain(|entry| {
                            let should_remove = entry.voice_name == stolen_voice_name
                                && entry.note == steal_note
                                && entry.node_id == Some(-2);
                            if should_remove {
                                log::debug!(
                                    "[SC-MIDI-STEAL] Canceling orphaned scheduled note-off: voice='{}' note={} beat={:.2}",
                                    entry.voice_name, entry.note, entry.beat
                                );
                            }
                            !should_remove
                        });
                    });
                }

                // Create note-on trigger synth packet with packed MIDI data
                // Pack format: (device << 21) | (channel << 14) | (note << 7) | velocity
                // This allows all data to be sent in a single SendTrig, avoiding accumulator issues
                let packed_note_on = ((device_id as u32) << 21)
                    | ((channel as u32) << 14)
                    | ((note as u32) << 7)
                    | (velocity as u32);

                let note_on_node_id = self.shared.with_state_write(|state| state.allocate_synth_node());
                let note_on_packet = rosc::OscPacket::Message(rosc::OscMessage {
                    addr: "/s_new".to_string(),
                    args: vec![
                        rosc::OscType::String("vibelang_midi_note_on".to_string()),
                        rosc::OscType::Int(note_on_node_id),
                        rosc::OscType::Int(0), // addToHead
                        rosc::OscType::Int(0), // default group
                        rosc::OscType::String("packed_data".to_string()),
                        rosc::OscType::Float(packed_note_on as f32),
                    ],
                });
                log::debug!("[SC-MIDI] Adding note_on packet to bundle: node_id={} packed={} (device={} ch={} note={} vel={})",
                    note_on_node_id, packed_note_on, device_id, channel, note, velocity);
                packets.push(note_on_packet);

                // Track active note for voice stealing
                // Use -2 as marker for SC-managed MIDI notes (the synth handles note-off via OSC)
                // This is different from -1 which is used by the direct MIDI path (handle_note_on)
                self.shared.with_state_write(|state| {
                    if let Some(voice) = state.voices.get_mut(&voice_name) {
                        voice.active_notes.entry(note).or_default().push(-2); // -2 marker for SC-managed MIDI
                    }
                });

                // Schedule note-off as a separate bundle at off_beat with packed MIDI data
                // Pack format: (device << 14) | (channel << 7) | note
                let packed_note_off = ((device_id as u32) << 14)
                    | ((channel as u32) << 7)
                    | (note as u32);

                let note_off_node_id = self.shared.with_state_write(|state| state.allocate_synth_node());
                let note_off_packet = rosc::OscPacket::Message(rosc::OscMessage {
                    addr: "/s_new".to_string(),
                    args: vec![
                        rosc::OscType::String("vibelang_midi_note_off".to_string()),
                        rosc::OscType::Int(note_off_node_id),
                        rosc::OscType::Int(0), // addToHead
                        rosc::OscType::Int(0), // default group
                        rosc::OscType::String("packed_data".to_string()),
                        rosc::OscType::Float(packed_note_off as f32),
                    ],
                });

                // Send note-off bundle slightly before off_beat to ensure proper re-triggering.
                // When the same note is repeated (e.g., "1 1" in a melody), the note-off for
                // the previous note and note-on for the next note have the same beat time.
                // We subtract an offset (0.01 beats ≈ 5ms at 120 BPM, 10ms at 60 BPM) to ensure
                // the note-off fires before the note-on, allowing the synth to re-trigger properly.
                // This margin accounts for network jitter in the OSC round-trip.
                let note_off_beat = BeatTime::from_float((off_beat.to_float() - 0.01).max(0.0));
                log::debug!("[SC-MIDI] Sending note-off bundle at beat {:?} (off_beat={:?})", note_off_beat, off_beat);
                if let Err(e) = self.osc_sender.send_bundle_at_beat(
                    note_off_beat,
                    vec![note_off_packet],
                    &self.transport,
                    now
                ) {
                    log::error!("[SC-MIDI] Failed to send note-off bundle: {}", e);
                }

                // Schedule cleanup of active_notes at off_beat (slightly before to match note-off timing)
                // Use -2 marker to indicate SC-managed MIDI (synth handles the actual MIDI note-off)
                {
                    let voice_name_clone = voice_name.clone();
                    let note_to_schedule = note; // Capture note value explicitly
                    let scheduled_beat = note_off_beat.to_float();
                    log::debug!(
                        "[NOTE_LIFECYCLE] voice='{}' event='SCHEDULED_OFF' note={} marker=-2 beat={:.2}",
                        voice_name, note_to_schedule, scheduled_beat
                    );
                    self.shared.with_state_write(|state| {
                        state.scheduled_note_offs.push(ScheduledNoteOff {
                            beat: scheduled_beat,
                            voice_name: voice_name_clone,
                            note: note_to_schedule,
                            node_id: Some(-2), // -2 marker for SC-managed MIDI (don't send MIDI in handle_note_off)
                        });
                    });
                }

                continue; // Skip regular synth packet building for this event
            }

            if let Some((packet, note_off_info)) = self.build_synth_packet(&event, live_instant) {
                packets.push(packet);
                if let Some((voice_name, note, node_id, duration)) = note_off_info {
                    note_offs_to_schedule.push((voice_name, note, node_id, duration));
                }
            }
        }

        // Send bundle with timetag (via OscSender for centralized handling)
        if !packets.is_empty() {
            log::debug!("[BUNDLE] About to send bundle with {} packets at beat {:?}", packets.len(), beat_time);
            // Debug: Log what packets we're sending
            for packet in &packets {
                if let rosc::OscPacket::Message(msg) = packet {
                    if msg.addr == "/s_new" {
                        if let Some(rosc::OscType::String(name)) = msg.args.first() {
                            log::debug!("[BUNDLE] Sending /s_new for '{}' at beat {:?}", name, beat_time);
                        }
                    }
                }
            }

            // OscSender handles both capturing to score and sending to scsynth
            if let Err(e) = self.osc_sender.send_bundle_at_beat(beat_time, packets, &self.transport, now) {
                log::error!("[BUNDLE] Failed to send timed bundle: {}", e);
            }
        }

        // Schedule note-offs based on the scheduled beat time (not current time)
        let beat_float = beat_time.to_float();
        for (voice_name, note, node_id, duration) in note_offs_to_schedule {
            let off_beat = beat_float + duration as f64;
            log::debug!("[NOTE_OFF] Scheduling note-off for '{}' note {} node {} at beat {} (event_beat={}, duration={})",
                voice_name, note, node_id, off_beat, beat_float, duration);
            self.shared.with_state_write(|state| {
                state.scheduled_note_offs.push(ScheduledNoteOff {
                    beat: off_beat,
                    voice_name,
                    note,
                    node_id: Some(node_id),
                });
            });
        }
    }

    /// Build an OSC packet for a synth event.
    /// Returns the packet and optional note-off scheduling info (voice_name, note, node_id, duration).
    /// `live_instant` is when the synth will be live on scsynth (used for pending node tracking).
    fn build_synth_packet(&mut self, event: &BeatEvent, live_instant: Instant) -> Option<(OscPacket, Option<(String, u8, i32, f32)>)> {
        // Get note and velocity from event for SFZ region matching
        let freq = event.controls.iter()
            .find(|(k, _)| k == "freq")
            .map(|(_, v)| *v as f64)
            .unwrap_or(440.0);
        let note = (69.0 + 12.0 * (freq / 440.0).log2()).round() as u8;
        let velocity = event.controls.iter()
            .find(|(k, _)| k == "amp")
            .map(|(_, v)| (*v * 127.0) as u8)
            .unwrap_or(100);

        // Resolve synth_def, voice info, and optional SFZ parameters (buffer_id, rate)
        let (synth_def, voice_params, voice_gain, sfz_params) = if event.synth_def == "trigger" || event.synth_def == "melody_note" {
            let voice_result = event.voice_name.as_ref().and_then(|voice_name| {
                self.shared.with_state_read(|state| {
                    let voice = state.voices.get(voice_name);
                    if voice.is_none() {
                        log::warn!("[BUILD_SYNTH] Voice '{}' NOT FOUND for event! Available voices: {:?}",
                            voice_name, state.voices.keys().collect::<Vec<_>>());
                    }
                    voice.map(|v| {
                        let synth = v.synth_name.clone().unwrap_or_else(|| event.synth_def.clone());
                        let params = v.params.clone();
                        let gain = v.gain;

                        // Check if this is an SFZ voice
                        let sfz = v.sfz_instrument.as_ref().and_then(|sfz_id| {
                            state.sfz_instruments.get(sfz_id).and_then(|instrument| {
                                // Find matching regions for this note/velocity
                                // TODO: Store round-robin state per voice for proper RR handling
                                let mut rr_state = vibelang_sfz::RoundRobinState::new();
                                let regions = vibelang_sfz::find_matching_regions(
                                    instrument,
                                    note,
                                    velocity,
                                    vibelang_sfz::TriggerMode::Attack,
                                    &mut rr_state,
                                );
                                if regions.is_empty() {
                                    log::warn!("[SFZ] No matching region for note {} velocity {} in '{}'", note, velocity, sfz_id);
                                    None
                                } else {
                                    // Use the first matching region
                                    let region = &regions[0];
                                    let buffer_id = region.buffer_id;
                                    let num_channels = region.num_channels;
                                    let pitch_keycenter = region.opcodes.pitch_keycenter.unwrap_or(note);
                                    // Calculate playback rate: target_freq / sample_root_freq
                                    let target_freq = 440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0);
                                    let sample_root_freq = 440.0 * 2.0_f64.powf((pitch_keycenter as f64 - 69.0) / 12.0);
                                    let rate = (target_freq / sample_root_freq) as f32;
                                    log::debug!("[SFZ] Matched region: buf={}, channels={}, pitch_keycenter={}, rate={:.4}", buffer_id, num_channels, pitch_keycenter, rate);
                                    Some((buffer_id as f32, rate, num_channels))
                                }
                            })
                        });

                        (synth, params, gain, sfz)
                    })
                })
            });
            if voice_result.is_none() && event.voice_name.is_some() {
                log::warn!("[BUILD_SYNTH] Using fallback synthdef '{}' because voice lookup failed",
                    event.synth_def);
            }
            voice_result.unwrap_or_else(|| (event.synth_def.clone(), std::collections::HashMap::new(), 1.0, None))
        } else {
            (event.synth_def.clone(), std::collections::HashMap::new(), 1.0, None)
        };

        // Get group node ID and audio bus
        let (group_id, audio_bus, group_params) = event
            .group_path
            .as_ref()
            .and_then(|path| {
                self.shared.with_state_read(|state| {
                    let result = state.groups.get(path).map(|g| (
                        g.node_id.unwrap_or(1),
                        g.audio_bus,
                        g.params.clone(),
                    ));
                    if result.is_none() {
                        log::warn!("[BUILD_SYNTH] Group '{}' NOT FOUND in state! Available groups: {:?}",
                            path, state.groups.keys().collect::<Vec<_>>());
                    }
                    result
                })
            })
            .unwrap_or_else(|| {
                log::warn!("[BUILD_SYNTH] No group_path in event, using defaults (group=1, bus=0)");
                (1, 0, std::collections::HashMap::new())
            });

        // Build merged controls with MULTIPLICATIVE amp semantics
        // final_amp = event_amp × voice_gain × voice.params["amp"] × group.params["amp"]
        // Each layer is a multiplier: event velocity × voice level × voice fade × group fade
        let mut merged_controls: Vec<(String, f32)> = Vec::new();

        // Voice params (except amp which is handled specially)
        for (k, v) in &voice_params {
            if k != "amp" {
                merged_controls.push((k.clone(), *v));
            }
        }

        // Group params (except amp which is handled specially)
        for (k, v) in &group_params {
            if k != "amp" {
                merged_controls.push((k.clone(), *v));
            }
        }

        // Output bus
        merged_controls.push(("out".to_string(), audio_bus as f32));

        // Calculate final amp with full multiplication chain
        let event_amp = event.controls.iter().find(|(k, _)| k == "amp").map(|(_, v)| *v).unwrap_or(1.0);
        let voice_fade_amp = voice_params.get("amp").copied().unwrap_or(1.0);
        let group_fade_amp = group_params.get("amp").copied().unwrap_or(1.0);
        let final_amp = event_amp * voice_gain as f32 * voice_fade_amp * group_fade_amp;

        if let Some(voice_name) = &event.voice_name {
            log::debug!("[AMP CALC] voice='{}' final={:.4} = event({:.2}) × gain({:.4}) × voice_fade({:.4}) × group_fade({:.4})",
                voice_name, final_amp, event_amp, voice_gain, voice_fade_amp, group_fade_amp);
        }
        merged_controls.push(("amp".to_string(), final_amp));

        // Event params (gate handled specially)
        let mut gate_duration: Option<f32> = None;
        for (k, v) in &event.controls {
            if k == "amp" {
                continue;
            }
            if k == "gate" {
                gate_duration = Some(*v);
                merged_controls.push(("gate".to_string(), 1.0));
            } else {
                merged_controls.push((k.clone(), *v));
            }
        }

        // Add SFZ parameters (buffer ID and playback rate) if this is an SFZ voice
        // Also override synthdef based on sample channel count
        let synth_def = if let Some((buffer_id, rate, num_channels)) = sfz_params {
            merged_controls.push(("bufnum".to_string(), buffer_id));
            merged_controls.push(("rate".to_string(), rate));
            // Select mono or stereo synthdef based on sample channel count
            let sfz_synthdef = if num_channels == 1 {
                "sfz_voice_mono".to_string()
            } else {
                "sfz_voice_stereo".to_string()
            };
            log::debug!("[SFZ] Using synthdef '{}' with bufnum={}, rate={:.4}", sfz_synthdef, buffer_id, rate);
            sfz_synthdef
        } else {
            synth_def
        };

        // Allocate node ID
        let node_id = self.shared.with_state_write(|state| state.allocate_synth_node());

        log::trace!("[S_NEW] Creating synth '{}' node {} in group {} with controls: {:?}",
            synth_def, node_id, group_id,
            merged_controls.iter().map(|(k, v)| format!("{}={:.3}", k, v)).collect::<Vec<_>>());

        // Build OSC message: /s_new synthdef node_id add_action target [controls...]
        // Use AddToHead (0) so voices execute BEFORE effects in the group
        let mut args: Vec<OscType> = vec![
            OscType::String(synth_def),
            OscType::Int(node_id),
            OscType::Int(0), // addToHead - voices must execute before effects
            OscType::Int(group_id),
        ];
        for (k, v) in &merged_controls {
            args.push(OscType::String(k.clone()));
            args.push(OscType::Float(*v));
        }

        let packet = OscPacket::Message(OscMessage {
            addr: "/s_new".to_string(),
            args,
        });

        // Calculate note from freq for tracking
        let freq = event.controls.iter()
            .find(|(k, _)| k == "freq")
            .map(|(_, v)| *v as f64)
            .unwrap_or(440.0);
        let note = (69.0 + 12.0 * (freq / 440.0).log2()).round() as u8;

        // Track the synth
        self.shared.with_state_write(|state| {
            state.active_synths.insert(
                node_id,
                ActiveSynth {
                    node_id,
                    group_paths: event.group_path.iter().cloned().collect(),
                    voice_names: event.voice_name.iter().cloned().collect(),
                    pattern_names: event.pattern_name.iter().cloned().collect(),
                    melody_names: event.melody_name.iter().cloned().collect(),
                },
            );

            // Mark as pending (sent in timed bundle, not yet confirmed on scsynth)
            state.pending_nodes.insert(node_id, live_instant);

            // Also track in voice's active_notes for voice parameter fades
            if let Some(voice_name) = &event.voice_name {
                if let Some(voice) = state.voices.get_mut(voice_name) {
                    voice.active_notes.entry(note).or_default().push(node_id);
                }
            }
        });

        // Prepare note-off info if needed
        let note_off_info = if let Some(duration) = gate_duration {
            event.voice_name.as_ref().map(|name| (name.clone(), note, node_id, duration))
        } else {
            None
        };

        Some((packet, note_off_info))
    }

    #[allow(dead_code)]
    fn fire_event(&mut self, event: BeatEvent) {
        // Skip if scrub muted
        if self.shared.with_state_read(|s| s.scrub_muted) {
            return;
        }

        // Handle fade events - these trigger parameter automation, not synths
        if let Some(fade) = event.fade {
            log::info!("[FADE] Starting fade '{}' on {}:{} from {} to {} over {} beats",
                fade.name, fade.target_name, fade.param_name,
                fade.start_value, fade.target_value, fade.duration_beats);
            self.start_fade_from_clip(fade);
            return;
        }

        // Resolve synth_def and get voice info (params, gain) for merging
        let (synth_def, voice_params, voice_gain) = if event.synth_def == "trigger" || event.synth_def == "melody_note" {
            // Look up the voice's synth name and params
            event.voice_name.as_ref().and_then(|voice_name| {
                self.shared.with_state_read(|state| {
                    state.voices.get(voice_name).map(|v| (
                        v.synth_name.clone().unwrap_or_else(|| event.synth_def.clone()),
                        v.params.clone(),
                        v.gain,
                    ))
                })
            }).unwrap_or_else(|| (event.synth_def.clone(), std::collections::HashMap::new(), 1.0))
        } else {
            (event.synth_def.clone(), std::collections::HashMap::new(), 1.0)
        };

        // Get group node ID and audio bus
        let (group_id, audio_bus, group_params) = event
            .group_path
            .as_ref()
            .and_then(|path| {
                self.shared.with_state_read(|state| {
                    state.groups.get(path).map(|g| (
                        g.node_id.unwrap_or(1),
                        g.audio_bus,
                        g.params.clone(),
                    ))
                })
            })
            .unwrap_or((1, 0, std::collections::HashMap::new()));

        // Build merged controls with MULTIPLICATIVE amp semantics
        // final_amp = event_amp × voice_gain × voice.params["amp"] × group.params["amp"]
        let mut merged_controls: Vec<(String, f32)> = Vec::new();

        // Voice params (except amp which is handled specially)
        for (k, v) in &voice_params {
            if k != "amp" {
                merged_controls.push((k.clone(), *v));
            }
        }

        // Group params (except amp which is handled specially)
        for (k, v) in &group_params {
            if k != "amp" {
                merged_controls.push((k.clone(), *v));
            }
        }

        // Add output bus
        merged_controls.push(("out".to_string(), audio_bus as f32));

        // Calculate final amp with full multiplication chain
        let event_amp = event.controls.iter().find(|(k, _)| k == "amp").map(|(_, v)| *v).unwrap_or(1.0);
        let voice_fade_amp = voice_params.get("amp").copied().unwrap_or(1.0);
        let group_fade_amp = group_params.get("amp").copied().unwrap_or(1.0);
        let final_amp = event_amp * voice_gain as f32 * voice_fade_amp * group_fade_amp;
        merged_controls.push(("amp".to_string(), final_amp));

        // Add event params last (overrides voice/group params), except amp which we already handled
        // Also handle gate specially: extract duration but always send gate=1 to scsynth
        let mut gate_duration: Option<f32> = None;
        for (k, v) in &event.controls {
            if k == "amp" {
                // Already handled above
                continue;
            }
            if k == "gate" {
                // Store duration for scheduling note-off, but send gate=1 to scsynth
                gate_duration = Some(*v);
                merged_controls.push(("gate".to_string(), 1.0));
            } else {
                merged_controls.push((k.clone(), *v));
            }
        }

        let controls: Vec<(&str, f32)> = merged_controls
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();

        // Allocate node ID
        let node_id = self.shared.with_state_write(|state| state.allocate_synth_node());

        // Create synth - use AddToHead so voices execute BEFORE effects in the group
        let current_beat = self.transport.beat_at(Instant::now()).to_float();
        if let Err(e) = self.osc_sender.s_new(
            OscTiming::Now,
            &synth_def,
            NodeId::new(node_id),
            AddAction::AddToHead,
            Target::from(group_id),
            &controls,
            current_beat,
        ) {
            log::error!("Failed to create synth '{}': {}", &synth_def, e);
            return;
        }

        // Track the synth
        self.shared.with_state_write(|state| {
            state.active_synths.insert(
                node_id,
                ActiveSynth {
                    node_id,
                    group_paths: event.group_path.iter().cloned().collect(),
                    voice_names: event.voice_name.iter().cloned().collect(),
                    pattern_names: event.pattern_name.iter().cloned().collect(),
                    melody_names: event.melody_name.iter().cloned().collect(),
                },
            );
        });

        // For events with a gate duration, schedule a note-off
        if let Some(duration) = gate_duration {
            // Extract freq for MIDI note tracking (optional - used by some voice types)
            let freq = event.controls.iter()
                .find(|(k, _)| k == "freq")
                .map(|(_, v)| *v as f64)
                .unwrap_or(440.0);
            // Convert freq back to MIDI note for tracking: note = 69 + 12 * log2(freq / 440)
            let note = (69.0 + 12.0 * (freq / 440.0).log2()).round() as u8;

            if let Some(voice_name) = &event.voice_name {
                let current_beat = self.transport.beat_at(std::time::Instant::now()).to_float();
                let off_beat = current_beat + duration as f64;
                log::debug!("[NOTE_OFF] Scheduling note-off for '{}' note {} at beat {} (current={}, duration={})",
                    voice_name, note, off_beat, current_beat, duration);
                self.shared.with_state_write(|state| {
                    state.scheduled_note_offs.push(ScheduledNoteOff {
                        beat: off_beat,
                        voice_name: voice_name.clone(),
                        note,
                        node_id: Some(node_id),
                    });
                });
            }
        }
    }

    fn handle_register_group(
        &mut self,
        name: String,
        path: String,
        parent_path: Option<String>,
        mut node_id: i32,
        source_location: crate::api::context::SourceLocation,
    ) {
        let generation = self.shared.with_state_read(|s| s.reload_generation);

        // Check if the group already exists in state (was created externally)
        let already_exists = self
            .shared
            .with_state_read(|state| state.groups.contains_key(&path));

        if already_exists {
            // Update generation and source_location even if group exists
            self.shared.with_state_write(|state| {
                if let Some(group) = state.groups.get_mut(&path) {
                    group.generation = generation;
                    group.source_location = source_location;
                }
            });
            log::debug!("Group '{}' already exists, updated generation and source_location", path);
            return;
        }

        // Track if node was pre-created externally (non-zero node_id means SC group already exists)
        let externally_created = node_id != 0;

        // Allocate node ID if not provided (0 means allocate)
        if node_id == 0 {
            node_id = self.shared.with_state_write(|state| state.allocate_group_node());
        }

        // Create the group on SuperCollider (only if not externally created)
        if !externally_created {
            // Get parent's node_id and link_synth_node_id
            let (parent_id, parent_link_synth) = parent_path
                .as_ref()
                .and_then(|pp| {
                    self.shared.with_state_read(|state| {
                        state.groups.get(pp).map(|g| (g.node_id, g.link_synth_node_id))
                    })
                })
                .unwrap_or((Some(0), None));

            let parent_node_id = parent_id.unwrap_or(0);

            // IMPORTANT: If parent has a link synth, we must place the new group BEFORE it!
            // Otherwise the link synth executes before the child group's audio is written,
            // causing the child's audio to never reach the parent's bus.
            let (add_action, target) = if let Some(link_node) = parent_link_synth {
                log::info!(
                    "[GROUP] Creating group '{}' BEFORE parent's link synth (node {})",
                    path, link_node
                );
                (AddAction::AddBefore, Target::from(link_node))
            } else {
                log::info!(
                    "[GROUP] Creating group '{}' at HEAD of parent (node {})",
                    path, parent_node_id
                );
                (AddAction::AddToHead, Target::from(parent_node_id))
            };

            if let Err(e) = self.osc_sender.g_new(
                NodeId::new(node_id),
                add_action,
                target,
            ) {
                log::error!("Failed to create group '{}': {}", path, e);
                return;
            }
        }

        // Allocate audio bus (always required, never optional)
        let audio_bus = self.shared.with_state_write(|state| state.allocate_audio_bus());

        // Store in state
        self.shared.with_state_write(|state| {
            let mut group = GroupState::new(name, path.clone(), parent_path, audio_bus);
            group.node_id = Some(node_id);
            group.generation = generation;
            group.source_location = source_location;
            state.groups.insert(path, group);
            state.bump_version();
        });
    }

    fn handle_set_group_param(&mut self, path_or_name: &str, param: &str, value: f32) {
        // Update state - try to find group by path first, then by name
        let actual_path = self.shared.with_state_write(|state| {
            // First try exact path match
            if let Some(group) = state.groups.get_mut(path_or_name) {
                group.params.insert(param.to_string(), value);
                state.bump_version();
                return Some(path_or_name.to_string());
            }

            // If not found, search by name (path ending in .name or just name)
            let found_path = state.groups.iter().find(|(path, g)| {
                g.name == path_or_name || path.ends_with(&format!(".{}", path_or_name))
            }).map(|(path, _)| path.clone());

            if let Some(ref path) = found_path {
                if let Some(group) = state.groups.get_mut(path) {
                    group.params.insert(param.to_string(), value);
                    state.bump_version();
                }
            }

            found_path
        });

        match &actual_path {
            Some(path) => {
                log::trace!("[GROUP PARAM] Set {}:{}={}", path, param, value);

                // For amp parameter, also update the link synth in real-time
                // This allows the mixer fader to have immediate effect on audio output
                if param == "amp" {
                    let link_node_id = self.shared.with_state_read(|state| {
                        state.groups.get(path).and_then(|g| g.link_synth_node_id)
                    });

                    if let Some(node_id) = link_node_id {
                        // Send n_set to update the link synth's amp parameter
                        let current_beat = self.transport.beat_at(Instant::now()).to_float();
                        let _ = self.osc_sender.n_set(
                            OscTiming::Now,
                            NodeId::new(node_id),
                            &[("amp", value)],
                            current_beat,
                        );
                        log::trace!("[GROUP PARAM] Updated link synth {} amp={}", node_id, value);
                    }
                }

                // Note: For other params, we only update state, not running synths.
                // Group params affect NEW synths - the final amp is calculated as:
                // event_amp × voice_gain × group_amp × voice_amp
            }
            None => {
                log::trace!("[GROUP PARAM] Group '{}' not found when setting {}={}", path_or_name, param, value);
            }
        }
    }

    fn set_group_run_state(&mut self, path: &str, running: bool) {
        let node_to_set = self.shared.with_state_write(|state| {
            let node_id = state.groups.get_mut(path).and_then(|group| {
                group.muted = !running;
                group.node_id
            });
            state.bump_version();
            node_id
        });
        if let Some(node_id) = node_to_set {
            let current_beat = self.transport.beat_at(Instant::now()).to_float();
            let _ = self.osc_sender.n_run(OscTiming::Now, NodeId::new(node_id), running, current_beat);
        }
    }

    fn finalize_groups(&mut self) {
        // Get all groups that need link synths, along with their last effect node
        // IMPORTANT: Sort groups so children are processed BEFORE parents.
        // This ensures parent link synths execute AFTER children have written to the parent bus.

        // Debug: log all groups and their link synth status
        self.shared.with_state_read(|state| {
            log::info!("[FINALIZE_GROUPS] Checking {} groups for link synth creation", state.groups.len());
            for (path, g) in &state.groups {
                log::info!("[FINALIZE_GROUPS]   '{}': link_synth={:?}, audio_bus={:?}, node_id={:?}",
                    path, g.link_synth_node_id, g.audio_bus, g.node_id);
            }
        });

        let mut groups: Vec<(String, i32, Option<String>, i32, Option<i32>)> = self.shared.with_state_read(|state| {
            state
                .groups
                .values()
                .filter(|g| g.link_synth_node_id.is_none())
                .map(|g| {
                    // Find all effects for this group, sorted by position
                    let mut group_effects: Vec<_> = state
                        .effects
                        .values()
                        .filter(|e| e.group_path == g.path)
                        .collect();
                    group_effects.sort_by_key(|e| e.position);

                    // Get the last effect's node ID (if any)
                    let last_effect_node = group_effects.last().and_then(|e| e.node_id);

                    (
                        g.path.clone(),
                        g.audio_bus,  // audio_bus is always set (i32, not Option)
                        g.parent_path.clone(),
                        g.node_id.unwrap_or(0),
                        last_effect_node,
                    )
                })
                .collect()
        });

        log::info!("[FINALIZE_GROUPS] {} groups need link synths: {:?}",
            groups.len(), groups.iter().map(|(p, _, _, _, _)| p.as_str()).collect::<Vec<_>>());

        // Sort by path depth (descending) so children are processed before parents.
        // Children have more slashes in their path (e.g., "main/Drums" vs "main").
        groups.sort_by(|a, b| {
            let depth_a = a.0.matches('/').count();
            let depth_b = b.0.matches('/').count();
            depth_b.cmp(&depth_a) // Reverse order: deeper first
        });

        for (path, in_bus, parent_path, group_node_id, last_effect_node) in groups {
            // Determine output bus (parent's bus or 0 for main output)
            let out_bus = parent_path
                .as_ref()
                .and_then(|pp| {
                    self.shared.with_state_read(|state| {
                        state.groups.get(pp).map(|g| g.audio_bus)
                    })
                })
                .unwrap_or(0);

            // Skip if in_bus == out_bus (e.g., main group with audio_bus=0 and no parent)
            // Creating a link synth that reads from and writes to the same bus would just double the audio
            if in_bus == out_bus {
                log::debug!(
                    "[LINK] Skipping link synth for '{}': in_bus == out_bus == {}",
                    path, in_bus
                );
                continue;
            }

            // Allocate link synth node
            let link_node_id = self.shared.with_state_write(|state| state.allocate_synth_node());

            // Create the link synth AFTER all effects
            // - If there are effects, add after the last one
            // - If no effects, add to tail (after voices)
            let (add_action, target) = if let Some(last_node) = last_effect_node {
                log::info!(
                    "[LINK] Creating link synth for '{}' AFTER last effect (node {})",
                    path,
                    last_node
                );
                (AddAction::AddAfter, Target::from(last_node))
            } else {
                log::info!(
                    "[LINK] Creating link synth for '{}' at TAIL (no effects)",
                    path
                );
                (AddAction::AddToTail, Target::from(group_node_id))
            };

            log::info!(
                "[LINK] Link synth for '{}': inbus={}, outbus={}",
                path,
                in_bus,
                out_bus
            );

            if let Err(e) = self.osc_sender.s_new(
                OscTiming::Setup,
                "system_link_audio",
                NodeId::new(link_node_id),
                add_action,
                target,
                &[("inbus", in_bus as f32), ("outbus", out_bus as f32)],
                0.0, // current_beat (not used for Setup timing)
            ) {
                log::error!("Failed to create link synth for '{}': {}", path, e);
                continue;
            }

            // Store link synth ID
            self.shared.with_state_write(|state| {
                if let Some(group) = state.groups.get_mut(&path) {
                    group.link_synth_node_id = Some(link_node_id);
                    state.bump_version();
                }
            });
        }

        // Finalize the reload: capture new snapshot, compute diff, and queue changes
        self.finalize_reload();
    }

    /// Capture a snapshot of the current state for reload diffing.
    /// If `filter_by_generation` is Some, only include entities with that generation.
    /// This is used for the "after" snapshot to only include entities touched by the script.
    fn capture_state_snapshot_filtered(&self, filter_generation: Option<u64>) -> StateSnapshot {
        self.shared.with_state_read(|state| {
            let mut snapshot = StateSnapshot::new();

            // Snapshot groups - root groups (no parent) are always included to protect them
            for (path, group) in &state.groups {
                let is_root = group.parent_path.is_none();
                if is_root || filter_generation.is_none_or(|gen| group.generation == gen) {
                    snapshot.add(EntityKind::Group, path.clone(), group.content_hash());
                }
            }

            // Snapshot voices (filter by generation if specified)
            for (name, voice) in &state.voices {
                if filter_generation.is_none_or(|gen| voice.generation == gen) {
                    snapshot.add(EntityKind::Voice, name.clone(), voice.content_hash());
                }
            }

            // Snapshot patterns (filter by generation if specified)
            for (name, pattern) in &state.patterns {
                if filter_generation.is_none_or(|gen| pattern.generation == gen) {
                    snapshot.add(EntityKind::Pattern, name.clone(), pattern.content_hash());
                }
            }

            // Snapshot melodies (filter by generation if specified)
            for (name, melody) in &state.melodies {
                if filter_generation.is_none_or(|gen| melody.generation == gen) {
                    snapshot.add(EntityKind::Melody, name.clone(), melody.content_hash());
                }
            }

            // Snapshot sequences (filter by generation if specified)
            for (name, seq) in &state.sequences {
                if filter_generation.is_none_or(|gen| seq.generation == gen) {
                    snapshot.add(EntityKind::Sequence, name.clone(), seq.content_hash());
                }
            }

            // Snapshot effects (filter by generation if specified)
            for (id, effect) in &state.effects {
                if filter_generation.is_none_or(|gen| effect.generation == gen) {
                    snapshot.add(EntityKind::Effect, id.clone(), effect.content_hash());
                }
            }

            snapshot
        })
    }

    /// Capture a snapshot of ALL entities in state (for "before" snapshot).
    fn capture_state_snapshot(&self) -> StateSnapshot {
        self.capture_state_snapshot_filtered(None)
    }

    /// Finalize a reload by computing the diff and queuing changes.
    fn finalize_reload(&mut self) {
        // Get current generation - only entities with this generation were touched by the script
        let current_generation = self.shared.with_state_read(|s| s.reload_generation);

        // Capture new snapshot filtered to ONLY entities touched by this script run
        // This is key: commented-out entities won't have the current generation
        let new_snapshot = self.capture_state_snapshot_filtered(Some(current_generation));
        let current_beat = self.transport.beat_at(Instant::now()).to_float();

        log::debug!(
            "[RELOAD] Captured new snapshot with {} entities (generation {})",
            new_snapshot.total_count(),
            current_generation
        );

        // Compute diff and queue changes
        if let Some(pending) = self.reload_manager.finalize_reload(new_snapshot, current_beat) {
            let (keep, add, update, remove) = pending.change_counts();
            log::info!(
                "[RELOAD] Diff computed: {} keep, {} add, {} update, {} remove",
                keep, add, update, remove
            );

            // Log details about what will change
            for op in &pending.changes {
                match op {
                    ChangeOp::Keep { kind, id } => {
                        log::debug!("[RELOAD]   KEEP {} '{}'", kind, id);
                    }
                    ChangeOp::Add { kind, id } => {
                        log::info!("[RELOAD]   ADD {} '{}'", kind, id);
                    }
                    ChangeOp::Update { kind, id, .. } => {
                        log::info!("[RELOAD]   UPDATE {} '{}'", kind, id);
                    }
                    ChangeOp::Remove { kind, id } => {
                        log::info!("[RELOAD]   REMOVE {} '{}'", kind, id);
                    }
                }
            }
        }

        // Stop running voices that didn't get .run() called this generation
        let stale_running_voices: Vec<(String, i32)> = self.shared.with_state_read(|state| {
            state.voices.iter()
                .filter(|(_, v)| v.running && v.run_generation != current_generation && v.running_node_id.is_some())
                .map(|(name, v)| (name.clone(), v.running_node_id.unwrap()))
                .collect()
        });

        if !stale_running_voices.is_empty() {
            let current_beat = self.transport.beat_at(Instant::now()).to_float();
            for (name, node_id) in &stale_running_voices {
                log::info!("[RELOAD] Stopping stale running voice '{}' (node {})", name, node_id);
                let _ = self.osc_sender.n_free(OscTiming::Now, NodeId::new(*node_id), current_beat);
            }

            // Update voice state to mark them as no longer running
            self.shared.with_state_write(|state| {
                for (name, _) in &stale_running_voices {
                    if let Some(voice) = state.voices.get_mut(name) {
                        voice.running = false;
                        voice.running_node_id = None;
                    }
                }
                state.bump_version();
            });
        }

        // NOTE: Old generation-based cleanup is disabled. We now use diff-based cleanup
        // which only removes entities that were actually removed from the script,
        // not just entities with old generations. This preserves unchanged entities.
    }

    /// Process pending reload at quantization boundary.
    /// This applies queued changes when the transport reaches the target beat.
    fn process_pending_reload(&mut self, current_beat: f64) {
        // Check if we should apply pending changes
        if !self.reload_manager.should_apply(current_beat) {
            return;
        }

        // Take the pending reload
        let pending = match self.reload_manager.take_pending_reload() {
            Some(p) => p,
            None => return,
        };

        log::info!(
            "[RELOAD] Applying changes at beat {:.2} (target was {:.2})",
            current_beat,
            pending.apply_at_beat
        );

        // Process each change operation
        for op in &pending.changes {
            match op {
                ChangeOp::Keep { .. } => {
                    // No action needed - entity unchanged
                }
                ChangeOp::Add { kind, id } => {
                    // New entity - already added to state during script execution
                    log::debug!("[RELOAD] Applied ADD {} '{}'", kind, id);
                }
                ChangeOp::Update { kind, id, .. } => {
                    // Updated entity - content already in state from script execution.
                    // Scheduler will naturally pick up new content at next loop iteration.
                    log::debug!("[RELOAD] Applied UPDATE {} '{}' (will use new content at next iteration)", kind, id);
                }
                ChangeOp::Remove { kind, id } => {
                    // Entity removed - clean up immediately
                    self.remove_entity(*kind, id.clone());
                }
            }
        }
    }

    /// Remove an entity that was deleted from the script.
    fn remove_entity(&mut self, kind: EntityKind, id: String) {
        log::info!("[RELOAD] Removing {} '{}'", kind, id);

        // Reset scheduler loop tracking for loops
        match kind {
            EntityKind::Pattern | EntityKind::Melody | EntityKind::Sequence => {
                self.scheduler.reset_loop(&id);
            }
            _ => {}
        }

        // Remove from state and free any associated nodes
        match kind {
            EntityKind::Pattern => {
                self.shared.with_state_write(|state| {
                    state.patterns.remove(&id);
                    state.bump_version();
                });
            }
            EntityKind::Melody => {
                self.shared.with_state_write(|state| {
                    state.melodies.remove(&id);
                    state.bump_version();
                });
            }
            EntityKind::Sequence => {
                self.shared.with_state_write(|state| {
                    state.sequences.remove(&id);
                    state.active_sequences.remove(&id);
                    state.bump_version();
                });
            }
            EntityKind::Voice => {
                // Release any active synths for this voice first
                let (synths_to_release, running_node) = self.shared.with_state_write(|state| {
                    let nodes: Vec<i32> = state
                        .active_synths
                        .iter()
                        .filter(|(_, synth)| synth.voice_names.contains(&id))
                        .map(|(nid, _)| *nid)
                        .collect();
                    for &node_id in &nodes {
                        state.active_synths.remove(&node_id);
                    }
                    // Also get running node if voice was running
                    let running_node = state.voices.get(&id).and_then(|v| v.running_node_id);
                    state.voices.remove(&id);
                    state.bump_version();
                    (nodes, running_node)
                });
                let current_beat = self.transport.beat_at(Instant::now()).to_float();
                for node_id in synths_to_release {
                    let _ = self.osc_sender.n_set(OscTiming::Now, NodeId::new(node_id), &[("gate", 0.0f32)], current_beat);
                }
                // Free the running node if one exists
                if let Some(nid) = running_node {
                    let _ = self.osc_sender.n_free(OscTiming::Now, NodeId::new(nid), current_beat);
                }
            }
            EntityKind::Effect => {
                let node_id = self.shared.with_state_read(|state| {
                    state.effects.get(&id).and_then(|e| e.node_id)
                });
                if let Some(nid) = node_id {
                    let current_beat = self.transport.beat_at(Instant::now()).to_float();
                    let _ = self.osc_sender.n_free(OscTiming::Now, NodeId::new(nid), current_beat);
                }
                self.shared.with_state_write(|state| {
                    state.effects.remove(&id);
                    state.bump_version();
                });
            }
            EntityKind::Group => {
                // Protect root-level groups from removal
                let is_root = self.shared.with_state_read(|state| {
                    state.groups.get(&id).map(|g| g.parent_path.is_none()).unwrap_or(false)
                });
                if is_root {
                    log::debug!("[RELOAD] Protecting root group '{}' from removal", id);
                    return;
                }

                // Get group info before removal
                let group_info = self.shared.with_state_read(|state| {
                    state.groups.get(&id).map(|g| (g.node_id, g.link_synth_node_id))
                });
                if let Some((node_id, link_node_id)) = group_info {
                    let current_beat = self.transport.beat_at(Instant::now()).to_float();
                    // Free link synth first, then group
                    if let Some(lnid) = link_node_id {
                        let _ = self.osc_sender.n_free(OscTiming::Now, NodeId::new(lnid), current_beat);
                    }
                    if let Some(nid) = node_id {
                        let _ = self.osc_sender.n_free(OscTiming::Now, NodeId::new(nid), current_beat);
                    }
                }
                self.shared.with_state_write(|state| {
                    state.groups.remove(&id);
                    state.bump_version();
                });
            }
        }
    }

    fn trigger_voice(
        &mut self,
        name: &str,
        synth_name: Option<String>,
        group_path: Option<String>,
        params: Vec<(String, f32)>,
    ) -> Option<i32> {
        // Check if voice is routed to MIDI output - if so, don't create SuperCollider synth
        let is_midi_voice = self.shared.with_state_read(|state| {
            state
                .voices
                .get(name)
                .map(|v| v.midi_output_device_id.is_some())
                .unwrap_or(false)
        });

        if is_midi_voice {
            log::debug!(
                "[TRIGGER] Voice '{}' is routed to MIDI output, skipping synth creation",
                name
            );
            return None; // MIDI voices don't need SuperCollider synths
        }

        let voice_info = self.shared.with_state_read(|state| {
            state.voices.get(name).map(|v| {
                (
                    v.synth_name.clone(),
                    v.group_path.clone(),
                    v.params.clone(),
                    v.gain,
                )
            })
        });

        let Some((default_synth, default_group, voice_params, gain)) = voice_info else {
            log::warn!("Voice '{}' not found", name);
            return None;
        };

        let synth_def = synth_name.or(default_synth).unwrap_or_else(|| "default".to_string());
        let group = group_path.unwrap_or(default_group);

        // Get group node ID and audio bus
        let (group_id, audio_bus) = self.shared.with_state_read(|state| {
            let group_state = state.groups.get(&group);
            (
                group_state.and_then(|g| g.node_id).unwrap_or(1),
                group_state.map(|g| g.audio_bus).unwrap_or(0),
            )
        });

        // Merge params
        let mut all_params: Vec<(String, f32)> = voice_params.into_iter().collect();
        all_params.push(("amp".to_string(), gain as f32));
        all_params.push(("out".to_string(), audio_bus as f32));
        all_params.extend(params);

        // Debug log the parameters being sent
        log::debug!(
            "[TRIGGER] Voice '{}' synthdef='{}' group='{}' group_id={} audio_bus={} params={:?}",
            name,
            synth_def,
            group,
            group_id,
            audio_bus,
            all_params
        );

        // Allocate node
        let node_id = self.shared.with_state_write(|state| state.allocate_synth_node());

        // Create synth - use AddToHead so voices execute BEFORE effects in the group
        let controls: Vec<(&str, f32)> = all_params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let current_beat = self.transport.beat_at(Instant::now()).to_float();
        if let Err(e) = self.osc_sender.s_new(
            OscTiming::Now,
            &synth_def,
            NodeId::new(node_id),
            AddAction::AddToHead,
            Target::from(group_id),
            &controls,
            current_beat,
        ) {
            log::error!("Failed to trigger voice '{}': {}", name, e);
            return None;
        }

        Some(node_id)
    }

    fn handle_note_on(&mut self, voice_name: &str, note: u8, velocity: u8, duration: Option<f64>) {
        // Check if voice is routed to MIDI output
        let midi_output_info = self.shared.with_state_read(|state| {
            if let Some(voice) = state.voices.get(voice_name) {
                if let Some(device_id) = voice.midi_output_device_id {
                    // Default to channel 0 if not specified
                    let channel = voice.midi_channel.unwrap_or(0);
                    // Get the device's event_tx
                    if let Some(device) = state.midi_output_config.devices.get(&device_id) {
                        return Some((device.event_tx.clone(), channel));
                    }
                }
            }
            None
        });

        // If this voice is routed to MIDI output, send MIDI message instead
        if let Some((event_tx, channel)) = midi_output_info {
            // Check polyphony limit and steal oldest voice if needed (for MIDI output)
            let voice_to_steal = self.shared.with_state_read(|state| {
                if let Some(voice) = state.voices.get(voice_name) {
                    // Count total active notes
                    let active_count: usize = voice.active_notes.values().map(|v| v.len()).sum();

                    if voice.polyphony > 0 && active_count >= voice.polyphony as usize {
                        // Find the oldest note to steal (use first entry)
                        let oldest = voice.active_notes.iter()
                            .flat_map(|(n, ids)| ids.iter().map(move |id| (*n, *id)))
                            .next(); // Just get the first one for MIDI

                        if let Some((steal_note, _)) = oldest {
                            log::debug!(
                                "[MIDI_VOICE_STEAL] Voice '{}' at polyphony limit ({}), stealing note {}",
                                voice_name, voice.polyphony, steal_note
                            );
                            return Some(steal_note);
                        }
                    }
                }
                None
            });

            // Send Note OFF for stolen voice before new Note ON
            if let Some(steal_note) = voice_to_steal {
                let midi_off = crate::midi::QueuedMidiEvent::note_off(channel, steal_note);
                let _ = event_tx.send(midi_off);
                log::debug!("[MIDI_OUT] Voice '{}' note_off (stolen): note={}, ch={}", voice_name, steal_note, channel + 1);

                // Remove from active notes AND cancel orphaned scheduled note-off
                let voice_name_owned = voice_name.to_string();
                self.shared.with_state_write(|state| {
                    if let Some(voice) = state.voices.get_mut(voice_name) {
                        if let Some(notes) = voice.active_notes.get_mut(&steal_note) {
                            // Look for -1 marker (direct MIDI path)
                            if let Some(pos) = notes.iter().position(|&id| id == -1) {
                                notes.remove(pos);
                            } else {
                                notes.pop(); // Fallback: remove any instance
                            }
                            if notes.is_empty() {
                                voice.active_notes.remove(&steal_note);
                            }
                        }
                    }
                    // Cancel the orphaned scheduled note-off for the stolen note
                    state.scheduled_note_offs.retain(|entry| {
                        let should_remove = entry.voice_name == voice_name_owned
                            && entry.note == steal_note
                            && entry.node_id == Some(-1);
                        if should_remove {
                            log::debug!(
                                "[MIDI_VOICE_STEAL] Canceling orphaned scheduled note-off: voice='{}' note={} beat={:.2}",
                                entry.voice_name, entry.note, entry.beat
                            );
                        }
                        !should_remove
                    });
                });
            }

            let midi_event = crate::midi::QueuedMidiEvent::note_on(channel, note, velocity);
            let _ = event_tx.send(midi_event);
            log::debug!("[MIDI_OUT] Voice '{}' note_on: note={}, vel={}, ch={}", voice_name, note, velocity, channel + 1);

            // Track active MIDI notes for note-off (using negative "node_id" as marker)
            self.shared.with_state_write(|state| {
                if let Some(voice) = state.voices.get_mut(voice_name) {
                    // Use -1 as a marker for MIDI notes (no actual SuperCollider node)
                    voice.active_notes.entry(note).or_default().push(-1);
                }
            });

            // Schedule note-off if duration specified
            if let Some(dur) = duration {
                let current_beat = self.transport.beat_at(Instant::now()).to_float();
                let off_beat = current_beat + dur;
                self.shared.with_state_write(|state| {
                    state.scheduled_note_offs.push(ScheduledNoteOff {
                        beat: off_beat,
                        voice_name: voice_name.to_string(),
                        note,
                        node_id: Some(-1), // Marker for MIDI note
                    });
                });
            }
            return;
        }

        // Check polyphony limit and steal oldest voice if needed
        let voice_to_steal = self.shared.with_state_read(|state| {
            if let Some(voice) = state.voices.get(voice_name) {
                // Count total active voices across all notes
                let active_count: usize = voice.active_notes.values().map(|v| v.len()).sum();

                if voice.polyphony > 0 && active_count >= voice.polyphony as usize {
                    // Find the oldest voice (lowest node_id) to steal
                    let oldest_node = voice.active_notes.iter()
                        .flat_map(|(n, ids)| ids.iter().map(move |id| (*n, *id)))
                        .min_by_key(|(_, id)| *id);

                    if let Some((steal_note, steal_node_id)) = oldest_node {
                        log::debug!(
                            "[VOICE_STEAL] Voice '{}' at polyphony limit ({}), stealing node {} (note {})",
                            voice_name, voice.polyphony, steal_node_id, steal_note
                        );
                        return Some((steal_note, steal_node_id));
                    }
                }
            }
            None
        });

        // Steal the oldest voice if needed
        if let Some((steal_note, steal_node_id)) = voice_to_steal {
            self.handle_note_off(voice_name, steal_note, Some(steal_node_id));
        }

        // For now, use simple synth triggering
        // Full SFZ support will come later
        let params = vec![
            ("note".to_string(), note as f32),
            ("freq".to_string(), 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)),
            ("velocity".to_string(), velocity as f32 / 127.0),
            ("gate".to_string(), 1.0),
        ];

        if let Some(node_id) = self.trigger_voice(voice_name, None, None, params) {
            // Track the active note for later note-off
            self.shared.with_state_write(|state| {
                if let Some(voice) = state.voices.get_mut(voice_name) {
                    voice.active_notes.entry(note).or_default().push(node_id);
                    log::debug!("[NOTE_ON] Voice '{}' note {} -> node {}", voice_name, note, node_id);
                }
            });

            // Schedule note-off if duration specified
            if let Some(dur) = duration {
                let current_beat = self.transport.beat_at(Instant::now()).to_float();
                let off_beat = current_beat + dur;
                self.shared.with_state_write(|state| {
                    state.scheduled_note_offs.push(ScheduledNoteOff {
                        beat: off_beat,
                        voice_name: voice_name.to_string(),
                        note,
                        node_id: Some(node_id),
                    });
                });
            }
        }
    }

    fn handle_note_off(&mut self, voice_name: &str, note: u8, specific_node_id: Option<i32>) {
        // Check if this is a MIDI note (node_id == -1 or -2) or voice is routed to MIDI output
        // -1 = direct MIDI path (from handle_note_on) - we need to send MIDI note-off
        // -2 = SC-managed MIDI path (from fire_events_bundled) - synth already sends MIDI via OSC
        let midi_output_info = self.shared.with_state_read(|state| {
            if let Some(voice) = state.voices.get(voice_name) {
                if let Some(device_id) = voice.midi_output_device_id {
                    let channel = voice.midi_channel.unwrap_or(0);
                    if let Some(device) = state.midi_output_config.devices.get(&device_id) {
                        return Some((device.event_tx.clone(), channel));
                    }
                }
            }
            None
        });

        // Handle MIDI note-off
        if let Some((event_tx, channel)) = midi_output_info {
            // For SC-managed MIDI notes (-2), only clean up tracking - the synth already sent note-off
            if specific_node_id == Some(-2) {
                log::debug!("[MIDI OUTPUT] Cleanup only (SC-managed): voice='{}' ch={} note={}", voice_name, channel + 1, note);
                // Just clean up active_notes, don't send MIDI (synth handles it via OSC)
                self.shared.with_state_write(|state| {
                    if let Some(voice) = state.voices.get_mut(voice_name) {
                        if let Some(node_ids) = voice.active_notes.get_mut(&note) {
                            if let Some(pos) = node_ids.iter().position(|&id| id == -2) {
                                node_ids.remove(pos);
                            }
                            if node_ids.is_empty() {
                                voice.active_notes.remove(&note);
                            }
                        }
                    }
                });
                return;
            }

            // For direct MIDI notes (-1), send note-off and clean up
            if specific_node_id == Some(-1) || specific_node_id.is_none() {
                let midi_event = crate::midi::QueuedMidiEvent::note_off(channel, note);
                let _ = event_tx.send(midi_event);
                log::info!("[MIDI OUTPUT] note_off: voice='{}' ch={} note={}", voice_name, channel + 1, note);

                // Remove from tracking
                self.shared.with_state_write(|state| {
                    if let Some(voice) = state.voices.get_mut(voice_name) {
                        if let Some(node_ids) = voice.active_notes.get_mut(&note) {
                            // Remove one -1 marker (or all if no specific node)
                            if specific_node_id == Some(-1) {
                                if let Some(pos) = node_ids.iter().position(|&id| id == -1) {
                                    node_ids.remove(pos);
                                }
                            } else {
                                node_ids.retain(|&id| id != -1);
                            }
                            if node_ids.is_empty() {
                                voice.active_notes.remove(&note);
                            }
                        }
                    }
                });
            }
            return;
        }

        // If we have a specific node ID, just release that node
        if let Some(node_id) = specific_node_id {
            // Skip negative markers - they're handled above
            if node_id < 0 {
                return;
            }

            log::debug!("[NOTE_OFF] Releasing specific node {} for voice '{}'", node_id, voice_name);

            // Remove from tracking BEFORE sending gate=0, so fades don't try to update it
            // Also cancel any scheduled note-off for this specific node
            let voice_name_owned = voice_name.to_string();
            self.shared.with_state_write(|state| {
                state.active_synths.remove(&node_id);
                state.pending_nodes.remove(&node_id);
                if let Some(voice) = state.voices.get_mut(voice_name) {
                    // Remove this specific node from the note's node list
                    if let Some(node_ids) = voice.active_notes.get_mut(&note) {
                        node_ids.retain(|&id| id != node_id);
                        if node_ids.is_empty() {
                            voice.active_notes.remove(&note);
                        }
                    }
                }
                // Cancel any scheduled note-off for this specific node (prevents orphaned note-offs)
                state.scheduled_note_offs.retain(|entry| {
                    let should_remove = entry.voice_name == voice_name_owned
                        && entry.note == note
                        && entry.node_id == Some(node_id);
                    if should_remove {
                        log::debug!(
                            "[NOTE_OFF] Canceling scheduled note-off: voice='{}' note={} node={} beat={:.2}",
                            entry.voice_name, entry.note, node_id, entry.beat
                        );
                    }
                    !should_remove
                });
            });

            let current_beat = self.transport.beat_at(Instant::now()).to_float();
            let _ = self.osc_sender.n_set(OscTiming::Now, NodeId::new(node_id), &[("gate", 0.0f32)], current_beat);
            return;
        }

        // Look up nodes for this specific note from active_notes tracking
        let nodes_to_release: Vec<i32> = self.shared.with_state_write(|state| {
            if let Some(voice) = state.voices.get_mut(voice_name) {
                // Get nodes for this specific note
                if let Some(node_ids) = voice.active_notes.remove(&note) {
                    // Remove these nodes from active_synths tracking
                    for &node_id in &node_ids {
                        state.active_synths.remove(&node_id);
                        state.pending_nodes.remove(&node_id);
                    }
                    return node_ids;
                }
            }
            vec![]
        });

        if nodes_to_release.is_empty() {
            log::debug!("[NOTE_OFF] No active nodes found for voice '{}' note {}", voice_name, note);
            return;
        }

        log::debug!("[NOTE_OFF] Releasing {} node(s) for voice '{}' note {}", nodes_to_release.len(), voice_name, note);
        let current_beat = self.transport.beat_at(Instant::now()).to_float();
        for node_id in nodes_to_release {
            // Skip -1 markers (used for MIDI notes, not real SC nodes)
            if node_id >= 0 {
                let _ = self.osc_sender.n_set(OscTiming::Now, NodeId::new(node_id), &[("gate", 0.0f32)], current_beat);
            }
        }
    }

    /// Add an effect to a group.
    ///
    /// Effects are inserted after all voices but before the link synth.
    /// Multiple effects are chained in the order they are added.
    fn handle_add_effect(
        &mut self,
        id: String,
        synthdef: String,
        group_path: String,
        params: std::collections::HashMap<String, f32>,
        source_location: crate::api::context::SourceLocation,
    ) {
        // Check if effect already exists with the same synthdef
        let existing_effect = self.shared.with_state_read(|state| {
            state.effects.get(&id).map(|e| {
                (
                    e.node_id,
                    e.synthdef_name.clone(),
                    e.group_path.clone(),
                    e.params.clone(),
                )
            })
        });

        if let Some((existing_node_id, existing_synthdef, existing_group, existing_params)) =
            existing_effect
        {
            // Effect already exists - check if we can just update it
            if existing_synthdef == synthdef && existing_group == group_path {
                // Same synthdef and group - just update generation and params
                log::debug!(
                    "[EFFECT] Effect '{}' already exists, updating generation and params",
                    id
                );

                // Update params that changed
                if let Some(node_id) = existing_node_id {
                    let current_beat = self.transport.beat_at(Instant::now()).to_float();
                    for (param, value) in &params {
                        if existing_params.get(param) != Some(value) {
                            let _ = self.osc_sender.n_set(
                                OscTiming::Now,
                                NodeId::new(node_id),
                                &[(param.as_str(), *value)],
                                current_beat,
                            );
                        }
                    }
                }

                // Update state (generation and params)
                let generation = self.shared.with_state_read(|s| s.reload_generation);
                self.shared.with_state_write(|state| {
                    if let Some(effect) = state.effects.get_mut(&id) {
                        effect.generation = generation;
                        effect.params = params;
                    }
                    state.bump_version();
                });
                return;
            }

            // Different synthdef or group - need to recreate
            log::info!(
                "[EFFECT] Effect '{}' group changed from '{}' to '{}' - will recreate",
                id, existing_group, group_path
            );
            if let Some(nid) = existing_node_id {
                log::info!(
                    "[EFFECT] Freeing old node {} for effect '{}'",
                    nid, id
                );
                let current_beat = self.transport.beat_at(Instant::now()).to_float();
                let _ = self.osc_sender.n_free(OscTiming::Now, NodeId::new(nid), current_beat);
            }
        }

        // Get group's node ID and audio bus
        // Also find existing effects on this group to determine proper ordering
        let (group_node_id, group_bus, last_effect_node, next_position, link_synth_node) =
            self.shared.with_state_read(|state| {
                let group = state.groups.get(&group_path);

                // Find all effects for this group, sorted by position
                let mut group_effects: Vec<_> = state
                    .effects
                    .values()
                    .filter(|e| e.group_path == group_path && e.id != id)
                    .collect();
                group_effects.sort_by_key(|e| e.position);

                // Get the last effect's node ID (if any)
                let last_node = group_effects.last().and_then(|e| e.node_id);

                // Calculate next position
                let next_pos = group_effects.len();

                (
                    group.and_then(|g| g.node_id),
                    group.map(|g| g.audio_bus),  // audio_bus is always set (i32, not Option)
                    last_node,
                    next_pos,
                    group.and_then(|g| g.link_synth_node_id),
                )
            });

        let target_node_id = group_node_id;
        let bus_in = group_bus.expect("Group must have an audio bus allocated");
        let bus_out = bus_in; // Effects process in-place on the group's bus

        if target_node_id.is_none() {
            log::warn!("[EFFECT] Cannot add effect '{}': group '{}' not found or has no node ID", id, group_path);
            // Debug: list available groups
            self.shared.with_state_read(|state| {
                log::warn!("[EFFECT] Available groups: {:?}", state.groups.keys().collect::<Vec<_>>());
            });
            return;
        }

        log::info!("[EFFECT] Creating effect '{}' in group '{}' with bus {}", id, group_path, bus_in);

        // Allocate a node ID for the effect
        let node_id = self.shared.with_state_write(|state| state.allocate_synth_node());

        // Build controls with bus routing and user parameters
        let mut controls: Vec<(String, f32)> = vec![
            ("__fx_bus_in".to_string(), bus_in as f32),
            ("__fx_bus_out".to_string(), bus_out as f32),
        ];
        controls.extend(params.iter().map(|(k, v)| (k.clone(), *v)));

        // Create the effect synth with proper ordering
        // - If there are existing effects, add AFTER the last one
        // - If no effects but link synth exists, add BEFORE the link synth
        // - If no effects and no link synth, add to TAIL (voices use AddToHead, so this goes after them)
        let (add_action, target) = if let Some(last_node) = last_effect_node {
            // Add after the last effect in the chain
            (AddAction::AddAfter, Target::from(last_node))
        } else if let Some(link_node) = link_synth_node {
            // No effects yet, but link synth exists - add BEFORE link synth
            // This ensures: Voice → Effect → LinkSynth
            (AddAction::AddBefore, Target::from(link_node))
        } else {
            // First effect, no link synth yet - add to tail (voices use AddToHead, so this goes after them)
            (AddAction::AddToTail, Target::from(target_node_id.unwrap()))
        };

        log::debug!(
            "[EFFECT] Adding effect '{}' ({}) to group '{}' at position {} (action: {:?}, bus: {})",
            id,
            synthdef,
            group_path,
            next_position,
            add_action,
            bus_in
        );

        // OscSender handles both sending to scsynth and score capture
        // Effects are created during setup, so use Setup timing for score capture
        let controls_vec: Vec<(String, f32)> = controls.iter().map(|(k, v)| (k.clone(), *v)).collect();
        if let Err(e) = self.osc_sender.s_new(
            OscTiming::Setup,
            &synthdef,
            NodeId::new(node_id),
            add_action,
            target,
            &controls_vec,
            0.0, // current_beat not used for Setup timing
        ) {
            log::error!("[EFFECT] Failed to create effect '{}': {}", id, e);
            return;
        }

        // Store effect state with position
        let generation = self.shared.with_state_read(|s| s.reload_generation);
        self.shared.with_state_write(|state| {
            let effect = EffectState {
                id: id.clone(),
                synthdef_name: synthdef,
                group_path: group_path.clone(),
                node_id: Some(node_id),
                bus_in,
                bus_out,
                params,
                generation,
                position: next_position,
                vst_plugin: None,
                source_location: source_location.clone(),
            };
            state.effects.insert(id.clone(), effect);
            state.bump_version();
        });

        log::info!("[EFFECT] Created effect '{}' (node {}) on bus {}", id, node_id, bus_in);
    }

    fn queue_loop_start(&mut self, name: &str, kind: LoopKind) {
        let quantization = self.shared.with_state_read(|s| s.quantization_beats);
        let current_beat = self.transport.beat_at(Instant::now()).to_float();
        let next_beat = ((current_beat / quantization).ceil() * quantization).max(0.0);

        self.shared.with_state_write(|state| {
            match kind {
                LoopKind::Pattern => {
                    if let Some(p) = state.patterns.get_mut(name) {
                        log::info!("Starting pattern '{}' at beat {}", name, next_beat);
                        p.status = LoopStatus::Playing { start_beat: next_beat };
                        state.bump_version();
                    } else {
                        log::warn!("Pattern '{}' not found in state", name);
                    }
                }
                LoopKind::Melody => {
                    if let Some(m) = state.melodies.get_mut(name) {
                        m.status = LoopStatus::Playing { start_beat: next_beat };
                        state.bump_version();
                    }
                }
                LoopKind::Sequence => {
                    // Sequences are handled separately via start_sequence
                }
            }
        });
    }

    fn stop_loop(&mut self, name: &str, kind: LoopKind) {
        self.shared.with_state_write(|state| {
            match kind {
                LoopKind::Pattern => {
                    if let Some(p) = state.patterns.get_mut(name) {
                        p.status = LoopStatus::Stopped;
                        state.bump_version();
                    }
                }
                LoopKind::Melody => {
                    if let Some(m) = state.melodies.get_mut(name) {
                        m.status = LoopStatus::Stopped;
                        state.bump_version();
                    }
                }
                LoopKind::Sequence => {
                    // Sequences are handled separately
                }
            }
        });
    }

    fn start_sequence(&mut self, name: &str, play_once: bool) {
        // Check if sequence is already running - if so, preserve its state
        let already_running = self.shared.with_state_read(|state| {
            state.active_sequences.contains_key(name)
        });

        if already_running {
            log::info!(
                "[SEQUENCE] Sequence '{}' already running, preserving anchor and triggered_clips",
                name
            );
            return;
        }

        let quantization = self.shared.with_state_read(|s| s.quantization_beats);
        let current_beat = self.transport.beat_at(Instant::now()).to_float();
        let anchor_beat = ((current_beat / quantization).ceil() * quantization).max(0.0);

        log::info!("[SEQUENCE] Starting sequence '{}' at anchor beat {:.2} (play_once={})", name, anchor_beat, play_once);

        self.shared.with_state_write(|state| {
            // If play_once is true, update the sequence definition
            if play_once {
                if let Some(seq_def) = state.sequences.get_mut(name) {
                    seq_def.play_once = true;
                }
            }

            state.active_sequences.insert(
                name.to_string(),
                ActiveSequence {
                    anchor_beat,
                    paused: false,
                    triggered_clips: HashMap::new(),
                    last_iteration: 0,
                    completed: false,
                },
            );
            state.bump_version();
        });
    }

    /// Process active sequences.
    /// Note: Fades are now handled via the event system (materialize_sequence creates fade events).
    fn process_active_sequences(&mut self, _current_beat: f64) {
        // Fades, patterns, and melodies are all handled via materialize_sequence.
        // This function is kept for potential future use (e.g., loop state management).
    }

    /// Start a fade from a FadeDefinition.
    #[allow(dead_code)]
    fn start_fade_from_definition(&mut self, fade: &crate::sequences::FadeDefinition) {
        use crate::events::FadeTargetType;

        let tempo = self.shared.with_state_read(|s| s.tempo);
        let beats_per_second = tempo / 60.0;
        let duration_seconds = fade.duration_beats / beats_per_second;

        let fade_job = ActiveFadeJob {
            target_type: fade.target_type.clone(),
            target_name: fade.target_name.clone(),
            param_name: fade.param_name.clone(),
            start_value: fade.from,
            target_value: fade.to,
            start_time: Instant::now(),
            duration_seconds,
            delay_seconds: 0.0,
            last_value: None,
            completed: false,
        };

        // Update the parameter in state immediately so synths created at the same beat
        // will use the fade's start value (they read from state when building the s_new packet)
        self.shared.with_state_write(|state| {
            // Update state first so new synths get the correct initial value
            match &fade.target_type {
                FadeTargetType::Group => {
                    if let Some(group) = state.groups.get_mut(&fade.target_name) {
                        group.params.insert(fade.param_name.clone(), fade.from);
                    }
                }
                FadeTargetType::Voice => {
                    if let Some(voice) = state.voices.get_mut(&fade.target_name) {
                        voice.params.insert(fade.param_name.clone(), fade.from);
                    }
                }
                FadeTargetType::Pattern => {
                    if let Some(pattern) = state.patterns.get_mut(&fade.target_name) {
                        pattern.params.insert(fade.param_name.clone(), fade.from);
                    }
                }
                FadeTargetType::Melody => {
                    if let Some(melody) = state.melodies.get_mut(&fade.target_name) {
                        melody.params.insert(fade.param_name.clone(), fade.from);
                    }
                }
                FadeTargetType::Effect => {
                    if let Some(effect) = state.effects.get_mut(&fade.target_name) {
                        effect.params.insert(fade.param_name.clone(), fade.from);
                    }
                }
            }
            state.fades.push(fade_job);
            state.bump_version();
        });

        // NOTE: We do NOT send n_set here - see comment in start_fade_from_clip
    }

    /// Start a fade from a FadeClip (used for scheduled fade events).
    fn start_fade_from_clip(&mut self, fade: crate::events::FadeClip) {
        let tempo = self.shared.with_state_read(|s| s.tempo);
        let beats_per_second = tempo / 60.0;
        let duration_seconds = fade.duration_beats / beats_per_second;

        let fade_job = ActiveFadeJob {
            target_type: fade.target_type.clone(),
            target_name: fade.target_name.clone(),
            param_name: fade.param_name.clone(),
            start_value: fade.start_value,
            target_value: fade.target_value,
            start_time: Instant::now(),
            duration_seconds,
            delay_seconds: 0.0,
            last_value: None,
            completed: false,
        };

        // Update the parameter in state immediately so synths created at the same beat
        // will use the fade's start value (they read from state when building the s_new packet)
        self.shared.with_state_write(|state| {
            // Update state first so new synths get the correct initial value
            match &fade.target_type {
                FadeTargetType::Group => {
                    if let Some(group) = state.groups.get_mut(&fade.target_name) {
                        group.params.insert(fade.param_name.clone(), fade.start_value);
                    }
                }
                FadeTargetType::Voice => {
                    if let Some(voice) = state.voices.get_mut(&fade.target_name) {
                        voice.params.insert(fade.param_name.clone(), fade.start_value);
                    }
                }
                FadeTargetType::Pattern => {
                    if let Some(pattern) = state.patterns.get_mut(&fade.target_name) {
                        pattern.params.insert(fade.param_name.clone(), fade.start_value);
                    }
                }
                FadeTargetType::Melody => {
                    if let Some(melody) = state.melodies.get_mut(&fade.target_name) {
                        melody.params.insert(fade.param_name.clone(), fade.start_value);
                    }
                }
                FadeTargetType::Effect => {
                    if let Some(effect) = state.effects.get_mut(&fade.target_name) {
                        effect.params.insert(fade.param_name.clone(), fade.start_value);
                    }
                }
            }
            state.fades.push(fade_job);
            state.bump_version();
        });

        // NOTE: We do NOT send n_set here because:
        // 1. New synths created at the same beat will read from state (which we just updated)
        // 2. The s_new is sent in a timed bundle - the synth doesn't exist on scsynth yet
        // 3. The update_fades() function will send n_set to existing synths in subsequent ticks
    }

    /// Apply a fade value to a target.
    fn apply_fade_value(&mut self, target_type: &crate::events::FadeTargetType, target_name: &str, param_name: &str, value: f32) {
        use crate::events::FadeTargetType;

        match target_type {
            FadeTargetType::Group => {
                self.handle_set_group_param(target_name, param_name, value);
            }
            FadeTargetType::Voice => {
                // Get all active node IDs for this voice - no pending check needed
                // since we ensure n_set always follows s_new in timed bundles
                let node_ids: Vec<i32> = self.shared.with_state_write(|state| {
                    if let Some(voice) = state.voices.get_mut(target_name) {
                        voice.params.insert(param_name.to_string(), value);
                        voice.active_notes.values().flatten().copied().collect()
                    } else {
                        Vec::new()
                    }
                });
                log::trace!("[FADE] Voice '{}' param '{}' = {} | {} nodes to update",
                    target_name, param_name, value, node_ids.len());
                for node_id in &node_ids {
                    // OscSender handles both sending and score capture
                    let current_beat = self.transport.beat_at(std::time::Instant::now()).to_float();
                    // n_set may fail for nodes not yet live on scsynth - that's OK
                    let _ = self.osc_sender.n_set(
                        OscTiming::Now,
                        NodeId::new(*node_id),
                        &[(param_name.to_string(), value)],
                        current_beat,
                    );
                }
            }
            FadeTargetType::Pattern => {
                self.shared.with_state_write(|state| {
                    if let Some(pattern) = state.patterns.get_mut(target_name) {
                        pattern.params.insert(param_name.to_string(), value);
                    }
                });
            }
            FadeTargetType::Melody => {
                self.shared.with_state_write(|state| {
                    if let Some(melody) = state.melodies.get_mut(target_name) {
                        melody.params.insert(param_name.to_string(), value);
                    }
                });
            }
            FadeTargetType::Effect => {
                let node_to_update = self.shared.with_state_write(|state| {
                    if let Some(effect) = state.effects.get_mut(target_name) {
                        effect.params.insert(param_name.to_string(), value);
                        effect.node_id
                    } else {
                        None
                    }
                });
                if let Some(node_id) = node_to_update {
                    // OscSender handles both sending and score capture
                    let current_beat = self.transport.beat_at(std::time::Instant::now()).to_float();
                    let _ = self.osc_sender.n_set(
                        OscTiming::Now,
                        NodeId::new(node_id),
                        &[(param_name.to_string(), value)],
                        current_beat,
                    );
                }
            }
        }
    }

    fn process_scheduled_note_offs(&mut self, current_beat: f64) {
        let due_offs: Vec<ScheduledNoteOff> = self.shared.with_state_write(|state| {
            let due: Vec<_> = state
                .scheduled_note_offs
                .iter()
                .filter(|n| n.beat <= current_beat)
                .cloned()
                .collect();
            state.scheduled_note_offs.retain(|n| n.beat > current_beat);
            due
        });

        for note_off in due_offs {
            log::debug!(
                "[NOTE_LIFECYCLE] voice='{}' event='PROCESSING_OFF' note={} marker={:?} scheduled_beat={:.2} current_beat={:.2}",
                note_off.voice_name, note_off.note, note_off.node_id, note_off.beat, current_beat
            );

            // Capture note-off to score at the scheduled beat time (OscSender handles capture)
            // Skip n_set for MIDI notes (node_id == -1 or -2 is a marker, not a real SC node)
            if let Some(node_id) = note_off.node_id {
                if node_id >= 0 {
                    let _ = self.osc_sender.n_set(
                        OscTiming::AtBeat(BeatTime::from_float(note_off.beat)),
                        NodeId::new(node_id),
                        &[("gate".to_string(), 0.0f32)],
                        note_off.beat, // current_beat param for fallback
                    );
                }
            }

            self.handle_note_off(&note_off.voice_name, note_off.note, note_off.node_id);
        }
    }

    fn update_fades(&mut self, now: Instant) {
        // Get completed fades and update values
        let updates: Vec<(FadeTargetType, String, String, f32)> = self.shared.with_state_write(|state| {
            let mut updates = Vec::new();
            for fade in &mut state.fades {
                if fade.completed {
                    continue;
                }
                let elapsed = now.duration_since(fade.start_time).as_secs_f64();
                if elapsed < fade.delay_seconds {
                    continue;
                }
                let t = ((elapsed - fade.delay_seconds) / fade.duration_seconds).min(1.0);
                let value = fade.start_value + (fade.target_value - fade.start_value) * t as f32;

                if fade.last_value != Some(value) {
                    fade.last_value = Some(value);
                    updates.push((
                        fade.target_type.clone(),
                        fade.target_name.clone(),
                        fade.param_name.clone(),
                        value,
                    ));
                }

                if t >= 1.0 {
                    fade.completed = true;
                }
            }
            state.fades.retain(|f| !f.completed);
            updates
        });

        // Apply fade updates using the shared helper
        for (target_type, target_name, param_name, value) in updates {
            self.apply_fade_value(&target_type, &target_name, &param_name, value);
        }
    }

    /// Load a sample into SuperCollider and store its info in state.
    /// The path should already be resolved (absolute path) by the caller.
    /// If a sample with the same ID and path is already loaded, this is a no-op.
    fn handle_load_sample(&mut self, id: String, path: String) {
        // Path should already be resolved by the Rhai thread
        let path_str = path;

        // Check if this sample is already loaded with the same path
        let existing = self.shared.with_state_read(|state| {
            state.samples.get(&id).map(|s| s.path.clone())
        });

        if let Some(existing_path) = existing {
            if existing_path == path_str {
                log::debug!(
                    "[SAMPLE] Sample '{}' already loaded from '{}', skipping reload",
                    id,
                    path_str
                );
                return;
            }
            // Different path - need to free the old buffer and reload
            log::info!(
                "[SAMPLE] Sample '{}' path changed from '{}' to '{}', reloading",
                id,
                existing_path,
                path_str
            );
            // Free the old buffer
            if let Some(old_buffer) = self.shared.with_state_write(|state| {
                state.samples.remove(&id).map(|s| s.buffer_id)
            }) {
                let current_beat = self.transport.beat_at(Instant::now()).to_float();
                let _ = self.osc_sender.b_free(OscTiming::Now, BufNum::new(old_buffer), current_beat);
            }
        }

        // Allocate a buffer ID
        let buffer_id = self.shared.with_state_write(|state| state.allocate_buffer_id());

        log::info!(
            "[SAMPLE] Loading sample '{}' from '{}' into buffer {}",
            id,
            path_str,
            buffer_id
        );

        // Read WAV metadata using hound
        let wav_meta = Self::read_wav_metadata(&path_str);
        let num_channels = wav_meta.as_ref().map(|m| m.num_channels).unwrap_or(2);
        let num_frames = wav_meta.as_ref().map(|m| m.num_frames).unwrap_or(0);
        let sample_rate = wav_meta.as_ref().map(|m| m.sample_rate).unwrap_or(44100.0);

        if let Some(meta) = &wav_meta {
            log::debug!(
                "[SAMPLE] Detected WAV metadata – channels: {}, rate: {}, frames: {}",
                meta.num_channels,
                meta.sample_rate,
                meta.num_frames
            );
        } else {
            log::warn!(
                "[SAMPLE] Could not read WAV metadata for '{}', falling back to stereo @ 44.1kHz",
                path_str
            );
        }

        // Load the sample into the buffer using b_allocRead (OscSender handles capture)
        let current_beat = self.transport.beat_at(std::time::Instant::now()).to_float();
        if let Err(e) = self.osc_sender.b_alloc_read(OscTiming::Now, BufNum::new(buffer_id), &path_str, current_beat) {
            log::error!(
                "[SAMPLE] Failed to load sample '{}' from '{}': {}",
                id,
                path_str,
                e
            );
            return;
        }

        // Wait a moment for buffer to load
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Generate the SynthDef name for this sample
        let synthdef_name = format!("__sample_{}", id);

        // Store sample info in state
        let sample_info = SampleInfo {
            id: id.clone(),
            path: path_str.clone(),
            buffer_id,
            num_channels,
            num_frames,
            sample_rate,
            synthdef_name: synthdef_name.clone(),
            slices: Vec::new(),
        };

        self.shared.with_state_write(|state| {
            state.samples.insert(id.clone(), sample_info);
            state.bump_version();
        });

        log::info!(
            "[SAMPLE] Successfully loaded sample '{}' (buffer {}, {} channels, {} frames)",
            id,
            buffer_id,
            num_channels,
            num_frames
        );
    }

    /// Run a voice continuously (for line-in processing, drones, etc.).
    ///
    /// Unlike melody/pattern triggers, this starts the synth immediately
    /// and keeps it running until stopped or the script is reloaded.
    fn handle_run_voice(&mut self, name: String) {
        // Get voice info from state
        let voice_info = self.shared.with_state_read(|state| {
            state.voices.get(&name).map(|v| {
                (
                    v.synth_name.clone(),
                    v.group_path.clone(),
                    v.params.clone(),
                    v.gain,
                    v.running,
                    v.running_node_id,
                )
            })
        });

        let Some((synth_name, group_path, params, gain, already_running, existing_node)) = voice_info else {
            log::warn!("[RUN_VOICE] Voice '{}' not found", name);
            return;
        };

        let Some(synthdef) = synth_name else {
            log::warn!("[RUN_VOICE] Voice '{}' has no synth defined", name);
            return;
        };

        // If already running with same config, just mark as still running
        if already_running && existing_node.is_some() {
            log::debug!("[RUN_VOICE] Voice '{}' already running, updating params", name);
            // Update params on the running node
            if let Some(node_id) = existing_node {
                let current_beat = self.transport.beat_at(Instant::now()).to_float();
                for (param, value) in &params {
                    let _ = self.osc_sender.n_set(OscTiming::Now, NodeId::new(node_id), &[(param.as_str(), *value)], current_beat);
                }
                // Update gain
                let _ = self.osc_sender.n_set(OscTiming::Now, NodeId::new(node_id), &[("amp", gain as f32)], current_beat);
            }
            // Mark voice as still running and update run_generation
            let generation = self.shared.with_state_read(|s| s.reload_generation);
            self.shared.with_state_write(|state| {
                if let Some(voice) = state.voices.get_mut(&name) {
                    voice.running = true;
                    voice.run_generation = generation;
                }
                state.bump_version();
            });
            return;
        }

        // Free existing node if there is one (synthdef changed)
        if let Some(nid) = existing_node {
            log::debug!("[RUN_VOICE] Voice '{}' synthdef changed, freeing old node {}", name, nid);
            let current_beat = self.transport.beat_at(Instant::now()).to_float();
            let _ = self.osc_sender.n_free(OscTiming::Now, NodeId::new(nid), current_beat);
        }

        // Get group's audio bus for output routing
        let (group_node_id, output_bus) = self.shared.with_state_read(|state| {
            state.groups.get(&group_path)
                .map(|g| (g.node_id, g.audio_bus))
                .unwrap_or((None, 0))
        });

        // Allocate a node ID
        let node_id = self.shared.with_state_write(|state| state.allocate_synth_node());

        // Build control parameters
        let mut controls: Vec<(String, f32)> = params.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        controls.push(("out".to_string(), output_bus as f32));
        controls.push(("amp".to_string(), gain as f32));

        // Create the synth in the group (or root if no group)
        let target = group_node_id
            .map(Target::new)
            .unwrap_or_else(Target::root);

        let current_beat = self.transport.beat_at(Instant::now()).to_float();
        if let Err(e) = self.osc_sender.s_new(
            OscTiming::Now,
            &synthdef,
            NodeId::new(node_id),
            AddAction::AddToHead,  // Voices go to head so they execute before effects
            target,
            &controls.iter().map(|(k, v)| (k.as_str(), *v)).collect::<Vec<_>>(),
            current_beat,
        ) {
            log::error!("[RUN_VOICE] Failed to create synth for voice '{}': {}", name, e);
            return;
        }

        // Update voice state
        let generation = self.shared.with_state_read(|s| s.reload_generation);
        self.shared.with_state_write(|state| {
            if let Some(voice) = state.voices.get_mut(&name) {
                voice.running = true;
                voice.running_node_id = Some(node_id);
                voice.run_generation = generation;
            }
            state.bump_version();
        });

        log::info!(
            "[RUN_VOICE] Voice '{}' now running (node {}) with synthdef '{}' on bus {}",
            name, node_id, synthdef, output_bus
        );
    }

    /// Read WAV metadata from a file.
    fn read_wav_metadata(path: &str) -> Option<WavMetadata> {
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);
        let wav_reader = hound::WavReader::new(reader).ok()?;

        let spec = wav_reader.spec();
        let duration = wav_reader.duration();

        Some(WavMetadata {
            num_channels: spec.channels as i32,
            sample_rate: spec.sample_rate as f32,
            num_frames: duration as i32,
        })
    }
}

/// WAV metadata for sample loading.
struct WavMetadata {
    num_channels: i32,
    sample_rate: f32,
    num_frames: i32,
}

/// Create the system_link_audio synthdef bytes.
///
/// This synthdef routes audio from a group bus to the main output with:
/// - amp parameter for gain control
/// - Metering via SendTrig (peak and RMS for L/R channels)
///
/// Signal flow:
///   In.ar(inbus) → × amp → Out.ar(outbus)
///                     ↓
///              Peak + Amplitude → SendTrig (at 20Hz)
///
/// SendTrig IDs:
///   0: peak_left, 1: peak_right, 2: rms_left, 3: rms_right
fn create_system_link_audio_bytes() -> Result<Vec<u8>> {
    // Build the synthdef manually without depending on vibelang-dsp
    // Format: SuperCollider synthdef file format v2

    use std::io::Write;
    let mut buf = Vec::new();

    // File header
    buf.write_all(b"SCgf")?; // Magic
    buf.write_all(&2i32.to_be_bytes())?; // Version 2
    buf.write_all(&1i16.to_be_bytes())?; // Number of synthdefs

    // SynthDef name
    let name = b"system_link_audio";
    buf.push(name.len() as u8);
    buf.write_all(name)?;

    // Constants (7 total)
    // 0: 0.0 (SendTrig ID 0 for peak_left)
    // 1: 1.0 (SendTrig ID 1 for peak_right)
    // 2: 2.0 (SendTrig ID 2 for rms_left)
    // 3: 3.0 (SendTrig ID 3 for rms_right)
    // 4: 20.0 (Impulse frequency - 20Hz for meter updates)
    // 5: 0.01 (Amplitude attack time)
    // 6: 0.1 (Amplitude release time)
    buf.write_all(&7i32.to_be_bytes())?; // num constants
    buf.write_all(&0.0f32.to_be_bytes())?; // constant 0
    buf.write_all(&1.0f32.to_be_bytes())?; // constant 1
    buf.write_all(&2.0f32.to_be_bytes())?; // constant 2
    buf.write_all(&3.0f32.to_be_bytes())?; // constant 3
    buf.write_all(&20.0f32.to_be_bytes())?; // constant 4
    buf.write_all(&0.01f32.to_be_bytes())?; // constant 5
    buf.write_all(&0.1f32.to_be_bytes())?; // constant 6

    // Parameters: inbus=0, outbus=0, amp=1.0
    buf.write_all(&3i32.to_be_bytes())?; // num params
    buf.write_all(&0.0f32.to_be_bytes())?; // inbus default = 0
    buf.write_all(&0.0f32.to_be_bytes())?; // outbus default = 0
    buf.write_all(&1.0f32.to_be_bytes())?; // amp default = 1.0

    // Param names
    buf.write_all(&3i32.to_be_bytes())?; // num param names
    // inbus
    let inbus_name = b"inbus";
    buf.push(inbus_name.len() as u8);
    buf.write_all(inbus_name)?;
    buf.write_all(&0i32.to_be_bytes())?; // index 0
    // outbus
    let outbus_name = b"outbus";
    buf.push(outbus_name.len() as u8);
    buf.write_all(outbus_name)?;
    buf.write_all(&1i32.to_be_bytes())?; // index 1
    // amp
    let amp_name = b"amp";
    buf.push(amp_name.len() as u8);
    buf.write_all(amp_name)?;
    buf.write_all(&2i32.to_be_bytes())?; // index 2

    // UGens (14 total)
    buf.write_all(&14i32.to_be_bytes())?; // num ugens

    // Helper to write a constant input reference
    fn write_const_input(buf: &mut Vec<u8>, const_idx: i32) -> std::io::Result<()> {
        buf.write_all(&(-1i32).to_be_bytes())?; // -1 means constant
        buf.write_all(&const_idx.to_be_bytes())?;
        Ok(())
    }

    // Helper to write a UGen input reference
    fn write_ugen_input(buf: &mut Vec<u8>, ugen_idx: i32, output_idx: i32) -> std::io::Result<()> {
        buf.write_all(&ugen_idx.to_be_bytes())?;
        buf.write_all(&output_idx.to_be_bytes())?;
        Ok(())
    }

    // UGen 0: Control (control rate, 3 outputs: inbus, outbus, amp)
    let control_name = b"Control";
    buf.push(control_name.len() as u8);
    buf.write_all(control_name)?;
    buf.push(1); // rate: control
    buf.write_all(&0i32.to_be_bytes())?; // num inputs
    buf.write_all(&3i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    buf.push(1); // output 0 rate: control (inbus)
    buf.push(1); // output 1 rate: control (outbus)
    buf.push(1); // output 2 rate: control (amp)

    // UGen 1: In.ar (audio rate, 2 outputs: left, right)
    let in_name = b"In";
    buf.push(in_name.len() as u8);
    buf.write_all(in_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&1i32.to_be_bytes())?; // num inputs
    buf.write_all(&2i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 0, 0)?; // input: Control output 0 (inbus)
    buf.push(2); // output 0 rate: audio (left)
    buf.push(2); // output 1 rate: audio (right)

    // UGen 2: BinaryOpUGen * (left × amp) - audio rate
    let binop_name = b"BinaryOpUGen";
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&2i16.to_be_bytes())?; // special index: 2 = multiplication
    write_ugen_input(&mut buf, 1, 0)?; // input 0: In output 0 (left)
    write_ugen_input(&mut buf, 0, 2)?; // input 1: Control output 2 (amp)
    buf.push(2); // output rate: audio

    // UGen 3: BinaryOpUGen * (right × amp) - audio rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&2i16.to_be_bytes())?; // special index: 2 = multiplication
    write_ugen_input(&mut buf, 1, 1)?; // input 0: In output 1 (right)
    write_ugen_input(&mut buf, 0, 2)?; // input 1: Control output 2 (amp)
    buf.push(2); // output rate: audio

    // UGen 4: Out.ar (outputs scaled audio to outbus)
    let out_name = b"Out";
    buf.push(out_name.len() as u8);
    buf.write_all(out_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 0, 1)?; // input 0: Control output 1 (outbus)
    write_ugen_input(&mut buf, 2, 0)?; // input 1: BinaryOpUGen#2 (scaled_left)
    write_ugen_input(&mut buf, 3, 0)?; // input 2: BinaryOpUGen#3 (scaled_right)

    // UGen 5: Impulse.kr (20Hz trigger for meter updates)
    let impulse_name = b"Impulse";
    buf.push(impulse_name.len() as u8);
    buf.write_all(impulse_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs (freq, phase)
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_const_input(&mut buf, 4)?; // input 0: constant 4 (20.0 Hz)
    write_const_input(&mut buf, 0)?; // input 1: constant 0 (0.0 phase)
    buf.push(1); // output rate: control

    // UGen 6: Peak.kr (left channel peak, reset by Impulse)
    let peak_name = b"Peak";
    buf.push(peak_name.len() as u8);
    buf.write_all(peak_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 2, 0)?; // input 0: BinaryOpUGen#2 (scaled_left)
    write_ugen_input(&mut buf, 5, 0)?; // input 1: Impulse#5 (reset trigger)
    buf.push(1); // output rate: control

    // UGen 7: Peak.kr (right channel peak, reset by Impulse)
    buf.push(peak_name.len() as u8);
    buf.write_all(peak_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 3, 0)?; // input 0: BinaryOpUGen#3 (scaled_right)
    write_ugen_input(&mut buf, 5, 0)?; // input 1: Impulse#5 (reset trigger)
    buf.push(1); // output rate: control

    // UGen 8: Amplitude.kr (left channel RMS-like)
    let amplitude_name = b"Amplitude";
    buf.push(amplitude_name.len() as u8);
    buf.write_all(amplitude_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 2, 0)?; // input 0: BinaryOpUGen#2 (scaled_left)
    write_const_input(&mut buf, 5)?; // input 1: constant 5 (0.01 attack)
    write_const_input(&mut buf, 6)?; // input 2: constant 6 (0.1 release)
    buf.push(1); // output rate: control

    // UGen 9: Amplitude.kr (right channel RMS-like)
    buf.push(amplitude_name.len() as u8);
    buf.write_all(amplitude_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 3, 0)?; // input 0: BinaryOpUGen#3 (scaled_right)
    write_const_input(&mut buf, 5)?; // input 1: constant 5 (0.01 attack)
    write_const_input(&mut buf, 6)?; // input 2: constant 6 (0.1 release)
    buf.push(1); // output rate: control

    // UGen 10: SendTrig.kr (send peak_left)
    let sendtrig_name = b"SendTrig";
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 5, 0)?; // input 0: Impulse#5 (trigger)
    write_const_input(&mut buf, 0)?; // input 1: constant 0 (ID = 0 for peak_left)
    write_ugen_input(&mut buf, 6, 0)?; // input 2: Peak#6 (peak_left value)

    // UGen 11: SendTrig.kr (send peak_right)
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 5, 0)?; // input 0: Impulse#5 (trigger)
    write_const_input(&mut buf, 1)?; // input 1: constant 1 (ID = 1 for peak_right)
    write_ugen_input(&mut buf, 7, 0)?; // input 2: Peak#7 (peak_right value)

    // UGen 12: SendTrig.kr (send rms_left)
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 5, 0)?; // input 0: Impulse#5 (trigger)
    write_const_input(&mut buf, 2)?; // input 1: constant 2 (ID = 2 for rms_left)
    write_ugen_input(&mut buf, 8, 0)?; // input 2: Amplitude#8 (rms_left value)

    // UGen 13: SendTrig.kr (send rms_right)
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 5, 0)?; // input 0: Impulse#5 (trigger)
    write_const_input(&mut buf, 3)?; // input 1: constant 3 (ID = 3 for rms_right)
    write_ugen_input(&mut buf, 9, 0)?; // input 2: Amplitude#9 (rms_right value)

    // Variants: none
    buf.write_all(&0i16.to_be_bytes())?;

    Ok(buf)
}
