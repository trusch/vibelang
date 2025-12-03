//! Runtime thread for VibeLang.
//!
//! The runtime thread is the heart of VibeLang. It:
//! - Owns the state manager
//! - Runs the beat scheduler
//! - Processes state messages
//! - Communicates with SuperCollider

use crate::events::{BeatEvent, FadeTargetType};
use crate::scheduler::{EventScheduler, LoopKind, LoopSnapshot};
use crate::scsynth::{AddAction, BufNum, NodeId, Scsynth, Target};
use crate::scsynth_process::ScsynthProcess;
use rosc::{OscMessage, OscPacket, OscType};
use crate::state::{
    ActiveFadeJob, ActiveSequence, ActiveSynth, EffectState, GroupState, LoopStatus, MelodyState,
    PatternState, SampleInfo, ScheduledEvent, ScheduledNoteOff, ScriptState, SequenceRunLog,
    StateManager, StateMessage, VoiceState,
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
    /// Uses port 57110 and generates system synthdefs automatically.
    pub fn start_default() -> Result<Self> {
        // Generate system_link_audio synthdef bytes
        let system_synthdef_bytes = create_system_link_audio_bytes()?;
        Self::start(57110, &system_synthdef_bytes)
    }

    /// Start the VibeLang runtime.
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
        // Start scsynth
        log::info!("1. Starting scsynth server...");
        let process = ScsynthProcess::start(port)?;

        // Wait for scsynth to initialize
        std::thread::sleep(Duration::from_millis(1000));

        // Connect to scsynth
        log::info!("2. Connecting to scsynth...");
        let addr = format!("127.0.0.1:{}", port);
        let scsynth = Scsynth::new(&addr)?;
        log::info!("   Connected to scsynth");

        // Load system synthdefs
        scsynth.d_recv_bytes(system_synthdef_bytes.to_vec())?;
        log::info!("   Loaded system_link_audio synthdef");

        // Load SFZ synthdefs
        for (name, bytes) in vibelang_sfz::create_sfz_synthdefs() {
            scsynth.d_recv_bytes(bytes)?;
            log::info!("   Loaded {} synthdef", name);
        }

        // Load sample voice synthdefs (PlayBuf and Warp1 based)
        for (name, bytes) in crate::sample_synthdef::create_sample_synthdefs() {
            scsynth.d_recv_bytes(bytes)?;
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

        // Create runtime handle
        let handle = RuntimeHandle {
            message_tx,
            state_manager: state_manager.clone(),
            scsynth: scsynth.clone(),
            shutdown: shutdown.clone(),
        };

        // Start runtime thread
        log::info!("3. Starting runtime thread...");
        let thread_scsynth = scsynth.clone();
        let thread_state = state_manager.clone();
        let thread_shutdown = shutdown.clone();
        let thread_handle = thread::spawn(move || {
            let mut rt = RuntimeThread::new(thread_scsynth, thread_state, message_rx);
            rt.run(thread_shutdown);
        });

        // Start the scheduler
        handle.send(StateMessage::StartScheduler)?;
        log::info!("   Runtime started");

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
    sc: Scsynth,
    shared: StateManager,
    message_rx: Receiver<StateMessage>,
    scheduler: EventScheduler,
    transport: TransportClock,
    last_tick: Instant,
}

impl RuntimeThread {
    fn new(sc: Scsynth, shared: StateManager, message_rx: Receiver<StateMessage>) -> Self {
        Self {
            sc,
            shared,
            message_rx,
            scheduler: EventScheduler::new(),
            transport: TransportClock::new(),
            last_tick: Instant::now(),
        }
    }

    fn run(&mut self, shutdown: Arc<AtomicBool>) {
        let interval = Duration::from_millis(1);

        while !shutdown.load(Ordering::Relaxed) {
            self.drain_messages();
            self.poll_osc_messages();
            self.tick();
            thread::sleep(interval);
        }
    }

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
                        // Log failures at trace level - "node not found" is expected when
                        // fades try to update synths that have already finished playing
                        log::trace!("[OSC] scsynth failure: {:?}", msg.args);
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
                self.scheduler.reset();
                self.shared.with_state_write(|state| {
                    state.transport_running = true;
                    state.current_beat = 0.0;
                    state.bump_version();
                });
            }
            StateMessage::StopScheduler => {
                self.transport.stop(Instant::now());
                self.shared.with_state_write(|state| {
                    state.transport_running = false;
                    state.bump_version();
                });
            }
            StateMessage::SeekTransport { beat } => {
                let now = Instant::now();
                let target_beat = beat.max(0.0);
                self.transport.seek(BeatTime::from_float(target_beat), now);
                self.scheduler.reset();
                self.shared.with_state_write(|state| {
                    state.current_beat = target_beat;
                    state.bump_version();
                });
            }
            StateMessage::BeginReload => {
                self.shared.with_state_write(|state| {
                    state.reload_generation += 1;
                    state.bump_version();
                });
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
            } => {
                self.handle_register_group(name, path, parent_path, node_id);
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
            } => {
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
                    state.bump_version();
                });
            }
            StateMessage::DeleteVoice { name } => {
                self.shared.with_state_write(|state| {
                    state.voices.remove(&name);
                    state.bump_version();
                });
            }
            StateMessage::SetVoiceParam { name, param, value } => {
                self.shared.with_state_write(|state| {
                    if let Some(voice) = state.voices.get_mut(&name) {
                        voice.params.insert(param, value);
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
                    state.bump_version();
                });
            }
            StateMessage::DeletePattern { name } => {
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
                        for node_id in nodes_to_release {
                            let _ = self.sc.n_set(NodeId::new(node_id), &[("gate".to_string(), 0.0)]);
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
                self.start_sequence(&name);
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

            // === Effects ===
            StateMessage::AddEffect {
                id,
                synthdef,
                group_path,
                params,
                bus_in: _,
                bus_out: _,
            } => {
                self.handle_add_effect(id, synthdef, group_path, params);
            }
            StateMessage::RemoveEffect { id } => {
                let node_to_free = self.shared.with_state_write(|state| {
                    let node = state.effects.remove(&id).and_then(|e| e.node_id);
                    state.bump_version();
                    node
                });
                if let Some(node_id) = node_to_free {
                    let _ = self.sc.n_free(NodeId::new(node_id));
                }
            }
            StateMessage::SetEffectParam { id, param, value } => {
                let node_to_update = self.shared.with_state_write(|state| {
                    let node_id = state.effects.get_mut(&id).map(|effect| {
                        effect.params.insert(param.clone(), value);
                        effect.node_id
                    }).flatten();
                    state.bump_version();
                    node_id
                });
                if let Some(node_id) = node_to_update {
                    let _ = self.sc.n_set(NodeId::new(node_id), &[(param, value)]);
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
                    let _ = self.sc.b_free(BufNum::new(buffer_id));
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

        // Process active sequences - start/stop patterns based on current beat
        self.process_active_sequences(current_beat);

        // Collect loops that need event expansion
        let loops = self.collect_active_loops();

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
        let current_beat = self.transport.beat_at(Instant::now()).to_float();

        // First pass: update iteration tracking and clear triggered_clips on new iterations
        self.shared.with_state_write(|state| {
            for (seq_name, active) in state.active_sequences.iter_mut() {
                if active.paused {
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
                                state.melodies.get(name).map(|m| m.group_path.clone()),
                                state.melodies.get(name).and_then(|m| m.voice_name.clone()),
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
        // Skip if scrub muted
        if self.shared.with_state_read(|s| s.scrub_muted) {
            return;
        }

        // Convert beat time to OSC timestamp and get the Instant when synths will be live
        let (live_instant, timestamp) = self.transport.beat_to_timestamp_and_instant(beat_time, now);

        // Build OSC packets for each event
        let mut packets: Vec<OscPacket> = Vec::new();
        let mut note_offs_to_schedule: Vec<(String, u8, i32, f32)> = Vec::new(); // (voice_name, note, node_id, duration)

        for event in events {
            if let Some((packet, note_off_info)) = self.build_synth_packet(&event, live_instant) {
                packets.push(packet);
                if let Some((voice_name, note, node_id, duration)) = note_off_info {
                    note_offs_to_schedule.push((voice_name, note, node_id, duration));
                }
            }
        }

        // Send bundle with timetag
        if !packets.is_empty() {
            log::trace!("[BUNDLE] Sending timed bundle with {} packets at beat {:?}", packets.len(), beat_time);
            if let Err(e) = self.sc.osc.send_bundle(Some(timestamp), packets) {
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
            event.voice_name.as_ref().and_then(|voice_name| {
                self.shared.with_state_read(|state| {
                    state.voices.get(voice_name).map(|v| {
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
            }).unwrap_or_else(|| (event.synth_def.clone(), std::collections::HashMap::new(), 1.0, None))
        } else {
            (event.synth_def.clone(), std::collections::HashMap::new(), 1.0, None)
        };

        // Get group node ID and audio bus
        let (group_id, audio_bus, group_params) = event
            .group_path
            .as_ref()
            .and_then(|path| {
                self.shared.with_state_read(|state| {
                    state.groups.get(path).map(|g| (
                        g.node_id.unwrap_or(1),
                        g.audio_bus.unwrap_or(0),
                        g.params.clone(),
                    ))
                })
            })
            .unwrap_or((1, 0, std::collections::HashMap::new()));

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
                        g.audio_bus.unwrap_or(0),
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

        // Create synth
        if let Err(e) = self.sc.s_new(
            &synth_def,
            NodeId::new(node_id),
            AddAction::AddToTail,
            Target::from(group_id),
            &controls,
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
    ) {
        // Check if the group already exists in state (was created externally)
        let already_exists = self
            .shared
            .with_state_read(|state| state.groups.contains_key(&path));

        if already_exists {
            log::debug!("Group '{}' already exists, skipping creation", path);
            return;
        }

        // Allocate node ID if not provided (0 means allocate)
        if node_id == 0 {
            node_id = self.shared.with_state_write(|state| state.allocate_group_node());
        }

        // Create the group on SuperCollider
        let parent_id = parent_path
            .as_ref()
            .and_then(|pp| {
                self.shared
                    .with_state_read(|state| state.groups.get(pp).and_then(|g| g.node_id))
            })
            .unwrap_or(0); // Root group

        if let Err(e) = self.sc.g_new(
            NodeId::new(node_id),
            AddAction::AddToTail,
            Target::from(parent_id),
        ) {
            log::error!("Failed to create group '{}': {}", path, e);
            return;
        }

        // Allocate audio bus
        let audio_bus = self.shared.with_state_write(|state| state.allocate_audio_bus());

        // Store in state
        self.shared.with_state_write(|state| {
            let mut group = GroupState::new(name, path.clone(), parent_path);
            group.node_id = Some(node_id);
            group.audio_bus = Some(audio_bus);
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
                // Note: We only update state, not running synths.
                // Group params affect NEW synths - the final amp is calculated as:
                // event_amp × voice_gain × group_amp × voice_amp
                // So the fade affects subsequent notes, not already-playing ones.
            }
            None => {
                log::trace!("[GROUP PARAM] Group '{}' not found when setting {}={}", path_or_name, param, value);
            }
        }
    }

    fn set_group_run_state(&mut self, path: &str, running: bool) {
        let node_to_set = self.shared.with_state_write(|state| {
            let node_id = state.groups.get_mut(path).map(|group| {
                group.muted = !running;
                group.node_id
            }).flatten();
            state.bump_version();
            node_id
        });
        if let Some(node_id) = node_to_set {
            let _ = self.sc.n_run(NodeId::new(node_id), running);
        }
    }

    fn finalize_groups(&mut self) {
        // Get all groups that need link synths, along with their last effect node
        // IMPORTANT: Sort groups so children are processed BEFORE parents.
        // This ensures parent link synths execute AFTER children have written to the parent bus.
        let mut groups: Vec<(String, i32, Option<String>, i32, Option<i32>)> = self.shared.with_state_read(|state| {
            state
                .groups
                .values()
                .filter(|g| g.link_synth_node_id.is_none() && g.audio_bus.is_some())
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
                        g.audio_bus.unwrap(),
                        g.parent_path.clone(),
                        g.node_id.unwrap_or(0),
                        last_effect_node,
                    )
                })
                .collect()
        });

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
                        state.groups.get(pp).and_then(|g| g.audio_bus)
                    })
                })
                .unwrap_or(0);

            // Allocate link synth node
            let link_node_id = self.shared.with_state_write(|state| state.allocate_synth_node());

            // Create the link synth AFTER all effects
            // - If there are effects, add after the last one
            // - If no effects, add to tail (after voices)
            let (add_action, target) = if let Some(last_node) = last_effect_node {
                log::debug!(
                    "[LINK] Creating link synth for '{}' AFTER last effect (node {})",
                    path,
                    last_node
                );
                (AddAction::AddAfter, Target::from(last_node))
            } else {
                log::debug!(
                    "[LINK] Creating link synth for '{}' at TAIL (no effects)",
                    path
                );
                (AddAction::AddToTail, Target::from(group_node_id))
            };

            log::debug!(
                "[LINK] Link synth for '{}': inbus={}, outbus={}",
                path,
                in_bus,
                out_bus
            );

            if let Err(e) = self.sc.s_new(
                "system_link_audio",
                NodeId::new(link_node_id),
                add_action,
                target,
                &[("inbus", in_bus as f32), ("outbus", out_bus as f32)],
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
    }

    fn trigger_voice(
        &mut self,
        name: &str,
        synth_name: Option<String>,
        group_path: Option<String>,
        params: Vec<(String, f32)>,
    ) {
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
            return;
        };

        let synth_def = synth_name.or(default_synth).unwrap_or_else(|| "default".to_string());
        let group = group_path.unwrap_or(default_group);

        // Get group node ID and audio bus
        let (group_id, audio_bus) = self.shared.with_state_read(|state| {
            let group_state = state.groups.get(&group);
            (
                group_state.and_then(|g| g.node_id).unwrap_or(1),
                group_state.and_then(|g| g.audio_bus).unwrap_or(0),
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
        if let Err(e) = self.sc.s_new(
            &synth_def,
            NodeId::new(node_id),
            AddAction::AddToHead,
            Target::from(group_id),
            &controls,
        ) {
            log::error!("Failed to trigger voice '{}': {}", name, e);
        }
    }

    fn handle_note_on(&mut self, voice_name: &str, note: u8, velocity: u8, duration: Option<f64>) {
        // For now, use simple synth triggering
        // Full SFZ support will come later
        let params = vec![
            ("note".to_string(), note as f32),
            ("freq".to_string(), 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)),
            ("velocity".to_string(), velocity as f32 / 127.0),
            ("gate".to_string(), 1.0),
        ];
        self.trigger_voice(voice_name, None, None, params);

        // Schedule note-off if duration specified
        if let Some(dur) = duration {
            let current_beat = self.transport.beat_at(Instant::now()).to_float();
            let off_beat = current_beat + dur;
            self.shared.with_state_write(|state| {
                state.scheduled_note_offs.push(ScheduledNoteOff {
                    beat: off_beat,
                    voice_name: voice_name.to_string(),
                    note,
                    node_id: None,
                });
            });
        }
    }

    fn handle_note_off(&mut self, voice_name: &str, note: u8, specific_node_id: Option<i32>) {
        // If we have a specific node ID, just release that node
        if let Some(node_id) = specific_node_id {
            log::debug!("[NOTE_OFF] Releasing specific node {} for voice '{}'", node_id, voice_name);

            // Remove from tracking BEFORE sending gate=0, so fades don't try to update it
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
            });

            let _ = self.sc.n_set(NodeId::new(node_id), &[("gate".to_string(), 0.0)]);
            return;
        }

        // Otherwise, release all synths for this voice (legacy behavior for voices that don't track individual notes)
        let nodes_to_release: Vec<i32> = self.shared.with_state_write(|state| {
            let nodes: Vec<i32> = state
                .active_synths
                .iter()
                .filter(|(_, s)| s.voice_names.contains(&voice_name.to_string()))
                .map(|(id, _)| *id)
                .collect();

            // Remove all these nodes from tracking
            for &node_id in &nodes {
                state.active_synths.remove(&node_id);
                state.pending_nodes.remove(&node_id);
            }
            if let Some(voice) = state.voices.get_mut(voice_name) {
                voice.active_notes.clear();
            }

            nodes
        });

        log::debug!("[NOTE_OFF] Releasing {} nodes for voice '{}'", nodes_to_release.len(), voice_name);
        for node_id in nodes_to_release {
            let _ = self.sc.n_set(NodeId::new(node_id), &[("gate".to_string(), 0.0)]);
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
                    for (param, value) in &params {
                        if existing_params.get(param) != Some(value) {
                            let _ = self
                                .sc
                                .n_set(NodeId::new(node_id), &[(param.as_str(), *value)]);
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
            if let Some(nid) = existing_node_id {
                log::debug!(
                    "[EFFECT] Effect '{}' synthdef/group changed, freeing old node {}",
                    id,
                    nid
                );
                let _ = self.sc.n_free(NodeId::new(nid));
            }
        }

        // Get group's node ID and audio bus
        // Also find existing effects on this group to determine proper ordering
        let (group_node_id, group_bus, last_effect_node, next_position) =
            self.shared.with_state_read(|state| {
                let group_info = state
                    .groups
                    .get(&group_path)
                    .map(|g| (g.node_id, g.audio_bus));

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
                    group_info.and_then(|(n, _)| n),
                    group_info.and_then(|(_, b)| b),
                    last_node,
                    next_pos,
                )
            });

        let target_node_id = group_node_id;
        let bus_in = group_bus.unwrap_or(0);
        let bus_out = bus_in; // Effects process in-place on the group's bus

        if target_node_id.is_none() {
            log::warn!("[EFFECT] Cannot add effect '{}': group '{}' has no node ID", id, group_path);
            return;
        }

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
        // - If this is the first effect, add to TAIL (voices use AddToHead, so this goes after them)
        // - Link synth (added in FinalizeGroups) will be added AFTER all effects
        let (add_action, target) = if let Some(last_node) = last_effect_node {
            // Add after the last effect in the chain
            (AddAction::AddAfter, Target::from(last_node))
        } else {
            // First effect - add to tail (voices use AddToHead, so this goes after them)
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

        if let Err(e) = self.sc.s_new(
            &synthdef,
            NodeId::new(node_id),
            add_action,
            target,
            &controls
                .iter()
                .map(|(k, v)| (k.as_str(), *v))
                .collect::<Vec<_>>(),
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

    fn start_sequence(&mut self, name: &str) {
        // Check if sequence is already running - if so, preserve its state
        let already_running = self.shared.with_state_read(|state| {
            state.active_sequences.contains_key(name)
        });

        if already_running {
            log::debug!(
                "[SEQUENCE] Sequence '{}' already running, preserving anchor and triggered_clips",
                name
            );
            return;
        }

        let quantization = self.shared.with_state_read(|s| s.quantization_beats);
        let current_beat = self.transport.beat_at(Instant::now()).to_float();
        let anchor_beat = ((current_beat / quantization).ceil() * quantization).max(0.0);

        log::info!("[SEQUENCE] Starting sequence '{}' at anchor beat {:.2}", name, anchor_beat);

        self.shared.with_state_write(|state| {
            state.active_sequences.insert(
                name.to_string(),
                ActiveSequence {
                    anchor_beat,
                    paused: false,
                    triggered_clips: HashMap::new(),
                    last_iteration: 0,
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
                    // n_set may fail for nodes not yet live on scsynth - that's OK
                    let _ = self.sc.n_set(NodeId::new(*node_id), &[(param_name, value)]);
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
                    let _ = self.sc.n_set(NodeId::new(node_id), &[(param_name, value)]);
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
                let _ = self.sc.b_free(BufNum::new(old_buffer));
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

        // Load the sample into the buffer using b_allocRead
        if let Err(e) = self.sc.b_alloc_read(BufNum::new(buffer_id), &path_str) {
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
/// This synthdef routes audio from a group bus to the main output.
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

    // Constants: none
    buf.write_all(&0i32.to_be_bytes())?;

    // Parameters: inbus=0, outbus=0
    buf.write_all(&2i32.to_be_bytes())?; // num params
    buf.write_all(&0.0f32.to_be_bytes())?; // inbus default = 0
    buf.write_all(&0.0f32.to_be_bytes())?; // outbus default = 0

    // Param names
    buf.write_all(&2i32.to_be_bytes())?; // num param names
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

    // UGens: Control, In.ar, Out.ar
    buf.write_all(&3i32.to_be_bytes())?; // num ugens

    // UGen 0: Control (control rate, 2 outputs)
    let control_name = b"Control";
    buf.push(control_name.len() as u8);
    buf.write_all(control_name)?;
    buf.push(1); // rate: control
    buf.write_all(&0i32.to_be_bytes())?; // num inputs
    buf.write_all(&2i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    // Output rates
    buf.push(1); // output 0 rate: control
    buf.push(1); // output 1 rate: control

    // UGen 1: In.ar (audio rate, 2 outputs)
    let in_name = b"In";
    buf.push(in_name.len() as u8);
    buf.write_all(in_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&1i32.to_be_bytes())?; // num inputs
    buf.write_all(&2i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    // Input 0: from Control output 0 (inbus)
    buf.write_all(&0i32.to_be_bytes())?; // ugen index
    buf.write_all(&0i32.to_be_bytes())?; // output index
    // Output rates
    buf.push(2); // output 0 rate: audio
    buf.push(2); // output 1 rate: audio

    // UGen 2: Out.ar (audio rate, 0 outputs)
    let out_name = b"Out";
    buf.push(out_name.len() as u8);
    buf.write_all(out_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    // Input 0: from Control output 1 (outbus)
    buf.write_all(&0i32.to_be_bytes())?; // ugen index
    buf.write_all(&1i32.to_be_bytes())?; // output index
    // Input 1: from In output 0 (left)
    buf.write_all(&1i32.to_be_bytes())?; // ugen index
    buf.write_all(&0i32.to_be_bytes())?; // output index
    // Input 2: from In output 1 (right)
    buf.write_all(&1i32.to_be_bytes())?; // ugen index
    buf.write_all(&1i32.to_be_bytes())?; // output index

    // Variants: none
    buf.write_all(&0i16.to_be_bytes())?;

    Ok(buf)
}
