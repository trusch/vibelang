//! Voices handler implementation.

use crate::backend::{AddAction, Backend};
use crate::compat::{Instant, RwLock};
use crate::state::{State, VoiceState};
use crate::traits::{VoiceConfig, Voices};
use crate::types::{NodeId, ParamMap, VoiceId};
use crate::validation::Validate;
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "midi")]
use crate::midi::{
    has_modulator_cc_mappings, pack_note_off, pack_note_on, send_cc_for_param, send_modulator_ccs,
    QueuedMidiEvent, ScheduledMidiEvent,
};
#[cfg(feature = "midi")]
use crate::types::MidiDeviceId;
#[cfg(feature = "midi")]
use crossbeam_channel::Sender;
#[cfg(feature = "midi")]
use std::sync::Mutex;

/// Shared map of MIDI output device senders.
///
/// This is passed from the MidiHandler to VoicesHandler so that
/// voice parameter changes can be sent as MIDI CC when appropriate.
///
/// Uses `ScheduledMidiEvent` to support both immediate and timestamp-based
/// scheduling for sub-millisecond timing precision.
#[cfg(feature = "midi")]
pub type MidiOutputChannels = Arc<Mutex<HashMap<MidiDeviceId, Sender<ScheduledMidiEvent>>>>;

/// Handler for voice operations.
pub struct VoicesHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Optional MIDI output channels for sending CC on MIDI voices.
    #[cfg(feature = "midi")]
    midi_outputs: Option<MidiOutputChannels>,
}

impl<B: Backend> VoicesHandler<B> {
    /// Create a new voices handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self {
            backend,
            state,
            #[cfg(feature = "midi")]
            midi_outputs: None,
        }
    }

    /// Set the MIDI output channels for sending CC on MIDI voices.
    ///
    /// This should be called by the runtime after creating the MidiHandler
    /// to connect voice parameter changes to MIDI output.
    #[cfg(feature = "midi")]
    pub fn set_midi_outputs(&mut self, outputs: MidiOutputChannels) {
        self.midi_outputs = Some(outputs);
    }

    /// Get a MIDI sender for a device ID (if available).
    #[cfg(feature = "midi")]
    fn get_midi_sender(&self, device_id: MidiDeviceId) -> Option<Sender<ScheduledMidiEvent>> {
        self.midi_outputs
            .as_ref()
            .and_then(|outputs| outputs.lock().ok())
            .and_then(|guard| guard.get(&device_id).cloned())
    }

    /// Send a note-on scheduled for a specific time.
    ///
    /// This is the lookahead-aware version of `note_on`. If `timestamp` is provided,
    /// the MIDI event will be queued and sent at exactly that wall-clock time.
    /// If `timestamp` is None, behaves like immediate `note_on`.
    ///
    /// For MIDI voices, this achieves sub-millisecond timing precision by scheduling
    /// events ahead of time. For audio synths, this falls back to immediate triggering
    /// (audio synths use SC's built-in scheduling for sample-accurate timing).
    #[cfg(feature = "midi")]
    pub async fn note_on_at(
        &self,
        id: VoiceId,
        note: u8,
        velocity: f32,
        timestamp: Option<Instant>,
    ) -> Result<()> {
        // If no timestamp provided, use immediate scheduling
        let timestamp = timestamp.unwrap_or_else(Instant::now);

        // Check for MIDI output - schedule with timestamp
        let (midi_output, midi_channel) = {
            let state = self.state.read().await;
            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
            (voice.config.midi_output, voice.config.midi_channel)
        };

        if let Some(device_id) = midi_output {
            let midi_velocity = (velocity.clamp(0.0, 1.0) * 127.0) as u8;

            // Try direct path with timestamp
            if let Some(sender) = self.get_midi_sender(device_id) {
                let event = QueuedMidiEvent::NoteOn {
                    channel: midi_channel,
                    note,
                    velocity: midi_velocity,
                };
                let scheduled = event.at(timestamp);
                if sender.try_send(scheduled).is_ok() {
                    tracing::debug!(
                        "Voice {:?}: scheduled MIDI note-on: ch={}, note={}, vel={}, ts={:?}",
                        id,
                        midi_channel,
                        note,
                        midi_velocity,
                        timestamp
                    );
                    return Ok(());
                }
            }
            // Fallback to immediate via trait method
        }

        // For non-MIDI voices or fallback, use immediate note_on
        self.note_on(id, note, velocity).await
    }

    /// Send a note-on with per-note voice parameters, scheduled for a specific time.
    ///
    /// Like `note_on_at`, but also merges `extra_params` into the synth creation
    /// params. These params override the voice's defaults for this specific note only
    /// (e.g., cutoff, pan, resonance set via `C4[cutoff=2000]` notation).
    ///
    /// For MIDI voices, extra params are sent as CC messages if mappings exist.
    #[cfg(feature = "midi")]
    pub async fn note_on_at_with_params(
        &self,
        id: VoiceId,
        note: u8,
        velocity: f32,
        timestamp: Option<Instant>,
        extra_params: &ParamMap,
    ) -> Result<()> {
        // If no timestamp provided, use immediate scheduling
        let timestamp = timestamp.unwrap_or_else(Instant::now);

        // Check for MIDI output - schedule with timestamp
        let (midi_output, midi_channel) = {
            let state = self.state.read().await;
            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
            (voice.config.midi_output, voice.config.midi_channel)
        };

        if let Some(device_id) = midi_output {
            let midi_velocity = (velocity.clamp(0.0, 1.0) * 127.0) as u8;

            // Send extra params as CC if mappings exist
            if let Some(sender) = self.get_midi_sender(device_id) {
                // Send per-note CC params before note-on
                let state = self.state.read().await;
                if let Some(voice) = state.voices.get(&id) {
                    for (param, value) in extra_params {
                        if let Some(&cc) = voice.config.param_cc_map.get(param) {
                            let cc_value = (value.clamp(0.0, 1.0) * 127.0) as u8;
                            let cc_event = QueuedMidiEvent::ControlChange {
                                channel: midi_channel,
                                cc,
                                value: cc_value,
                            };
                            let _ = sender.try_send(cc_event.at(timestamp));
                        }
                    }
                }
                drop(state);

                let event = QueuedMidiEvent::NoteOn {
                    channel: midi_channel,
                    note,
                    velocity: midi_velocity,
                };
                let scheduled = event.at(timestamp);
                if sender.try_send(scheduled).is_ok() {
                    tracing::debug!(
                        "Voice {:?}: scheduled MIDI note-on with params: ch={}, note={}, vel={}, params={:?}",
                        id, midi_channel, note, midi_velocity, extra_params
                    );
                    return Ok(());
                }
            }
            // Fallback to immediate via with_params method
        }

        // For non-MIDI voices or fallback, use immediate note_on_with_params
        self.note_on_with_params(id, note, velocity, extra_params)
            .await
    }

    /// Send a note-on with per-note voice parameters (immediate).
    ///
    /// Like `note_on`, but also merges `extra_params` into the synth creation
    /// params. These params override the voice's defaults for this specific note only
    /// (e.g., cutoff, pan, resonance set via `C4[cutoff=2000]` notation).
    pub async fn note_on_with_params(
        &self,
        id: VoiceId,
        note: u8,
        velocity: f32,
        extra_params: &ParamMap,
    ) -> Result<()> {
        // Check for MIDI output first
        #[cfg(feature = "midi")]
        {
            let (midi_output, midi_channel, node_id) = {
                let mut state = self.state.write().await;
                let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
                let midi_out = voice.config.midi_output;
                let ch = voice.config.midi_channel;
                let node = if midi_out.is_some() {
                    Some(state.alloc_node_id())
                } else {
                    None
                };
                (midi_out, ch, node)
            };

            if let Some(device_id) = midi_output {
                let midi_velocity = (velocity.clamp(0.0, 1.0) * 127.0) as u8;

                // Send extra params as CC if mappings exist
                if let Some(sender) = self.get_midi_sender(device_id) {
                    let state = self.state.read().await;
                    if let Some(voice) = state.voices.get(&id) {
                        for (param, value) in extra_params {
                            if let Some(&cc) = voice.config.param_cc_map.get(param) {
                                let cc_value = (value.clamp(0.0, 1.0) * 127.0) as u8;
                                let cc_event = QueuedMidiEvent::ControlChange {
                                    channel: midi_channel,
                                    cc,
                                    value: cc_value,
                                };
                                let _ = sender.try_send(cc_event.immediate());
                            }
                        }
                    }
                }

                // Try direct path first
                if let Some(sender) = self.get_midi_sender(device_id) {
                    let event = QueuedMidiEvent::NoteOn {
                        channel: midi_channel,
                        note,
                        velocity: midi_velocity,
                    };
                    if sender.try_send(event.immediate()).is_ok() {
                        tracing::debug!(
                            "Voice {:?}: direct MIDI note-on with params: ch={}, note={}, vel={}, params={:?}",
                            id, midi_channel, note, midi_velocity, extra_params
                        );
                        return Ok(());
                    }
                }

                // Fallback: Use sample-accurate MIDI output via synthdef
                let packed_data =
                    pack_note_on(device_id.0 as u8, midi_channel, note, midi_velocity);
                let mut params = std::collections::HashMap::new();
                params.insert("packed_data".to_string(), packed_data);

                let node_id = node_id.ok_or(Error::backend_msg("MIDI node ID not allocated"))?;
                if let Err(_e) = self
                    .backend
                    .create_synth(
                        "midi_out",
                        node_id,
                        NodeId::new(0),
                        AddAction::Head,
                        &params,
                    )
                    .await
                {
                    tracing::debug!(
                        "Voice {:?}: SC MIDI note-on with params (fallback): ch={}, note={}, vel={}",
                        id, midi_channel, note, midi_velocity
                    );
                }
                return Ok(());
            }
        }

        // Gather info and allocate node while holding lock
        let (node_id, group_node_id, synthdef, params, old_node, modulations_to_apply) = {
            let mut state = self.state.write().await;

            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;

            let group = state
                .groups
                .get(&voice.config.group)
                .ok_or(Error::GroupNotFound(voice.config.group))?;

            let synthdef = voice.config.synthdef.clone();
            let group_node_id = group.node_id;

            // Build params with note info
            let mut params = voice.config.params.clone();
            params.insert("freq".to_string(), midi_to_freq(note));
            params.insert("amp".to_string(), velocity);
            params.insert("gate".to_string(), 1.0);

            // Merge per-note extra params (overrides defaults)
            for (k, v) in extra_params {
                params.insert(k.clone(), *v);
            }

            // Set output bus to group's audio bus (for proper routing)
            params.insert("out".to_string(), group.audio_bus.0 as f32);

            // Convert sample offset/length from seconds to synth params
            if let Some(sample_id) = voice.config.sample_id {
                if let Some(sample_info) = state.samples.get(&sample_id) {
                    let sample_rate = sample_info.sample_rate;
                    let duration = sample_info.duration_secs;
                    let is_warp = voice.config.synthdef.contains("warp");

                    let offset_secs = params.remove("_offset_secs").unwrap_or(0.0) as f64;
                    let release_secs = params.remove("_release_secs").unwrap_or(0.0) as f64;
                    let length_secs = params.remove("_length_secs");

                    let effective_length = if let Some(len) = length_secs {
                        Some((len as f64 - release_secs).max(0.01))
                    } else if release_secs > 0.0 {
                        let remaining = duration - offset_secs - release_secs;
                        if remaining > 0.0 {
                            Some(remaining)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if is_warp {
                        let start_norm = (offset_secs / duration).min(1.0);
                        params.insert("startPos".to_string(), start_norm as f32);
                        if let Some(len) = effective_length {
                            let end_norm = ((offset_secs + len) / duration).min(1.0);
                            params.insert("endPos".to_string(), end_norm as f32);
                        }
                    } else {
                        let start_frame = (offset_secs * sample_rate) as f32;
                        params.insert("startPos".to_string(), start_frame);
                        if let Some(len) = effective_length {
                            let end_frame = ((offset_secs + len) * sample_rate) as f32;
                            params.insert("endPos".to_string(), end_frame);
                        }
                    }
                }
            }

            // Collect modulations to apply after synth creation
            let mut modulations_to_apply: Vec<(String, u32)> = Vec::new();
            for (param_name, modulator_id) in &voice.config.modulations {
                if let Some(modulator_state) = state.modulators.get(modulator_id) {
                    let control_bus = modulator_state.control_bus.raw();
                    modulations_to_apply.push((param_name.clone(), control_bus));
                }
            }

            let node_id = state.alloc_node_id();

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;
            let old_node = voice.note_nodes.remove(&note);
            voice.note_nodes.insert(note, node_id);

            (
                node_id,
                group_node_id,
                synthdef,
                params,
                old_node,
                modulations_to_apply,
            )
        };

        // Free old node if any (lock released)
        if let Some(old) = old_node {
            let _ = self.backend.free_node(old).await;
        }

        // Create synth
        self.backend
            .create_synth(&synthdef, node_id, group_node_id, AddAction::Head, &params)
            .await
            .map_err(Error::backend)?;

        // Apply modulations
        for (param_name, control_bus) in modulations_to_apply {
            if let Err(e) = self
                .backend
                .map_param_to_bus(node_id, &param_name, control_bus)
                .await
            {
                tracing::error!(
                    "Failed to map param '{}' to control bus {}: {}",
                    param_name,
                    control_bus,
                    e
                );
            }
        }

        Ok(())
    }

    /// Send a note-off scheduled for a specific time.
    ///
    /// This is the lookahead-aware version of `note_off`. If `timestamp` is provided,
    /// the MIDI event will be queued and sent at exactly that wall-clock time.
    /// If `timestamp` is None, behaves like immediate `note_off`.
    #[cfg(feature = "midi")]
    pub async fn note_off_at(
        &self,
        id: VoiceId,
        note: u8,
        timestamp: Option<Instant>,
    ) -> Result<()> {
        // If no timestamp provided, use immediate scheduling
        let timestamp = timestamp.unwrap_or_else(Instant::now);

        // Check for MIDI output - schedule with timestamp
        let (midi_output, midi_channel) = {
            let state = self.state.read().await;
            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
            (voice.config.midi_output, voice.config.midi_channel)
        };

        if let Some(device_id) = midi_output {
            // Try direct path with timestamp
            if let Some(sender) = self.get_midi_sender(device_id) {
                let event = QueuedMidiEvent::NoteOff {
                    channel: midi_channel,
                    note,
                };
                let scheduled = event.at(timestamp);
                if sender.try_send(scheduled).is_ok() {
                    tracing::debug!(
                        "Voice {:?}: scheduled MIDI note-off: ch={}, note={}, ts={:?}",
                        id,
                        midi_channel,
                        note,
                        timestamp
                    );
                    return Ok(());
                }
            }
            // Fallback to immediate via trait method
        }

        // For non-MIDI voices or fallback, use immediate note_off
        self.note_off(id, note).await
    }

    /// Tick modulator outputs for MIDI voices.
    ///
    /// This method polls modulator values and sends MIDI CC messages for
    /// MIDI voices that have modulator-to-CC mappings configured.
    ///
    /// Called by the runtime's tick loop.
    ///
    /// ## Performance
    ///
    /// Uses batch control bus reading to minimize OSC roundtrips.
    /// All control buses are requested in parallel, then responses are
    /// collected with a single timeout window. This allows ~500+ CC/sec
    /// throughput even with many modulators.
    #[cfg(feature = "midi")]
    pub async fn tick_modulators(&self) {
        // Collect MIDI voices with modulator CC mappings, and all unique control buses
        #[allow(clippy::type_complexity)]
        let (midi_voices_info, all_buses): (
            Vec<(VoiceConfig, Vec<(String, u32)>)>,
            Vec<u32>,
        ) = {
            let state = self.state.read().await;
            let mut all_buses_set = std::collections::HashSet::new();

            let voices: Vec<_> = state
                .voices
                .values()
                .filter(|v| has_modulator_cc_mappings(&v.config))
                .filter_map(|v| {
                    // Ensure the voice has MIDI output configured
                    let _device_id = v.config.midi_output?;

                    // Collect modulator control buses for this voice
                    let modulator_buses: Vec<(String, u32)> = v
                        .config
                        .modulations
                        .iter()
                        .filter_map(|(param, mod_id)| {
                            // Only include if there's a CC mapping for this param
                            if !v.config.param_cc_map.contains_key(param) {
                                return None;
                            }
                            // Get the modulator's control bus
                            state.modulators.get(mod_id).map(|m| {
                                let bus = m.control_bus.raw();
                                all_buses_set.insert(bus);
                                (mod_id.0.to_string(), bus)
                            })
                        })
                        .collect();

                    if modulator_buses.is_empty() {
                        return None;
                    }

                    Some((v.config.clone(), modulator_buses))
                })
                .collect();

            let buses: Vec<u32> = all_buses_set.into_iter().collect();
            (voices, buses)
        };

        if midi_voices_info.is_empty() || all_buses.is_empty() {
            return;
        }

        // Batch read ALL control buses in one call
        let bus_values = match self.backend.get_control_buses(&all_buses).await {
            Ok(values) => values,
            Err(e) => {
                tracing::warn!("Failed to batch read control buses: {:?}", e);
                return;
            }
        };

        // Distribute values to each voice and send CC
        for (config, modulator_buses) in midi_voices_info {
            let device_id = match config.midi_output {
                Some(id) => id,
                None => continue,
            };

            let sender = match self.get_midi_sender(device_id) {
                Some(s) => s,
                None => continue,
            };

            // Build modulator values map from batch results
            let mut modulator_values = HashMap::new();
            for (mod_id_str, control_bus) in modulator_buses {
                if let Some(&value) = bus_values.get(&control_bus) {
                    modulator_values.insert(mod_id_str, value);
                }
            }

            // Send CC messages for modulated parameters
            let sent = send_modulator_ccs(&config, &modulator_values, &sender);
            if sent > 0 {
                tracing::trace!(
                    "MIDI voice '{}': sent {} modulator CC messages",
                    config.name,
                    sent
                );
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Voices for VoicesHandler<B> {
    async fn create(&self, id: VoiceId, config: VoiceConfig) -> Result<()> {
        // Validate configuration before acquiring lock
        config.validate()?;

        let mut state = self.state.write().await;

        if state.voices.contains_key(&id) {
            return Err(Error::VoiceExists(id));
        }

        // Verify the group exists
        if !state.groups.contains_key(&config.group) {
            return Err(Error::GroupNotFound(config.group));
        }

        // Verify the synthdef exists (unless using SFZ instrument)
        if config.sfz_instrument.is_none()
            && !config.synthdef.is_empty()
            && !state.synthdefs.contains(&config.synthdef)
        {
            return Err(Error::SynthDefNotFound(config.synthdef.clone()));
        }

        // Store state
        state.voices.insert(
            id,
            VoiceState {
                id,
                config,
                active_nodes: Vec::new(),
                note_nodes: HashMap::new(),
                round_robin_position: 0,
                pending_params: HashMap::new(),
            },
        );

        Ok(())
    }

    async fn delete(&self, id: VoiceId) -> Result<()> {
        let nodes_to_free = {
            let mut state = self.state.write().await;
            let voice = state.voices.remove(&id).ok_or(Error::VoiceNotFound(id))?;
            voice.active_nodes
        };

        // Free all active synth nodes (lock released)
        for node_id in nodes_to_free {
            let _ = self.backend.free_node(node_id).await;
        }

        Ok(())
    }

    async fn graceful_delete(&self, id: VoiceId) -> Result<()> {
        let nodes_to_release = {
            let mut state = self.state.write().await;
            let voice = state.voices.remove(&id).ok_or(Error::VoiceNotFound(id))?;
            voice.active_nodes
        };

        // Set gate=0 on all active synth nodes to trigger release envelope.
        // The synths will free themselves via doneAction=2 when the envelope completes.
        // This avoids abrupt audio cuts during voice config updates.
        for node_id in nodes_to_release {
            tracing::debug!(
                "Voice {:?}: graceful release - setting gate=0 on node {:?}",
                id,
                node_id
            );
            let _ = self.backend.set_param(node_id, "gate", 0.0).await;
        }

        Ok(())
    }

    async fn trigger(&self, id: VoiceId, params: &ParamMap) -> Result<()> {
        // Gather info and allocate node while holding lock
        // modulations_to_apply: Vec<(param_name, control_bus)>
        let (
            node_id,
            group_node_id,
            synthdef,
            merged_params,
            old_nodes,
            choke_nodes,
            modulations_to_apply,
        ) = {
            let mut state = self.state.write().await;

            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;

            let group = state
                .groups
                .get(&voice.config.group)
                .ok_or(Error::GroupNotFound(voice.config.group))?;

            // Merge default params with trigger params
            let mut merged_params = voice.config.params.clone();
            merged_params.extend(params.clone());

            // Set output bus to group's audio bus (for proper routing)
            merged_params.insert("out".to_string(), group.audio_bus.0 as f32);

            // Convert sample offset/length from seconds to synth params
            if let Some(sample_id) = voice.config.sample_id {
                if let Some(sample_info) = state.samples.get(&sample_id) {
                    let sample_rate = sample_info.sample_rate;
                    let duration = sample_info.duration_secs;
                    let is_warp = voice.config.synthdef.contains("warp");

                    // Get and remove temporary params
                    let offset_secs = merged_params.remove("_offset_secs").unwrap_or(0.0) as f64;
                    let release_secs = merged_params.remove("_release_secs").unwrap_or(0.0) as f64;
                    let length_secs = merged_params.remove("_length_secs");

                    // Calculate effective length for fade-out
                    // If no explicit length, use full sample minus release time
                    let effective_length = if let Some(len) = length_secs {
                        // Explicit length - subtract release for fade
                        Some((len as f64 - release_secs).max(0.01))
                    } else if release_secs > 0.0 {
                        // Full sample minus release for proper fade
                        let remaining = duration - offset_secs - release_secs;
                        if remaining > 0.0 {
                            Some(remaining)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if is_warp {
                        // Warp voice uses normalized 0-1 positions
                        let start_norm = (offset_secs / duration).min(1.0);
                        merged_params.insert("startPos".to_string(), start_norm as f32);

                        if let Some(len) = effective_length {
                            let end_norm = ((offset_secs + len) / duration).min(1.0);
                            merged_params.insert("endPos".to_string(), end_norm as f32);
                        }
                        // else endPos defaults to 1.0 (full sample)
                    } else {
                        // Sample voice uses frame positions
                        let start_frame = (offset_secs * sample_rate) as f32;
                        merged_params.insert("startPos".to_string(), start_frame);

                        if let Some(len) = effective_length {
                            let end_frame = ((offset_secs + len) * sample_rate) as f32;
                            merged_params.insert("endPos".to_string(), end_frame);
                        }
                        // else endPos defaults to -1 (full sample)
                    }

                    tracing::debug!(
                        "Voice {:?}: sample conversion - offset={:.2}s, length={:?}s, release={:.2}s, is_warp={}",
                        id, offset_secs, effective_length, release_secs, is_warp
                    );
                }
            }

            let synthdef = voice.config.synthdef.clone();
            let group_node_id = group.node_id;
            let polyphony = voice.config.polyphony as usize;
            let round_robin_count = voice.config.round_robin_count;
            let choke_group = voice.config.choke_group.clone();

            // Collect modulations to apply after synth creation
            // (param_name, control_bus)
            let mut modulations_to_apply: Vec<(String, u32)> = Vec::new();
            tracing::debug!(
                "Voice {:?}: has {} modulations configured",
                id,
                voice.config.modulations.len()
            );
            for (param_name, modulator_id) in &voice.config.modulations {
                tracing::debug!(
                    "Voice {:?}: checking modulation for param '{}' with modulator {:?}",
                    id,
                    param_name,
                    modulator_id
                );
                if let Some(modulator_state) = state.modulators.get(modulator_id) {
                    let control_bus = modulator_state.control_bus.raw();
                    modulations_to_apply.push((param_name.clone(), control_bus));
                    tracing::debug!(
                        "Voice {:?}: will map param '{}' to control bus {} (modulator {:?})",
                        id,
                        param_name,
                        control_bus,
                        modulator_id
                    );
                } else {
                    tracing::warn!(
                        "Voice {:?}: modulator {:?} not found for param '{}', skipping modulation. Available modulators: {:?}",
                        id, modulator_id, param_name, state.modulators.keys().collect::<Vec<_>>()
                    );
                }
            }

            let node_id = state.alloc_node_id();

            // Handle choke groups: collect nodes to choke from other voices in same group
            let mut choke_nodes = Vec::new();
            if let Some(ref choke) = choke_group {
                for (voice_id, other_voice) in state.voices.iter_mut() {
                    // Skip the voice being triggered
                    if *voice_id == id {
                        continue;
                    }
                    // Check if in same choke group
                    if other_voice.config.choke_group.as_ref() == Some(choke) {
                        // Collect all active nodes to choke
                        choke_nodes.append(&mut other_voice.active_nodes);
                        choke_nodes.extend(other_voice.note_nodes.drain().map(|(_, n)| n));
                    }
                }
            }

            // Update voice state
            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;
            voice.active_nodes.push(node_id);

            // Handle round-robin: add `rr` parameter and increment position
            if round_robin_count > 0 {
                merged_params.insert("rr".to_string(), voice.round_robin_position as f32);
                voice.round_robin_position = (voice.round_robin_position + 1) % round_robin_count;
            }

            // Collect nodes to free (polyphony management)
            let mut old_nodes = Vec::new();
            while voice.active_nodes.len() > polyphony {
                if let Some(old_node) = voice.active_nodes.first().copied() {
                    voice.active_nodes.remove(0);
                    old_nodes.push(old_node);
                }
            }

            (
                node_id,
                group_node_id,
                synthdef,
                merged_params,
                old_nodes,
                choke_nodes,
                modulations_to_apply,
            )
        };

        // Choke nodes from other voices in the same choke group (lock released)
        for choke_node in choke_nodes {
            let _ = self.backend.free_node(choke_node).await;
        }

        // Create synth in backend
        // Use Head so voices execute before effects/link synths (which are at tail)
        self.backend
            .create_synth(
                &synthdef,
                node_id,
                group_node_id,
                AddAction::Head,
                &merged_params,
            )
            .await
            .map_err(Error::backend)?;

        // Apply modulations: map parameters to control buses
        for (param_name, control_bus) in modulations_to_apply {
            if let Err(e) = self
                .backend
                .map_param_to_bus(node_id, &param_name, control_bus)
                .await
            {
                tracing::error!(
                    "Failed to map param '{}' to control bus {}: {}",
                    param_name,
                    control_bus,
                    e
                );
            }
        }

        // Free old nodes (polyphony limit)
        for old_node in old_nodes {
            let _ = self.backend.free_node(old_node).await;
        }

        Ok(())
    }

    async fn stop(&self, id: VoiceId) -> Result<()> {
        let nodes_to_free = {
            let mut state = self.state.write().await;

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;

            let nodes: Vec<NodeId> = voice.active_nodes.drain(..).collect();
            voice.note_nodes.clear();
            nodes
        };

        // Free all active synth nodes (lock released)
        for node_id in nodes_to_free {
            let _ = self.backend.free_node(node_id).await;
        }

        Ok(())
    }

    async fn note_on(&self, id: VoiceId, note: u8, velocity: f32) -> Result<()> {
        // Check for MIDI output first - use sample-accurate timing via synthdef
        #[cfg(feature = "midi")]
        {
            let (midi_output, midi_channel, node_id) = {
                let mut state = self.state.write().await;
                let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
                let midi_output = voice.config.midi_output;
                let midi_channel = voice.config.midi_channel;
                let node_id = if midi_output.is_some() {
                    Some(state.alloc_node_id())
                } else {
                    None
                };
                (midi_output, midi_channel, node_id)
            };

            if let Some(device_id) = midi_output {
                let midi_velocity = (velocity.clamp(0.0, 1.0) * 127.0) as u8;

                // Try direct path first (lower latency, ~2ms vs ~20ms)
                // Uses immediate scheduling - for lookahead scheduling, use try_send with timestamp
                if let Some(sender) = self.get_midi_sender(device_id) {
                    let event = QueuedMidiEvent::NoteOn {
                        channel: midi_channel,
                        note,
                        velocity: midi_velocity,
                    };
                    // Send immediately (no lookahead for direct note_on calls)
                    if sender.try_send(event.immediate()).is_ok() {
                        tracing::debug!(
                            "Voice {:?}: direct MIDI note-on: ch={}, note={}, vel={}",
                            id,
                            midi_channel,
                            note,
                            midi_velocity
                        );
                        return Ok(());
                    }
                }

                // Fallback: Use sample-accurate MIDI output via synthdef
                // The synthdef fires a SendTrig at creation time, which the
                // MidiRealtimeService receives and converts to actual MIDI bytes.
                let packed_data =
                    pack_note_on(device_id.0 as u8, midi_channel, note, midi_velocity);

                let mut params = std::collections::HashMap::new();
                params.insert("packed_data".to_string(), packed_data);

                // Create synth at root node - it will fire and free itself immediately
                // Safety: node_id is Some when midi_output is Some (allocated above)
                let node_id = node_id.ok_or(Error::backend_msg("MIDI node ID not allocated"))?;
                if let Err(e) = self
                    .backend
                    .create_synth(
                        "vibelang_midi_note_on",
                        node_id,
                        NodeId::new(0), // root node
                        AddAction::Tail,
                        &params,
                    )
                    .await
                {
                    tracing::warn!(
                        "Voice {:?}: failed to create MIDI note-on synth: {:?}",
                        id,
                        e
                    );
                } else {
                    tracing::debug!(
                        "Voice {:?}: SC MIDI note-on (fallback): ch={}, note={}, vel={}",
                        id,
                        midi_channel,
                        note,
                        midi_velocity
                    );
                }
                return Ok(());
            }
        }

        // Gather info and allocate node while holding lock
        let (node_id, group_node_id, synthdef, params, old_node, modulations_to_apply) = {
            let mut state = self.state.write().await;

            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;

            let group = state
                .groups
                .get(&voice.config.group)
                .ok_or(Error::GroupNotFound(voice.config.group))?;

            let synthdef = voice.config.synthdef.clone();
            let group_node_id = group.node_id;

            // Build params with note info
            let mut params = voice.config.params.clone();
            params.insert("freq".to_string(), midi_to_freq(note));
            params.insert("amp".to_string(), velocity);
            params.insert("gate".to_string(), 1.0);

            // Set output bus to group's audio bus (for proper routing)
            params.insert("out".to_string(), group.audio_bus.0 as f32);

            // Convert sample offset/length from seconds to synth params
            if let Some(sample_id) = voice.config.sample_id {
                if let Some(sample_info) = state.samples.get(&sample_id) {
                    let sample_rate = sample_info.sample_rate;
                    let duration = sample_info.duration_secs;
                    let is_warp = voice.config.synthdef.contains("warp");

                    // Get and remove temporary params
                    let offset_secs = params.remove("_offset_secs").unwrap_or(0.0) as f64;
                    let release_secs = params.remove("_release_secs").unwrap_or(0.0) as f64;
                    let length_secs = params.remove("_length_secs");

                    // Calculate effective length for fade-out
                    let effective_length = if let Some(len) = length_secs {
                        Some((len as f64 - release_secs).max(0.01))
                    } else if release_secs > 0.0 {
                        let remaining = duration - offset_secs - release_secs;
                        if remaining > 0.0 {
                            Some(remaining)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if is_warp {
                        let start_norm = (offset_secs / duration).min(1.0);
                        params.insert("startPos".to_string(), start_norm as f32);

                        if let Some(len) = effective_length {
                            let end_norm = ((offset_secs + len) / duration).min(1.0);
                            params.insert("endPos".to_string(), end_norm as f32);
                        }
                    } else {
                        let start_frame = (offset_secs * sample_rate) as f32;
                        params.insert("startPos".to_string(), start_frame);

                        if let Some(len) = effective_length {
                            let end_frame = ((offset_secs + len) * sample_rate) as f32;
                            params.insert("endPos".to_string(), end_frame);
                        }
                    }

                    tracing::debug!(
                        "Voice {:?} note_on: sample conversion - offset={:.2}s, length={:?}s",
                        id,
                        offset_secs,
                        effective_length
                    );
                }
            }

            // Collect modulations to apply after synth creation
            let mut modulations_to_apply: Vec<(String, u32)> = Vec::new();
            tracing::debug!(
                "Voice {:?} note_on: has {} modulations configured",
                id,
                voice.config.modulations.len()
            );
            for (param_name, modulator_id) in &voice.config.modulations {
                tracing::debug!(
                    "Voice {:?} note_on: checking modulation for param '{}' with modulator {:?}",
                    id,
                    param_name,
                    modulator_id
                );
                if let Some(modulator_state) = state.modulators.get(modulator_id) {
                    let control_bus = modulator_state.control_bus.raw();
                    modulations_to_apply.push((param_name.clone(), control_bus));
                    tracing::debug!(
                        "Voice {:?} note_on: will map param '{}' to control bus {} (modulator {:?})",
                        id, param_name, control_bus, modulator_id
                    );
                } else {
                    tracing::warn!(
                        "Voice {:?} note_on: modulator {:?} not found for param '{}'. Available modulators: {:?}",
                        id, modulator_id, param_name, state.modulators.keys().collect::<Vec<_>>()
                    );
                }
            }

            let node_id = state.alloc_node_id();

            // Update voice state
            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;

            // If note already playing, collect it for cleanup
            let old_node = voice.note_nodes.remove(&note);

            // Track note -> node mapping
            voice.note_nodes.insert(note, node_id);

            (
                node_id,
                group_node_id,
                synthdef,
                params,
                old_node,
                modulations_to_apply,
            )
        };

        // Free old node if any (lock released)
        if let Some(old) = old_node {
            let _ = self.backend.free_node(old).await;
        }

        // Create synth
        // Use Head so voices execute before effects/link synths (which are at tail)
        self.backend
            .create_synth(&synthdef, node_id, group_node_id, AddAction::Head, &params)
            .await
            .map_err(Error::backend)?;

        // Apply modulations: map parameters to control buses
        for (param_name, control_bus) in modulations_to_apply {
            if let Err(e) = self
                .backend
                .map_param_to_bus(node_id, &param_name, control_bus)
                .await
            {
                tracing::error!(
                    "Failed to map param '{}' to control bus {}: {}",
                    param_name,
                    control_bus,
                    e
                );
            }
        }

        Ok(())
    }

    async fn note_off(&self, id: VoiceId, note: u8) -> Result<()> {
        // Check for MIDI output first - use sample-accurate timing via synthdef
        #[cfg(feature = "midi")]
        {
            let (midi_output, midi_channel, node_id) = {
                let mut state = self.state.write().await;
                let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
                let midi_output = voice.config.midi_output;
                let midi_channel = voice.config.midi_channel;
                let node_id = if midi_output.is_some() {
                    Some(state.alloc_node_id())
                } else {
                    None
                };
                (midi_output, midi_channel, node_id)
            };

            if let Some(device_id) = midi_output {
                // Try direct path first (lower latency, ~2ms vs ~20ms)
                // Uses immediate scheduling - for lookahead scheduling, use try_send with timestamp
                if let Some(sender) = self.get_midi_sender(device_id) {
                    let event = QueuedMidiEvent::NoteOff {
                        channel: midi_channel,
                        note,
                    };
                    // Send immediately (no lookahead for direct note_off calls)
                    if sender.try_send(event.immediate()).is_ok() {
                        tracing::debug!(
                            "Voice {:?}: direct MIDI note-off: ch={}, note={}",
                            id,
                            midi_channel,
                            note
                        );
                        return Ok(());
                    }
                }

                // Fallback: Use sample-accurate MIDI output via synthdef
                let packed_data = pack_note_off(device_id.0 as u8, midi_channel, note);

                let mut params = std::collections::HashMap::new();
                params.insert("packed_data".to_string(), packed_data);

                // Create synth at root node - it will fire and free itself immediately
                // Safety: node_id is Some when midi_output is Some (allocated above)
                let node_id = node_id.ok_or(Error::backend_msg("MIDI node ID not allocated"))?;
                if let Err(e) = self
                    .backend
                    .create_synth(
                        "vibelang_midi_note_off",
                        node_id,
                        NodeId::new(0), // root node
                        AddAction::Tail,
                        &params,
                    )
                    .await
                {
                    tracing::warn!(
                        "Voice {:?}: failed to create MIDI note-off synth: {:?}",
                        id,
                        e
                    );
                } else {
                    tracing::debug!(
                        "Voice {:?}: SC MIDI note-off (fallback): ch={}, note={}",
                        id,
                        midi_channel,
                        note
                    );
                }
                return Ok(());
            }
        }

        let (node_to_release, is_sample_voice) = {
            let mut state = self.state.write().await;

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;

            let is_sample_voice =
                voice.config.sample_id.is_some() || voice.config.sfz_instrument.is_some();
            (voice.note_nodes.remove(&note), is_sample_voice)
        };

        // Release the note (lock released)
        if let Some(node_id) = node_to_release {
            if is_sample_voice {
                // Sample/SFZ synths don't respond to gate - free the node directly.
                let _ = self.backend.free_node(node_id).await;
            } else {
                self.backend
                    .set_param(node_id, "gate", 0.0)
                    .await
                    .map_err(Error::backend)?;
            }
        }

        Ok(())
    }

    async fn mute(&self, id: VoiceId, muted: bool) -> Result<()> {
        let mut state = self.state.write().await;

        let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;

        voice.config.muted = muted;

        Ok(())
    }

    async fn set_param(&self, id: VoiceId, param: &str, value: f32) -> Result<()> {
        // Get all active nodes, voice config, and update the default param value
        #[cfg(feature = "midi")]
        let (nodes, voice_config): (Vec<NodeId>, VoiceConfig) = {
            let mut state = self.state.write().await;

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;

            // Update the default param value for future triggers
            voice.config.params.insert(param.to_string(), value);

            // Collect all active nodes (both trigger nodes and note nodes)
            let mut nodes: Vec<NodeId> = voice.active_nodes.clone();
            nodes.extend(voice.note_nodes.values().copied());

            (nodes, voice.config.clone())
        };

        #[cfg(not(feature = "midi"))]
        let nodes: Vec<NodeId> = {
            let mut state = self.state.write().await;

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;

            // Update the default param value for future triggers
            voice.config.params.insert(param.to_string(), value);

            // Collect all active nodes (both trigger nodes and note nodes)
            let mut nodes: Vec<NodeId> = voice.active_nodes.clone();
            nodes.extend(voice.note_nodes.values().copied());
            nodes
        };

        // Send MIDI CC if this is a MIDI voice with CC mapping for this param
        #[cfg(feature = "midi")]
        {
            let sent = send_cc_for_param(&voice_config, param, value, |device_id| {
                self.get_midi_sender(device_id)
            });
            if sent {
                tracing::debug!(
                    "Voice {:?}: sent MIDI CC for param '{}' = {}",
                    id,
                    param,
                    value
                );
            }
        }

        // Set param on all active synths (lock released)
        // For MIDI voices, this also updates the scsynth nodes (if any)
        for node_id in nodes {
            let _ = self.backend.set_param(node_id, param, value).await;
        }

        Ok(())
    }
}

/// Convert MIDI note number to frequency in Hz.
fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{AddAction, BufferInfo};
    use crate::compat::Instant;
    use crate::state::GroupState;
    use crate::types::{BufferId, BusId, GroupId};
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

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

    /// Mock backend that tracks synth creations and node operations.
    struct MockBackend {
        synths_created: AtomicU32,
        nodes_freed: AtomicU32,
        params_set: AtomicU32,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                synths_created: AtomicU32::new(0),
                nodes_freed: AtomicU32::new(0),
                params_set: AtomicU32::new(0),
            }
        }

        fn synths_created(&self) -> u32 {
            self.synths_created.load(Ordering::Relaxed)
        }

        fn nodes_freed(&self) -> u32 {
            self.nodes_freed.load(Ordering::Relaxed)
        }

        fn params_set(&self) -> u32 {
            self.params_set.load(Ordering::Relaxed)
        }
    }

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
            self.synths_created.fetch_add(1, Ordering::Relaxed);
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
            self.nodes_freed.fetch_add(1, Ordering::Relaxed);
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
            self.params_set.fetch_add(1, Ordering::Relaxed);
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

    /// Create a handler with a group and synthdef already registered.
    fn create_handler_with_group() -> (
        VoicesHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = VoicesHandler::new(backend.clone(), state.clone());
        (handler, backend, state)
    }

    /// Set up state with a group and synthdef registered.
    async fn setup_state_with_group(state: &Arc<RwLock<State>>) {
        let mut state_write = state.write().await;

        // Register the synthdef
        state_write.synthdefs.insert("test_synth".to_string());

        // Create a group
        let group_id = GroupId::new(1);
        state_write.groups.insert(
            group_id,
            GroupState {
                id: group_id,
                name: "TestGroup".to_string(),
                parent: None,
                node_id: NodeId(100),
                audio_bus: BusId(16),
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
            },
        );
    }

    // =========================================================================
    // Voice Creation Tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_voice() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));

        let result = handler.create(voice_id, config).await;
        assert!(result.is_ok(), "Voice creation should succeed");

        // Verify voice is in state
        let state_read = state.read().await;
        assert!(state_read.voices.contains_key(&voice_id));
    }

    #[tokio::test]
    async fn test_create_voice_duplicate_fails() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));

        handler.create(voice_id, config.clone()).await.unwrap();

        // Try to create again
        let result = handler.create(voice_id, config).await;
        assert!(result.is_err(), "Duplicate voice creation should fail");
    }

    #[tokio::test]
    async fn test_create_voice_group_not_found() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        // Reference a non-existent group
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(999));

        let result = handler.create(voice_id, config).await;
        assert!(result.is_err(), "Should fail with non-existent group");
    }

    #[tokio::test]
    async fn test_create_voice_synthdef_not_found() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        // Reference a non-existent synthdef
        let config = VoiceConfig::new("test_voice", "nonexistent_synth", GroupId::new(1));

        let result = handler.create(voice_id, config).await;
        assert!(result.is_err(), "Should fail with non-existent synthdef");
    }

    // =========================================================================
    // Voice Deletion Tests
    // =========================================================================

    #[tokio::test]
    async fn test_delete_voice() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let result = handler.delete(voice_id).await;
        assert!(result.is_ok(), "Voice deletion should succeed");

        // Verify voice is removed
        let state_read = state.read().await;
        assert!(!state_read.voices.contains_key(&voice_id));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_voice_fails() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let result = handler.delete(VoiceId::new(999)).await;
        assert!(result.is_err(), "Deleting non-existent voice should fail");
    }

    #[tokio::test]
    async fn test_delete_voice_frees_nodes() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        // Trigger to create synth nodes
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

        // Delete should free nodes
        handler.delete(voice_id).await.unwrap();

        assert!(
            backend.nodes_freed() >= 2,
            "Nodes should be freed on delete"
        );
    }

    // =========================================================================
    // Voice Trigger Tests
    // =========================================================================

    #[tokio::test]
    async fn test_trigger_voice() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let result = handler.trigger(voice_id, &ParamMap::new()).await;
        assert!(result.is_ok(), "Trigger should succeed");

        assert_eq!(backend.synths_created(), 1, "One synth should be created");
    }

    #[tokio::test]
    async fn test_trigger_nonexistent_voice_fails() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let result = handler.trigger(VoiceId::new(999), &ParamMap::new()).await;
        assert!(result.is_err(), "Triggering non-existent voice should fail");
    }

    #[tokio::test]
    async fn test_trigger_with_params() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let mut params = ParamMap::new();
        params.insert("freq".to_string(), 880.0);
        params.insert("amp".to_string(), 0.5);

        let result = handler.trigger(voice_id, &params).await;
        assert!(result.is_ok(), "Trigger with params should succeed");
        assert_eq!(backend.synths_created(), 1);
    }

    #[tokio::test]
    async fn test_trigger_respects_polyphony() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let mut config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        config.polyphony = 2; // Max 2 simultaneous voices
        handler.create(voice_id, config).await.unwrap();

        // Trigger 5 times
        for _ in 0..5 {
            handler.trigger(voice_id, &ParamMap::new()).await.unwrap();
        }

        // Should have created 5 synths
        assert_eq!(backend.synths_created(), 5);

        // Should have freed 3 old nodes (5 - polyphony 2 = 3)
        assert_eq!(
            backend.nodes_freed(),
            3,
            "Excess nodes should be freed for polyphony"
        );
    }

    // =========================================================================
    // Voice Stop Tests
    // =========================================================================

    #[tokio::test]
    async fn test_stop_voice() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        // Trigger twice
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

        // Stop should free all nodes
        let result = handler.stop(voice_id).await;
        assert!(result.is_ok(), "Stop should succeed");

        assert_eq!(
            backend.nodes_freed(),
            2,
            "All nodes should be freed on stop"
        );
    }

    #[tokio::test]
    async fn test_stop_nonexistent_voice_fails() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let result = handler.stop(VoiceId::new(999)).await;
        assert!(result.is_err(), "Stopping non-existent voice should fail");
    }

    // =========================================================================
    // Note On/Off Tests
    // =========================================================================

    #[tokio::test]
    async fn test_note_on() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let result = handler.note_on(voice_id, 60, 0.8).await; // C4, velocity 0.8
        assert!(result.is_ok(), "Note on should succeed");

        assert_eq!(backend.synths_created(), 1, "One synth should be created");
    }

    #[tokio::test]
    async fn test_note_on_same_note_replaces() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        // Play same note twice
        handler.note_on(voice_id, 60, 0.8).await.unwrap();
        handler.note_on(voice_id, 60, 0.9).await.unwrap();

        // Should create 2 synths, free 1 (the first one)
        assert_eq!(backend.synths_created(), 2);
        assert_eq!(
            backend.nodes_freed(),
            1,
            "Old note should be freed when same note played again"
        );
    }

    #[tokio::test]
    async fn test_note_off() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        handler.note_on(voice_id, 60, 0.8).await.unwrap();

        let result = handler.note_off(voice_id, 60).await;
        assert!(result.is_ok(), "Note off should succeed");

        // Note off sets gate to 0
        assert_eq!(backend.params_set(), 1, "Gate param should be set to 0");
    }

    #[tokio::test]
    async fn test_note_off_no_active_note() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        // Note off for a note that isn't playing
        let result = handler.note_off(voice_id, 60).await;
        assert!(
            result.is_ok(),
            "Note off for inactive note should succeed (no-op)"
        );

        assert_eq!(backend.params_set(), 0, "No params should be set");
    }

    #[tokio::test]
    async fn test_note_on_nonexistent_voice_fails() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let result = handler.note_on(VoiceId::new(999), 60, 0.8).await;
        assert!(
            result.is_err(),
            "Note on for non-existent voice should fail"
        );
    }

    #[tokio::test]
    async fn test_note_off_nonexistent_voice_fails() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let result = handler.note_off(VoiceId::new(999), 60).await;
        assert!(
            result.is_err(),
            "Note off for non-existent voice should fail"
        );
    }

    // =========================================================================
    // Mute Tests
    // =========================================================================

    #[tokio::test]
    async fn test_mute_voice() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let result = handler.mute(voice_id, true).await;
        assert!(result.is_ok(), "Mute should succeed");

        let state_read = state.read().await;
        let voice = state_read.voices.get(&voice_id).unwrap();
        assert!(voice.config.muted, "Voice should be muted");
    }

    #[tokio::test]
    async fn test_unmute_voice() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        handler.mute(voice_id, true).await.unwrap();
        handler.mute(voice_id, false).await.unwrap();

        let state_read = state.read().await;
        let voice = state_read.voices.get(&voice_id).unwrap();
        assert!(!voice.config.muted, "Voice should be unmuted");
    }

    #[tokio::test]
    async fn test_mute_nonexistent_voice_fails() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let result = handler.mute(VoiceId::new(999), true).await;
        assert!(result.is_err(), "Muting non-existent voice should fail");
    }

    // =========================================================================
    // Set Param Tests
    // =========================================================================

    #[tokio::test]
    async fn test_set_param() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let result = handler.set_param(voice_id, "freq", 880.0).await;
        assert!(result.is_ok(), "Set param should succeed");

        // Verify param is stored for future triggers
        let state_read = state.read().await;
        let voice = state_read.voices.get(&voice_id).unwrap();
        assert_eq!(*voice.config.params.get("freq").unwrap(), 880.0);
    }

    #[tokio::test]
    async fn test_set_param_updates_active_synths() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        // Create some active synths
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

        // Set param should update all active synths
        handler.set_param(voice_id, "freq", 880.0).await.unwrap();

        assert_eq!(
            backend.params_set(),
            2,
            "Param should be set on all active synths"
        );
    }

    #[tokio::test]
    async fn test_set_param_nonexistent_voice_fails() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let result = handler.set_param(VoiceId::new(999), "freq", 440.0).await;
        assert!(
            result.is_err(),
            "Setting param on non-existent voice should fail"
        );
    }

    // =========================================================================
    // Round-Robin Tests
    // =========================================================================

    #[tokio::test]
    async fn test_round_robin_increments() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let mut config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        config.round_robin_count = 4;
        handler.create(voice_id, config).await.unwrap();

        // Trigger multiple times
        for expected_rr in 0..8 {
            handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

            let state_read = state.read().await;
            let voice = state_read.voices.get(&voice_id).unwrap();
            let expected = ((expected_rr + 1) % 4) as u32;
            assert_eq!(
                voice.round_robin_position, expected,
                "RR position should wrap around"
            );
        }
    }

    // =========================================================================
    // Choke Groups Tests
    // =========================================================================

    #[tokio::test]
    async fn test_choke_group_frees_other_voices() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        // Create two voices in the same choke group
        let voice1_id = VoiceId::new(1);
        let mut config1 = VoiceConfig::new("closed_hat", "test_synth", GroupId::new(1));
        config1.choke_group = Some("hats".to_string());
        handler.create(voice1_id, config1).await.unwrap();

        let voice2_id = VoiceId::new(2);
        let mut config2 = VoiceConfig::new("open_hat", "test_synth", GroupId::new(1));
        config2.choke_group = Some("hats".to_string());
        handler.create(voice2_id, config2).await.unwrap();

        // Trigger voice1
        handler.trigger(voice1_id, &ParamMap::new()).await.unwrap();

        // Trigger voice2 - should choke voice1
        handler.trigger(voice2_id, &ParamMap::new()).await.unwrap();

        // 2 synths created, 1 freed (from choke)
        assert_eq!(backend.synths_created(), 2);
        assert_eq!(
            backend.nodes_freed(),
            1,
            "Choke group should free other voice's node"
        );
    }

    #[tokio::test]
    async fn test_different_choke_groups_do_not_interfere() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        // Create two voices in different choke groups
        let voice1_id = VoiceId::new(1);
        let mut config1 = VoiceConfig::new("closed_hat", "test_synth", GroupId::new(1));
        config1.choke_group = Some("hats".to_string());
        handler.create(voice1_id, config1).await.unwrap();

        let voice2_id = VoiceId::new(2);
        let mut config2 = VoiceConfig::new("conga", "test_synth", GroupId::new(1));
        config2.choke_group = Some("congas".to_string());
        handler.create(voice2_id, config2).await.unwrap();

        // Trigger both
        handler.trigger(voice1_id, &ParamMap::new()).await.unwrap();
        handler.trigger(voice2_id, &ParamMap::new()).await.unwrap();

        // 2 synths created, 0 freed (different choke groups)
        assert_eq!(backend.synths_created(), 2);
        assert_eq!(
            backend.nodes_freed(),
            0,
            "Different choke groups should not interfere"
        );
    }

    // =========================================================================
    // MIDI Frequency Conversion Tests
    // =========================================================================

    #[test]
    fn test_midi_to_freq_a4() {
        let freq = midi_to_freq(69); // A4
        assert!((freq - 440.0).abs() < 0.01, "A4 should be 440 Hz");
    }

    #[test]
    fn test_midi_to_freq_c4() {
        let freq = midi_to_freq(60); // C4
        assert!((freq - 261.63).abs() < 0.1, "C4 should be ~261.63 Hz");
    }

    #[test]
    fn test_midi_to_freq_a5() {
        let freq = midi_to_freq(81); // A5
        assert!((freq - 880.0).abs() < 0.01, "A5 should be 880 Hz");
    }

    #[test]
    fn test_midi_to_freq_a3() {
        let freq = midi_to_freq(57); // A3
        assert!((freq - 220.0).abs() < 0.01, "A3 should be 220 Hz");
    }

    // =========================================================================
    // Note On With Params Tests
    // =========================================================================

    #[tokio::test]
    async fn test_note_on_with_params_creates_synth() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let mut extra_params = ParamMap::new();
        extra_params.insert("cutoff".to_string(), 2000.0);
        extra_params.insert("resonance".to_string(), 0.8);

        let result = handler
            .note_on_with_params(voice_id, 60, 0.8, &extra_params)
            .await;
        assert!(result.is_ok(), "note_on_with_params should succeed");
        assert_eq!(backend.synths_created(), 1, "One synth should be created");
    }

    #[tokio::test]
    async fn test_note_on_with_empty_params_works() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let result = handler
            .note_on_with_params(voice_id, 60, 0.8, &ParamMap::new())
            .await;
        assert!(
            result.is_ok(),
            "note_on_with_params with empty params should work like note_on"
        );
        assert_eq!(backend.synths_created(), 1);
    }

    #[tokio::test]
    async fn test_note_on_with_params_nonexistent_voice_fails() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let result = handler
            .note_on_with_params(VoiceId::new(999), 60, 0.8, &ParamMap::new())
            .await;
        assert!(
            result.is_err(),
            "note_on_with_params for non-existent voice should fail"
        );
    }

    #[tokio::test]
    async fn test_note_on_with_params_replaces_same_note() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let mut params = ParamMap::new();
        params.insert("cutoff".to_string(), 2000.0);

        // Play same note twice with params
        handler
            .note_on_with_params(voice_id, 60, 0.8, &params)
            .await
            .unwrap();
        handler
            .note_on_with_params(voice_id, 60, 0.9, &params)
            .await
            .unwrap();

        // Should create 2 synths, free 1 (the first one)
        assert_eq!(backend.synths_created(), 2);
        assert_eq!(
            backend.nodes_freed(),
            1,
            "Old note should be freed when same note played again"
        );
    }

    #[tokio::test]
    async fn test_note_on_with_params_does_not_affect_voice_defaults() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let mut extra_params = ParamMap::new();
        extra_params.insert("cutoff".to_string(), 2000.0);

        handler
            .note_on_with_params(voice_id, 60, 0.8, &extra_params)
            .await
            .unwrap();

        // Verify the voice's default params were NOT modified
        let state_read = state.read().await;
        let voice = state_read.voices.get(&voice_id).unwrap();
        assert!(
            !voice.config.params.contains_key("cutoff"),
            "Per-note params should NOT be saved to voice defaults"
        );
    }

    #[tokio::test]
    async fn test_note_on_with_params_different_notes() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        // Play different notes with different params
        let mut params_a = ParamMap::new();
        params_a.insert("cutoff".to_string(), 2000.0);

        let mut params_b = ParamMap::new();
        params_b.insert("cutoff".to_string(), 500.0);

        handler
            .note_on_with_params(voice_id, 60, 0.8, &params_a)
            .await
            .unwrap();
        handler
            .note_on_with_params(voice_id, 64, 0.8, &params_b)
            .await
            .unwrap();

        // Both notes should have their own synths
        assert_eq!(
            backend.synths_created(),
            2,
            "Two different notes should create two synths"
        );
        assert_eq!(
            backend.nodes_freed(),
            0,
            "Different notes should not free each other"
        );
    }
}
